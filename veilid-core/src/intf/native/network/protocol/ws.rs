use super::*;
use crate::intf::native::utils::async_peek_stream::*;
use crate::intf::*;
use crate::network_manager::{NetworkManager, MAX_MESSAGE_SIZE};
use crate::*;
use async_std::io;
use async_std::net::*;
use async_std::sync::Mutex as AsyncMutex;
use async_tls::TlsConnector;
use async_tungstenite::tungstenite::protocol::Message;
use async_tungstenite::{accept_async, client_async, WebSocketStream};
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

pub type WebSocketNetworkConnectionAccepted = WebsocketNetworkConnection<AsyncPeekStream>;
pub type WebsocketNetworkConnectionWSS =
    WebsocketNetworkConnection<async_tls::client::TlsStream<async_std::net::TcpStream>>;
pub type WebsocketNetworkConnectionWS = WebsocketNetworkConnection<async_std::net::TcpStream>;

struct WebSocketNetworkConnectionInner<T>
where
    T: io::Read + io::Write + Send + Unpin + 'static,
{
    ws_stream: WebSocketStream<T>,
}

pub struct WebsocketNetworkConnection<T>
where
    T: io::Read + io::Write + Send + Unpin + 'static,
{
    tls: bool,
    inner: Arc<AsyncMutex<WebSocketNetworkConnectionInner<T>>>,
}

impl<T> Clone for WebsocketNetworkConnection<T>
where
    T: io::Read + io::Write + Send + Unpin + 'static,
{
    fn clone(&self) -> Self {
        Self {
            tls: self.tls,
            inner: self.inner.clone(),
        }
    }
}

impl<T> fmt::Debug for WebsocketNetworkConnection<T>
where
    T: io::Read + io::Write + Send + Unpin + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", std::any::type_name::<Self>())
    }
}

impl<T> PartialEq for WebsocketNetworkConnection<T>
where
    T: io::Read + io::Write + Send + Unpin + 'static,
{
    fn eq(&self, other: &Self) -> bool {
        self.tls == other.tls && Arc::as_ptr(&self.inner) == Arc::as_ptr(&other.inner)
    }
}

impl<T> Eq for WebsocketNetworkConnection<T> where T: io::Read + io::Write + Send + Unpin + 'static {}

impl<T> WebsocketNetworkConnection<T>
where
    T: io::Read + io::Write + Send + Unpin + 'static,
{
    pub fn new(tls: bool, ws_stream: WebSocketStream<T>) -> Self {
        Self {
            tls,
            inner: Arc::new(AsyncMutex::new(WebSocketNetworkConnectionInner {
                ws_stream,
            })),
        }
    }

    pub fn protocol_type(&self) -> ProtocolType {
        if self.tls {
            ProtocolType::WSS
        } else {
            ProtocolType::WS
        }
    }
    pub fn send(&self, message: Vec<u8>) -> SystemPinBoxFuture<Result<(), ()>> {
        let inner = self.inner.clone();

        Box::pin(async move {
            if message.len() > MAX_MESSAGE_SIZE {
                return Err(());
            }
            let mut inner = inner.lock().await;
            inner
                .ws_stream
                .send(Message::binary(message))
                .await
                .map_err(drop)
        })
    }
    pub fn recv(&self) -> SystemPinBoxFuture<Result<Vec<u8>, ()>> {
        let inner = self.inner.clone();

        Box::pin(async move {
            let mut inner = inner.lock().await;

            let out = match inner.ws_stream.next().await {
                Some(Ok(Message::Binary(v))) => v,
                _ => {
                    trace!("websocket recv failed");
                    return Err(());
                }
            };
            if out.len() > MAX_MESSAGE_SIZE {
                Err(())
            } else {
                Ok(out)
            }
        })
    }
}

///////////////////////////////////////////////////////////
///
struct WebsocketProtocolHandlerInner {
    tls: bool,
    network_manager: NetworkManager,
    local_address: SocketAddr,
    request_path: Vec<u8>,
    connection_initial_timeout: u64,
}

#[derive(Clone)]
pub struct WebsocketProtocolHandler
where
    Self: TcpProtocolHandler,
{
    inner: Arc<WebsocketProtocolHandlerInner>,
}
impl WebsocketProtocolHandler {
    pub fn new(network_manager: NetworkManager, tls: bool, local_address: SocketAddr) -> Self {
        let config = network_manager.config();
        let c = config.get();
        let path = format!("GET {}", c.network.protocol.ws.path.trim_end_matches('/'));
        let connection_initial_timeout = if tls {
            c.network.tls.connection_initial_timeout
        } else {
            c.network.connection_initial_timeout
        };

        let inner = WebsocketProtocolHandlerInner {
            tls,
            network_manager,
            local_address,
            request_path: path.as_bytes().to_vec(),
            connection_initial_timeout,
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn on_accept_async(
        self,
        ps: AsyncPeekStream,
        socket_addr: SocketAddr,
    ) -> Result<bool, String> {
        let request_path_len = self.inner.request_path.len() + 2;
        let mut peekbuf: Vec<u8> = vec![0u8; request_path_len];
        match io::timeout(
            Duration::from_micros(self.inner.connection_initial_timeout),
            ps.peek_exact(&mut peekbuf),
        )
        .await
        {
            Ok(_) => (),
            Err(e) => {
                return Err(format!("failed to peek stream: {:?}", e));
            }
        }
        // Check for websocket path
        let matches_path = &peekbuf[0..request_path_len - 2] == self.inner.request_path.as_slice()
            && (peekbuf[request_path_len - 2] == b' '
                || (peekbuf[request_path_len - 2] == b'/'
                    && peekbuf[request_path_len - 1] == b' '));

        if !matches_path {
            trace!("not websocket");
            return Ok(false);
        }
        trace!("found websocket");

        let ws_stream = match accept_async(ps).await {
            Ok(s) => s,
            Err(e) => {
                return Err(format!("failed websockets handshake: {:?}", e));
            }
        };

        // Wrap the websocket in a NetworkConnection and register it
        let protocol_type = if self.inner.tls {
            ProtocolType::WSS
        } else {
            ProtocolType::WS
        };

        let peer_addr = PeerAddress::new(
            Address::from_socket_addr(socket_addr),
            socket_addr.port(),
            protocol_type,
        );

        let conn = NetworkConnection::WsAccepted(WebsocketNetworkConnection::new(
            self.inner.tls,
            ws_stream,
        ));
        self.inner
            .network_manager
            .clone()
            .on_new_connection(
                ConnectionDescriptor::new(peer_addr, self.inner.local_address),
                conn,
            )
            .await?;
        Ok(true)
    }

    pub async fn connect(
        network_manager: NetworkManager,
        dial_info: &DialInfo,
    ) -> Result<NetworkConnection, String> {
        let (tls, request, domain, port, protocol_type) = match &dial_info {
            DialInfo::WS(di) => (
                false,
                di.path.clone(),
                di.host.clone(),
                di.port,
                ProtocolType::WS,
            ),
            DialInfo::WSS(di) => (
                true,
                di.path.clone(),
                di.host.clone(),
                di.port,
                ProtocolType::WSS,
            ),
            _ => panic!("invalid dialinfo for WS/WSS protocol"),
        };

        let tcp_stream = TcpStream::connect(format!("{}:{}", &domain, &port))
            .await
            .map_err(|e| format!("failed to connect tcp stream: {}", e))?;
        let local_addr = tcp_stream
            .local_addr()
            .map_err(|e| format!("can't get local address for tcp stream: {}", e))?;
        let peer_socket_addr = tcp_stream
            .peer_addr()
            .map_err(|e| format!("can't get peer address for tcp stream: {}", e))?;
        let peer_addr = PeerAddress::new(
            Address::from_socket_addr(peer_socket_addr),
            peer_socket_addr.port(),
            protocol_type,
        );

        if tls {
            let connector = TlsConnector::default();
            let tls_stream = connector
                .connect(domain, tcp_stream)
                .await
                .map_err(|e| format!("can't connect tls: {}", e))?;
            let (ws_stream, _response) = client_async(request, tls_stream)
                .await
                .map_err(|e| format!("wss negotation failed: {}", e))?;
            let conn = NetworkConnection::Wss(WebsocketNetworkConnection::new(tls, ws_stream));
            network_manager
                .on_new_connection(
                    ConnectionDescriptor::new(peer_addr, local_addr),
                    conn.clone(),
                )
                .await?;
            Ok(conn)
        } else {
            let (ws_stream, _response) = client_async(request, tcp_stream)
                .await
                .map_err(|e| format!("ws negotiate failed: {}", e))?;
            let conn = NetworkConnection::Ws(WebsocketNetworkConnection::new(tls, ws_stream));
            network_manager
                .on_new_connection(
                    ConnectionDescriptor::new(peer_addr, local_addr),
                    conn.clone(),
                )
                .await?;
            Ok(conn)
        }
    }
}

impl TcpProtocolHandler for WebsocketProtocolHandler {
    fn on_accept(
        &self,
        stream: AsyncPeekStream,
        peer_addr: SocketAddr,
    ) -> SystemPinBoxFuture<Result<bool, String>> {
        Box::pin(self.clone().on_accept_async(stream, peer_addr))
    }
}
