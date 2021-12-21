mod debug;
pub use debug::*;

pub use crate::rpc_processor::InfoAnswer;
use crate::*;
use attachment_manager::AttachmentManager;
use core::fmt;
use network_manager::NetworkManager;
use routing_table::*;
use rpc_processor::{RPCError, RPCProcessor};
use xx::*;

pub use crate::dht::key::{generate_secret, DHTKey, DHTKeySecret};
pub use crate::xx::{
    IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, SystemPinBoxFuture,
    ToSocketAddrs,
};
pub use alloc::string::ToString;
pub use core::str::FromStr;

/////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug, PartialOrd, PartialEq, Eq, Ord)]
pub enum VeilidAPIError {
    Timeout,
    Shutdown,
    NodeNotFound(NodeId),
    NoDialInfo(NodeId),
    Internal(String),
    Unimplemented(String),
    ParseError {
        message: String,
        value: String,
    },
    InvalidArgument {
        context: String,
        argument: String,
        value: String,
    },
    MissingArgument {
        context: String,
        argument: String,
    },
}

impl fmt::Display for VeilidAPIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            VeilidAPIError::Timeout => write!(f, "VeilidAPIError::Timeout"),
            VeilidAPIError::Shutdown => write!(f, "VeilidAPIError::Shutdown"),
            VeilidAPIError::NodeNotFound(ni) => write!(f, "VeilidAPIError::NodeNotFound({})", ni),
            VeilidAPIError::NoDialInfo(ni) => write!(f, "VeilidAPIError::NoDialInfo({})", ni),
            VeilidAPIError::Internal(e) => write!(f, "VeilidAPIError::Internal({})", e),
            VeilidAPIError::Unimplemented(e) => write!(f, "VeilidAPIError::Unimplemented({})", e),
            VeilidAPIError::ParseError { message, value } => {
                write!(f, "VeilidAPIError::ParseError({}: {})", message, value)
            }
            VeilidAPIError::InvalidArgument {
                context,
                argument,
                value,
            } => {
                write!(
                    f,
                    "VeilidAPIError::InvalidArgument({}: {} = {})",
                    context, argument, value
                )
            }
            VeilidAPIError::MissingArgument { context, argument } => {
                write!(
                    f,
                    "VeilidAPIError::MissingArgument({}: {})",
                    context, argument
                )
            }
        }
    }
}

fn convert_rpc_error(x: RPCError) -> VeilidAPIError {
    match x {
        RPCError::Timeout => VeilidAPIError::Timeout,
        RPCError::Unimplemented(s) => VeilidAPIError::Unimplemented(s),
        RPCError::Internal(s) => VeilidAPIError::Internal(s),
        RPCError::Protocol(s) => VeilidAPIError::Internal(s),
        RPCError::InvalidFormat => VeilidAPIError::Internal("Invalid packet format".to_owned()),
    }
}

macro_rules! map_rpc_error {
    () => {
        |x| convert_rpc_error(x)
    };
}

macro_rules! parse_error {
    ($msg:expr, $val:expr) => {
        VeilidAPIError::ParseError {
            message: $msg.to_string(),
            value: $val.to_string(),
        }
    };
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug, Default, PartialOrd, PartialEq, Eq, Ord)]
pub struct NodeId {
    pub key: DHTKey,
}
impl NodeId {
    pub fn new(key: DHTKey) -> Self {
        assert!(key.valid);
        Self { key }
    }
}
impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.key.encode())
    }
}

#[derive(Clone, Debug, Default, PartialOrd, PartialEq, Eq, Ord)]
pub struct ValueKey {
    pub key: DHTKey,
    pub subkey: Option<String>,
}
impl ValueKey {
    pub fn new(key: DHTKey) -> Self {
        Self { key, subkey: None }
    }
    pub fn new_subkey(key: DHTKey, subkey: String) -> Self {
        Self {
            key,
            subkey: if subkey.is_empty() {
                None
            } else {
                Some(subkey)
            },
        }
    }
}

#[derive(Clone, Debug, Default, PartialOrd, PartialEq, Eq, Ord)]
pub struct BlockId {
    pub key: DHTKey,
}
impl BlockId {
    pub fn new(key: DHTKey) -> Self {
        Self { key }
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash, Default)]
pub struct SenderInfo {
    pub socket_address: Option<SocketAddr>,
}

#[derive(Clone, Debug, Default)]
pub struct NodeInfo {
    pub can_route: bool,
    pub will_route: bool,
    pub can_tunnel: bool,
    pub will_tunnel: bool,
    pub can_signal_lease: bool,
    pub will_signal_lease: bool,
    pub can_relay_lease: bool,
    pub will_relay_lease: bool,
    pub can_validate_dial_info: bool,
    pub will_validate_dial_info: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum ProtocolType {
    UDP,
    TCP,
    WS,
    WSS,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum ProtocolNetworkType {
    UDPv4,
    UDPv6,
    TCPv4,
    TCPv6,
}

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum Address {
    IPV4(Ipv4Addr),
    IPV6(Ipv6Addr),
}

impl Default for Address {
    fn default() -> Self {
        Address::IPV4(Ipv4Addr::new(0, 0, 0, 0))
    }
}

impl Address {
    pub fn from_socket_addr(sa: SocketAddr) -> Address {
        match sa {
            SocketAddr::V4(v4) => Address::IPV4(*v4.ip()),
            SocketAddr::V6(v6) => Address::IPV6(*v6.ip()),
        }
    }
    pub fn address_string(&self) -> String {
        match self {
            Address::IPV4(v4) => v4.to_string(),
            Address::IPV6(v6) => v6.to_string(),
        }
    }
    pub fn address_string_with_port(&self, port: u16) -> String {
        match self {
            Address::IPV4(v4) => format!("{}:{}", v4.to_string(), port),
            Address::IPV6(v6) => format!("[{}]:{}", v6.to_string(), port),
        }
    }
    pub fn is_public(&self) -> bool {
        match self {
            Address::IPV4(v4) => ipv4addr_is_global(&v4),
            Address::IPV6(v6) => ipv6addr_is_global(&v6),
        }
    }
    pub fn is_private(&self) -> bool {
        match self {
            Address::IPV4(v4) => ipv4addr_is_private(&v4),
            Address::IPV6(v6) => ipv6addr_is_unicast_site_local(&v6),
        }
    }
    pub fn to_ip_addr(&self) -> IpAddr {
        match self {
            Self::IPV4(a) => IpAddr::V4(*a),
            Self::IPV6(a) => IpAddr::V6(*a),
        }
    }
    pub fn to_socket_addr(&self, port: u16) -> SocketAddr {
        SocketAddr::new(self.to_ip_addr(), port)
    }
    pub fn to_canonical(&self) -> Address {
        match self {
            Address::IPV4(v4) => Address::IPV4(*v4),
            Address::IPV6(v6) => match v6.to_ipv4() {
                Some(v4) => Address::IPV4(v4),
                None => Address::IPV6(*v6),
            },
        }
    }
}

impl FromStr for Address {
    type Err = VeilidAPIError;
    fn from_str(host: &str) -> Result<Address, VeilidAPIError> {
        if let Ok(addr) = Ipv4Addr::from_str(host) {
            Ok(Address::IPV4(addr))
        } else if let Ok(addr) = Ipv6Addr::from_str(host) {
            Ok(Address::IPV6(addr))
        } else {
            Err(parse_error!("Address::from_str failed", host))
        }
    }
}

#[derive(Copy, Default, Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct SocketAddress {
    address: Address,
    port: u16,
}

impl SocketAddress {
    pub fn new(address: Address, port: u16) -> Self {
        Self { address, port }
    }
    pub fn from_socket_addr(sa: SocketAddr) -> SocketAddress {
        Self {
            address: Address::from_socket_addr(sa),
            port: sa.port(),
        }
    }
    pub fn address(&self) -> Address {
        self.address
    }
    pub fn port(&self) -> u16 {
        self.port
    }
    pub fn to_canonical(&self) -> SocketAddress {
        SocketAddress {
            address: self.address.to_canonical(),
            port: self.port,
        }
    }
    pub fn to_ip_addr(&self) -> IpAddr {
        self.address.to_ip_addr()
    }
    pub fn to_socket_addr(&self) -> SocketAddr {
        self.address.to_socket_addr(self.port)
    }
}

impl fmt::Display for SocketAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}:{}", self.to_ip_addr(), self.port)
    }
}

impl FromStr for SocketAddress {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<SocketAddress, VeilidAPIError> {
        let split = s.rsplit_once(':').ok_or_else(|| {
            parse_error!("SocketAddress::from_str missing colon port separator", s)
        })?;
        let address = Address::from_str(split.0)?;
        let port = u16::from_str(split.1).map_err(|e| {
            parse_error!(
                format!("SocketAddress::from_str failed parting port: {}", e),
                s
            )
        })?;
        Ok(SocketAddress { address, port })
    }
}

#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct DialInfoUDP {
    pub socket_address: SocketAddress,
}

#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct DialInfoTCP {
    pub socket_address: SocketAddress,
}

#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct DialInfoWS {
    pub socket_address: SocketAddress,
    pub request: String,
}

#[derive(Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub struct DialInfoWSS {
    pub socket_address: SocketAddress,
    pub request: String,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum DialInfo {
    UDP(DialInfoUDP),
    TCP(DialInfoTCP),
    WS(DialInfoWS),
    WSS(DialInfoWSS),
}
impl Default for DialInfo {
    fn default() -> Self {
        DialInfo::UDP(DialInfoUDP::default())
    }
}

impl fmt::Display for DialInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            DialInfo::UDP(di) => write!(f, "udp|{}", di.socket_address),
            DialInfo::TCP(di) => write!(f, "tcp|{}", di.socket_address),
            DialInfo::WS(di) => write!(f, "ws|{}|{}", di.socket_address, di.request),
            DialInfo::WSS(di) => write!(f, "wss|{}|{}", di.socket_address, di.request),
        }
    }
}

impl FromStr for DialInfo {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<DialInfo, VeilidAPIError> {
        let (proto, rest) = s.split_once('|').ok_or_else(|| {
            parse_error!("SocketAddress::from_str missing protocol '|' separator", s)
        })?;
        match proto {
            "udp" => {
                let socket_address = SocketAddress::from_str(rest)?;
                Ok(DialInfo::udp(socket_address))
            }
            "tcp" => {
                let socket_address = SocketAddress::from_str(rest)?;
                Ok(DialInfo::tcp(socket_address))
            }
            "ws" => {
                let (sa, rest) = s.split_once('|').ok_or_else(|| {
                    parse_error!(
                        "SocketAddress::from_str missing socket address '|' separator",
                        s
                    )
                })?;
                let socket_address = SocketAddress::from_str(sa)?;
                DialInfo::try_ws(socket_address, rest.to_string())
            }
            "wss" => {
                let (sa, rest) = s.split_once('|').ok_or_else(|| {
                    parse_error!(
                        "SocketAddress::from_str missing socket address '|' separator",
                        s
                    )
                })?;
                let socket_address = SocketAddress::from_str(sa)?;
                DialInfo::try_wss(socket_address, rest.to_string())
            }
        }
    }
}

impl DialInfo {
    pub fn udp_from_socketaddr(socket_addr: SocketAddr) -> Self {
        Self::UDP(DialInfoUDP {
            socket_address: SocketAddress::from_socket_addr(socket_addr).to_canonical(),
        })
    }
    pub fn tcp_from_socketaddr(socket_addr: SocketAddr) -> Self {
        Self::TCP(DialInfoTCP {
            socket_address: SocketAddress::from_socket_addr(socket_addr).to_canonical(),
        })
    }
    pub fn udp(socket_address: SocketAddress) -> Self {
        Self::UDP(DialInfoUDP {
            socket_address: socket_address.to_canonical(),
        })
    }
    pub fn tcp(socket_address: SocketAddress) -> Self {
        Self::TCP(DialInfoTCP {
            socket_address: socket_address.to_canonical(),
        })
    }
    pub fn try_ws(socket_address: SocketAddress, url: String) -> Result<Self, VeilidAPIError> {
        let split_url = SplitUrl::from_str(&url)
            .map_err(|e| parse_error!(format!("unable to split WS url: {}", e), url))?;
        if split_url.scheme != "ws" || !url.starts_with("ws://") {
            return Err(parse_error!("incorrect scheme for WS dialinfo", url));
        }
        let url_port = split_url.port.unwrap_or(80u16);
        if url_port != socket_address.port() {
            return Err(parse_error!(
                "socket address port doesn't match url port",
                url
            ));
        }
        Ok(Self::WS(DialInfoWS {
            socket_address: socket_address.to_canonical(),
            request: url[5..].to_string(),
        }))
    }
    pub fn try_wss(socket_address: SocketAddress, url: String) -> Result<Self, VeilidAPIError> {
        let split_url = SplitUrl::from_str(&url)
            .map_err(|e| parse_error!(format!("unable to split WSS url: {}", e), url))?;
        if split_url.scheme != "wss" || !url.starts_with("wss://") {
            return Err(parse_error!("incorrect scheme for WSS dialinfo", url));
        }
        let url_port = split_url.port.unwrap_or(443u16);
        if url_port != socket_address.port() {
            return Err(parse_error!(
                "socket address port doesn't match url port",
                url
            ));
        }
        if !Address::from_str(&split_url.host).is_err() {
            return Err(parse_error!(
                "WSS url can not use address format, only hostname format",
                url
            ));
        }
        Ok(Self::WSS(DialInfoWSS {
            socket_address: socket_address.to_canonical(),
            request: url[6..].to_string(),
        }))
    }
    pub fn protocol_type(&self) -> ProtocolType {
        match self {
            Self::UDP(_) => ProtocolType::UDP,
            Self::TCP(_) => ProtocolType::TCP,
            Self::WS(_) => ProtocolType::WS,
            Self::WSS(_) => ProtocolType::WSS,
        }
    }
    pub fn protocol_network_type(&self) -> ProtocolNetworkType {
        match self {
            Self::UDP(di) => match di.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::UDPv4,
                Address::IPV6(_) => ProtocolNetworkType::UDPv6,
            },
            Self::TCP(di) => match di.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::TCPv4,
                Address::IPV6(_) => ProtocolNetworkType::TCPv6,
            },
            Self::WS(di) => match di.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::TCPv4,
                Address::IPV6(_) => ProtocolNetworkType::TCPv6,
            },
            Self::WSS(di) => match di.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::TCPv4,
                Address::IPV6(_) => ProtocolNetworkType::TCPv6,
            },
        }
    }
    pub fn socket_address(&self) -> SocketAddress {
        match self {
            Self::UDP(di) => di.socket_address,
            Self::TCP(di) => di.socket_address,
            Self::WS(di) => di.socket_address,
            Self::WSS(di) => di.socket_address,
        }
    }
    pub fn to_ip_addr(&self) -> IpAddr {
        match self {
            Self::UDP(di) => di.socket_address.to_ip_addr(),
            Self::TCP(di) => di.socket_address.to_ip_addr(),
            Self::WS(di) => di.socket_address.to_ip_addr(),
            Self::WSS(di) => di.socket_address.to_ip_addr(),
        }
    }
    pub fn port(&self) -> u16 {
        match self {
            Self::UDP(di) => di.socket_address.port,
            Self::TCP(di) => di.socket_address.port,
            Self::WS(di) => di.socket_address.port,
            Self::WSS(di) => di.socket_address.port,
        }
    }
    pub fn to_socket_addr(&self) -> SocketAddr {
        match self {
            Self::UDP(di) => di.socket_address.to_socket_addr(),
            Self::TCP(di) => di.socket_address.to_socket_addr(),
            Self::WS(di) => di.socket_address.to_socket_addr(),
            Self::WSS(di) => di.socket_address.to_socket_addr(),
        }
    }
    pub fn request(&self) -> Option<String> {
        match self {
            Self::UDP(_) => None,
            Self::TCP(_) => None,
            Self::WS(di) => Some(format!("ws://{}", di.request)),
            Self::WSS(di) => Some(format!("wss://{}", di.request)),
        }
    }
    pub fn is_public(&self) -> bool {
        self.socket_address().address().is_public()
    }

    pub fn is_private(&self) -> bool {
        self.socket_address().address().is_private()
    }

    pub fn is_valid(&self) -> bool {
        let socket_address = self.socket_address();
        let address = socket_address.address();
        let port = socket_address.port();
        (address.is_public() || address.is_private()) && port > 0
    }
}

//////////////////////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug)]
pub enum PeerScope {
    All,
    Global,
    Local,
}

#[derive(Clone, Debug, Default)]
pub struct PeerInfo {
    pub node_id: NodeId,
    pub dial_infos: Vec<DialInfo>,
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct PeerAddress {
    pub socket_address: SocketAddress,
    pub protocol_type: ProtocolType,
}

impl PeerAddress {
    pub fn new(socket_address: SocketAddress, protocol_type: ProtocolType) -> Self {
        Self {
            socket_address,
            protocol_type,
        }
    }

    pub fn to_socket_addr(&self) -> SocketAddr {
        self.socket_address.to_socket_addr()
    }

    pub fn protocol_network_type(&self) -> ProtocolNetworkType {
        match self.protocol_type {
            ProtocolType::UDP => match self.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::UDPv4,
                Address::IPV6(_) => ProtocolNetworkType::UDPv6,
            },
            ProtocolType::TCP => match self.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::TCPv4,
                Address::IPV6(_) => ProtocolNetworkType::TCPv6,
            },
            ProtocolType::WS => match self.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::TCPv4,
                Address::IPV6(_) => ProtocolNetworkType::TCPv6,
            },
            ProtocolType::WSS => match self.socket_address.address() {
                Address::IPV4(_) => ProtocolNetworkType::TCPv4,
                Address::IPV6(_) => ProtocolNetworkType::TCPv6,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionDescriptor {
    pub remote: PeerAddress,
    pub local: Option<SocketAddr>,
}

impl ConnectionDescriptor {
    pub fn new(remote: PeerAddress, local: SocketAddr) -> Self {
        Self {
            remote,
            local: Some(local),
        }
    }
    pub fn new_no_local(remote: PeerAddress) -> Self {
        Self {
            remote,
            local: None,
        }
    }
    pub fn protocol_type(&self) -> ProtocolType {
        self.remote.protocol_type
    }
    pub fn protocol_network_type(&self) -> ProtocolNetworkType {
        self.remote.protocol_network_type()
    }
}

//////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug, Default, Eq, PartialEq, PartialOrd, Ord)]
pub struct NodeDialInfoSingle {
    pub node_id: NodeId,
    pub dial_info: DialInfo,
}

impl fmt::Display for NodeDialInfoSingle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}@{}", self.node_id, self.dial_info)
    }
}

impl FromStr for NodeDialInfoSingle {
    type Err = VeilidAPIError;
    fn from_str(s: &str) -> Result<NodeDialInfoSingle, VeilidAPIError> {
        // split out node id from the dial info
        let (node_id_str, rest) = s.split_once('@').ok_or_else(|| {
            parse_error!(
                "NodeDialInfoSingle::from_str missing @ node id separator",
                s
            )
        })?;

        // parse out node id
        let node_id = NodeId::new(DHTKey::try_decode(node_id_str).map_err(|e| {
            parse_error!(
                format!("NodeDialInfoSingle::from_str couldn't parse node id: {}", e),
                s
            )
        })?);
        // parse out dial info
        let dial_info = DialInfo::from_str(rest)?;

        // return completed NodeDialInfoSingle
        Ok(NodeDialInfoSingle { node_id, dial_info })
    }
}

#[derive(Clone, Debug, Default)]
pub struct LatencyStats {
    pub fastest: u64, // fastest latency in the ROLLING_LATENCIES_SIZE last latencies
    pub average: u64, // average latency over the ROLLING_LATENCIES_SIZE last latencies
    pub slowest: u64, // slowest latency in the ROLLING_LATENCIES_SIZE last latencies
}

#[derive(Clone, Debug, Default)]
pub struct TransferStatsDownUp {
    pub down: TransferStats,
    pub up: TransferStats,
}

#[derive(Clone, Debug, Default)]
pub struct TransferStats {
    pub total: u64,   // total amount transferred ever
    pub maximum: u64, // maximum rate over the ROLLING_TRANSFERS_SIZE last amounts
    pub average: u64, // average rate over the ROLLING_TRANSFERS_SIZE last amounts
    pub minimum: u64, // minimum rate over the ROLLING_TRANSFERS_SIZE last amounts
}

#[derive(Clone, Debug, Default)]
pub struct PingStats {
    pub in_flight: u32,         // number of pings issued that have yet to be answered
    pub total_sent: u32,        // number of pings that have been sent in the total_time range
    pub total_returned: u32, // number of pings that have been returned by the node in the total_time range
    pub consecutive_pongs: u32, // number of pongs that have been received and returned consecutively without a lost ping
    pub last_pinged: Option<u64>, // when the peer was last pinged
    pub first_consecutive_pong_time: Option<u64>, // the timestamp of the first pong in a series of consecutive pongs
    pub recent_lost_pings: u32, // number of pings that have been lost since we lost reliability
}

#[derive(Clone, Debug, Default)]
pub struct PeerStats {
    pub time_added: u64,               // when the peer was added to the routing table
    pub last_seen: Option<u64>, // when the peer was last seen for any reason, including when we first attempted to reach out to it
    pub ping_stats: PingStats,  // information about pings
    pub latency: Option<LatencyStats>, // latencies for communications with the peer
    pub transfer: TransferStatsDownUp, // Stats for communications with the peer
    pub node_info: Option<NodeInfo>, // Last known node info
}

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        pub type ValueChangeCallback =
            Arc<dyn Fn(ValueKey, Vec<u8>) -> SystemPinBoxFuture<()> + 'static>;
    } else {
        pub type ValueChangeCallback =
            Arc<dyn Fn(ValueKey, Vec<u8>) -> SystemPinBoxFuture<()> + Send + Sync + 'static>;
    }
}

#[derive(Clone, Debug, PartialOrd, PartialEq, Eq, Ord)]
pub enum TunnelMode {
    Raw,
    Turn,
}

type TunnelId = u64;

#[derive(Clone, Debug)]
pub struct TunnelEndpoint {
    pub node_id: NodeId,          // the node id of the tunnel endpoint
    pub dial_info: Vec<DialInfo>, // multiple ways of how to get to the node
    pub mode: TunnelMode,
}

impl Default for TunnelEndpoint {
    fn default() -> Self {
        Self {
            node_id: NodeId::default(),
            dial_info: Vec::new(),
            mode: TunnelMode::Raw,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FullTunnel {
    pub id: TunnelId,
    pub timeout: u64,
    pub local: TunnelEndpoint,
    pub remote: TunnelEndpoint,
}

#[derive(Clone, Debug, Default)]
pub struct PartialTunnel {
    pub id: TunnelId,
    pub timeout: u64,
    pub local: TunnelEndpoint,
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug, Default)]
pub struct RouteHopSpec {
    pub dial_info: NodeDialInfoSingle,
}

#[derive(Clone, Debug, Default)]
pub struct PrivateRouteSpec {
    //
    pub public_key: DHTKey,
    pub secret_key: DHTKeySecret,
    pub hops: Vec<RouteHopSpec>,
}

#[derive(Clone, Debug, Default)]
pub struct SafetyRouteSpec {
    pub public_key: DHTKey,
    pub secret_key: DHTKeySecret,
    pub hops: Vec<RouteHopSpec>,
}

impl SafetyRouteSpec {
    pub fn new() -> Self {
        let (pk, sk) = generate_secret();
        SafetyRouteSpec {
            public_key: pk,
            secret_key: sk,
            hops: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct RoutingContextOptions {
    pub safety_route_spec: Option<SafetyRouteSpec>,
    pub private_route_spec: Option<PrivateRouteSpec>,
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Clone, Debug, Default)]
pub struct SearchDHTAnswer {
    pub node_id: NodeId,
    pub dial_info: Vec<DialInfo>,
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

pub struct RoutingContextInner {
    api: VeilidAPI,
    options: RoutingContextOptions,
}

impl Drop for RoutingContextInner {
    fn drop(&mut self) {
        // self.api
        //     .borrow_mut()
        //     .routing_contexts
        //     //.remove(&self.id);
    }
}

#[derive(Clone)]
pub struct RoutingContext {
    inner: Arc<Mutex<RoutingContextInner>>,
}

impl RoutingContext {
    fn new(api: VeilidAPI, options: RoutingContextOptions) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RoutingContextInner { api, options })),
        }
    }

    pub fn api(&self) -> VeilidAPI {
        self.inner.lock().api.clone()
    }

    ///////////////////////////////////
    ///

    pub async fn get_value(&self, _value_key: ValueKey) -> Result<Vec<u8>, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn set_value(
        &self,
        _value_key: ValueKey,
        _value: Vec<u8>,
    ) -> Result<bool, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn watch_value(
        &self,
        _value_key: ValueKey,
        _callback: ValueChangeCallback,
    ) -> Result<bool, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn cancel_watch_value(&self, _value_key: ValueKey) -> Result<bool, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn find_block(&self, _block_id: BlockId) -> Result<Vec<u8>, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn supply_block(&self, _block_id: BlockId) -> Result<bool, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn signal(&self, _data: Vec<u8>) -> Result<bool, VeilidAPIError> {
        panic!("unimplemented");
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////

struct VeilidAPIInner {
    core: Option<VeilidCore>,
}

impl fmt::Debug for VeilidAPIInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VeilidAPIInner: {}",
            match self.core {
                Some(_) => "active",
                None => "shutdown",
            }
        )
    }
}

impl Drop for VeilidAPIInner {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            intf::spawn_local(core.shutdown()).detach();
        }
    }
}

#[derive(Clone, Debug)]
pub struct VeilidAPI {
    inner: Arc<Mutex<VeilidAPIInner>>,
}

#[derive(Clone, Debug, Default)]
pub struct VeilidAPIWeak {
    inner: Weak<Mutex<VeilidAPIInner>>,
}

impl VeilidAPIWeak {
    pub fn upgrade(&self) -> Option<VeilidAPI> {
        self.inner.upgrade().map(|v| VeilidAPI { inner: v })
    }
}

impl VeilidAPI {
    pub(crate) fn new(core: VeilidCore) -> Self {
        Self {
            inner: Arc::new(Mutex::new(VeilidAPIInner { core: Some(core) })),
        }
    }
    pub fn weak(&self) -> VeilidAPIWeak {
        VeilidAPIWeak {
            inner: Arc::downgrade(&self.inner),
        }
    }
    fn core(&self) -> Result<VeilidCore, VeilidAPIError> {
        Ok(self
            .inner
            .lock()
            .core
            .as_ref()
            .ok_or(VeilidAPIError::Shutdown)?
            .clone())
    }
    fn config(&self) -> Result<VeilidConfig, VeilidAPIError> {
        Ok(self.core()?.config())
    }
    fn attachment_manager(&self) -> Result<AttachmentManager, VeilidAPIError> {
        Ok(self.core()?.attachment_manager())
    }
    fn network_manager(&self) -> Result<NetworkManager, VeilidAPIError> {
        Ok(self.attachment_manager()?.network_manager())
    }
    fn rpc_processor(&self) -> Result<RPCProcessor, VeilidAPIError> {
        Ok(self.network_manager()?.rpc_processor())
    }

    pub async fn shutdown(self) {
        let core = { self.inner.lock().core.take() };
        if let Some(core) = core {
            core.shutdown().await;
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.inner.lock().core.is_none()
    }

    ////////////////////////////////////////////////////////////////
    // Attach/Detach

    // issue state changed updates for updating clients
    pub async fn send_state_update(&self) -> Result<(), VeilidAPIError> {
        trace!("VeilidCore::send_state_update");
        let attachment_manager = self.attachment_manager()?;
        attachment_manager.send_state_update().await;
        Ok(())
    }

    // connect to the network
    pub async fn attach(&self) -> Result<(), VeilidAPIError> {
        trace!("VeilidCore::attach");
        let attachment_manager = self.attachment_manager()?;
        attachment_manager.request_attach().await;
        Ok(())
    }

    // disconnect from the network
    pub async fn detach(&self) -> Result<(), VeilidAPIError> {
        trace!("VeilidCore::detach");
        let attachment_manager = self.attachment_manager()?;
        attachment_manager.request_detach().await;
        Ok(())
    }

    // wait for state change
    // xxx: this should not use 'sleep', perhaps this function should be eliminated anyway
    // xxx: it should really only be used for test anyway, and there is probably a better way to do this regardless
    // xxx: that doesn't wait forever and can time out
    pub async fn wait_for_state(&self, state: VeilidState) -> Result<(), VeilidAPIError> {
        loop {
            intf::sleep(500).await;
            match state {
                VeilidState::Attachment(cs) => {
                    if self.attachment_manager()?.get_state() == cs {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    ////////////////////////////////////////////////////////////////
    // Direct Node Access (pretty much for testing only)

    pub async fn info(&self, node_id: NodeId) -> Result<InfoAnswer, VeilidAPIError> {
        let rpc = self.rpc_processor()?;
        let routing_table = rpc.routing_table();
        let node_ref = match routing_table.lookup_node_ref(node_id.key) {
            None => return Err(VeilidAPIError::NodeNotFound(node_id)),
            Some(nr) => nr,
        };
        let info_answer = rpc
            .rpc_call_info(node_ref)
            .await
            .map_err(map_rpc_error!())?;
        Ok(info_answer)
    }

    pub async fn validate_dial_info(
        &self,
        node_id: NodeId,
        dial_info: DialInfo,
        redirect: bool,
        alternate_port: bool,
    ) -> Result<bool, VeilidAPIError> {
        let rpc = self.rpc_processor()?;
        let routing_table = rpc.routing_table();
        let node_ref = match routing_table.lookup_node_ref(node_id.key) {
            None => return Err(VeilidAPIError::NodeNotFound(node_id)),
            Some(nr) => nr,
        };
        rpc.rpc_call_validate_dial_info(node_ref.clone(), dial_info, redirect, alternate_port)
            .await
            .map_err(map_rpc_error!())
    }

    pub async fn search_dht(&self, node_id: NodeId) -> Result<SearchDHTAnswer, VeilidAPIError> {
        let rpc_processor = self.rpc_processor()?;
        let config = self.config()?;
        let (count, fanout, timeout) = {
            let c = config.get();
            (
                c.network.dht.resolve_node_count,
                c.network.dht.resolve_node_fanout,
                c.network.dht.resolve_node_timeout,
            )
        };

        let node_ref = rpc_processor
            .search_dht_single_key(node_id.key, count, fanout, timeout)
            .await
            .map_err(map_rpc_error!())?;

        let answer = node_ref.operate(|e| SearchDHTAnswer {
            node_id: NodeId::new(node_ref.node_id()),
            dial_info: e.dial_info(),
        });

        Ok(answer)
    }

    pub async fn search_dht_multi(
        &self,
        node_id: NodeId,
    ) -> Result<Vec<SearchDHTAnswer>, VeilidAPIError> {
        let rpc_processor = self.rpc_processor()?;
        let config = self.config()?;
        let (count, fanout, timeout) = {
            let c = config.get();
            (
                c.network.dht.resolve_node_count,
                c.network.dht.resolve_node_fanout,
                c.network.dht.resolve_node_timeout,
            )
        };

        let node_refs = rpc_processor
            .search_dht_multi_key(node_id.key, count, fanout, timeout)
            .await
            .map_err(map_rpc_error!())?;

        let mut answer = Vec::<SearchDHTAnswer>::new();
        for nr in node_refs {
            let a = nr.operate(|e| SearchDHTAnswer {
                node_id: NodeId::new(nr.node_id()),
                dial_info: e.dial_info(),
            });
            answer.push(a);
        }

        Ok(answer)
    }

    ////////////////////////////////////////////////////////////////
    // Safety / Private Route Handling

    pub async fn new_safety_route_spec(
        &self,
        _hops: u8,
    ) -> Result<SafetyRouteSpec, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn new_private_route_spec(
        &self,
        _hops: u8,
    ) -> Result<PrivateRouteSpec, VeilidAPIError> {
        panic!("unimplemented");
    }

    ////////////////////////////////////////////////////////////////
    // Routing Contexts
    //
    // Safety route specified here is for _this_ node's anonymity as a sender, used via the 'route' operation
    // Private route specified here is for _this_ node's anonymity as a receiver, passed out via the 'respond_to' field for replies

    pub async fn safe_private(
        &self,
        safety_route_spec: SafetyRouteSpec,
        private_route_spec: PrivateRouteSpec,
    ) -> RoutingContext {
        self.routing_context(RoutingContextOptions {
            safety_route_spec: Some(safety_route_spec),
            private_route_spec: Some(private_route_spec),
        })
        .await
    }

    pub async fn safe_public(&self, safety_route_spec: SafetyRouteSpec) -> RoutingContext {
        self.routing_context(RoutingContextOptions {
            safety_route_spec: Some(safety_route_spec),
            private_route_spec: None,
        })
        .await
    }

    pub async fn unsafe_private(&self, private_route_spec: PrivateRouteSpec) -> RoutingContext {
        self.routing_context(RoutingContextOptions {
            safety_route_spec: None,
            private_route_spec: Some(private_route_spec),
        })
        .await
    }

    pub async fn unsafe_public(&self) -> RoutingContext {
        self.routing_context(RoutingContextOptions {
            safety_route_spec: None,
            private_route_spec: None,
        })
        .await
    }
    pub async fn routing_context(&self, options: RoutingContextOptions) -> RoutingContext {
        RoutingContext::new(self.clone(), options)
    }

    ////////////////////////////////////////////////////////////////
    // Tunnel Building

    pub async fn start_tunnel(
        &self,
        _endpoint_mode: TunnelMode,
        _depth: u8,
    ) -> Result<PartialTunnel, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn complete_tunnel(
        &self,
        _endpoint_mode: TunnelMode,
        _depth: u8,
        _partial_tunnel: PartialTunnel,
    ) -> Result<FullTunnel, VeilidAPIError> {
        panic!("unimplemented");
    }

    pub async fn cancel_tunnel(&self, _tunnel_id: TunnelId) -> Result<bool, VeilidAPIError> {
        panic!("unimplemented");
    }
}
