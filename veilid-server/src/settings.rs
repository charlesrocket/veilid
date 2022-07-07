#![allow(clippy::bool_assert_comparison)]

use crate::*;

use directories::*;
use parking_lot::*;

use serde_derive::*;
use std::ffi::OsStr;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use url::Url;
use veilid_core::xx::*;
use veilid_core::*;

pub fn load_default_config() -> EyreResult<config::Config> {
    let default_config = String::from(
        r#"---
daemon:
    enabled: false
client_api:
    enabled: true
    listen_address: 'localhost:5959'
auto_attach: true
logging:
    system:
        enabled: false
        level: 'info'
    terminal:
        enabled: true
        level: 'info'
    file: 
        enabled: false
        path: ''
        append: true
        level: 'info'
    api:
        enabled: true
        level: 'info'
    otlp:
        enabled: false
        level: 'trace'
        grpc_endpoint: 'localhost:4317'
testing:
    subnode_index: 0
core:
    protected_store:
        allow_insecure_fallback: true
        always_use_insecure_storage: true
        insecure_fallback_directory: '%INSECURE_FALLBACK_DIRECTORY%'
        delete: false
    table_store:
        directory: '%TABLE_STORE_DIRECTORY%'
        delete: false
    block_store:
        directory: '%BLOCK_STORE_DIRECTORY%'
        delete: false
    network:
        connection_initial_timeout_ms: 2000
        connection_inactivity_timeout_ms: 60000
        max_connections_per_ip4: 8
        max_connections_per_ip6_prefix: 8
        max_connections_per_ip6_prefix_size: 56
        max_connection_frequency_per_min: 8
        client_whitelist_timeout_ms: 300000 
        reverse_connection_receipt_time_ms: 5000 
        hole_punch_receipt_time_ms: 5000 
        node_id: ''
        node_id_secret: ''
        bootstrap: ['bootstrap-dev.veilid.net']
        bootstrap_nodes: []
        routing_table:
            limit_over_attached: 64
            limit_fully_attached: 32
            limit_attached_strong: 16
            limit_attached_good: 8
            limit_attached_weak: 4
        rpc: 
            concurrency: 0
            queue_size: 1024
            max_timestamp_behind_ms: 10000
            max_timestamp_ahead_ms: 10000
            timeout_ms: 10000
            max_route_hop_count: 7
        dht:
            resolve_node_timeout:
            resolve_node_count: 20
            resolve_node_fanout: 3
            max_find_node_count: 20
            get_value_timeout:
            get_value_count: 20
            get_value_fanout: 3
            set_value_timeout:
            set_value_count: 20
            set_value_fanout: 5
            min_peer_count: 20
            min_peer_refresh_time_ms: 2000
            validate_dial_info_receipt_time_ms: 2000
        upnp: false
        natpmp: false
        enable_local_peer_scope: false
        restricted_nat_retries: 0
        tls:
            certificate_path: '%CERTIFICATE_PATH%'
            private_key_path: '%PRIVATE_KEY_PATH%'
            connection_initial_timeout_ms: 2000
        application:
            https:
                enabled: false
                listen_address: ':5150'
                path: 'app'
                # url: 'https://localhost:5150'
            http:
                enabled: false
                listen_address: ':5150'
                path: 'app'
                # url: 'http://localhost:5150'
        protocol:
            udp:
                enabled: true
                socket_pool_size: 0
                listen_address: ':5150'
                # public_address: ''
            tcp:
                connect: true
                listen: true
                max_connections: 32
                listen_address: ':5150'
                #'public_address: ''
            ws:
                connect: true
                listen: true
                max_connections: 16
                listen_address: ':5150'
                path: 'ws'
                # url: 'ws://localhost:5150/ws'
            wss:
                connect: true
                listen: false
                max_connections: 16
                listen_address: ':5150'
                path: 'ws'
                # url: ''
        "#,
    )
    .replace(
        "%TABLE_STORE_DIRECTORY%",
        &Settings::get_default_table_store_path().to_string_lossy(),
    )
    .replace(
        "%BLOCK_STORE_DIRECTORY%",
        &Settings::get_default_block_store_path().to_string_lossy(),
    )
    .replace(
        "%INSECURE_FALLBACK_DIRECTORY%",
        &Settings::get_default_protected_store_insecure_fallback_directory().to_string_lossy(),
    )
    .replace(
        "%CERTIFICATE_PATH%",
        &Settings::get_default_certificate_directory()
            .join("server.crt")
            .to_string_lossy(),
    )
    .replace(
        "%PRIVATE_KEY_PATH%",
        &Settings::get_default_private_key_directory()
            .join("server.key")
            .to_string_lossy(),
    );
    config::Config::builder()
        .add_source(config::File::from_str(
            &default_config,
            config::FileFormat::Yaml,
        ))
        .build()
        .wrap_err("failed to parse default config")
}

pub fn load_config(cfg: config::Config, config_file: &Path) -> EyreResult<config::Config> {
    if let Some(config_file_str) = config_file.to_str() {
        config::Config::builder()
            .add_source(cfg)
            .add_source(config::File::new(config_file_str, config::FileFormat::Yaml))
            .build()
            .wrap_err("failed to load config")
    } else {
        bail!("config file path is not valid UTF-8")
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
impl<'de> serde::Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_ascii_lowercase().as_str() {
            "off" => Ok(LogLevel::Off),
            "error" => Ok(LogLevel::Error),
            "warn" => Ok(LogLevel::Warn),
            "info" => Ok(LogLevel::Info),
            "debug" => Ok(LogLevel::Debug),
            "trace" => Ok(LogLevel::Trace),
            _ => Err(serde::de::Error::custom(format!(
                "Invalid log level: {}",
                s
            ))),
        }
    }
}
impl serde::Serialize for LogLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            LogLevel::Off => "off",
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };
        s.serialize(serializer)
    }
}

pub fn convert_loglevel(log_level: LogLevel) -> veilid_core::VeilidConfigLogLevel {
    match log_level {
        LogLevel::Off => veilid_core::VeilidConfigLogLevel::Off,
        LogLevel::Error => veilid_core::VeilidConfigLogLevel::Error,
        LogLevel::Warn => veilid_core::VeilidConfigLogLevel::Warn,
        LogLevel::Info => veilid_core::VeilidConfigLogLevel::Info,
        LogLevel::Debug => veilid_core::VeilidConfigLogLevel::Debug,
        LogLevel::Trace => veilid_core::VeilidConfigLogLevel::Trace,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUrl {
    pub urlstring: String,
    pub url: Url,
}

impl ParsedUrl {
    pub fn offset_port(&mut self, offset: u16) -> EyreResult<()> {
        // Bump port on url
        self.url
            .set_port(Some(self.url.port().unwrap() + offset))
            .map_err(|_| eyre!("failed to set port on url"))?;
        self.urlstring = self.url.to_string();
        Ok(())
    }
}

impl FromStr for ParsedUrl {
    type Err = url::ParseError;
    fn from_str(s: &str) -> Result<ParsedUrl, url::ParseError> {
        let mut url = Url::parse(s)?;
        if url.scheme().to_lowercase() == "http" && url.port().is_none() {
            url.set_port(Some(80))
                .map_err(|_| url::ParseError::InvalidPort)?
        }
        if url.scheme().to_lowercase() == "https" && url.port().is_none() {
            url.set_port(Some(443))
                .map_err(|_| url::ParseError::InvalidPort)?;
        }
        let parsed_urlstring = url.to_string();
        Ok(Self {
            urlstring: parsed_urlstring,
            url,
        })
    }
}

impl<'de> serde::Deserialize<'de> for ParsedUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ParsedUrl::from_str(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for ParsedUrl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.urlstring.serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedNodeDialInfo {
    pub node_dial_info_string: String,
    pub node_dial_info: veilid_core::NodeDialInfo,
}

// impl ParsedNodeDialInfo {
//     pub fn offset_port(&mut self, offset: u16) -> Result<(), ()> {
//         // Bump port on dial_info
//         self.node_dial_info
//             .dial_info
//             .set_port(self.node_dial_info.dial_info.port() + 1);
//         self.node_dial_info_string = self.node_dial_info.to_string();
//         Ok(())
//     }
// }

impl FromStr for ParsedNodeDialInfo {
    type Err = veilid_core::VeilidAPIError;
    fn from_str(
        node_dial_info_string: &str,
    ) -> Result<ParsedNodeDialInfo, veilid_core::VeilidAPIError> {
        let node_dial_info = veilid_core::NodeDialInfo::from_str(node_dial_info_string)?;
        Ok(Self {
            node_dial_info_string: node_dial_info_string.to_owned(),
            node_dial_info,
        })
    }
}

impl<'de> serde::Deserialize<'de> for ParsedNodeDialInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        ParsedNodeDialInfo::from_str(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for ParsedNodeDialInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.node_dial_info_string.serialize(serializer)
    }
}

#[derive(Debug, PartialEq)]
pub struct NamedSocketAddrs {
    pub name: String,
    pub addrs: Vec<SocketAddr>,
}

impl FromStr for NamedSocketAddrs {
    type Err = std::io::Error;
    fn from_str(s: &str) -> Result<NamedSocketAddrs, std::io::Error> {
        let addr_iter = listen_address_to_socket_addrs(s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        Ok(NamedSocketAddrs {
            name: s.to_owned(),
            addrs: addr_iter,
        })
    }
}

impl<'de> serde::Deserialize<'de> for NamedSocketAddrs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NamedSocketAddrs::from_str(s.as_str()).map_err(serde::de::Error::custom)
    }
}

impl serde::Serialize for NamedSocketAddrs {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.name.serialize(serializer)
    }
}

impl NamedSocketAddrs {
    pub fn offset_port(&mut self, offset: u16) -> EyreResult<()> {
        // Bump port on name
        if let Some(split) = self.name.rfind(':') {
            let hoststr = &self.name[0..split];
            let portstr = &self.name[split + 1..];
            let port: u16 = portstr.parse::<u16>().wrap_err("failed to parse port")? + offset;

            self.name = format!("{}:{}", hoststr, port);
        } else {
            bail!("no port specified to offset");
        }

        // Bump port on addresses
        for addr in self.addrs.iter_mut() {
            addr.set_port(addr.port() + offset);
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Terminal {
    pub enabled: bool,
    pub level: LogLevel,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct File {
    pub enabled: bool,
    pub path: String,
    pub append: bool,
    pub level: LogLevel,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct System {
    pub enabled: bool,
    pub level: LogLevel,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Api {
    pub enabled: bool,
    pub level: LogLevel,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Otlp {
    pub enabled: bool,
    pub level: LogLevel,
    pub grpc_endpoint: NamedSocketAddrs,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ClientApi {
    pub enabled: bool,
    pub listen_address: NamedSocketAddrs,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Logging {
    pub system: System,
    pub terminal: Terminal,
    pub file: File,
    pub api: Api,
    pub otlp: Otlp,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Https {
    pub enabled: bool,
    pub listen_address: NamedSocketAddrs,
    pub path: PathBuf,
    pub url: Option<ParsedUrl>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Http {
    pub enabled: bool,
    pub listen_address: NamedSocketAddrs,
    pub path: PathBuf,
    pub url: Option<ParsedUrl>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Application {
    pub https: Https,
    pub http: Http,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Udp {
    pub enabled: bool,
    pub socket_pool_size: u32,
    pub listen_address: NamedSocketAddrs,
    pub public_address: Option<NamedSocketAddrs>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Tcp {
    pub connect: bool,
    pub listen: bool,
    pub max_connections: u32,
    pub listen_address: NamedSocketAddrs,
    pub public_address: Option<NamedSocketAddrs>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Ws {
    pub connect: bool,
    pub listen: bool,
    pub max_connections: u32,
    pub listen_address: NamedSocketAddrs,
    pub path: PathBuf,
    pub url: Option<ParsedUrl>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Wss {
    pub connect: bool,
    pub listen: bool,
    pub max_connections: u32,
    pub listen_address: NamedSocketAddrs,
    pub path: PathBuf,
    pub url: Option<ParsedUrl>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Protocol {
    pub udp: Udp,
    pub tcp: Tcp,
    pub ws: Ws,
    pub wss: Wss,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Tls {
    pub certificate_path: PathBuf,
    pub private_key_path: PathBuf,
    pub connection_initial_timeout_ms: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Rpc {
    pub concurrency: u32,
    pub queue_size: u32,
    pub max_timestamp_behind_ms: Option<u32>,
    pub max_timestamp_ahead_ms: Option<u32>,
    pub timeout_ms: u32,
    pub max_route_hop_count: u8,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Dht {
    pub resolve_node_timeout_ms: Option<u32>,
    pub resolve_node_count: u32,
    pub resolve_node_fanout: u32,
    pub max_find_node_count: u32,
    pub get_value_timeout_ms: Option<u32>,
    pub get_value_count: u32,
    pub get_value_fanout: u32,
    pub set_value_timeout_ms: Option<u32>,
    pub set_value_count: u32,
    pub set_value_fanout: u32,
    pub min_peer_count: u32,
    pub min_peer_refresh_time_ms: u32,
    pub validate_dial_info_receipt_time_ms: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RoutingTable {
    pub limit_over_attached: u32,
    pub limit_fully_attached: u32,
    pub limit_attached_strong: u32,
    pub limit_attached_good: u32,
    pub limit_attached_weak: u32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Network {
    pub connection_initial_timeout_ms: u32,
    pub connection_inactivity_timeout_ms: u32,
    pub max_connections_per_ip4: u32,
    pub max_connections_per_ip6_prefix: u32,
    pub max_connections_per_ip6_prefix_size: u32,
    pub max_connection_frequency_per_min: u32,
    pub client_whitelist_timeout_ms: u32,
    pub reverse_connection_receipt_time_ms: u32,
    pub hole_punch_receipt_time_ms: u32,
    pub node_id: veilid_core::DHTKey,
    pub node_id_secret: veilid_core::DHTKeySecret,
    pub bootstrap: Vec<String>,
    pub bootstrap_nodes: Vec<ParsedNodeDialInfo>,
    pub routing_table: RoutingTable,
    pub rpc: Rpc,
    pub dht: Dht,
    pub upnp: bool,
    pub natpmp: bool,
    pub enable_local_peer_scope: bool,
    pub restricted_nat_retries: u32,
    pub tls: Tls,
    pub application: Application,
    pub protocol: Protocol,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Testing {
    pub subnode_index: u16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TableStore {
    pub directory: PathBuf,
    pub delete: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockStore {
    pub directory: PathBuf,
    pub delete: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProtectedStore {
    pub allow_insecure_fallback: bool,
    pub always_use_insecure_storage: bool,
    pub insecure_fallback_directory: PathBuf,
    pub delete: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Core {
    pub protected_store: ProtectedStore,
    pub table_store: TableStore,
    pub block_store: BlockStore,
    pub network: Network,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Daemon {
    pub enabled: bool,
    pub pid_file: Option<String>,
    pub chroot: Option<String>,
    pub working_directory: Option<String>,
    pub user: Option<String>,
    pub group: Option<String>,
    pub stdout_file: Option<String>,
    pub stderr_file: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SettingsInner {
    pub daemon: Daemon,
    pub client_api: ClientApi,
    pub auto_attach: bool,
    pub logging: Logging,
    pub testing: Testing,
    pub core: Core,
}

#[derive(Clone, Debug)]
pub struct Settings {
    inner: Arc<RwLock<SettingsInner>>,
}

impl Settings {
    pub fn new(config_file: Option<&OsStr>) -> EyreResult<Self> {
        // Load the default config
        let mut cfg = load_default_config()?;

        // Merge in the config file if we have one
        if let Some(config_file) = config_file {
            let config_file_path = Path::new(config_file);
            // If the user specifies a config file on the command line then it must exist
            cfg = load_config(cfg, config_file_path)?;
        }

        // Generate config
        let inner: SettingsInner = cfg.try_deserialize()?;

        //
        Ok(Self {
            inner: Arc::new(RwLock::new(inner)),
        })
    }
    pub fn read(&self) -> RwLockReadGuard<SettingsInner> {
        self.inner.read()
    }
    pub fn write(&self) -> RwLockWriteGuard<SettingsInner> {
        self.inner.write()
    }

    pub fn apply_subnode_index(&self) -> EyreResult<()> {
        let mut settingsrw = self.write();
        let idx = settingsrw.testing.subnode_index;
        if idx == 0 {
            return Ok(());
        }

        // bump client api port
        (*settingsrw).client_api.listen_address.offset_port(idx)?;

        // bump protocol ports
        (*settingsrw)
            .core
            .network
            .protocol
            .udp
            .listen_address
            .offset_port(idx)?;
        (*settingsrw)
            .core
            .network
            .protocol
            .tcp
            .listen_address
            .offset_port(idx)?;
        (*settingsrw)
            .core
            .network
            .protocol
            .ws
            .listen_address
            .offset_port(idx)?;
        if let Some(url) = &mut (*settingsrw).core.network.protocol.ws.url {
            url.offset_port(idx)?;
        }
        (*settingsrw)
            .core
            .network
            .protocol
            .wss
            .listen_address
            .offset_port(idx)?;
        if let Some(url) = &mut (*settingsrw).core.network.protocol.wss.url {
            url.offset_port(idx)?;
        }
        // bump application ports
        (*settingsrw)
            .core
            .network
            .application
            .http
            .listen_address
            .offset_port(idx)?;
        if let Some(url) = &mut (*settingsrw).core.network.application.http.url {
            url.offset_port(idx)?;
        }
        (*settingsrw)
            .core
            .network
            .application
            .https
            .listen_address
            .offset_port(idx)?;
        if let Some(url) = &mut (*settingsrw).core.network.application.https.url {
            url.offset_port(idx)?;
        }
        Ok(())
    }

    pub fn get_default_config_path() -> PathBuf {
        #[cfg(unix)]
        {
            let globalpath = PathBuf::from("/etc/veilid-server/veilid-server.conf");
            if globalpath.exists() {
                return globalpath;
            }
        }

        let mut cfg_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid")
        {
            PathBuf::from(my_proj_dirs.config_dir())
        } else {
            PathBuf::from("./")
        };
        cfg_path.push("veilid-server.conf");

        cfg_path
    }

    pub fn get_default_table_store_path() -> PathBuf {
        #[cfg(unix)]
        {
            let globalpath = PathBuf::from("/var/db/veilid-server/table_store");
            if globalpath.exists() {
                return globalpath;
            }
        }

        let mut ts_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid") {
            PathBuf::from(my_proj_dirs.data_local_dir())
        } else {
            PathBuf::from("./")
        };
        ts_path.push("table_store");

        ts_path
    }

    pub fn get_default_block_store_path() -> PathBuf {
        #[cfg(unix)]
        {
            let globalpath = PathBuf::from("/var/db/veilid-server/block_store");
            if globalpath.exists() {
                return globalpath;
            }
        }

        let mut bs_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid") {
            PathBuf::from(my_proj_dirs.data_local_dir())
        } else {
            PathBuf::from("./")
        };
        bs_path.push("block_store");

        bs_path
    }

    pub fn get_default_protected_store_insecure_fallback_directory() -> PathBuf {
        #[cfg(unix)]
        {
            let globalpath = PathBuf::from("/var/db/veilid-server/protected_store");
            if globalpath.exists() {
                return globalpath;
            }
        }

        let mut ps_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid") {
            PathBuf::from(my_proj_dirs.data_local_dir())
        } else {
            PathBuf::from("./")
        };
        ps_path.push("protected_store");

        ps_path
    }

    pub fn get_default_certificate_directory() -> PathBuf {
        #[cfg(unix)]
        {
            let mut globalpath = PathBuf::from("/etc/veilid-server");
            if globalpath.exists() {
                globalpath.push("ssl");
                globalpath.push("certs");
                return globalpath;
            }
        }

        let mut c_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid") {
            PathBuf::from(my_proj_dirs.data_local_dir())
        } else {
            PathBuf::from("./")
        };
        c_path.push("ssl");
        c_path.push("certs");
        c_path
    }

    pub fn get_default_private_key_directory() -> PathBuf {
        #[cfg(unix)]
        {
            let mut globalpath = PathBuf::from("/etc/veilid-server");
            if globalpath.exists() {
                globalpath.push("ssl");
                globalpath.push("keys");
                return globalpath;
            }
        }

        let mut pk_path = if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "Veilid") {
            PathBuf::from(my_proj_dirs.data_local_dir())
        } else {
            PathBuf::from("./")
        };
        pk_path.push("ssl");
        pk_path.push("keys");
        pk_path
    }

    pub fn set(&self, key: &str, value: &str) -> EyreResult<()> {
        let mut inner = self.inner.write();

        macro_rules! set_config_value {
            ($innerkey:expr, $value:expr) => {{
                let innerkeyname = &stringify!($innerkey)[6..];
                if innerkeyname == key {
                    match veilid_core::deserialize_json(value) {
                        Ok(v) => {
                            $innerkey = v;
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(eyre!(
                                "invalid type for key {}, value: {}: {}",
                                key,
                                value,
                                e
                            ))
                        }
                    }
                }
            }};
        }
        set_config_value!(inner.daemon.enabled, value);
        set_config_value!(inner.client_api.enabled, value);
        set_config_value!(inner.client_api.listen_address, value);
        set_config_value!(inner.auto_attach, value);
        set_config_value!(inner.logging.system.enabled, value);
        set_config_value!(inner.logging.system.level, value);
        set_config_value!(inner.logging.terminal.enabled, value);
        set_config_value!(inner.logging.terminal.level, value);
        set_config_value!(inner.logging.file.enabled, value);
        set_config_value!(inner.logging.file.path, value);
        set_config_value!(inner.logging.file.append, value);
        set_config_value!(inner.logging.file.level, value);
        set_config_value!(inner.logging.api.enabled, value);
        set_config_value!(inner.logging.api.level, value);
        set_config_value!(inner.logging.otlp.enabled, value);
        set_config_value!(inner.logging.otlp.level, value);
        set_config_value!(inner.logging.otlp.grpc_endpoint, value);
        set_config_value!(inner.testing.subnode_index, value);
        set_config_value!(inner.core.protected_store.allow_insecure_fallback, value);
        set_config_value!(
            inner.core.protected_store.always_use_insecure_storage,
            value
        );
        set_config_value!(
            inner.core.protected_store.insecure_fallback_directory,
            value
        );
        set_config_value!(inner.core.protected_store.delete, value);
        set_config_value!(inner.core.table_store.directory, value);
        set_config_value!(inner.core.table_store.delete, value);
        set_config_value!(inner.core.block_store.directory, value);
        set_config_value!(inner.core.block_store.delete, value);
        set_config_value!(inner.core.network.connection_initial_timeout_ms, value);
        set_config_value!(inner.core.network.connection_inactivity_timeout_ms, value);
        set_config_value!(inner.core.network.max_connections_per_ip4, value);
        set_config_value!(inner.core.network.max_connections_per_ip6_prefix, value);
        set_config_value!(
            inner.core.network.max_connections_per_ip6_prefix_size,
            value
        );
        set_config_value!(inner.core.network.max_connection_frequency_per_min, value);
        set_config_value!(inner.core.network.client_whitelist_timeout_ms, value);
        set_config_value!(inner.core.network.reverse_connection_receipt_time_ms, value);
        set_config_value!(inner.core.network.hole_punch_receipt_time_ms, value);
        set_config_value!(inner.core.network.node_id, value);
        set_config_value!(inner.core.network.node_id_secret, value);
        set_config_value!(inner.core.network.bootstrap, value);
        set_config_value!(inner.core.network.bootstrap_nodes, value);
        set_config_value!(inner.core.network.routing_table.limit_over_attached, value);
        set_config_value!(inner.core.network.routing_table.limit_fully_attached, value);
        set_config_value!(
            inner.core.network.routing_table.limit_attached_strong,
            value
        );
        set_config_value!(inner.core.network.routing_table.limit_attached_good, value);
        set_config_value!(inner.core.network.routing_table.limit_attached_weak, value);
        set_config_value!(inner.core.network.rpc.concurrency, value);
        set_config_value!(inner.core.network.rpc.queue_size, value);
        set_config_value!(inner.core.network.rpc.max_timestamp_behind_ms, value);
        set_config_value!(inner.core.network.rpc.max_timestamp_ahead_ms, value);
        set_config_value!(inner.core.network.rpc.timeout_ms, value);
        set_config_value!(inner.core.network.rpc.max_route_hop_count, value);
        set_config_value!(inner.core.network.dht.resolve_node_timeout_ms, value);
        set_config_value!(inner.core.network.dht.resolve_node_count, value);
        set_config_value!(inner.core.network.dht.resolve_node_fanout, value);
        set_config_value!(inner.core.network.dht.max_find_node_count, value);
        set_config_value!(inner.core.network.dht.get_value_timeout_ms, value);
        set_config_value!(inner.core.network.dht.get_value_count, value);
        set_config_value!(inner.core.network.dht.get_value_fanout, value);
        set_config_value!(inner.core.network.dht.set_value_timeout_ms, value);
        set_config_value!(inner.core.network.dht.set_value_count, value);
        set_config_value!(inner.core.network.dht.set_value_fanout, value);
        set_config_value!(inner.core.network.dht.min_peer_count, value);
        set_config_value!(inner.core.network.dht.min_peer_refresh_time_ms, value);
        set_config_value!(
            inner.core.network.dht.validate_dial_info_receipt_time_ms,
            value
        );
        set_config_value!(inner.core.network.upnp, value);
        set_config_value!(inner.core.network.natpmp, value);
        set_config_value!(inner.core.network.enable_local_peer_scope, value);
        set_config_value!(inner.core.network.restricted_nat_retries, value);
        set_config_value!(inner.core.network.tls.certificate_path, value);
        set_config_value!(inner.core.network.tls.private_key_path, value);
        set_config_value!(inner.core.network.tls.connection_initial_timeout_ms, value);
        set_config_value!(inner.core.network.application.https.enabled, value);
        set_config_value!(inner.core.network.application.https.listen_address, value);
        set_config_value!(inner.core.network.application.https.path, value);
        set_config_value!(inner.core.network.application.https.url, value);
        set_config_value!(inner.core.network.application.http.enabled, value);
        set_config_value!(inner.core.network.application.http.listen_address, value);
        set_config_value!(inner.core.network.application.http.path, value);
        set_config_value!(inner.core.network.application.http.url, value);
        set_config_value!(inner.core.network.protocol.udp.enabled, value);
        set_config_value!(inner.core.network.protocol.udp.socket_pool_size, value);
        set_config_value!(inner.core.network.protocol.udp.listen_address, value);
        set_config_value!(inner.core.network.protocol.udp.public_address, value);
        set_config_value!(inner.core.network.protocol.tcp.connect, value);
        set_config_value!(inner.core.network.protocol.tcp.listen, value);
        set_config_value!(inner.core.network.protocol.tcp.max_connections, value);
        set_config_value!(inner.core.network.protocol.tcp.listen_address, value);
        set_config_value!(inner.core.network.protocol.tcp.public_address, value);
        set_config_value!(inner.core.network.protocol.ws.connect, value);
        set_config_value!(inner.core.network.protocol.ws.listen, value);
        set_config_value!(inner.core.network.protocol.ws.max_connections, value);
        set_config_value!(inner.core.network.protocol.ws.listen_address, value);
        set_config_value!(inner.core.network.protocol.ws.path, value);
        set_config_value!(inner.core.network.protocol.ws.url, value);
        set_config_value!(inner.core.network.protocol.wss.connect, value);
        set_config_value!(inner.core.network.protocol.wss.listen, value);
        set_config_value!(inner.core.network.protocol.wss.max_connections, value);
        set_config_value!(inner.core.network.protocol.wss.listen_address, value);
        set_config_value!(inner.core.network.protocol.wss.path, value);
        set_config_value!(inner.core.network.protocol.wss.url, value);
        Err(eyre!("settings key not found"))
    }

    pub fn get_core_config_callback(&self) -> veilid_core::ConfigCallback {
        let inner = self.inner.clone();

        Arc::new(move |key: String| {
            let inner = inner.read();
            let out: ConfigCallbackReturn = match key.as_str() {
                "program_name" => Ok(Box::new("veilid-server".to_owned())),
                "namespace" => Ok(Box::new(if inner.testing.subnode_index == 0 {
                    "".to_owned()
                } else {
                    format!("subnode{}", inner.testing.subnode_index)
                })),
                "capabilities.protocol_udp" => Ok(Box::new(true)),
                "capabilities.protocol_connect_tcp" => Ok(Box::new(true)),
                "capabilities.protocol_accept_tcp" => Ok(Box::new(true)),
                "capabilities.protocol_connect_ws" => Ok(Box::new(true)),
                "capabilities.protocol_accept_ws" => Ok(Box::new(true)),
                "capabilities.protocol_connect_wss" => Ok(Box::new(true)),
                "capabilities.protocol_accept_wss" => Ok(Box::new(true)),
                "protected_store.allow_insecure_fallback" => {
                    Ok(Box::new(inner.core.protected_store.allow_insecure_fallback))
                }
                "protected_store.always_use_insecure_storage" => Ok(Box::new(
                    inner.core.protected_store.always_use_insecure_storage,
                )),
                "protected_store.insecure_fallback_directory" => Ok(Box::new(
                    inner
                        .core
                        .protected_store
                        .insecure_fallback_directory
                        .to_string_lossy()
                        .to_string(),
                )),
                "protected_store.delete" => Ok(Box::new(inner.core.protected_store.delete)),

                "table_store.directory" => Ok(Box::new(
                    inner
                        .core
                        .table_store
                        .directory
                        .to_string_lossy()
                        .to_string(),
                )),
                "table_store.delete" => Ok(Box::new(inner.core.table_store.delete)),

                "block_store.directory" => Ok(Box::new(
                    inner
                        .core
                        .block_store
                        .directory
                        .to_string_lossy()
                        .to_string(),
                )),
                "block_store.delete" => Ok(Box::new(inner.core.block_store.delete)),

                "network.connection_initial_timeout_ms" => {
                    Ok(Box::new(inner.core.network.connection_initial_timeout_ms))
                }
                "network.connection_inactivity_timeout_ms" => Ok(Box::new(
                    inner.core.network.connection_inactivity_timeout_ms,
                )),
                "network.max_connections_per_ip4" => {
                    Ok(Box::new(inner.core.network.max_connections_per_ip4))
                }
                "network.max_connections_per_ip6_prefix" => {
                    Ok(Box::new(inner.core.network.max_connections_per_ip6_prefix))
                }
                "network.max_connections_per_ip6_prefix_size" => Ok(Box::new(
                    inner.core.network.max_connections_per_ip6_prefix_size,
                )),
                "network.max_connection_frequency_per_min" => Ok(Box::new(
                    inner.core.network.max_connection_frequency_per_min,
                )),
                "network.client_whitelist_timeout_ms" => {
                    Ok(Box::new(inner.core.network.client_whitelist_timeout_ms))
                }
                "network.reverse_connection_receipt_time_ms" => Ok(Box::new(
                    inner.core.network.reverse_connection_receipt_time_ms,
                )),
                "network.hole_punch_receipt_time_ms" => {
                    Ok(Box::new(inner.core.network.hole_punch_receipt_time_ms))
                }
                "network.node_id" => Ok(Box::new(inner.core.network.node_id)),
                "network.node_id_secret" => Ok(Box::new(inner.core.network.node_id_secret)),
                "network.bootstrap" => Ok(Box::new(inner.core.network.bootstrap.clone())),
                "network.bootstrap_nodes" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .bootstrap_nodes
                        .clone()
                        .into_iter()
                        .map(|e| e.node_dial_info_string)
                        .collect::<Vec<String>>(),
                )),
                "network.routing_table.limit_over_attached" => Ok(Box::new(
                    inner.core.network.routing_table.limit_over_attached,
                )),
                "network.routing_table.limit_fully_attached" => Ok(Box::new(
                    inner.core.network.routing_table.limit_fully_attached,
                )),
                "network.routing_table.limit_attached_strong" => Ok(Box::new(
                    inner.core.network.routing_table.limit_attached_strong,
                )),
                "network.routing_table.limit_attached_good" => Ok(Box::new(
                    inner.core.network.routing_table.limit_attached_good,
                )),
                "network.routing_table.limit_attached_weak" => Ok(Box::new(
                    inner.core.network.routing_table.limit_attached_weak,
                )),
                "network.rpc.concurrency" => Ok(Box::new(inner.core.network.rpc.concurrency)),
                "network.rpc.queue_size" => Ok(Box::new(inner.core.network.rpc.queue_size)),
                "network.rpc.max_timestamp_behind_ms" => {
                    Ok(Box::new(inner.core.network.rpc.max_timestamp_behind_ms))
                }
                "network.rpc.max_timestamp_ahead_ms" => {
                    Ok(Box::new(inner.core.network.rpc.max_timestamp_ahead_ms))
                }
                "network.rpc.timeout_ms" => Ok(Box::new(inner.core.network.rpc.timeout_ms)),
                "network.rpc.max_route_hop_count" => {
                    Ok(Box::new(inner.core.network.rpc.max_route_hop_count))
                }
                "network.dht.resolve_node_timeout_ms" => {
                    Ok(Box::new(inner.core.network.dht.resolve_node_timeout_ms))
                }
                "network.dht.resolve_node_count" => {
                    Ok(Box::new(inner.core.network.dht.resolve_node_count))
                }
                "network.dht.resolve_node_fanout" => {
                    Ok(Box::new(inner.core.network.dht.resolve_node_fanout))
                }
                "network.dht.max_find_node_count" => {
                    Ok(Box::new(inner.core.network.dht.max_find_node_count))
                }
                "network.dht.get_value_timeout_ms" => {
                    Ok(Box::new(inner.core.network.dht.get_value_timeout_ms))
                }
                "network.dht.get_value_count" => {
                    Ok(Box::new(inner.core.network.dht.get_value_count))
                }
                "network.dht.get_value_fanout" => {
                    Ok(Box::new(inner.core.network.dht.get_value_fanout))
                }
                "network.dht.set_value_timeout_ms" => {
                    Ok(Box::new(inner.core.network.dht.set_value_timeout_ms))
                }
                "network.dht.set_value_count" => {
                    Ok(Box::new(inner.core.network.dht.set_value_count))
                }
                "network.dht.set_value_fanout" => {
                    Ok(Box::new(inner.core.network.dht.set_value_fanout))
                }
                "network.dht.min_peer_count" => Ok(Box::new(inner.core.network.dht.min_peer_count)),
                "network.dht.min_peer_refresh_time_ms" => {
                    Ok(Box::new(inner.core.network.dht.min_peer_refresh_time_ms))
                }
                "network.dht.validate_dial_info_receipt_time_ms" => Ok(Box::new(
                    inner.core.network.dht.validate_dial_info_receipt_time_ms,
                )),
                "network.upnp" => Ok(Box::new(inner.core.network.upnp)),
                "network.natpmp" => Ok(Box::new(inner.core.network.natpmp)),
                "network.enable_local_peer_scope" => {
                    Ok(Box::new(inner.core.network.enable_local_peer_scope))
                }
                "network.restricted_nat_retries" => {
                    Ok(Box::new(inner.core.network.restricted_nat_retries))
                }
                "network.tls.certificate_path" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .tls
                        .certificate_path
                        .to_string_lossy()
                        .to_string(),
                )),
                "network.tls.private_key_path" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .tls
                        .private_key_path
                        .to_string_lossy()
                        .to_string(),
                )),
                "network.tls.connection_initial_timeout_ms" => Ok(Box::new(
                    inner.core.network.tls.connection_initial_timeout_ms,
                )),
                "network.application.https.enabled" => {
                    Ok(Box::new(inner.core.network.application.https.enabled))
                }
                "network.application.https.listen_address" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .application
                        .https
                        .listen_address
                        .name
                        .clone(),
                )),
                "network.application.https.path" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .application
                        .https
                        .path
                        .to_string_lossy()
                        .to_string(),
                )),
                "network.application.https.url" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .application
                        .https
                        .url
                        .as_ref()
                        .map(|a| a.urlstring.clone()),
                )),
                "network.application.http.enabled" => {
                    Ok(Box::new(inner.core.network.application.http.enabled))
                }
                "network.application.http.listen_address" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .application
                        .http
                        .listen_address
                        .name
                        .clone(),
                )),
                "network.application.http.path" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .application
                        .http
                        .path
                        .to_string_lossy()
                        .to_string(),
                )),
                "network.application.http.url" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .application
                        .http
                        .url
                        .as_ref()
                        .map(|a| a.urlstring.clone()),
                )),
                "network.protocol.udp.enabled" => {
                    Ok(Box::new(inner.core.network.protocol.udp.enabled))
                }
                "network.protocol.udp.socket_pool_size" => {
                    Ok(Box::new(inner.core.network.protocol.udp.socket_pool_size))
                }
                "network.protocol.udp.listen_address" => Ok(Box::new(
                    inner.core.network.protocol.udp.listen_address.name.clone(),
                )),
                "network.protocol.udp.public_address" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .protocol
                        .udp
                        .public_address
                        .as_ref()
                        .map(|a| a.name.clone()),
                )),
                "network.protocol.tcp.connect" => {
                    Ok(Box::new(inner.core.network.protocol.tcp.connect))
                }
                "network.protocol.tcp.listen" => {
                    Ok(Box::new(inner.core.network.protocol.tcp.listen))
                }
                "network.protocol.tcp.max_connections" => {
                    Ok(Box::new(inner.core.network.protocol.tcp.max_connections))
                }
                "network.protocol.tcp.listen_address" => Ok(Box::new(
                    inner.core.network.protocol.tcp.listen_address.name.clone(),
                )),
                "network.protocol.tcp.public_address" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .protocol
                        .tcp
                        .public_address
                        .as_ref()
                        .map(|a| a.name.clone()),
                )),
                "network.protocol.ws.connect" => {
                    Ok(Box::new(inner.core.network.protocol.ws.connect))
                }
                "network.protocol.ws.listen" => Ok(Box::new(inner.core.network.protocol.ws.listen)),
                "network.protocol.ws.max_connections" => {
                    Ok(Box::new(inner.core.network.protocol.ws.max_connections))
                }
                "network.protocol.ws.listen_address" => Ok(Box::new(
                    inner.core.network.protocol.ws.listen_address.name.clone(),
                )),
                "network.protocol.ws.path" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .protocol
                        .ws
                        .path
                        .to_string_lossy()
                        .to_string(),
                )),
                "network.protocol.ws.url" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .protocol
                        .ws
                        .url
                        .as_ref()
                        .map(|a| a.urlstring.clone()),
                )),
                "network.protocol.wss.connect" => {
                    Ok(Box::new(inner.core.network.protocol.wss.connect))
                }
                "network.protocol.wss.listen" => {
                    Ok(Box::new(inner.core.network.protocol.wss.listen))
                }
                "network.protocol.wss.max_connections" => {
                    Ok(Box::new(inner.core.network.protocol.wss.max_connections))
                }
                "network.protocol.wss.listen_address" => Ok(Box::new(
                    inner.core.network.protocol.wss.listen_address.name.clone(),
                )),
                "network.protocol.wss.path" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .protocol
                        .wss
                        .path
                        .to_string_lossy()
                        .to_string(),
                )),
                "network.protocol.wss.url" => Ok(Box::new(
                    inner
                        .core
                        .network
                        .protocol
                        .wss
                        .url
                        .as_ref()
                        .map(|a| a.urlstring.clone()),
                )),
                _ => Err(VeilidAPIError::generic(format!(
                    "config key '{}' doesn't exist",
                    key
                ))),
            };
            out
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_default_config() {
        let cfg = load_default_config().unwrap();
        let inner = cfg.try_deserialize::<SettingsInner>().unwrap();
        println!("default settings: {:?}", inner);
    }

    #[test]
    #[serial]
    fn test_default_config_settings() {
        let settings = Settings::new(None).unwrap();

        let s = settings.read();
        assert_eq!(s.daemon.enabled, false);
        assert_eq!(s.daemon.pid_file, None);
        assert_eq!(s.daemon.chroot, None);
        assert_eq!(s.daemon.working_directory, None);
        assert_eq!(s.daemon.user, None);
        assert_eq!(s.daemon.group, None);
        assert_eq!(s.daemon.stdout_file, None);
        assert_eq!(s.daemon.stderr_file, None);
        assert_eq!(s.client_api.enabled, true);
        assert_eq!(s.client_api.listen_address.name, "localhost:5959");
        assert_eq!(
            s.client_api.listen_address.addrs,
            listen_address_to_socket_addrs("localhost:5959").unwrap()
        );
        assert_eq!(s.auto_attach, true);
        assert_eq!(s.logging.system.enabled, false);
        assert_eq!(s.logging.system.level, LogLevel::Info);
        assert_eq!(s.logging.terminal.enabled, true);
        assert_eq!(s.logging.terminal.level, LogLevel::Info);
        assert_eq!(s.logging.file.enabled, false);
        assert_eq!(s.logging.file.path, "");
        assert_eq!(s.logging.file.append, true);
        assert_eq!(s.logging.file.level, LogLevel::Info);
        assert_eq!(s.logging.api.enabled, false);
        assert_eq!(s.logging.api.level, LogLevel::Info);
        assert_eq!(s.logging.otlp.enabled, false);
        assert_eq!(s.logging.otlp.level, LogLevel::Trace);
        assert_eq!(
            s.logging.otlp.grpc_endpoint,
            NamedSocketAddrs::from_str("localhost:4317").unwrap()
        );
        assert_eq!(s.testing.subnode_index, 0);

        assert_eq!(
            s.core.table_store.directory,
            Settings::get_default_table_store_path()
        );
        assert_eq!(s.core.table_store.delete, false);

        assert_eq!(
            s.core.block_store.directory,
            Settings::get_default_block_store_path()
        );
        assert_eq!(s.core.block_store.delete, false);

        assert_eq!(s.core.protected_store.allow_insecure_fallback, true);
        assert_eq!(s.core.protected_store.always_use_insecure_storage, true);
        assert_eq!(
            s.core.protected_store.insecure_fallback_directory,
            Settings::get_default_protected_store_insecure_fallback_directory()
        );
        assert_eq!(s.core.protected_store.delete, false);

        assert_eq!(s.core.network.connection_initial_timeout_ms, 2_000u32);
        assert_eq!(s.core.network.connection_inactivity_timeout_ms, 60_000u32);
        assert_eq!(s.core.network.max_connections_per_ip4, 8u32);
        assert_eq!(s.core.network.max_connections_per_ip6_prefix, 8u32);
        assert_eq!(s.core.network.max_connections_per_ip6_prefix_size, 56u32);
        assert_eq!(s.core.network.max_connection_frequency_per_min, 8u32);
        assert_eq!(s.core.network.client_whitelist_timeout_ms, 300_000u32);
        assert_eq!(s.core.network.reverse_connection_receipt_time_ms, 5_000u32);
        assert_eq!(s.core.network.hole_punch_receipt_time_ms, 5_000u32);
        assert_eq!(s.core.network.node_id, veilid_core::DHTKey::default());
        assert_eq!(
            s.core.network.node_id_secret,
            veilid_core::DHTKeySecret::default()
        );
        //
        assert_eq!(
            s.core.network.bootstrap,
            vec!["bootstrap-dev.veilid.net".to_owned()]
        );
        assert_eq!(s.core.network.bootstrap_nodes, vec![]);
        //
        assert_eq!(s.core.network.rpc.concurrency, 0);
        assert_eq!(s.core.network.rpc.queue_size, 1024);
        assert_eq!(s.core.network.rpc.max_timestamp_behind_ms, Some(10_000u32));
        assert_eq!(s.core.network.rpc.max_timestamp_ahead_ms, Some(10_000u32));
        assert_eq!(s.core.network.rpc.timeout_ms, 10_000u32);
        assert_eq!(s.core.network.rpc.max_route_hop_count, 7);
        //
        assert_eq!(s.core.network.dht.resolve_node_timeout_ms, None);
        assert_eq!(s.core.network.dht.resolve_node_count, 20u32);
        assert_eq!(s.core.network.dht.resolve_node_fanout, 3u32);
        assert_eq!(s.core.network.dht.max_find_node_count, 20u32);
        assert_eq!(s.core.network.dht.get_value_timeout_ms, None);
        assert_eq!(s.core.network.dht.get_value_count, 20u32);
        assert_eq!(s.core.network.dht.get_value_fanout, 3u32);
        assert_eq!(s.core.network.dht.set_value_timeout_ms, None);
        assert_eq!(s.core.network.dht.set_value_count, 20u32);
        assert_eq!(s.core.network.dht.set_value_fanout, 5u32);
        assert_eq!(s.core.network.dht.min_peer_count, 20u32);
        assert_eq!(s.core.network.dht.min_peer_refresh_time_ms, 2_000u32);
        assert_eq!(
            s.core.network.dht.validate_dial_info_receipt_time_ms,
            2_000u32
        );
        //
        assert_eq!(s.core.network.upnp, false);
        assert_eq!(s.core.network.natpmp, false);
        assert_eq!(s.core.network.enable_local_peer_scope, false);
        assert_eq!(s.core.network.restricted_nat_retries, 0u32);
        //
        assert_eq!(
            s.core.network.tls.certificate_path,
            Settings::get_default_certificate_directory().join("server.crt")
        );
        assert_eq!(
            s.core.network.tls.private_key_path,
            Settings::get_default_private_key_directory().join("server.key")
        );
        assert_eq!(s.core.network.tls.connection_initial_timeout_ms, 2_000u32);
        //
        assert_eq!(s.core.network.application.https.enabled, false);
        assert_eq!(
            s.core.network.application.https.listen_address.name,
            ":5150"
        );
        assert_eq!(
            s.core.network.application.https.listen_address.addrs,
            listen_address_to_socket_addrs(":5150").unwrap()
        );
        assert_eq!(
            s.core.network.application.https.path,
            std::path::PathBuf::from("app")
        );
        assert_eq!(s.core.network.application.https.url, None);
        assert_eq!(s.core.network.application.http.enabled, false);
        assert_eq!(s.core.network.application.http.listen_address.name, ":5150");
        assert_eq!(
            s.core.network.application.http.listen_address.addrs,
            listen_address_to_socket_addrs(":5150").unwrap()
        );
        assert_eq!(
            s.core.network.application.http.path,
            std::path::PathBuf::from("app")
        );
        assert_eq!(s.core.network.application.http.url, None);
        //
        assert_eq!(s.core.network.protocol.udp.enabled, true);
        assert_eq!(s.core.network.protocol.udp.socket_pool_size, 0);
        assert_eq!(s.core.network.protocol.udp.listen_address.name, ":5150");
        assert_eq!(
            s.core.network.protocol.udp.listen_address.addrs,
            listen_address_to_socket_addrs(":5150").unwrap()
        );
        assert_eq!(s.core.network.protocol.udp.public_address, None);

        //
        assert_eq!(s.core.network.protocol.tcp.connect, true);
        assert_eq!(s.core.network.protocol.tcp.listen, true);
        assert_eq!(s.core.network.protocol.tcp.max_connections, 32);
        assert_eq!(s.core.network.protocol.tcp.listen_address.name, ":5150");
        assert_eq!(
            s.core.network.protocol.tcp.listen_address.addrs,
            listen_address_to_socket_addrs(":5150").unwrap()
        );
        assert_eq!(s.core.network.protocol.tcp.public_address, None);

        //
        assert_eq!(s.core.network.protocol.ws.connect, true);
        assert_eq!(s.core.network.protocol.ws.listen, true);
        assert_eq!(s.core.network.protocol.ws.max_connections, 16);
        assert_eq!(s.core.network.protocol.ws.listen_address.name, ":5150");
        assert_eq!(
            s.core.network.protocol.ws.listen_address.addrs,
            listen_address_to_socket_addrs(":5150").unwrap()
        );
        assert_eq!(
            s.core.network.protocol.ws.path,
            std::path::PathBuf::from("ws")
        );
        assert_eq!(s.core.network.protocol.ws.url, None);
        //
        assert_eq!(s.core.network.protocol.wss.connect, true);
        assert_eq!(s.core.network.protocol.wss.listen, false);
        assert_eq!(s.core.network.protocol.wss.max_connections, 16);
        assert_eq!(s.core.network.protocol.wss.listen_address.name, ":5150");
        assert_eq!(
            s.core.network.protocol.wss.listen_address.addrs,
            listen_address_to_socket_addrs(":5150").unwrap()
        );
        assert_eq!(
            s.core.network.protocol.wss.path,
            std::path::PathBuf::from("ws")
        );
        assert_eq!(s.core.network.protocol.wss.url, None);
        //
    }
}
