use crate::*;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

mod connection_handle;
mod connection_limits;
mod connection_manager;
mod connection_table;
mod network_connection;
mod tasks;
mod types;

pub mod tests;

////////////////////////////////////////////////////////////////////////////////////////

pub use connection_manager::*;
pub use network_connection::*;
pub use types::*;

////////////////////////////////////////////////////////////////////////////////////////
use connection_handle::*;
use connection_limits::*;
use crypto::*;
use futures_util::stream::FuturesUnordered;
use hashlink::LruCache;
use intf::*;
#[cfg(not(target_arch = "wasm32"))]
use native::*;
use receipt_manager::*;
use routing_table::*;
use rpc_processor::*;
use storage_manager::*;
#[cfg(target_arch = "wasm32")]
use wasm::*;

////////////////////////////////////////////////////////////////////////////////////////

pub const MAX_MESSAGE_SIZE: usize = MAX_ENVELOPE_SIZE;
pub const IPADDR_TABLE_SIZE: usize = 1024;
pub const IPADDR_MAX_INACTIVE_DURATION_US: TimestampDuration = TimestampDuration::new(300_000_000u64); // 5 minutes
pub const PUBLIC_ADDRESS_CHANGE_DETECTION_COUNT: usize = 3;
pub const PUBLIC_ADDRESS_CHECK_CACHE_SIZE: usize = 8;
pub const PUBLIC_ADDRESS_CHECK_TASK_INTERVAL_SECS: u32 = 60;
pub const PUBLIC_ADDRESS_INCONSISTENCY_TIMEOUT_US: TimestampDuration = TimestampDuration::new(300_000_000u64); // 5 minutes
pub const PUBLIC_ADDRESS_INCONSISTENCY_PUNISHMENT_TIMEOUT_US: TimestampDuration = TimestampDuration::new(3600_000_000u64); // 60 minutes
pub const BOOT_MAGIC: &[u8; 4] = b"BOOT";

#[derive(Copy, Clone, Debug, Default)]
pub struct ProtocolConfig {
    pub outbound: ProtocolTypeSet,
    pub inbound: ProtocolTypeSet,
    pub family_global: AddressTypeSet,
    pub family_local: AddressTypeSet,
}

// Things we get when we start up and go away when we shut down
// Routing table is not in here because we want it to survive a network shutdown/startup restart
#[derive(Clone)]
struct NetworkComponents {
    net: Network,
    connection_manager: ConnectionManager,
    rpc_processor: RPCProcessor,
    receipt_manager: ReceiptManager,
}

// Statistics per address
#[derive(Clone, Default)]
pub struct PerAddressStats {
    last_seen_ts: Timestamp,
    transfer_stats_accounting: TransferStatsAccounting,
    transfer_stats: TransferStatsDownUp,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PerAddressStatsKey(IpAddr);

impl Default for PerAddressStatsKey {
    fn default() -> Self {
        Self(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    }
}

// Statistics about the low-level network
#[derive(Clone)]
pub struct NetworkManagerStats {
    self_stats: PerAddressStats,
    per_address_stats: LruCache<PerAddressStatsKey, PerAddressStats>,
}

impl Default for NetworkManagerStats {
    fn default() -> Self {
        Self {
            self_stats: PerAddressStats::default(),
            per_address_stats: LruCache::new(IPADDR_TABLE_SIZE),
        }
    }
}

#[derive(Debug)]
struct ClientWhitelistEntry {
    last_seen_ts: Timestamp,
}

#[derive(Copy, Clone, Debug)]
pub enum SendDataKind {
    Direct(ConnectionDescriptor),
    Indirect,
    Existing(ConnectionDescriptor),
}

/// Mechanism required to contact another node
#[derive(Clone, Debug)]
pub(crate) enum NodeContactMethod {
    /// Node is not reachable by any means
    Unreachable,
    /// Connection should have already existed
    Existing,
    /// Contact the node directly
    Direct(DialInfo),
    /// Request via signal the node connect back directly (relay, target)
    SignalReverse(NodeRef, NodeRef),
    /// Request via signal the node negotiate a hole punch (relay, target)
    SignalHolePunch(NodeRef, NodeRef),
    /// Must use an inbound relay to reach the node
    InboundRelay(NodeRef),
    /// Must use outbound relay to reach the node
    OutboundRelay(NodeRef),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd, Hash)]
struct PublicAddressCheckCacheKey(ProtocolType, AddressType);

// The mutable state of the network manager
struct NetworkManagerInner {
    stats: NetworkManagerStats,
    client_whitelist: LruCache<TypedKey, ClientWhitelistEntry>,
    public_address_check_cache:
        BTreeMap<PublicAddressCheckCacheKey, LruCache<IpAddr, SocketAddress>>,
    public_address_inconsistencies_table:
        BTreeMap<PublicAddressCheckCacheKey, HashMap<IpAddr, Timestamp>>,
}

struct NetworkManagerUnlockedInner {
    // Handles
    config: VeilidConfig,
    storage_manager: StorageManager,
    protected_store: ProtectedStore,
    table_store: TableStore,
    #[cfg(feature="unstable-blockstore")]
    block_store: BlockStore,
    crypto: Crypto,
    // Accessors
    routing_table: RwLock<Option<RoutingTable>>,
    components: RwLock<Option<NetworkComponents>>,
    update_callback: RwLock<Option<UpdateCallback>>,
    // Background processes
    rolling_transfers_task: TickTask<EyreReport>,
    public_address_check_task: TickTask<EyreReport>,
}

#[derive(Clone)]
pub struct NetworkManager {
    inner: Arc<Mutex<NetworkManagerInner>>,
    unlocked_inner: Arc<NetworkManagerUnlockedInner>,
}

impl NetworkManager {
    fn new_inner() -> NetworkManagerInner {
        NetworkManagerInner {
            stats: NetworkManagerStats::default(),
            client_whitelist: LruCache::new_unbounded(),
            public_address_check_cache: BTreeMap::new(),
            public_address_inconsistencies_table: BTreeMap::new(),
        }
    }
    fn new_unlocked_inner(
        config: VeilidConfig,
        storage_manager: StorageManager,
        protected_store: ProtectedStore,
        table_store: TableStore,
        #[cfg(feature="unstable-blockstore")]
        block_store: BlockStore,
        crypto: Crypto,
    ) -> NetworkManagerUnlockedInner {
        NetworkManagerUnlockedInner {
            config,
            storage_manager,
            protected_store,
            table_store,
            #[cfg(feature="unstable-blockstore")]
            block_store,
            crypto,
            routing_table: RwLock::new(None),
            components: RwLock::new(None),
            update_callback: RwLock::new(None),
            rolling_transfers_task: TickTask::new(ROLLING_TRANSFERS_INTERVAL_SECS),
            public_address_check_task: TickTask::new(PUBLIC_ADDRESS_CHECK_TASK_INTERVAL_SECS),
        }
    }

    pub fn new(
        config: VeilidConfig,
        storage_manager: StorageManager,
        protected_store: ProtectedStore,
        table_store: TableStore,
        #[cfg(feature="unstable-blockstore")]
        block_store: BlockStore,
        crypto: Crypto,
    ) -> Self {
        let this = Self {
            inner: Arc::new(Mutex::new(Self::new_inner())),
            unlocked_inner: Arc::new(Self::new_unlocked_inner(
                config,
                storage_manager,
                protected_store,
                table_store,
                #[cfg(feature="unstable-blockstore")]
                block_store,
                crypto,
            )),
        };

        this.setup_tasks();

        this
    }
    pub fn config(&self) -> VeilidConfig {
        self.unlocked_inner.config.clone()
    }
    pub fn with_config<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&VeilidConfigInner) -> R,
    {
        f(&*self.unlocked_inner.config.get())
    }
    pub fn storage_manager(&self) -> StorageManager {
        self.unlocked_inner.storage_manager.clone()
    }
    pub fn protected_store(&self) -> ProtectedStore {
        self.unlocked_inner.protected_store.clone()
    }
    pub fn table_store(&self) -> TableStore {
        self.unlocked_inner.table_store.clone()
    }
    #[cfg(feature="unstable-blockstore")]
    pub fn block_store(&self) -> BlockStore {
        self.unlocked_inner.block_store.clone()
    }
    pub fn crypto(&self) -> Crypto {
        self.unlocked_inner.crypto.clone()
    }
    pub fn routing_table(&self) -> RoutingTable {
        self.unlocked_inner
            .routing_table
            .read()
            .as_ref()
            .unwrap()
            .clone()
    }
    pub fn net(&self) -> Network {
        self.unlocked_inner
            .components
            .read()
            .as_ref()
            .unwrap()
            .net
            .clone()
    }
    pub fn rpc_processor(&self) -> RPCProcessor {
        self.unlocked_inner
            .components
            .read()
            .as_ref()
            .unwrap()
            .rpc_processor
            .clone()
    }
    pub fn receipt_manager(&self) -> ReceiptManager {
        self.unlocked_inner
            .components
            .read()
            .as_ref()
            .unwrap()
            .receipt_manager
            .clone()
    }
    pub fn connection_manager(&self) -> ConnectionManager {
        self.unlocked_inner
            .components
            .read()
            .as_ref()
            .unwrap()
            .connection_manager
            .clone()
    }
    pub fn update_callback(&self) -> UpdateCallback {
        self.unlocked_inner
            .update_callback
            .read()
            .as_ref()
            .unwrap()
            .clone()
    }

    #[instrument(level = "debug", skip_all, err)]
    pub async fn init(&self, update_callback: UpdateCallback) -> EyreResult<()> {
        let routing_table = RoutingTable::new(self.clone());
        routing_table.init().await?;
        *self.unlocked_inner.routing_table.write() = Some(routing_table.clone());
        *self.unlocked_inner.update_callback.write() = Some(update_callback);
        Ok(())
    }

    #[instrument(level = "debug", skip_all)]
    pub async fn terminate(&self) {
        let routing_table = self.unlocked_inner.routing_table.write().take();
        if let Some(routing_table) = routing_table {
            routing_table.terminate().await;
        }
        *self.unlocked_inner.update_callback.write() = None;
    }

    #[instrument(level = "debug", skip_all, err)]
    pub async fn internal_startup(&self) -> EyreResult<()> {
        trace!("NetworkManager::internal_startup begin");
        if self.unlocked_inner.components.read().is_some() {
            debug!("NetworkManager::internal_startup already started");
            return Ok(());
        }

        // Create network components
        let connection_manager = ConnectionManager::new(self.clone());
        let net = Network::new(
            self.clone(),
            self.routing_table(),
            connection_manager.clone(),
        );
        let rpc_processor = RPCProcessor::new(
            self.clone(),
            self.unlocked_inner
                .update_callback
                .read()
                .as_ref()
                .unwrap()
                .clone(),
        );
        let receipt_manager = ReceiptManager::new(self.clone());
        *self.unlocked_inner.components.write() = Some(NetworkComponents {
            net: net.clone(),
            connection_manager: connection_manager.clone(),
            rpc_processor: rpc_processor.clone(),
            receipt_manager: receipt_manager.clone(),
        });

        // Start network components
        connection_manager.startup().await;
        net.startup().await?;
        rpc_processor.startup().await?;
        receipt_manager.startup().await?;

        trace!("NetworkManager::internal_startup end");

        Ok(())
    }

    #[instrument(level = "debug", skip_all, err)]
    pub async fn startup(&self) -> EyreResult<()> {
        if let Err(e) = self.internal_startup().await {
            self.shutdown().await;
            return Err(e);
        }

        // Inform api clients that things have changed
        self.send_network_update();

        Ok(())
    }

    #[instrument(level = "debug", skip_all)]
    pub async fn shutdown(&self) {
        debug!("starting network manager shutdown");

        // Cancel all tasks
        self.cancel_tasks().await;

        // Shutdown network components if they started up
        debug!("shutting down network components");

        let components = self.unlocked_inner.components.read().clone();
        if let Some(components) = components {
            components.net.shutdown().await;
            components.rpc_processor.shutdown().await;
            components.receipt_manager.shutdown().await;
            components.connection_manager.shutdown().await;

            *self.unlocked_inner.components.write() = None;
        }

        // reset the state
        debug!("resetting network manager state");
        {
            *self.inner.lock() = NetworkManager::new_inner();
        }

        // send update
        debug!("sending network state update to api clients");
        self.send_network_update();

        debug!("finished network manager shutdown");
    }

    pub fn update_client_whitelist(&self, client: TypedKey) {
        let mut inner = self.inner.lock();
        match inner.client_whitelist.entry(client, |_k,_v| {
            // do nothing on LRU evict
        }) {
            hashlink::lru_cache::Entry::Occupied(mut entry) => {
                entry.get_mut().last_seen_ts = get_aligned_timestamp()
            }
            hashlink::lru_cache::Entry::Vacant(entry) => {
                entry.insert(ClientWhitelistEntry {
                    last_seen_ts: get_aligned_timestamp(),
                });
            }
        }
    }

    #[instrument(level = "trace", skip(self), ret)]
    pub fn check_client_whitelist(&self, client: TypedKey) -> bool {
        let mut inner = self.inner.lock();

        match inner.client_whitelist.entry(client, |_k,_v| {
            // do nothing on LRU evict
        }) {
            hashlink::lru_cache::Entry::Occupied(mut entry) => {
                entry.get_mut().last_seen_ts = get_aligned_timestamp();
                true
            }
            hashlink::lru_cache::Entry::Vacant(_) => false,
        }
    }

    pub fn purge_client_whitelist(&self) {
        let timeout_ms = self.with_config(|c| c.network.client_whitelist_timeout_ms);
        let mut inner = self.inner.lock();
        let cutoff_timestamp = get_aligned_timestamp() - TimestampDuration::new((timeout_ms as u64) * 1000u64);
        // Remove clients from the whitelist that haven't been since since our whitelist timeout
        while inner
            .client_whitelist
            .peek_lru()
            .map(|v| v.1.last_seen_ts < cutoff_timestamp)
            .unwrap_or_default()
        {
            let (k, v) = inner.client_whitelist.remove_lru().unwrap();
            trace!(key=?k, value=?v, "purge_client_whitelist: remove_lru")
        }
    }

    pub fn needs_restart(&self) -> bool {
        let net = self.net();
        net.needs_restart()
    }

    /// Get our node's capabilities in the PublicInternet routing domain
    fn generate_public_internet_node_status(&self) -> PublicInternetNodeStatus {
        let Some(own_peer_info) = self
            .routing_table()
            .get_own_peer_info(RoutingDomain::PublicInternet) else {
                return PublicInternetNodeStatus {
                    will_route: false,
                    will_tunnel: false,
                    will_signal: false,
                    will_relay: false,
                    will_validate_dial_info: false,
                };  
            };
        let own_node_info = own_peer_info.signed_node_info().node_info();

        let will_route = own_node_info.can_inbound_relay(); // xxx: eventually this may have more criteria added
        let will_tunnel = own_node_info.can_inbound_relay(); // xxx: we may want to restrict by battery life and network bandwidth at some point
        let will_signal = own_node_info.can_signal();
        let will_relay = own_node_info.can_inbound_relay();
        let will_validate_dial_info = own_node_info.can_validate_dial_info();

        PublicInternetNodeStatus {
            will_route,
            will_tunnel,
            will_signal,
            will_relay,
            will_validate_dial_info,
        }
    }
    /// Get our node's capabilities in the LocalNetwork routing domain
    fn generate_local_network_node_status(&self) -> LocalNetworkNodeStatus {
        let Some(own_peer_info) = self
            .routing_table()
            .get_own_peer_info(RoutingDomain::LocalNetwork) else {
                return LocalNetworkNodeStatus {
                    will_relay: false,
                    will_validate_dial_info: false,
                };  
            };

        let own_node_info = own_peer_info.signed_node_info().node_info();

        let will_relay = own_node_info.can_inbound_relay();
        let will_validate_dial_info = own_node_info.can_validate_dial_info();

        LocalNetworkNodeStatus {
            will_relay,
            will_validate_dial_info,
        }
    }

    pub fn generate_node_status(&self, routing_domain: RoutingDomain) -> NodeStatus {
        match routing_domain {
            RoutingDomain::PublicInternet => {
                NodeStatus::PublicInternet(self.generate_public_internet_node_status())
            }
            RoutingDomain::LocalNetwork => {
                NodeStatus::LocalNetwork(self.generate_local_network_node_status())
            }
        }
    }

    /// Generates a multi-shot/normal receipt
    #[instrument(level = "trace", skip(self, extra_data, callback), err)]
    pub fn generate_receipt<D: AsRef<[u8]>>(
        &self,
        expiration_us: u64,
        expected_returns: u32,
        extra_data: D,
        callback: impl ReceiptCallback,
    ) -> EyreResult<Vec<u8>> {
        let receipt_manager = self.receipt_manager();
        let routing_table = self.routing_table();

        // Generate receipt and serialized form to return
        let vcrypto = self.crypto().best();
        
        let nonce = vcrypto.random_nonce();
        let node_id = routing_table.node_id(vcrypto.kind());
        let node_id_secret = routing_table.node_id_secret_key(vcrypto.kind());
        
        let receipt = Receipt::try_new(best_envelope_version(), node_id.kind, nonce, node_id.value, extra_data)?;
        let out = receipt
            .to_signed_data(self.crypto(), &node_id_secret)
            .wrap_err("failed to generate signed receipt")?;

        // Record the receipt for later
        let exp_ts = get_aligned_timestamp() + expiration_us;
        receipt_manager.record_receipt(receipt, exp_ts, expected_returns, callback);

        Ok(out)
    }

    /// Generates a single-shot/normal receipt
    #[instrument(level = "trace", skip(self, extra_data), err)]
    pub fn generate_single_shot_receipt<D: AsRef<[u8]>>(
        &self,
        expiration_us: u64,
        extra_data: D,
    ) -> EyreResult<(Vec<u8>, EventualValueFuture<ReceiptEvent>)> {
        let receipt_manager = self.receipt_manager();
        let routing_table = self.routing_table();

        // Generate receipt and serialized form to return
        let vcrypto = self.crypto().best();
        
        let nonce = vcrypto.random_nonce();
        let node_id = routing_table.node_id(vcrypto.kind());
        let node_id_secret = routing_table.node_id_secret_key(vcrypto.kind());
        
        let receipt = Receipt::try_new(best_envelope_version(), node_id.kind, nonce, node_id.value, extra_data)?;
        let out = receipt
            .to_signed_data(self.crypto(), &node_id_secret)
            .wrap_err("failed to generate signed receipt")?;

        // Record the receipt for later
        let exp_ts = get_aligned_timestamp() + expiration_us;
        let eventual = SingleShotEventual::new(Some(ReceiptEvent::Cancelled));
        let instance = eventual.instance();
        receipt_manager.record_single_shot_receipt(receipt, exp_ts, eventual);

        Ok((out, instance))
    }

    /// Process a received out-of-band receipt
    #[instrument(level = "trace", skip(self, receipt_data), ret)]
    pub async fn handle_out_of_band_receipt<R: AsRef<[u8]>>(
        &self,
        receipt_data: R,
    ) -> NetworkResult<()> {
        let receipt_manager = self.receipt_manager();

        let receipt = match Receipt::from_signed_data(self.crypto(), receipt_data.as_ref()) {
            Err(e) => {
                return NetworkResult::invalid_message(e.to_string());
            }
            Ok(v) => v,
        };

        receipt_manager
            .handle_receipt(receipt, ReceiptReturned::OutOfBand)
            .await
    }

    /// Process a received in-band receipt
    #[instrument(level = "trace", skip(self, receipt_data), ret)]
    pub async fn handle_in_band_receipt<R: AsRef<[u8]>>(
        &self,
        receipt_data: R,
        inbound_noderef: NodeRef,
    ) -> NetworkResult<()> {
        let receipt_manager = self.receipt_manager();

        let receipt = match Receipt::from_signed_data(self.crypto(), receipt_data.as_ref()) {
            Err(e) => {
                return NetworkResult::invalid_message(e.to_string());
            }
            Ok(v) => v,
        };

        receipt_manager
            .handle_receipt(receipt, ReceiptReturned::InBand { inbound_noderef })
            .await
    }

    /// Process a received safety receipt
    #[instrument(level = "trace", skip(self, receipt_data), ret)]
    pub async fn handle_safety_receipt<R: AsRef<[u8]>>(
        &self,
        receipt_data: R,
    ) -> NetworkResult<()> {
        let receipt_manager = self.receipt_manager();

        let receipt = match Receipt::from_signed_data(self.crypto(), receipt_data.as_ref()) {
            Err(e) => {
                return NetworkResult::invalid_message(e.to_string());
            }
            Ok(v) => v,
        };

        receipt_manager
            .handle_receipt(receipt, ReceiptReturned::Safety)
            .await
    }

    /// Process a received private receipt
    #[instrument(level = "trace", skip(self, receipt_data), ret)]
    pub async fn handle_private_receipt<R: AsRef<[u8]>>(
        &self,
        receipt_data: R,
        private_route: PublicKey,
    ) -> NetworkResult<()> {
        let receipt_manager = self.receipt_manager();

        let receipt = match Receipt::from_signed_data(self.crypto(), receipt_data.as_ref()) {
            Err(e) => {
                return NetworkResult::invalid_message(e.to_string());
            }
            Ok(v) => v,
        };

        receipt_manager
            .handle_receipt(receipt, ReceiptReturned::Private { private_route })
            .await
    }

    // Process a received signal
    #[instrument(level = "trace", skip(self), err)]
    pub async fn handle_signal(&self, signal_info: SignalInfo) -> EyreResult<NetworkResult<()>> {
        match signal_info {
            SignalInfo::ReverseConnect { receipt, peer_info } => {
                let routing_table = self.routing_table();
                let rpc = self.rpc_processor();

                // Add the peer info to our routing table
                let peer_nr = match routing_table.register_node_with_peer_info(
                    RoutingDomain::PublicInternet,
                    peer_info,
                    false,
                ) {
                    Ok(nr) => nr,
                    Err(e) => {
                        return Ok(NetworkResult::invalid_message(
                            format!("unable to register reverse connect peerinfo: {}", e)
                        ));
                    }
                };

                // Make a reverse connection to the peer and send the receipt to it
                rpc.rpc_call_return_receipt(Destination::direct(peer_nr), receipt)
                    .await
                    .wrap_err("rpc failure")
            }
            SignalInfo::HolePunch { receipt, peer_info } => {
                let routing_table = self.routing_table();
                let rpc = self.rpc_processor();

                // Add the peer info to our routing table
                let mut peer_nr = match routing_table.register_node_with_peer_info(
                    RoutingDomain::PublicInternet,
                    peer_info,
                    false,
                ) {
                    Ok(nr) => nr,
                    Err(e) => {
                        return Ok(NetworkResult::invalid_message(
                            format!("unable to register hole punch connect peerinfo: {}", e)
                        ));
                    }
                };

                // Get the udp direct dialinfo for the hole punch
                let outbound_nrf = routing_table
                    .get_outbound_node_ref_filter(RoutingDomain::PublicInternet)
                    .with_protocol_type(ProtocolType::UDP);
                peer_nr.set_filter(Some(outbound_nrf));
                let hole_punch_dial_info_detail = peer_nr
                    .first_filtered_dial_info_detail()
                    .ok_or_else(|| eyre!("No hole punch capable dialinfo found for node"))?;

                // Now that we picked a specific dialinfo, further restrict the noderef to the specific address type
                let filter = peer_nr.take_filter().unwrap();
                let filter =
                    filter.with_address_type(hole_punch_dial_info_detail.dial_info.address_type());
                peer_nr.set_filter(Some(filter));

                // Do our half of the hole punch by sending an empty packet
                // Both sides will do this and then the receipt will get sent over the punched hole
                let connection_descriptor = network_result_try!(
                    self.net()
                        .send_data_to_dial_info(
                            hole_punch_dial_info_detail.dial_info.clone(),
                            Vec::new(),
                        )
                        .await?
                );

                // XXX: do we need a delay here? or another hole punch packet?

                // Set the hole punch as our 'last connection' to ensure we return the receipt over the direct hole punch
                peer_nr.set_last_connection(connection_descriptor, get_aligned_timestamp());

                // Return the receipt using the same dial info send the receipt to it
                rpc.rpc_call_return_receipt(Destination::direct(peer_nr), receipt)
                    .await
                    .wrap_err("rpc failure")
            }
        }
    }

    /// Builds an envelope for sending over the network
    #[instrument(level = "trace", skip(self, body), err)]
    fn build_envelope<B: AsRef<[u8]>>(
        &self,
        dest_node_id: TypedKey,
        version: u8,
        body: B,
    ) -> EyreResult<Vec<u8>> {
        // DH to get encryption key
        let routing_table = self.routing_table();
        let Some(vcrypto) = self.crypto().get(dest_node_id.kind) else {
            bail!("should not have a destination with incompatible crypto here");
        };

        let node_id = routing_table.node_id(vcrypto.kind());
        let node_id_secret = routing_table.node_id_secret_key(vcrypto.kind());

        // Get timestamp, nonce
        let ts = get_aligned_timestamp();
        let nonce = vcrypto.random_nonce();

        // Encode envelope
        let envelope = Envelope::new(version, node_id.kind, ts, nonce, node_id.value, dest_node_id.value);
        envelope
            .to_encrypted_data(self.crypto(), body.as_ref(), &node_id_secret)
            .wrap_err("envelope failed to encode")
    }

    /// Called by the RPC handler when we want to issue an RPC request or response
    /// node_ref is the direct destination to which the envelope will be sent
    /// If 'destination_node_ref' is specified, it can be different than the node_ref being sent to
    /// which will cause the envelope to be relayed
    #[instrument(level = "trace", skip(self, body), ret, err)]
    pub async fn send_envelope<B: AsRef<[u8]>>(
        &self,
        node_ref: NodeRef,
        destination_node_ref: Option<NodeRef>,
        body: B,
    ) -> EyreResult<NetworkResult<SendDataKind>> {

        let destination_node_ref = destination_node_ref.as_ref().unwrap_or(&node_ref).clone();
        
        if !node_ref.same_entry(&destination_node_ref) {
            log_net!(
                "sending envelope to {:?} via {:?}",
                destination_node_ref,
                node_ref
            );
        } else {
            log_net!("sending envelope to {:?}", node_ref);
        }

        let best_node_id = destination_node_ref.best_node_id();

        // Get node's envelope versions and see if we can send to it
        // and if so, get the max version we can use
        let Some(envelope_version) = destination_node_ref.best_envelope_version() else {
            bail!(
                "can't talk to this node {} because we dont support its envelope versions",
                node_ref
            );
        };

        // Build the envelope to send
        let out = self.build_envelope(best_node_id, envelope_version, body)?;

        // Send the envelope via whatever means necessary
        self.send_data(node_ref, out).await
    }

    /// Called by the RPC handler when we want to issue an direct receipt
    #[instrument(level = "trace", skip(self, rcpt_data), err)]
    pub async fn send_out_of_band_receipt(
        &self,
        dial_info: DialInfo,
        rcpt_data: Vec<u8>,
    ) -> EyreResult<()> {
        // Do we need to validate the outgoing receipt? Probably not
        // because it is supposed to be opaque and the
        // recipient/originator does the validation
        // Also, in the case of an old 'version', returning the receipt
        // should not be subject to our ability to decode it

        // Send receipt directly
        log_net!(debug "send_out_of_band_receipt: dial_info={}", dial_info);
        network_result_value_or_log!(self
            .net()
            .send_data_unbound_to_dial_info(dial_info, rcpt_data)
            .await? => {
                return Ok(());
            }
        );
        Ok(())
    }

    /// Send a reverse connection signal and wait for the return receipt over it
    /// Then send the data across the new connection
    /// Only usable for PublicInternet routing domain
    #[instrument(level = "trace", skip(self, data), err)]
    pub async fn do_reverse_connect(
        &self,
        relay_nr: NodeRef,
        target_nr: NodeRef,
        data: Vec<u8>,
    ) -> EyreResult<NetworkResult<ConnectionDescriptor>> {
        // Build a return receipt for the signal
        let receipt_timeout = ms_to_us(
            self.unlocked_inner
                .config
                .get()
                .network
                .reverse_connection_receipt_time_ms,
        );
        let (receipt, eventual_value) = self.generate_single_shot_receipt(receipt_timeout, [])?;

        // Get target routing domain
        let Some(routing_domain) = target_nr.best_routing_domain() else {
            return Ok(NetworkResult::no_connection_other("No routing domain for target"));
        };

        // Get our peer info
        let Some(peer_info) = self
            .routing_table()
            .get_own_peer_info(routing_domain) else {
            return Ok(NetworkResult::no_connection_other("Own peer info not available"));
        };    

        // Issue the signal
        let rpc = self.rpc_processor();
        network_result_try!(rpc
            .rpc_call_signal(
                Destination::relay(relay_nr, target_nr.clone()),
                SignalInfo::ReverseConnect { receipt, peer_info },
            )
            .await
            .wrap_err("failed to send signal")?);

        // Wait for the return receipt
        let inbound_nr = match eventual_value.await.take_value().unwrap() {
            ReceiptEvent::ReturnedPrivate { private_route: _ }
            | ReceiptEvent::ReturnedOutOfBand
            | ReceiptEvent::ReturnedSafety => {
                return Ok(NetworkResult::invalid_message(
                    "reverse connect receipt should be returned in-band",
                ));
            }
            ReceiptEvent::ReturnedInBand { inbound_noderef } => inbound_noderef,
            ReceiptEvent::Expired => {
                return Ok(NetworkResult::timeout());
            }
            ReceiptEvent::Cancelled => {
                bail!("reverse connect receipt cancelled from {:?}", target_nr);
            }
        };

        // We expect the inbound noderef to be the same as the target noderef
        // if they aren't the same, we should error on this and figure out what then hell is up
        if !target_nr.same_entry(&inbound_nr) {
            bail!("unexpected noderef mismatch on reverse connect");
        }

        // And now use the existing connection to send over
        if let Some(descriptor) = inbound_nr.last_connection() {
            match self
                .net()
                .send_data_to_existing_connection(descriptor, data)
                .await?
            {
                None => Ok(NetworkResult::value(descriptor)),
                Some(_) => Ok(NetworkResult::no_connection_other(
                    "unable to send over reverse connection",
                )),
            }
        } else {
            bail!("no reverse connection available")
        }
    }

    /// Send a hole punch signal and do a negotiating ping and wait for the return receipt
    /// Then send the data across the new connection
    /// Only usable for PublicInternet routing domain
    #[instrument(level = "trace", skip(self, data), err)]
    pub async fn do_hole_punch(
        &self,
        relay_nr: NodeRef,
        target_nr: NodeRef,
        data: Vec<u8>,
    ) -> EyreResult<NetworkResult<ConnectionDescriptor>> {
        // Ensure we are filtered down to UDP (the only hole punch protocol supported today)
        assert!(target_nr
            .filter_ref()
            .map(|nrf| nrf.dial_info_filter.protocol_type_set
                == ProtocolTypeSet::only(ProtocolType::UDP))
            .unwrap_or_default());

        // Build a return receipt for the signal
        let receipt_timeout = ms_to_us(
            self.unlocked_inner
                .config
                .get()
                .network
                .hole_punch_receipt_time_ms,
        );
        let (receipt, eventual_value) = self.generate_single_shot_receipt(receipt_timeout, [])?;

        // Get target routing domain
        let Some(routing_domain) = target_nr.best_routing_domain() else {
            return Ok(NetworkResult::no_connection_other("No routing domain for target"));
        };

        // Get our peer info
        let Some(peer_info) = self
            .routing_table()
            .get_own_peer_info(routing_domain) else {
                return Ok(NetworkResult::no_connection_other("Own peer info not available"));
            };

        // Get the udp direct dialinfo for the hole punch
        let hole_punch_did = target_nr
            .first_filtered_dial_info_detail()
            .ok_or_else(|| eyre!("No hole punch capable dialinfo found for node"))?;

        // Do our half of the hole punch by sending an empty packet
        // Both sides will do this and then the receipt will get sent over the punched hole
        // Don't bother storing the returned connection descriptor as the 'last connection' because the other side of the hole
        // punch should come through and create a real 'last connection' for us if this succeeds
        network_result_try!(
            self.net()
                .send_data_to_dial_info(hole_punch_did.dial_info, Vec::new())
                .await?
        );

        // Issue the signal
        let rpc = self.rpc_processor();
        network_result_try!(rpc
            .rpc_call_signal(
                Destination::relay(relay_nr, target_nr.clone()),
                SignalInfo::HolePunch { receipt, peer_info },
            )
            .await
            .wrap_err("failed to send signal")?);

        // Wait for the return receipt
        let inbound_nr = match eventual_value.await.take_value().unwrap() {
            ReceiptEvent::ReturnedPrivate { private_route: _ }
            | ReceiptEvent::ReturnedOutOfBand
            | ReceiptEvent::ReturnedSafety => {
                return Ok(NetworkResult::invalid_message(
                    "hole punch receipt should be returned in-band",
                ));
            }
            ReceiptEvent::ReturnedInBand { inbound_noderef } => inbound_noderef,
            ReceiptEvent::Expired => {
                return Ok(NetworkResult::timeout());
            }
            ReceiptEvent::Cancelled => {
                bail!("hole punch receipt cancelled from {}", target_nr);
            }
        };

        // We expect the inbound noderef to be the same as the target noderef
        // if they aren't the same, we should error on this and figure out what then hell is up
        if !target_nr.same_entry(&inbound_nr) {
            bail!(
                "unexpected noderef mismatch on hole punch {}, expected {}",
                inbound_nr,
                target_nr
            );
        }

        // And now use the existing connection to send over
        if let Some(descriptor) = inbound_nr.last_connection() {
            match self
                .net()
                .send_data_to_existing_connection(descriptor, data)
                .await?
            {
                None => Ok(NetworkResult::value(descriptor)),
                Some(_) => Ok(NetworkResult::no_connection_other(
                    "unable to send over hole punch",
                )),
            }
        } else {
            bail!("no hole punch available")
        }
    }

    /// Figure out how to reach a node from our own node over the best routing domain and reference the nodes we want to access
    /// Uses NodeRefs to ensure nodes are referenced, this is not a part of 'RoutingTable' because RoutingTable is not
    /// allowed to use NodeRefs due to recursive locking
    #[instrument(level = "trace", skip(self), ret)]
    pub(crate) fn get_node_contact_method(
        &self,
        target_node_ref: NodeRef,
    ) -> EyreResult<NodeContactMethod> {
        let routing_table = self.routing_table();

        // Figure out the best routing domain to get the contact method over
        let routing_domain = match target_node_ref.best_routing_domain() {
            Some(rd) => rd,
            None => {
                log_net!("no routing domain for node {:?}", target_node_ref);
                return Ok(NodeContactMethod::Unreachable);
            }
        };

        // Node A is our own node
        // Use whatever node info we've calculated so far
        let peer_a = routing_table.get_best_effort_own_peer_info(routing_domain);

        // Node B is the target node
        let peer_b = match target_node_ref.make_peer_info(routing_domain) {
            Some(ni) => ni,
            None => {
                log_net!("no node info for node {:?}", target_node_ref);
                return Ok(NodeContactMethod::Unreachable);
            }
        };

        // Dial info filter comes from the target node ref
        let dial_info_filter = target_node_ref.dial_info_filter();
        let mut sequencing = target_node_ref.sequencing();
        
        // If the node has had lost questions or failures to send, prefer sequencing
        // to improve reliability. The node may be experiencing UDP fragmentation drops
        // or other firewalling issues and may perform better with TCP.
        let unreliable = target_node_ref.peer_stats().rpc_stats.failed_to_send > 0 || target_node_ref.peer_stats().rpc_stats.recent_lost_answers > 0;
        if unreliable && sequencing < Sequencing::PreferOrdered {
            sequencing = Sequencing::PreferOrdered;
        }

        // Get the best contact method with these parameters from the routing domain
        let cm = routing_table.get_contact_method(
            routing_domain,
            &peer_a,
            &peer_b,
            dial_info_filter,
            sequencing,
        );

        // Translate the raw contact method to a referenced contact method
        Ok(match cm {
            ContactMethod::Unreachable => NodeContactMethod::Unreachable,
            ContactMethod::Existing => NodeContactMethod::Existing,
            ContactMethod::Direct(di) => NodeContactMethod::Direct(di),
            ContactMethod::SignalReverse(relay_key, target_key) => {
                let relay_nr = routing_table
                    .lookup_and_filter_noderef(relay_key, routing_domain.into(), dial_info_filter)?
                    .ok_or_else(|| eyre!("couldn't look up relay"))?;
                if !target_node_ref.node_ids().contains(&target_key) {
                    bail!("signalreverse target noderef didn't match target key: {:?} != {} for relay {}", target_node_ref, target_key, relay_key );
                }
                NodeContactMethod::SignalReverse(relay_nr, target_node_ref)
            }
            ContactMethod::SignalHolePunch(relay_key, target_key) => {
                let relay_nr = routing_table
                    .lookup_and_filter_noderef(relay_key, routing_domain.into(), dial_info_filter)?
                    .ok_or_else(|| eyre!("couldn't look up relay"))?;
                if !target_node_ref.node_ids().contains(&target_key) {
                    bail!("signalholepunch target noderef didn't match target key: {:?} != {} for relay {}", target_node_ref, target_key, relay_key );
                }
                // if any other protocol were possible here we could update this and do_hole_punch
                // but tcp hole punch is very very unreliable it seems
                let udp_target_node_ref = target_node_ref.filtered_clone(NodeRefFilter::new().with_protocol_type(ProtocolType::UDP));

                NodeContactMethod::SignalHolePunch(relay_nr, udp_target_node_ref)
            }
            ContactMethod::InboundRelay(relay_key) => {
                let relay_nr = routing_table
                    .lookup_and_filter_noderef(relay_key, routing_domain.into(), dial_info_filter)?
                    .ok_or_else(|| eyre!("couldn't look up relay"))?;
                NodeContactMethod::InboundRelay(relay_nr)
            }
            ContactMethod::OutboundRelay(relay_key) => {
                let relay_nr = routing_table
                    .lookup_and_filter_noderef(relay_key, routing_domain.into(), dial_info_filter)?
                    .ok_or_else(|| eyre!("couldn't look up relay"))?;
                NodeContactMethod::OutboundRelay(relay_nr)
            }
        })
    }

    /// Send raw data to a node
    ///
    /// We may not have dial info for a node, but have an existing connection for it
    /// because an inbound connection happened first, and no FindNodeQ has happened to that
    /// node yet to discover its dial info. The existing connection should be tried first
    /// in this case.
    ///
    /// Sending to a node requires determining a NetworkClass compatible mechanism
    pub fn send_data(
        &self,
        node_ref: NodeRef,
        data: Vec<u8>,
    ) -> SendPinBoxFuture<EyreResult<NetworkResult<SendDataKind>>> {
        let this = self.clone();
        Box::pin(
            async move {
                // info!("{}", format!("send_data to: {:?}", node_ref).red());

                // First try to send data to the last socket we've seen this peer on
                let data = if let Some(connection_descriptor) = node_ref.last_connection() {
                    // info!(
                    //     "{}",
                    //     format!("last_connection to: {:?}", connection_descriptor).red()
                    // );

                    match this
                        .net()
                        .send_data_to_existing_connection(connection_descriptor, data)
                        .await?
                    {
                        None => {
                            // info!(
                            //     "{}",
                            //     format!("sent to existing connection: {:?}", connection_descriptor)
                            //         .red()
                            // );

                            // Update timestamp for this last connection since we just sent to it
                            node_ref.set_last_connection(connection_descriptor, get_aligned_timestamp());

                            return Ok(NetworkResult::value(SendDataKind::Existing(
                                connection_descriptor,
                            )));
                        }
                        Some(d) => d,
                    }
                } else {
                    data
                };

                // info!("{}", "no existing connection".red());

                // If we don't have last_connection, try to reach out to the peer via its dial info
                let contact_method = this.get_node_contact_method(node_ref.clone())?;
                log_net!(
                    "send_data via {:?} to dialinfo {:?}",
                    contact_method,
                    node_ref
                );
                match contact_method {
                    NodeContactMethod::OutboundRelay(relay_nr)
                    | NodeContactMethod::InboundRelay(relay_nr) => {
                        network_result_try!(this.send_data(relay_nr, data).await?);
                        Ok(NetworkResult::value(SendDataKind::Indirect))
                    }
                    NodeContactMethod::Direct(dial_info) => {
                        let connection_descriptor = network_result_try!(
                            this.net().send_data_to_dial_info(dial_info, data).await?
                        );
                        // If we connected to this node directly, save off the last connection so we can use it again
                        node_ref.set_last_connection(connection_descriptor, get_aligned_timestamp());

                        Ok(NetworkResult::value(SendDataKind::Direct(
                            connection_descriptor,
                        )))
                    }
                    NodeContactMethod::SignalReverse(relay_nr, target_node_ref) => {
                        let connection_descriptor = network_result_try!(
                            this.do_reverse_connect(relay_nr, target_node_ref, data)
                                .await?
                        );
                        Ok(NetworkResult::value(SendDataKind::Direct(
                            connection_descriptor,
                        )))
                    }
                    NodeContactMethod::SignalHolePunch(relay_nr, target_node_ref) => {
                        let connection_descriptor = network_result_try!(
                            this.do_hole_punch(relay_nr, target_node_ref, data).await?
                        );
                        Ok(NetworkResult::value(SendDataKind::Direct(
                            connection_descriptor,
                        )))
                    }
                    NodeContactMethod::Existing => Ok(NetworkResult::no_connection_other(
                        "should have found an existing connection",
                    )),
                    NodeContactMethod::Unreachable => Ok(NetworkResult::no_connection_other(
                        "Can't send to this node",
                    )),
                }
            }
            .instrument(trace_span!("send_data")),
        )
    }

    // Direct bootstrap request handler (separate fallback mechanism from cheaper TXT bootstrap mechanism)
    #[instrument(level = "trace", skip(self), ret, err)]
    async fn handle_boot_request(
        &self,
        descriptor: ConnectionDescriptor,
    ) -> EyreResult<NetworkResult<()>> {
        let routing_table = self.routing_table();

        // Get a bunch of nodes with the various
        let bootstrap_nodes = routing_table.find_bootstrap_nodes_filtered(2);

        // Serialize out peer info
        let bootstrap_peerinfo: Vec<PeerInfo> = bootstrap_nodes
            .iter()
            .filter_map(|nr| nr.make_peer_info(RoutingDomain::PublicInternet))
            .collect();
        let json_bytes = serialize_json(bootstrap_peerinfo).as_bytes().to_vec();

        // Reply with a chunk of signed routing table
        match self
            .net()
            .send_data_to_existing_connection(descriptor, json_bytes)
            .await?
        {
            None => {
                // Bootstrap reply was sent
                Ok(NetworkResult::value(()))
            }
            Some(_) => Ok(NetworkResult::no_connection_other(
                "bootstrap reply could not be sent",
            )),
        }
    }

    // Direct bootstrap request
    #[instrument(level = "trace", err, skip(self))]
    pub async fn boot_request(&self, dial_info: DialInfo) -> EyreResult<Vec<PeerInfo>> {
        let timeout_ms = self.with_config(|c| c.network.rpc.timeout_ms);
        // Send boot magic to requested peer address
        let data = BOOT_MAGIC.to_vec();
        let out_data: Vec<u8> = network_result_value_or_log!(self
            .net()
            .send_recv_data_unbound_to_dial_info(dial_info, data, timeout_ms)
            .await? =>
        {
            return Ok(Vec::new());
        });

        let bootstrap_peerinfo: Vec<PeerInfo> =
            deserialize_json(std::str::from_utf8(&out_data).wrap_err("bad utf8 in boot peerinfo")?)
                .wrap_err("failed to deserialize boot peerinfo")?;

        Ok(bootstrap_peerinfo)
    }

    // Called when a packet potentially containing an RPC envelope is received by a low-level
    // network protocol handler. Processes the envelope, authenticates and decrypts the RPC message
    // and passes it to the RPC handler
    #[instrument(level = "trace", ret, err, skip(self, data), fields(data.len = data.len()))]
    async fn on_recv_envelope(
        &self,
        data: &[u8],
        connection_descriptor: ConnectionDescriptor,
    ) -> EyreResult<bool> {
        let root = span!(
            parent: None,
            Level::TRACE,
            "on_recv_envelope",
            "data.len" = data.len(),
            "descriptor" = ?connection_descriptor
        );
        let _root_enter = root.enter();

        log_net!(
            "envelope of {} bytes received from {:?}",
            data.len(),
            connection_descriptor
        );

        // Network accounting
        self.stats_packet_rcvd(
            connection_descriptor.remote_address().to_ip_addr(),
            ByteCount::new(data.len() as u64),
        );

        // If this is a zero length packet, just drop it, because these are used for hole punching
        // and possibly other low-level network connectivity tasks and will never require
        // more processing or forwarding
        if data.len() == 0 {
            return Ok(true);
        }

        // Ensure we can read the magic number
        if data.len() < 4 {
            log_net!(debug "short packet");
            return Ok(false);
        }

        // Get the routing domain for this data
        let routing_domain = match self
            .routing_table()
            .routing_domain_for_address(connection_descriptor.remote_address().address())
        {
            Some(rd) => rd,
            None => {
                log_net!(debug "no routing domain for envelope received from {:?}", connection_descriptor);
                return Ok(false);
            }
        };

        // Is this a direct bootstrap request instead of an envelope?
        if data[0..4] == *BOOT_MAGIC {
            network_result_value_or_log!(self.handle_boot_request(connection_descriptor).await? => {});
            return Ok(true);
        }

        // Is this an out-of-band receipt instead of an envelope?
        if data[0..3] == *RECEIPT_MAGIC {
            network_result_value_or_log!(self.handle_out_of_band_receipt(data).await => {});
            return Ok(true);
        }

        // Decode envelope header (may fail signature validation)
        let envelope = match Envelope::from_signed_data(self.crypto(), data) {
            Ok(v) => v,
            Err(e) => {
                log_net!(debug "envelope failed to decode: {}", e);
                return Ok(false);
            }
        };

        // Get timestamp range
        let (tsbehind, tsahead) = self.with_config(|c| {
            (
                c.network.rpc.max_timestamp_behind_ms.map(ms_to_us).map(TimestampDuration::new),
                c.network.rpc.max_timestamp_ahead_ms.map(ms_to_us).map(TimestampDuration::new),
            )
        });

        // Validate timestamp isn't too old
        let ts = get_aligned_timestamp();
        let ets = envelope.get_timestamp();
        if let Some(tsbehind) = tsbehind {
            if tsbehind.as_u64() != 0 && (ts > ets && ts.saturating_sub(ets) > tsbehind) {
                log_net!(debug
                    "envelope time was too far in the past: {}ms ",
                    timestamp_to_secs(ts.saturating_sub(ets).as_u64()) * 1000f64
                );
                return Ok(false);
            }
        }
        if let Some(tsahead) = tsahead {
            if tsahead.as_u64() != 0 && (ts < ets && ets.saturating_sub(ts) > tsahead) {
                log_net!(debug
                    "envelope time was too far in the future: {}ms",
                    timestamp_to_secs(ets.saturating_sub(ts).as_u64()) * 1000f64
                );
                return Ok(false);
            }
        }

        // Get routing table and rpc processor
        let routing_table = self.routing_table();
        let rpc = self.rpc_processor();

        // Peek at header and see if we need to relay this
        // If the recipient id is not our node id, then it needs relaying
        let sender_id = TypedKey::new(envelope.get_crypto_kind(), envelope.get_sender_id());
        let recipient_id = TypedKey::new(envelope.get_crypto_kind(), envelope.get_recipient_id());
        if !routing_table.matches_own_node_id(&[recipient_id]) {
            // See if the source node is allowed to resolve nodes
            // This is a costly operation, so only outbound-relay permitted
            // nodes are allowed to do this, for example PWA users

            let some_relay_nr = if self.check_client_whitelist(sender_id) {
                // Full relay allowed, do a full resolve_node
                match rpc.resolve_node(recipient_id, SafetySelection::Unsafe(Sequencing::default())).await {
                    Ok(v) => v,
                    Err(e) => {
                        log_net!(debug "failed to resolve recipient node for relay, dropping outbound relayed packet: {}" ,e);
                        return Ok(false);
                    }
                }
            } else {
                // If this is not a node in the client whitelist, only allow inbound relay
                // which only performs a lightweight lookup before passing the packet back out

                // See if we have the node in our routing table
                // We should, because relays are chosen by nodes that have established connectivity and
                // should be mutually in each others routing tables. The node needing the relay will be
                // pinging this node regularly to keep itself in the routing table
                match routing_table.lookup_node_ref(recipient_id) {
                    Ok(v) => v,
                    Err(e) => {
                        log_net!(debug "failed to look up recipient node for relay, dropping outbound relayed packet: {}" ,e);
                        return Ok(false);
                    }
                }
            };

            if let Some(relay_nr) = some_relay_nr {
                // Relay the packet to the desired destination
                log_net!("relaying {} bytes to {}", data.len(), relay_nr);
                network_result_value_or_log!(match self.send_data(relay_nr, data.to_vec())
                    .await {
                        Ok(v) => v,
                        Err(e) => {
                            log_net!(debug "failed to forward envelope: {}" ,e);
                            return Ok(false);    
                        }
                    } => {
                        return Ok(false);
                    }
                );
            }
            // Inform caller that we dealt with the envelope, but did not process it locally
            return Ok(false);
        }

        // DH to get decryption key (cached)
        let node_id_secret = routing_table.node_id_secret_key(envelope.get_crypto_kind());

        // Decrypt the envelope body
        let body = match envelope
            .decrypt_body(self.crypto(), data, &node_id_secret) {
                Ok(v) => v,
                Err(e) => {
                    log_net!(debug "failed to decrypt envelope body: {}",e);
                    // xxx: punish nodes that send messages that fail to decrypt eventually
                    return Ok(false);
                }
            };

        // Cache the envelope information in the routing table
        let source_noderef = match routing_table.register_node_with_existing_connection(
            TypedKey::new(envelope.get_crypto_kind(), envelope.get_sender_id()),
            connection_descriptor,
            ts,
        ) {
            Ok(v) => v,
            Err(e) => {
                // If the node couldn't be registered just skip this envelope,
                log_net!(debug "failed to register node with existing connection: {}", e);
                return Ok(false);
            }
        };
        source_noderef.add_envelope_version(envelope.get_version());

        // xxx: deal with spoofing and flooding here?

        // Pass message to RPC system
        rpc.enqueue_direct_message(
            envelope,
            source_noderef,
            connection_descriptor,
            routing_domain,
            body,
        )?;

        // Inform caller that we dealt with the envelope locally
        Ok(true)
    }

    // Callbacks from low level network for statistics gathering
    pub fn stats_packet_sent(&self, addr: IpAddr, bytes: ByteCount) {
        let inner = &mut *self.inner.lock();
        inner
            .stats
            .self_stats
            .transfer_stats_accounting
            .add_up(bytes);
        inner
            .stats
            .per_address_stats
            .entry(PerAddressStatsKey(addr), |_k,_v| {
                // do nothing on LRU evict
            })
            .or_insert(PerAddressStats::default())
            .transfer_stats_accounting
            .add_up(bytes);
    }

    pub fn stats_packet_rcvd(&self, addr: IpAddr, bytes: ByteCount) {
        let inner = &mut *self.inner.lock();
        inner
            .stats
            .self_stats
            .transfer_stats_accounting
            .add_down(bytes);
        inner
            .stats
            .per_address_stats
            .entry(PerAddressStatsKey(addr), |_k,_v| {
                // do nothing on LRU evict
            })
            .or_insert(PerAddressStats::default())
            .transfer_stats_accounting
            .add_down(bytes);
    }

    // Get stats
    pub fn get_stats(&self) -> NetworkManagerStats {
        let inner = self.inner.lock();
        inner.stats.clone()
    }

    pub fn get_veilid_state(&self) -> VeilidStateNetwork {
        let has_state = self
            .unlocked_inner
            .components
            .read()
            .as_ref()
            .map(|c| c.net.is_started())
            .unwrap_or(false);

        if !has_state {
            return VeilidStateNetwork {
                started: false,
                bps_down: 0.into(),
                bps_up: 0.into(),
                peers: Vec::new(),
                
            };
        }
        let routing_table = self.routing_table();

        let (bps_down, bps_up) = {
            let inner = self.inner.lock();
            (
                inner.stats.self_stats.transfer_stats.down.average,
                inner.stats.self_stats.transfer_stats.up.average,
            )
        };

        VeilidStateNetwork {
            started: true,
            bps_down,
            bps_up,
            peers: {
                let mut out = Vec::new();
                for (k, v) in routing_table.get_recent_peers() {
                    if let Ok(Some(nr)) = routing_table.lookup_node_ref(k) {
                        let peer_stats = nr.peer_stats();
                        let peer = PeerTableData {
                            node_ids: nr.node_ids().iter().copied().collect(),
                            peer_address: v.last_connection.remote().to_string(),
                            peer_stats,
                        };
                        out.push(peer);
                    }
                }
                out
            },
        }
    }

    fn send_network_update(&self) {
        let update_cb = self.unlocked_inner.update_callback.read().clone();
        if update_cb.is_none() {
            return;
        }
        let state = self.get_veilid_state();
        (update_cb.unwrap())(VeilidUpdate::Network(state));
    }

    // Determine if a local IP address has changed
    // this means we should restart the low level network and and recreate all of our dial info
    // Wait until we have received confirmation from N different peers
    pub fn report_local_network_socket_address(
        &self,
        _socket_address: SocketAddress,
        _connection_descriptor: ConnectionDescriptor,
        _reporting_peer: NodeRef,
    ) {
        // XXX: Nothing here yet.
    }

    // Determine if a global IP address has changed
    // this means we should recreate our public dial info if it is not static and rediscover it
    // Wait until we have received confirmation from N different peers
    pub fn report_public_internet_socket_address(
        &self,
        socket_address: SocketAddress, // the socket address as seen by the remote peer
        connection_descriptor: ConnectionDescriptor, // the connection descriptor used
        reporting_peer: NodeRef,       // the peer's noderef reporting the socket address
    ) {
        // debug code
        //info!("report_global_socket_address\nsocket_address: {:#?}\nconnection_descriptor: {:#?}\nreporting_peer: {:#?}", socket_address, connection_descriptor, reporting_peer);

        // Ignore these reports if we are currently detecting public dial info
        let net = self.net();
        if net.needs_public_dial_info_check() {
            return;
        }

        let routing_table = self.routing_table();
        let (detect_address_changes, ip6_prefix_size) = self.with_config(|c| {
            (
                c.network.detect_address_changes,
                c.network.max_connections_per_ip6_prefix_size as usize,
            )
        });

        // Get the ip(block) this report is coming from
        let ipblock = ip_to_ipblock(
            ip6_prefix_size,
            connection_descriptor.remote_address().to_ip_addr(),
        );

        // Store the reported address if it isn't denylisted
        let key = PublicAddressCheckCacheKey(
            connection_descriptor.protocol_type(),
            connection_descriptor.address_type(),
        );

        let mut inner = self.inner.lock();
        let inner = &mut *inner;

        let pacc = inner
            .public_address_check_cache
            .entry(key)
            .or_insert_with(|| LruCache::new(PUBLIC_ADDRESS_CHECK_CACHE_SIZE));
        let pait = inner
            .public_address_inconsistencies_table
            .entry(key)
            .or_insert_with(|| HashMap::new());
        if pait.contains_key(&ipblock) {
            return;
        }
        pacc.insert(ipblock, socket_address, |_k,_v| {
            // do nothing on LRU evict
        });

        // Determine if our external address has likely changed
        let mut bad_public_address_detection_punishment: Option<
            Box<dyn FnOnce() + Send + 'static>,
        > = None;
        let public_internet_network_class = routing_table
            .get_network_class(RoutingDomain::PublicInternet)
            .unwrap_or(NetworkClass::Invalid);
        let needs_public_address_detection =
            if matches!(public_internet_network_class, NetworkClass::InboundCapable) {
                // Get the dial info filter for this connection so we can check if we have any public dialinfo that may have changed
                let dial_info_filter = connection_descriptor.make_dial_info_filter();

                // Get current external ip/port from registered global dialinfo
                let current_addresses: BTreeSet<SocketAddress> = routing_table
                    .all_filtered_dial_info_details(
                        RoutingDomain::PublicInternet.into(),
                        &dial_info_filter,
                    )
                    .iter()
                    .map(|did| did.dial_info.socket_address())
                    .collect();

                // If we are inbound capable, but start to see inconsistent socket addresses from multiple reporting peers
                // then we zap the network class and re-detect it
                let mut inconsistencies = Vec::new();

                // Iteration goes from most recent to least recent node/address pair
                for (reporting_ip_block, a) in pacc {
                    // If this address is not one of our current addresses (inconsistent)
                    // and we haven't already denylisted the reporting source,
                    if !current_addresses.contains(a) && !pait.contains_key(reporting_ip_block) {
                        // Record the origin of the inconsistency
                        inconsistencies.push(*reporting_ip_block);
                    }
                }

                // If we have enough inconsistencies to consider changing our public dial info,
                // add them to our denylist (throttling) and go ahead and check for new
                // public dialinfo
                let inconsistent = if inconsistencies.len() >= PUBLIC_ADDRESS_CHANGE_DETECTION_COUNT
                {
                    let exp_ts = get_aligned_timestamp() + PUBLIC_ADDRESS_INCONSISTENCY_TIMEOUT_US;
                    for i in &inconsistencies {
                        pait.insert(*i, exp_ts);
                    }

                    // Run this routine if the inconsistent nodes turn out to be lying
                    let this = self.clone();
                    bad_public_address_detection_punishment = Some(Box::new(move || {
                        let mut inner = this.inner.lock();
                        let pait = inner
                            .public_address_inconsistencies_table
                            .entry(key)
                            .or_insert_with(|| HashMap::new());
                        let exp_ts =
                            get_aligned_timestamp() + PUBLIC_ADDRESS_INCONSISTENCY_PUNISHMENT_TIMEOUT_US;
                        for i in inconsistencies {
                            pait.insert(i, exp_ts);
                        }
                    }));

                    true
                } else {
                    false
                };

                // // debug code
                // if inconsistent {
                //     trace!("public_address_check_cache: {:#?}\ncurrent_addresses: {:#?}\ninconsistencies: {}", inner
                //                 .public_address_check_cache, current_addresses, inconsistencies);
                // }

                inconsistent
            } else if matches!(public_internet_network_class, NetworkClass::OutboundOnly) {
                // If we are currently outbound only, we don't have any public dial info
                // but if we are starting to see consistent socket address from multiple reporting peers
                // then we may be become inbound capable, so zap the network class so we can re-detect it and any public dial info

                let mut consistencies = 0;
                let mut consistent = false;
                let mut current_address = Option::<SocketAddress>::None;
                // Iteration goes from most recent to least recent node/address pair
                let pacc = inner
                    .public_address_check_cache
                    .entry(key)
                    .or_insert_with(|| LruCache::new(PUBLIC_ADDRESS_CHECK_CACHE_SIZE));

                for (_, a) in pacc {
                    if let Some(current_address) = current_address {
                        if current_address == *a {
                            consistencies += 1;
                            if consistencies >= PUBLIC_ADDRESS_CHANGE_DETECTION_COUNT {
                                consistent = true;
                                break;
                            }
                        }
                    } else {
                        current_address = Some(*a);
                    }
                }
                consistent
            } else {
                // If we are a webapp we never do this.
                // If we have invalid network class, then public address detection is already going to happen via the network_class_discovery task
                false
            };

        if needs_public_address_detection {
            if detect_address_changes {
                // Reset the address check cache now so we can start detecting fresh
                info!("Public address has changed, detecting public dial info");

                inner.public_address_check_cache.clear();

                // Re-detect the public dialinfo
                net.set_needs_public_dial_info_check(bad_public_address_detection_punishment);
            } else {
                warn!("Public address may have changed. Restarting the server may be required.");
                warn!("report_global_socket_address\nsocket_address: {:#?}\nconnection_descriptor: {:#?}\nreporting_peer: {:#?}", socket_address, connection_descriptor, reporting_peer);
                warn!(
                    "public_address_check_cache: {:#?}",
                    inner.public_address_check_cache
                );
            }
        }
    }

}
