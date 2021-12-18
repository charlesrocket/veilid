mod bucket;
mod bucket_entry;
mod debug;
mod dial_info_entry;
mod find_nodes;
mod node_ref;
mod stats_accounting;

use crate::dht::*;
use crate::intf::*;
use crate::network_manager::*;
use crate::rpc_processor::*;
use crate::xx::*;
use crate::*;
use alloc::collections::VecDeque;
use alloc::str::FromStr;
use bucket::*;
pub use bucket_entry::*;
pub use debug::*;
pub use dial_info_entry::*;
pub use find_nodes::*;
use futures_util::stream::{FuturesUnordered, StreamExt};
pub use node_ref::*;
pub use stats_accounting::*;

//////////////////////////////////////////////////////////////////////////

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub enum DialInfoOrigin {
    Static,
    Discovered,
    Mapped,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct DialInfoDetail {
    pub dial_info: DialInfo,
    pub origin: DialInfoOrigin,
    pub network_class: Option<NetworkClass>,
    pub timestamp: u64,
}

struct RoutingTableInner {
    network_manager: NetworkManager,
    node_id: DHTKey,
    node_id_secret: DHTKeySecret,
    buckets: Vec<Bucket>,
    local_dial_info: Vec<DialInfoDetail>,
    global_dial_info: Vec<DialInfoDetail>,
    bucket_entry_count: usize,
    // Waiters
    eventual_changed_dial_info: Eventual,
    // Transfer stats for this node
    stats_accounting: StatsAccounting,
    // latency: Option<LatencyStats>,
    transfer_stats: TransferStatsDownUp,
}

struct RoutingTableUnlockedInner {
    // Background processes
    rolling_transfers_task: TickTask,
    bootstrap_task: TickTask,
    peer_minimum_refresh_task: TickTask,
    ping_validator_task: TickTask,
}

#[derive(Clone)]
pub struct RoutingTable {
    config: VeilidConfig,
    inner: Arc<Mutex<RoutingTableInner>>,
    unlocked_inner: Arc<RoutingTableUnlockedInner>,
}

impl RoutingTable {
    fn new_inner(network_manager: NetworkManager) -> RoutingTableInner {
        RoutingTableInner {
            network_manager,
            node_id: DHTKey::default(),
            node_id_secret: DHTKeySecret::default(),
            buckets: Vec::new(),
            local_dial_info: Vec::new(),
            global_dial_info: Vec::new(),
            bucket_entry_count: 0,
            eventual_changed_dial_info: Eventual::new(),
            stats_accounting: StatsAccounting::new(),
            transfer_stats: TransferStatsDownUp::default(),
        }
    }
    fn new_unlocked_inner(config: VeilidConfig) -> RoutingTableUnlockedInner {
        let c = config.get();
        RoutingTableUnlockedInner {
            rolling_transfers_task: TickTask::new(ROLLING_TRANSFERS_INTERVAL_SECS),
            bootstrap_task: TickTask::new(1),
            peer_minimum_refresh_task: TickTask::new_us(c.network.dht.min_peer_refresh_time),
            ping_validator_task: TickTask::new(1),
        }
    }
    pub fn new(network_manager: NetworkManager) -> Self {
        let config = network_manager.config();
        let this = Self {
            config: config.clone(),
            inner: Arc::new(Mutex::new(Self::new_inner(network_manager))),
            unlocked_inner: Arc::new(Self::new_unlocked_inner(config)),
        };
        // Set rolling transfers tick task
        {
            let this2 = this.clone();
            this.unlocked_inner
                .rolling_transfers_task
                .set_routine(move |l, t| {
                    Box::pin(this2.clone().rolling_transfers_task_routine(l, t))
                });
        }
        // Set bootstrap tick task
        {
            let this2 = this.clone();
            this.unlocked_inner
                .bootstrap_task
                .set_routine(move |_l, _t| Box::pin(this2.clone().bootstrap_task_routine()));
        }
        // Set peer minimum refresh tick task
        {
            let this2 = this.clone();
            this.unlocked_inner
                .peer_minimum_refresh_task
                .set_routine(move |_l, _t| {
                    Box::pin(this2.clone().peer_minimum_refresh_task_routine())
                });
        }
        // Set ping validator tick task
        {
            let this2 = this.clone();
            this.unlocked_inner
                .ping_validator_task
                .set_routine(move |l, t| Box::pin(this2.clone().ping_validator_task_routine(l, t)));
        }
        this
    }

    pub fn network_manager(&self) -> NetworkManager {
        self.inner.lock().network_manager.clone()
    }
    pub fn rpc_processor(&self) -> RPCProcessor {
        self.network_manager().rpc_processor()
    }

    pub fn node_id(&self) -> DHTKey {
        self.inner.lock().node_id
    }

    pub fn node_id_secret(&self) -> DHTKeySecret {
        self.inner.lock().node_id_secret
    }

    pub fn has_local_dial_info(&self) -> bool {
        let inner = self.inner.lock();
        !inner.local_dial_info.is_empty()
    }

    pub fn local_dial_info(&self) -> Vec<DialInfoDetail> {
        let inner = self.inner.lock();
        inner.local_dial_info.clone()
    }

    pub fn local_dial_info_for_protocol(&self, protocol_type: ProtocolType) -> Vec<DialInfoDetail> {
        let inner = self.inner.lock();
        inner
            .local_dial_info
            .iter()
            .filter_map(|di| {
                if di.dial_info.protocol_type() != protocol_type {
                    None
                } else {
                    Some(di.clone())
                }
            })
            .collect()
    }

    pub fn local_dial_info_for_protocol_address_type(
        &self,
        protocol_address_type: ProtocolAddressType,
    ) -> Vec<DialInfoDetail> {
        let inner = self.inner.lock();
        inner
            .local_dial_info
            .iter()
            .filter_map(|di| {
                if di.dial_info.protocol_address_type() != protocol_address_type {
                    None
                } else {
                    Some(di.clone())
                }
            })
            .collect()
    }

    pub fn register_local_dial_info(&self, dial_info: DialInfo, origin: DialInfoOrigin) {
        let timestamp = get_timestamp();
        let mut inner = self.inner.lock();

        inner.local_dial_info.push(DialInfoDetail {
            dial_info: dial_info.clone(),
            origin,
            network_class: None,
            timestamp,
        });

        info!(
            "Local Dial Info: {}",
            NodeDialInfoSingle {
                node_id: NodeId::new(inner.node_id),
                dial_info
            }
            .to_string(),
        );
        debug!("    Origin: {:?}", origin);

        Self::trigger_changed_dial_info(&mut *inner);
    }

    pub fn clear_local_dial_info(&self) {
        let mut inner = self.inner.lock();
        inner.local_dial_info.clear();
        Self::trigger_changed_dial_info(&mut *inner);
    }

    pub fn has_global_dial_info(&self) -> bool {
        let inner = self.inner.lock();
        !inner.global_dial_info.is_empty()
    }

    pub fn global_dial_info(&self) -> Vec<DialInfoDetail> {
        let inner = self.inner.lock();
        inner.global_dial_info.clone()
    }

    pub fn global_dial_info_for_protocol(
        &self,
        protocol_type: ProtocolType,
    ) -> Vec<DialInfoDetail> {
        let inner = self.inner.lock();
        inner
            .global_dial_info
            .iter()
            .filter_map(|di| {
                if di.dial_info.protocol_type() != protocol_type {
                    None
                } else {
                    Some(di.clone())
                }
            })
            .collect()
    }
    pub fn global_dial_info_for_protocol_address_type(
        &self,
        protocol_address_type: ProtocolAddressType,
    ) -> Vec<DialInfoDetail> {
        let inner = self.inner.lock();
        inner
            .global_dial_info
            .iter()
            .filter_map(|di| {
                if di.dial_info.protocol_address_type() != protocol_address_type {
                    None
                } else {
                    Some(di.clone())
                }
            })
            .collect()
    }

    pub fn register_global_dial_info(
        &self,
        dial_info: DialInfo,
        network_class: Option<NetworkClass>,
        origin: DialInfoOrigin,
    ) {
        let ts = get_timestamp();
        let mut inner = self.inner.lock();

        inner.global_dial_info.push(DialInfoDetail {
            dial_info: dial_info.clone(),
            origin,
            network_class,
            timestamp: ts,
        });

        info!(
            "Public Dial Info: {}",
            NodeDialInfoSingle {
                node_id: NodeId::new(inner.node_id),
                dial_info
            }
            .to_string(),
        );
        debug!("    Origin: {:?}", origin);
        debug!("    Network Class: {:?}", network_class);
        Self::trigger_changed_dial_info(&mut *inner);
    }

    pub fn clear_global_dial_info(&self) {
        let mut inner = self.inner.lock();
        inner.global_dial_info.clear();
        Self::trigger_changed_dial_info(&mut *inner);
    }

    pub async fn wait_changed_dial_info(&self) {
        let inst = self
            .inner
            .lock()
            .eventual_changed_dial_info
            .instance_empty();
        inst.await;
    }
    fn trigger_changed_dial_info(inner: &mut RoutingTableInner) {
        let mut new_eventual = Eventual::new();
        core::mem::swap(&mut inner.eventual_changed_dial_info, &mut new_eventual);
        spawn(new_eventual.resolve()).detach();
    }

    fn bucket_depth(index: usize) -> usize {
        match index {
            0 => 256,
            1 => 128,
            2 => 64,
            3 => 32,
            4 => 16,
            5 => 8,
            6 => 4,
            7 => 4,
            8 => 4,
            9 => 4,
            _ => 4,
        }
    }

    pub async fn init(&self) -> Result<(), String> {
        let mut inner = self.inner.lock();
        // Size the buckets (one per bit)
        inner.buckets.reserve(DHT_KEY_LENGTH * 8);
        for _ in 0..DHT_KEY_LENGTH * 8 {
            let bucket = Bucket::new(self.clone());
            inner.buckets.push(bucket);
        }

        // make local copy of node id for easy access
        let c = self.config.get();
        inner.node_id = c.network.node_id;
        inner.node_id_secret = c.network.node_id_secret;

        Ok(())
    }

    pub async fn terminate(&self) {
        *self.inner.lock() = Self::new_inner(self.network_manager());
    }

    // Just match address and port to help sort dialinfoentries for buckets
    // because inbound connections will not have dialinfo associated with them
    // but should have ip addresses if they have changed
    fn dial_info_peer_address_match(dial_info: &DialInfo, peer_addr: &PeerAddress) -> bool {
        match dial_info {
            DialInfo::UDP(_) => {
                peer_addr.protocol_type == ProtocolType::UDP
                    && peer_addr.port == dial_info.port()
                    && peer_addr.address.address_string() == dial_info.address_string()
            }
            DialInfo::TCP(_) => {
                peer_addr.protocol_type == ProtocolType::TCP
                    && peer_addr.port == dial_info.port()
                    && peer_addr.address.address_string() == dial_info.address_string()
            }
            DialInfo::WS(_) => {
                peer_addr.protocol_type == ProtocolType::WS
                    && peer_addr.port == dial_info.port()
                    && peer_addr.address.address_string() == dial_info.address_string()
            }
            DialInfo::WSS(_) => {
                peer_addr.protocol_type == ProtocolType::WSS
                    && peer_addr.port == dial_info.port()
                    && peer_addr.address.address_string() == dial_info.address_string()
            }
        }
    }

    // Attempt to settle buckets and remove entries down to the desired number
    // which may not be possible due extant NodeRefs
    fn kick_bucket(inner: &mut RoutingTableInner, idx: usize) {
        let bucket = &mut inner.buckets[idx];
        let bucket_depth = Self::bucket_depth(idx);

        if let Some(dead_node_ids) = bucket.kick(bucket_depth) {
            // Remove counts
            inner.bucket_entry_count -= dead_node_ids.len();
            log_rtab!("Routing table now has {} nodes", inner.bucket_entry_count);

            // Now purge the routing table inner vectors
            //let filter = |k: &DHTKey| dead_node_ids.contains(k);
            //inner.closest_reliable_nodes.retain(filter);
            //inner.fastest_reliable_nodes.retain(filter);
            //inner.closest_nodes.retain(filter);
            //inner.fastest_nodes.retain(filter);
        }
    }

    fn find_bucket_index(inner: &RoutingTableInner, node_id: DHTKey) -> usize {
        distance(&node_id, &inner.node_id)
            .first_nonzero_bit()
            .unwrap()
    }

    fn drop_node_ref(&self, node_id: DHTKey) {
        // Reduce ref count on entry
        let mut inner = self.inner.lock();
        let idx = Self::find_bucket_index(&*inner, node_id);
        let new_ref_count = {
            let bucket = &mut inner.buckets[idx];
            let entry = bucket.entry_mut(&node_id).unwrap();
            entry.ref_count -= 1;
            entry.ref_count
        };

        // If this entry could possibly go away, kick the bucket
        if new_ref_count == 0 {
            // it important to do this in the same inner lock as the ref count decrease
            Self::kick_bucket(&mut *inner, idx);
        }
    }

    pub fn create_node_ref(&self, node_id: DHTKey) -> Result<NodeRef, String> {
        // Ensure someone isn't trying register this node itself
        if node_id == self.node_id() {
            return Err("can't register own node".to_owned()).map_err(logthru_rtab!(error));
        }

        // Insert into bucket, possibly evicting the newest bucket member
        let noderef = match self.lookup_node_ref(node_id) {
            None => {
                // Make new entry
                let mut inner = self.inner.lock();
                let idx = Self::find_bucket_index(&*inner, node_id);
                let nr = {
                    // Get the bucket for the entry
                    let bucket = &mut inner.buckets[idx];
                    // Add new entry
                    let nr = bucket.add_entry(node_id);

                    // Update count
                    inner.bucket_entry_count += 1;
                    log_rtab!("Routing table now has {} nodes", inner.bucket_entry_count);
                    nr
                };

                // Kick the bucket
                // It is important to do this in the same inner lock as the add_entry
                Self::kick_bucket(&mut *inner, idx);

                nr
            }
            Some(nr) => nr,
        };

        Ok(noderef)
    }

    pub fn lookup_node_ref(&self, node_id: DHTKey) -> Option<NodeRef> {
        let mut inner = self.inner.lock();
        let idx = Self::find_bucket_index(&*inner, node_id);
        let bucket = &mut inner.buckets[idx];
        bucket
            .entry_mut(&node_id)
            .map(|e| NodeRef::new(self.clone(), node_id, e))
    }

    // Shortcut function to add a node to our routing table if it doesn't exist
    // and add the dial info we have for it, since that's pretty common
    pub fn register_node_with_dial_info(
        &self,
        node_id: DHTKey,
        dial_infos: &[DialInfo],
    ) -> Result<NodeRef, String> {
        let nr = self.create_node_ref(node_id)?;
        nr.operate(move |e| -> Result<(), String> {
            for di in dial_infos {
                e.add_dial_info(di.clone())?;
            }
            Ok(())
        })?;

        Ok(nr)
    }

    // Shortcut function to add a node to our routing table if it doesn't exist
    // and add the last peer address we have for it, since that's pretty common
    pub fn register_node_with_existing_connection(
        &self,
        node_id: DHTKey,
        descriptor: ConnectionDescriptor,
        timestamp: u64,
    ) -> Result<NodeRef, String> {
        let nr = self.create_node_ref(node_id)?;
        nr.operate(move |e| {
            // set the most recent node address for connection finding and udp replies
            e.set_last_connection(descriptor, timestamp);
        });

        Ok(nr)
    }

    fn operate_on_bucket_entry<T, F>(&self, node_id: DHTKey, f: F) -> T
    where
        F: FnOnce(&mut BucketEntry) -> T,
    {
        let mut inner = self.inner.lock();
        let idx = Self::find_bucket_index(&*inner, node_id);
        let bucket = &mut inner.buckets[idx];
        let entry = bucket.entry_mut(&node_id).unwrap();
        f(entry)
    }

    pub async fn find_self(&self, node_ref: NodeRef) -> Result<Vec<NodeRef>, String> {
        let node_id = self.node_id();
        let rpc_processor = self.rpc_processor();

        let res = rpc_processor
            .rpc_call_find_node(
                Destination::Direct(node_ref.clone()),
                node_id,
                None,
                RespondTo::Sender,
            )
            .await
            .map_err(map_to_string)
            .map_err(logthru_rtab!())?;
        log_rtab!(
            "find_self for at {:?} answered in {}ms",
            &node_ref,
            timestamp_to_secs(res.latency) * 1000.0f64
        );

        // register nodes we'd found
        let mut out = Vec::<NodeRef>::with_capacity(res.peers.len());
        for p in res.peers {
            // if our own node if is in the list then ignore it, as we don't add ourselves to our own routing table
            if p.node_id.key == node_id {
                continue;
            }

            // register the node if it's new
            let nr = self
                .register_node_with_dial_info(p.node_id.key, &p.dial_infos)
                .map_err(map_to_string)
                .map_err(logthru_rtab!(
                    "couldn't register node {} at {:?}",
                    p.node_id.key,
                    &p.dial_infos
                ))?;
            out.push(nr);
        }
        Ok(out)
    }

    pub async fn reverse_find_node(&self, node_ref: NodeRef, wide: bool) {
        // Ask bootstrap node to 'find' our own node so we can get some more nodes near ourselves
        // and then contact those nodes to inform -them- that we exist

        // Ask bootstrap server for nodes closest to our own node
        let closest_nodes = match self.find_self(node_ref.clone()).await {
            Err(e) => {
                log_rtab!(error
                    "reverse_find_node: find_self failed for {:?}: {}",
                    &node_ref, e
                );
                return;
            }
            Ok(v) => v,
        };

        // Ask each node near us to find us as well
        if wide {
            for closest_nr in closest_nodes {
                match self.find_self(closest_nr.clone()).await {
                    Err(e) => {
                        log_rtab!(error
                            "reverse_find_node: closest node find_self failed for {:?}: {}",
                            &closest_nr, e
                        );
                        return;
                    }
                    Ok(v) => v,
                };
            }
        }
    }

    async fn bootstrap_task_routine(self) -> Result<(), String> {
        let bootstrap = {
            let c = self.config.get();
            c.network.bootstrap.clone()
        };

        log_rtab!("--- bootstrap_task");

        // Map all bootstrap entries to a single key with multiple dialinfo
        let mut bsmap: BTreeMap<DHTKey, Vec<DialInfo>> = BTreeMap::new();
        for b in bootstrap {
            let ndis = NodeDialInfoSingle::from_str(b.as_str())
                .map_err(logthru_rtab!("Invalid dial info in bootstrap entry: {}", b))?;
            let node_id = ndis.node_id.key;
            bsmap
                .entry(node_id)
                .or_insert_with(Vec::new)
                .push(ndis.dial_info);
        }
        log_rtab!("    bootstrap list: {:?}", bsmap);

        // Run all bootstrap operations concurrently
        let mut unord = FuturesUnordered::new();
        for (k, v) in bsmap {
            let nr = self
                .register_node_with_dial_info(k, &v)
                .map_err(logthru_rtab!("Couldn't add bootstrap node: {}", k))?;
            log_rtab!("    bootstrapping {} with {:?}", k.encode(), &v);
            unord.push(self.reverse_find_node(nr, true));
        }
        while unord.next().await.is_some() {}
        Ok(())
    }

    ///////////////////////////////////////////////////////////
    /// Peer ping validation

    // Ask our remaining peers to give us more peers before we go
    // back to the bootstrap servers to keep us from bothering them too much
    async fn peer_minimum_refresh_task_routine(self) -> Result<(), String> {
        log_rtab!("--- peer_minimum_refresh task");

        // get list of all peers we know about, even the unreliable ones, and ask them to bootstrap too
        let noderefs = {
            let mut inner = self.inner.lock();
            let mut noderefs = Vec::<NodeRef>::with_capacity(inner.bucket_entry_count);
            for b in &mut inner.buckets {
                for (k, entry) in b.entries_mut() {
                    noderefs.push(NodeRef::new(self.clone(), *k, entry))
                }
            }
            noderefs
        };
        log_rtab!("    refreshing with nodes: {:?}", noderefs);

        // do peer minimum search concurrently
        let mut unord = FuturesUnordered::new();
        for nr in noderefs {
            debug!("    --- peer minimum search with {:?}", nr);
            unord.push(self.reverse_find_node(nr, false));
        }
        while unord.next().await.is_some() {}

        Ok(())
    }

    // Ping each node in the routing table if they need to be pinged
    // to determine their reliability
    async fn ping_validator_task_routine(self, _last_ts: u64, cur_ts: u64) -> Result<(), String> {
        log_rtab!("--- ping_validator task");
        let rpc = self.rpc_processor();
        let mut inner = self.inner.lock();
        for b in &mut inner.buckets {
            for (k, entry) in b.entries_mut() {
                if entry.needs_ping(cur_ts) {
                    let nr = NodeRef::new(self.clone(), *k, entry);
                    log_rtab!(
                        "    --- ping validating: {:?} ({})",
                        nr,
                        entry.state_debug_info(cur_ts)
                    );
                    intf::spawn_local(rpc.clone().rpc_call_info(nr)).detach();
                }
            }
        }
        Ok(())
    }

    // Compute transfer statistics to determine how 'fast' a node is
    async fn rolling_transfers_task_routine(self, last_ts: u64, cur_ts: u64) -> Result<(), String> {
        log_rtab!("--- rolling_transfers task");
        let inner = &mut *self.inner.lock();

        // Roll our own node's transfers
        inner
            .stats_accounting
            .roll_transfers(last_ts, cur_ts, &mut inner.transfer_stats);

        // Roll all bucket entry transfers
        for b in &mut inner.buckets {
            b.roll_transfers(last_ts, cur_ts);
        }
        Ok(())
    }

    // Ticks about once per second
    // to run tick tasks which may run at slower tick rates as configured
    pub async fn tick(&self) -> Result<(), String> {
        // Do rolling transfers every ROLLING_TRANSFERS_INTERVAL_SECS secs
        self.unlocked_inner.rolling_transfers_task.tick().await?;

        // If routing table is empty, then add the bootstrap nodes to it
        if self.inner.lock().bucket_entry_count == 0 {
            self.unlocked_inner.bootstrap_task.tick().await?;
        }

        // If we still don't have enough peers, find nodes until we do
        let min_peer_count = {
            let c = self.config.get();
            c.network.dht.min_peer_count as usize
        };
        if self.inner.lock().bucket_entry_count < min_peer_count {
            self.unlocked_inner.peer_minimum_refresh_task.tick().await?;
        }
        // Ping validate some nodes to groom the table
        self.unlocked_inner.ping_validator_task.tick().await?;

        Ok(())
    }

    //////////////////////////////////////////////////////////////////////
    // Stats Accounting

    pub fn ping_sent(&self, node_ref: NodeRef, ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_up(bytes);
        node_ref.operate(|e| {
            e.ping_sent(ts, bytes);
        })
    }
    pub fn ping_rcvd(&self, node_ref: NodeRef, ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_down(bytes);
        node_ref.operate(|e| {
            e.ping_rcvd(ts, bytes);
        })
    }
    pub fn pong_sent(&self, node_ref: NodeRef, ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_up(bytes);
        node_ref.operate(|e| {
            e.pong_sent(ts, bytes);
        })
    }
    pub fn pong_rcvd(&self, node_ref: NodeRef, send_ts: u64, recv_ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_down(bytes);
        node_ref.operate(|e| {
            e.pong_rcvd(send_ts, recv_ts, bytes);
        })
    }
    pub fn ping_lost(&self, node_ref: NodeRef, ts: u64) {
        node_ref.operate(|e| {
            e.ping_lost(ts);
        })
    }
    pub fn question_sent(&self, node_ref: NodeRef, ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_up(bytes);
        node_ref.operate(|e| {
            e.question_sent(ts, bytes);
        })
    }
    pub fn question_rcvd(&self, node_ref: NodeRef, ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_down(bytes);
        node_ref.operate(|e| {
            e.question_rcvd(ts, bytes);
        })
    }
    pub fn answer_sent(&self, node_ref: NodeRef, ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_up(bytes);
        node_ref.operate(|e| {
            e.answer_sent(ts, bytes);
        })
    }
    pub fn answer_rcvd(&self, node_ref: NodeRef, send_ts: u64, recv_ts: u64, bytes: u64) {
        self.inner.lock().stats_accounting.add_down(bytes);
        node_ref.operate(|e| {
            e.answer_rcvd(send_ts, recv_ts, bytes);
        })
    }
    pub fn question_lost(&self, node_ref: NodeRef, ts: u64) {
        node_ref.operate(|e| {
            e.question_lost(ts);
        })
    }
}
