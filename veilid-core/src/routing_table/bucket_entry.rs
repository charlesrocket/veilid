use super::*;
use core::sync::atomic::{AtomicU32, Ordering};

/// Reliable pings are done with increased spacing between pings

/// - Start secs is the number of seconds between the first two pings
const RELIABLE_PING_INTERVAL_START_SECS: u32 = 10;
/// - Max secs is the maximum number of seconds between consecutive pings
const RELIABLE_PING_INTERVAL_MAX_SECS: u32 = 10 * 60;
/// - Multiplier changes the number of seconds between pings over time
///   making it longer as the node becomes more reliable
const RELIABLE_PING_INTERVAL_MULTIPLIER: f64 = 2.0;

/// Unreliable pings are done for a fixed amount of time while the
/// node is given a chance to come back online before it is made dead
/// If a node misses a single ping, it is marked unreliable and must
/// return reliable pings for the duration of the span before being
/// marked reliable again
///
/// - Span is the number of seconds total to attempt to validate the node
const UNRELIABLE_PING_SPAN_SECS: u32 = 60;
/// - Interval is the number of seconds between each ping
const UNRELIABLE_PING_INTERVAL_SECS: u32 = 5;

/// Keepalive pings are done occasionally to ensure holepunched public dialinfo
/// remains valid, as well as to make sure we remain in any relay node's routing table
const KEEPALIVE_PING_INTERVAL_SECS: u32 = 10;

/// How many times do we try to ping a never-reached node before we call it dead
const NEVER_REACHED_PING_COUNT: u32 = 3;

// Do not change order here, it will mess up other sorts

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BucketEntryState {
    Dead,
    Unreliable,
    Reliable,
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
struct LastConnectionKey(ProtocolType, AddressType);

/// Bucket entry information specific to the LocalNetwork RoutingDomain
#[derive(Debug)]
pub struct BucketEntryPublicInternet {
    /// The PublicInternet node info
    signed_node_info: Option<Box<SignedNodeInfo>>,
    /// If this node has seen our publicinternet node info
    seen_our_node_info: bool,
    /// Last known node status
    node_status: Option<PublicInternetNodeStatus>,
}

/// Bucket entry information specific to the LocalNetwork RoutingDomain
#[derive(Debug)]
pub struct BucketEntryLocalNetwork {
    /// The LocalNetwork node info
    signed_node_info: Option<Box<SignedNodeInfo>>,
    /// If this node has seen our localnetwork node info
    seen_our_node_info: bool,
    /// Last known node status
    node_status: Option<LocalNetworkNodeStatus>,
}

#[derive(Debug)]
pub struct BucketEntryInner {
    min_max_version: Option<(u8, u8)>,
    updated_since_last_network_change: bool,
    last_connections: BTreeMap<LastConnectionKey, (ConnectionDescriptor, u64)>,
    public_internet: BucketEntryPublicInternet,
    local_network: BucketEntryLocalNetwork,
    peer_stats: PeerStats,
    latency_stats_accounting: LatencyStatsAccounting,
    transfer_stats_accounting: TransferStatsAccounting,
    #[cfg(feature = "tracking")]
    next_track_id: usize,
    #[cfg(feature = "tracking")]
    node_ref_tracks: HashMap<usize, backtrace::Backtrace>,
}

impl BucketEntryInner {
    #[cfg(feature = "tracking")]
    pub fn track(&mut self) -> usize {
        let track_id = self.next_track_id;
        self.next_track_id += 1;
        self.node_ref_tracks
            .insert(track_id, backtrace::Backtrace::new_unresolved());
        track_id
    }

    #[cfg(feature = "tracking")]
    pub fn untrack(&mut self, track_id: usize) {
        self.node_ref_tracks.remove(&track_id);
    }

    // Less is faster
    pub fn cmp_fastest(e1: &Self, e2: &Self) -> std::cmp::Ordering {
        // Lower latency to the front
        if let Some(e1_latency) = &e1.peer_stats.latency {
            if let Some(e2_latency) = &e2.peer_stats.latency {
                e1_latency.average.cmp(&e2_latency.average)
            } else {
                std::cmp::Ordering::Less
            }
        } else if e2.peer_stats.latency.is_some() {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }

    // Less is more reliable then faster
    pub fn cmp_fastest_reliable(cur_ts: u64, e1: &Self, e2: &Self) -> std::cmp::Ordering {
        // Reverse compare so most reliable is at front
        let ret = e2.state(cur_ts).cmp(&e1.state(cur_ts));
        if ret != std::cmp::Ordering::Equal {
            return ret;
        }

        // Lower latency to the front
        if let Some(e1_latency) = &e1.peer_stats.latency {
            if let Some(e2_latency) = &e2.peer_stats.latency {
                e1_latency.average.cmp(&e2_latency.average)
            } else {
                std::cmp::Ordering::Less
            }
        } else if e2.peer_stats.latency.is_some() {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    }

    pub fn sort_fastest_reliable_fn(cur_ts: u64) -> impl FnMut(&Self, &Self) -> std::cmp::Ordering {
        move |e1, e2| Self::cmp_fastest_reliable(cur_ts, e1, e2)
    }

    pub fn clear_signed_node_info(&mut self, routing_domain: RoutingDomain) {
        // Get the correct signed_node_info for the chosen routing domain
        let opt_current_sni = match routing_domain {
            RoutingDomain::LocalNetwork => &mut self.local_network.signed_node_info,
            RoutingDomain::PublicInternet => &mut self.public_internet.signed_node_info,
        };
        *opt_current_sni = None;
    }

    // Retuns true if the node info changed
    pub fn update_signed_node_info(
        &mut self,
        routing_domain: RoutingDomain,
        signed_node_info: SignedNodeInfo,
    ) {
        // Get the correct signed_node_info for the chosen routing domain
        let opt_current_sni = match routing_domain {
            RoutingDomain::LocalNetwork => &mut self.local_network.signed_node_info,
            RoutingDomain::PublicInternet => &mut self.public_internet.signed_node_info,
        };

        // See if we have an existing signed_node_info to update or not
        if let Some(current_sni) = opt_current_sni {
            // If the timestamp hasn't changed or is less, ignore this update
            if signed_node_info.timestamp <= current_sni.timestamp {
                // If we received a node update with the same timestamp
                // we can make this node live again, but only if our network has recently changed
                // which may make nodes that were unreachable now reachable with the same dialinfo
                if !self.updated_since_last_network_change
                    && signed_node_info.timestamp == current_sni.timestamp
                {
                    // No need to update the signednodeinfo though since the timestamp is the same
                    // Touch the node and let it try to live again
                    self.updated_since_last_network_change = true;
                    self.touch_last_seen(intf::get_timestamp());
                }
                return;
            }
        }

        // Update the protocol min/max version we have
        self.min_max_version = Some((
            signed_node_info.node_info.min_version,
            signed_node_info.node_info.max_version,
        ));

        // Update the signed node info
        *opt_current_sni = Some(Box::new(signed_node_info));
        self.updated_since_last_network_change = true;
        self.touch_last_seen(intf::get_timestamp());
    }

    pub fn has_node_info(&self, routing_domain_set: RoutingDomainSet) -> bool {
        for routing_domain in routing_domain_set {
            // Get the correct signed_node_info for the chosen routing domain
            let opt_current_sni = match routing_domain {
                RoutingDomain::LocalNetwork => &self.local_network.signed_node_info,
                RoutingDomain::PublicInternet => &self.public_internet.signed_node_info,
            };
            if opt_current_sni.is_some() {
                return true;
            }
        }
        false
    }

    pub fn node_info(&self, routing_domain: RoutingDomain) -> Option<&NodeInfo> {
        let opt_current_sni = match routing_domain {
            RoutingDomain::LocalNetwork => &self.local_network.signed_node_info,
            RoutingDomain::PublicInternet => &self.public_internet.signed_node_info,
        };
        opt_current_sni.as_ref().map(|s| &s.node_info)
    }

    pub fn signed_node_info(&self, routing_domain: RoutingDomain) -> Option<&SignedNodeInfo> {
        let opt_current_sni = match routing_domain {
            RoutingDomain::LocalNetwork => &self.local_network.signed_node_info,
            RoutingDomain::PublicInternet => &self.public_internet.signed_node_info,
        };
        opt_current_sni.as_ref().map(|s| s.as_ref())
    }

    pub fn make_peer_info(&self, key: DHTKey, routing_domain: RoutingDomain) -> Option<PeerInfo> {
        let opt_current_sni = match routing_domain {
            RoutingDomain::LocalNetwork => &self.local_network.signed_node_info,
            RoutingDomain::PublicInternet => &self.public_internet.signed_node_info,
        };
        opt_current_sni.as_ref().map(|s| PeerInfo {
            node_id: NodeId::new(key),
            signed_node_info: *s.clone(),
        })
    }

    pub fn best_routing_domain(
        &self,
        routing_domain_set: RoutingDomainSet,
    ) -> Option<RoutingDomain> {
        for routing_domain in routing_domain_set {
            let opt_current_sni = match routing_domain {
                RoutingDomain::LocalNetwork => &self.local_network.signed_node_info,
                RoutingDomain::PublicInternet => &self.public_internet.signed_node_info,
            };
            if opt_current_sni.is_some() {
                return Some(routing_domain);
            }
        }
        None
    }

    fn descriptor_to_key(&self, last_connection: ConnectionDescriptor) -> LastConnectionKey {
        LastConnectionKey(
            last_connection.protocol_type(),
            last_connection.address_type(),
        )
    }

    // Stores a connection descriptor in this entry's table of last connections
    pub fn set_last_connection(&mut self, last_connection: ConnectionDescriptor, timestamp: u64) {
        let key = self.descriptor_to_key(last_connection);
        self.last_connections
            .insert(key, (last_connection, timestamp));
    }

    // Clears the table of last connections to ensure we create new ones and drop any existing ones
    pub fn clear_last_connections(&mut self) {
        self.last_connections.clear();
    }

    // Gets the 'last connection' that matches a specific connection key
    // pub(super) fn last_connection(
    //     &self,
    //     protocol_type: ProtocolType,
    //     address_type: AddressType,
    // ) -> Option<(ConnectionDescriptor, u64)> {
    //     let key = LastConnectionKey(protocol_type, address_type);
    //     self.last_connections.get(&key).cloned()
    // }

    // Gets all the 'last connections' that match a particular filter
    pub(super) fn last_connections(
        &self,
        routing_table_inner: &RoutingTableInner,
        filter: Option<NodeRefFilter>,
    ) -> Vec<(ConnectionDescriptor, u64)> {
        let mut out: Vec<(ConnectionDescriptor, u64)> = self
            .last_connections
            .iter()
            .filter_map(|(k, v)| {
                let include = if let Some(filter) = &filter {
                    let remote_address = v.0.remote_address().address();
                    if let Some(routing_domain) = RoutingTable::routing_domain_for_address_inner(
                        routing_table_inner,
                        remote_address,
                    ) {
                        if filter.routing_domain_set.contains(routing_domain)
                            && filter.dial_info_filter.protocol_type_set.contains(k.0)
                            && filter.dial_info_filter.address_type_set.contains(k.1)
                        {
                            // matches filter
                            true
                        } else {
                            // does not match filter
                            false
                        }
                    } else {
                        // no valid routing domain
                        false
                    }
                } else {
                    // no filter
                    true
                };
                if include {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .collect();
        // Sort with newest timestamps first
        out.sort_by(|a, b| b.1.cmp(&a.1));
        out
    }

    pub fn set_min_max_version(&mut self, min_max_version: (u8, u8)) {
        self.min_max_version = Some(min_max_version);
    }

    pub fn min_max_version(&self) -> Option<(u8, u8)> {
        self.min_max_version
    }

    pub fn state(&self, cur_ts: u64) -> BucketEntryState {
        if self.check_reliable(cur_ts) {
            BucketEntryState::Reliable
        } else if self.check_dead(cur_ts) {
            BucketEntryState::Dead
        } else {
            BucketEntryState::Unreliable
        }
    }

    pub fn peer_stats(&self) -> &PeerStats {
        &self.peer_stats
    }

    pub fn update_node_status(&mut self, status: NodeStatus) {
        match status {
            NodeStatus::LocalNetwork(ln) => {
                self.local_network.node_status = Some(ln);
            }
            NodeStatus::PublicInternet(pi) => {
                self.public_internet.node_status = Some(pi);
            }
        }
    }
    pub fn node_status(&self, routing_domain: RoutingDomain) -> Option<NodeStatus> {
        match routing_domain {
            RoutingDomain::LocalNetwork => self
                .local_network
                .node_status
                .as_ref()
                .map(|ln| NodeStatus::LocalNetwork(ln.clone())),
            RoutingDomain::PublicInternet => self
                .public_internet
                .node_status
                .as_ref()
                .map(|pi| NodeStatus::PublicInternet(pi.clone())),
        }
    }

    pub fn set_seen_our_node_info(&mut self, routing_domain: RoutingDomain, seen: bool) {
        match routing_domain {
            RoutingDomain::LocalNetwork => {
                self.local_network.seen_our_node_info = seen;
            }
            RoutingDomain::PublicInternet => {
                self.public_internet.seen_our_node_info = seen;
            }
        }
    }

    pub fn has_seen_our_node_info(&self, routing_domain: RoutingDomain) -> bool {
        match routing_domain {
            RoutingDomain::LocalNetwork => self.local_network.seen_our_node_info,
            RoutingDomain::PublicInternet => self.public_internet.seen_our_node_info,
        }
    }

    pub fn set_updated_since_last_network_change(&mut self, updated: bool) {
        self.updated_since_last_network_change = updated;
    }

    pub fn has_updated_since_last_network_change(&self) -> bool {
        self.updated_since_last_network_change
    }

    ///// stats methods
    // called every ROLLING_TRANSFERS_INTERVAL_SECS seconds
    pub(super) fn roll_transfers(&mut self, last_ts: u64, cur_ts: u64) {
        self.transfer_stats_accounting.roll_transfers(
            last_ts,
            cur_ts,
            &mut self.peer_stats.transfer,
        );
    }

    // Called for every round trip packet we receive
    fn record_latency(&mut self, latency: u64) {
        self.peer_stats.latency = Some(self.latency_stats_accounting.record_latency(latency));
    }

    ///// state machine handling
    pub(super) fn check_reliable(&self, cur_ts: u64) -> bool {
        // If we have had any failures to send, this is not reliable
        if self.peer_stats.rpc_stats.failed_to_send > 0 {
            return false;
        }

        // if we have seen the node consistently for longer that UNRELIABLE_PING_SPAN_SECS
        match self.peer_stats.rpc_stats.first_consecutive_seen_ts {
            None => false,
            Some(ts) => {
                cur_ts.saturating_sub(ts) >= (UNRELIABLE_PING_SPAN_SECS as u64 * 1000000u64)
            }
        }
    }
    pub(super) fn check_dead(&self, cur_ts: u64) -> bool {
        // If we have failured to send NEVER_REACHED_PING_COUNT times in a row, the node is dead
        if self.peer_stats.rpc_stats.failed_to_send >= NEVER_REACHED_PING_COUNT {
            return true;
        }
        // if we have not heard from the node at all for the duration of the unreliable ping span
        // a node is not dead if we haven't heard from it yet,
        // but we give it NEVER_REACHED_PING_COUNT chances to ping before we say it's dead
        match self.peer_stats.rpc_stats.last_seen_ts {
            None => self.peer_stats.rpc_stats.recent_lost_answers < NEVER_REACHED_PING_COUNT,
            Some(ts) => {
                cur_ts.saturating_sub(ts) >= (UNRELIABLE_PING_SPAN_SECS as u64 * 1000000u64)
            }
        }
    }

    /// Return the last time we either saw a node, or asked it a question
    fn latest_contact_time(&self) -> Option<u64> {
        self.peer_stats
            .rpc_stats
            .last_seen_ts
            .max(self.peer_stats.rpc_stats.last_question)
    }

    fn needs_constant_ping(&self, cur_ts: u64, interval: u64) -> bool {
        // If we have not either seen the node in the last 'interval' then we should ping it
        let latest_contact_time = self.latest_contact_time();

        match latest_contact_time {
            None => true,
            Some(latest_contact_time) => {
                // If we haven't done anything with this node in 'interval' seconds
                cur_ts.saturating_sub(latest_contact_time) >= (interval * 1000000u64)
            }
        }
    }

    // Check if this node needs a ping right now to validate it is still reachable
    pub(super) fn needs_ping(&self, cur_ts: u64, needs_keepalive: bool) -> bool {
        // See which ping pattern we are to use
        let state = self.state(cur_ts);

        // If this entry needs a keepalive (like a relay node),
        // then we should ping it regularly to keep our association alive
        if needs_keepalive {
            return self.needs_constant_ping(cur_ts, KEEPALIVE_PING_INTERVAL_SECS as u64);
        }

        // If we don't have node status for this node, then we should ping it to get some node status
        for routing_domain in RoutingDomainSet::all() {
            if self.has_node_info(routing_domain.into()) {
                if self.node_status(routing_domain).is_none() {
                    return true;
                }
            }
        }

        match state {
            BucketEntryState::Reliable => {
                // If we are in a reliable state, we need a ping on an exponential scale
                let latest_contact_time = self.latest_contact_time();

                match latest_contact_time {
                    None => {
                        error!("Peer is reliable, but not seen!");
                        true
                    }
                    Some(latest_contact_time) => {
                        let first_consecutive_seen_ts =
                            self.peer_stats.rpc_stats.first_consecutive_seen_ts.unwrap();
                        let start_of_reliable_time = first_consecutive_seen_ts
                            + ((UNRELIABLE_PING_SPAN_SECS - UNRELIABLE_PING_INTERVAL_SECS) as u64
                                * 1_000_000u64);
                        let reliable_cur = cur_ts.saturating_sub(start_of_reliable_time);
                        let reliable_last =
                            latest_contact_time.saturating_sub(start_of_reliable_time);

                        retry_falloff_log(
                            reliable_last,
                            reliable_cur,
                            RELIABLE_PING_INTERVAL_START_SECS as u64 * 1_000_000u64,
                            RELIABLE_PING_INTERVAL_MAX_SECS as u64 * 1_000_000u64,
                            RELIABLE_PING_INTERVAL_MULTIPLIER,
                        )
                    }
                }
            }
            BucketEntryState::Unreliable => {
                // If we are in an unreliable state, we need a ping every UNRELIABLE_PING_INTERVAL_SECS seconds
                self.needs_constant_ping(cur_ts, UNRELIABLE_PING_INTERVAL_SECS as u64)
            }
            BucketEntryState::Dead => false,
        }
    }

    pub(super) fn touch_last_seen(&mut self, ts: u64) {
        // Mark the node as seen
        if self
            .peer_stats
            .rpc_stats
            .first_consecutive_seen_ts
            .is_none()
        {
            self.peer_stats.rpc_stats.first_consecutive_seen_ts = Some(ts);
        }

        self.peer_stats.rpc_stats.last_seen_ts = Some(ts);
    }

    pub(super) fn _state_debug_info(&self, cur_ts: u64) -> String {
        let first_consecutive_seen_ts = if let Some(first_consecutive_seen_ts) =
            self.peer_stats.rpc_stats.first_consecutive_seen_ts
        {
            format!(
                "{}s ago",
                timestamp_to_secs(cur_ts.saturating_sub(first_consecutive_seen_ts))
            )
        } else {
            "never".to_owned()
        };
        let last_seen_ts_str = if let Some(last_seen_ts) = self.peer_stats.rpc_stats.last_seen_ts {
            format!(
                "{}s ago",
                timestamp_to_secs(cur_ts.saturating_sub(last_seen_ts))
            )
        } else {
            "never".to_owned()
        };

        format!(
            "state: {:?}, first_consecutive_seen_ts: {}, last_seen_ts: {}",
            self.state(cur_ts),
            first_consecutive_seen_ts,
            last_seen_ts_str
        )
    }

    ////////////////////////////////////////////////////////////////
    /// Called when rpc processor things happen

    pub(super) fn question_sent(&mut self, ts: u64, bytes: u64, expects_answer: bool) {
        self.transfer_stats_accounting.add_up(bytes);
        self.peer_stats.rpc_stats.messages_sent += 1;
        self.peer_stats.rpc_stats.failed_to_send = 0;
        if expects_answer {
            self.peer_stats.rpc_stats.questions_in_flight += 1;
            self.peer_stats.rpc_stats.last_question = Some(ts);
        }
    }
    pub(super) fn question_rcvd(&mut self, ts: u64, bytes: u64) {
        self.transfer_stats_accounting.add_down(bytes);
        self.peer_stats.rpc_stats.messages_rcvd += 1;
        self.touch_last_seen(ts);
    }
    pub(super) fn answer_sent(&mut self, bytes: u64) {
        self.transfer_stats_accounting.add_up(bytes);
        self.peer_stats.rpc_stats.messages_sent += 1;
        self.peer_stats.rpc_stats.failed_to_send = 0;
    }
    pub(super) fn answer_rcvd(&mut self, send_ts: u64, recv_ts: u64, bytes: u64) {
        self.transfer_stats_accounting.add_down(bytes);
        self.peer_stats.rpc_stats.messages_rcvd += 1;
        self.peer_stats.rpc_stats.questions_in_flight -= 1;
        self.record_latency(recv_ts - send_ts);
        self.touch_last_seen(recv_ts);
        self.peer_stats.rpc_stats.recent_lost_answers = 0;
    }
    pub(super) fn question_lost(&mut self) {
        self.peer_stats.rpc_stats.first_consecutive_seen_ts = None;
        self.peer_stats.rpc_stats.questions_in_flight -= 1;
        self.peer_stats.rpc_stats.recent_lost_answers += 1;
    }
    pub(super) fn failed_to_send(&mut self, ts: u64, expects_answer: bool) {
        if expects_answer {
            self.peer_stats.rpc_stats.last_question = Some(ts);
        }
        self.peer_stats.rpc_stats.failed_to_send += 1;
        self.peer_stats.rpc_stats.first_consecutive_seen_ts = None;
    }
}

#[derive(Debug)]
pub struct BucketEntry {
    pub(super) ref_count: AtomicU32,
    inner: RwLock<BucketEntryInner>,
}

impl BucketEntry {
    pub(super) fn new() -> Self {
        let now = intf::get_timestamp();
        Self {
            ref_count: AtomicU32::new(0),
            inner: RwLock::new(BucketEntryInner {
                min_max_version: None,
                updated_since_last_network_change: false,
                last_connections: BTreeMap::new(),
                local_network: BucketEntryLocalNetwork {
                    seen_our_node_info: false,
                    signed_node_info: None,
                    node_status: None,
                },
                public_internet: BucketEntryPublicInternet {
                    seen_our_node_info: false,
                    signed_node_info: None,
                    node_status: None,
                },
                peer_stats: PeerStats {
                    time_added: now,
                    rpc_stats: RPCStats::default(),
                    latency: None,
                    transfer: TransferStatsDownUp::default(),
                },
                latency_stats_accounting: LatencyStatsAccounting::new(),
                transfer_stats_accounting: TransferStatsAccounting::new(),
                #[cfg(feature = "tracking")]
                next_track_id: 0,
                #[cfg(feature = "tracking")]
                node_ref_tracks: HashMap::new(),
            }),
        }
    }

    pub(super) fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&BucketEntryInner) -> R,
    {
        let inner = self.inner.read();
        f(&*inner)
    }

    pub(super) fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut BucketEntryInner) -> R,
    {
        let mut inner = self.inner.write();
        f(&mut *inner)
    }
}

impl Drop for BucketEntry {
    fn drop(&mut self) {
        if self.ref_count.load(Ordering::Relaxed) != 0 {
            #[cfg(feature = "tracking")]
            {
                println!("NodeRef Tracking");
                for (id, bt) in &mut self.node_ref_tracks {
                    bt.resolve();
                    println!("Id: {}\n----------------\n{:#?}", id, bt);
                }
            }

            panic!(
                "bucket entry dropped with non-zero refcount: {:#?}",
                &*self.inner.read()
            )
        }
    }
}
