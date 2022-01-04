use crate::connection_table::*;
use crate::intf::*;
use crate::network_connection::*;
use crate::network_manager::*;
use crate::xx::*;
use crate::*;
use futures_util::future::{select, Either};
use futures_util::stream::{FuturesUnordered, StreamExt};

const CONNECTION_PROCESSOR_CHANNEL_SIZE: usize = 128usize;

///////////////////////////////////////////////////////////
// Connection manager

struct ConnectionManagerInner {
    connection_table: ConnectionTable,
    connection_processor_jh: Option<JoinHandle<()>>,
    connection_add_channel_tx: Option<utils::channel::Sender<SystemPinBoxFuture<()>>>,
}

impl core::fmt::Debug for ConnectionManagerInner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConnectionManagerInner")
            .field("connection_table", &self.connection_table)
            .finish()
    }
}

struct ConnectionManagerArc {
    network_manager: NetworkManager,
    inner: AsyncMutex<ConnectionManagerInner>,
}
impl core::fmt::Debug for ConnectionManagerArc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConnectionManagerArc")
            .field("inner", &self.inner)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionManager {
    arc: Arc<ConnectionManagerArc>,
}

impl ConnectionManager {
    fn new_inner() -> ConnectionManagerInner {
        ConnectionManagerInner {
            connection_table: ConnectionTable::new(),
            connection_processor_jh: None,
            connection_add_channel_tx: None,
        }
    }
    fn new_arc(network_manager: NetworkManager) -> ConnectionManagerArc {
        ConnectionManagerArc {
            network_manager,
            inner: AsyncMutex::new(Self::new_inner()),
        }
    }
    pub fn new(network_manager: NetworkManager) -> Self {
        Self {
            arc: Arc::new(Self::new_arc(network_manager)),
        }
    }

    pub fn network_manager(&self) -> NetworkManager {
        self.arc.network_manager.clone()
    }

    pub async fn startup(&self) {
        let mut inner = self.arc.inner.lock().await;
        let cac = utils::channel::channel(CONNECTION_PROCESSOR_CHANNEL_SIZE); // xxx move to config
        inner.connection_add_channel_tx = Some(cac.0);
        let rx = cac.1.clone();
        let this = self.clone();
        inner.connection_processor_jh = Some(spawn(this.connection_processor(rx)));
    }

    pub async fn shutdown(&self) {
        *self.arc.inner.lock().await = Self::new_inner();
    }

    // Returns a network connection if one already is established
    pub async fn get_connection(
        &self,
        descriptor: ConnectionDescriptor,
    ) -> Option<NetworkConnection> {
        let inner = self.arc.inner.lock().await;
        inner.connection_table.get_connection(descriptor)
    }

    // Internal routine to register new connection atomically
    async fn on_new_connection_internal(
        &self,
        inner: &mut ConnectionManagerInner,
        conn: NetworkConnection,
    ) -> Result<(), String> {
        let tx = inner
            .connection_add_channel_tx
            .as_ref()
            .ok_or_else(fn_string!("connection channel isn't open yet"))?
            .clone();

        let receiver_loop_future = Self::process_connection(self.clone(), conn.clone());
        tx.try_send(receiver_loop_future)
            .await
            .map_err(map_to_string)
            .map_err(logthru_net!(error "failed to start receiver loop"))?;

        // If the receiver loop started successfully,
        // add the new connection to the table
        inner.connection_table.add_connection(conn)
    }

    // Called by low-level network when any connection-oriented protocol connection appears
    // either from incoming or outgoing connections. Registers connection in the connection table for later access
    // and spawns a message processing loop for the connection
    pub async fn on_new_connection(&self, conn: NetworkConnection) -> Result<(), String> {
        let mut inner = self.arc.inner.lock().await;
        self.on_new_connection_internal(&mut *inner, conn).await
    }

    pub async fn get_or_create_connection(
        &self,
        local_addr: Option<SocketAddr>,
        dial_info: DialInfo,
    ) -> Result<NetworkConnection, String> {
        let peer_address = dial_info.to_peer_address();
        let descriptor = match local_addr {
            Some(la) => {
                ConnectionDescriptor::new(peer_address, SocketAddress::from_socket_addr(la))
            }
            None => ConnectionDescriptor::new_no_local(peer_address),
        };

        // If connection exists, then return it
        let mut inner = self.arc.inner.lock().await;

        if let Some(conn) = inner.connection_table.get_connection(descriptor) {
            return Ok(conn);
        }

        // If not, attempt new connection
        let conn = NetworkConnection::connect(local_addr, dial_info).await?;

        self.on_new_connection_internal(&mut *inner, conn.clone())
            .await?;

        Ok(conn)
    }

    // Connection receiver loop
    fn process_connection(
        this: ConnectionManager,
        conn: NetworkConnection,
    ) -> SystemPinBoxFuture<()> {
        let network_manager = this.network_manager();
        Box::pin(async move {
            //
            let descriptor = conn.connection_descriptor();
            loop {
                let res = conn.clone().recv().await;
                let message = match res {
                    Ok(v) => v,
                    Err(_) => break,
                };
                if let Err(e) = network_manager
                    .on_recv_envelope(message.as_slice(), descriptor)
                    .await
                {
                    log_net!(error e);
                    break;
                }
            }

            if let Err(e) = this
                .arc
                .inner
                .lock()
                .await
                .connection_table
                .remove_connection(descriptor)
            {
                log_net!(error e);
            }
        })
    }

    // Process connection oriented sockets in the background
    // This never terminates and must have its task cancelled once started
    // Task cancellation is performed by shutdown() by dropping the join handle
    async fn connection_processor(self, rx: utils::channel::Receiver<SystemPinBoxFuture<()>>) {
        let mut connection_futures: FuturesUnordered<SystemPinBoxFuture<()>> =
            FuturesUnordered::new();
        loop {
            // Either process an existing connection, or receive a new one to add to our list
            match select(connection_futures.next(), Box::pin(rx.recv())).await {
                Either::Left((x, _)) => {
                    // Processed some connection to completion, or there are none left
                    match x {
                        Some(()) => {
                            // Processed some connection to completion
                        }
                        None => {
                            // No connections to process, wait for one
                            match rx.recv().await {
                                Ok(v) => {
                                    connection_futures.push(v);
                                }
                                Err(e) => {
                                    log_net!(error "connection processor error: {:?}", e);
                                    // xxx: do something here?? should the network be restarted if this happens?
                                }
                            };
                        }
                    }
                }
                Either::Right((x, _)) => {
                    // Got a new connection future
                    match x {
                        Ok(v) => {
                            connection_futures.push(v);
                        }
                        Err(e) => {
                            log_net!(error "connection processor error: {:?}", e);
                            // xxx: do something here?? should the network be restarted if this happens?
                        }
                    };
                }
            }
        }
    }
}
