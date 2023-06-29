use crate::client_api_connection::*;
use crate::settings::Settings;
use crate::tools::*;
use crate::ui::*;
use indent::indent_all_by;
use std::net::SocketAddr;
use std::time::SystemTime;
use veilid_tools::*;

pub fn convert_loglevel(s: &str) -> Result<String, String> {
    match s.to_ascii_lowercase().as_str() {
        "off" => Ok("Off".to_owned()),
        "error" => Ok("Error".to_owned()),
        "warn" => Ok("Warn".to_owned()),
        "info" => Ok("Info".to_owned()),
        "debug" => Ok("Debug".to_owned()),
        "trace" => Ok("Trace".to_owned()),
        _ => Err(format!("Invalid log level: {}", s)),
    }
}

#[derive(PartialEq, Clone)]
pub enum ConnectionState {
    Disconnected,
    Connected(SocketAddr, SystemTime),
    Retrying(SocketAddr, SystemTime),
}
impl ConnectionState {
    pub fn is_disconnected(&self) -> bool {
        matches!(*self, Self::Disconnected)
    }
    pub fn is_connected(&self) -> bool {
        matches!(*self, Self::Connected(_, _))
    }
    pub fn is_retrying(&self) -> bool {
        matches!(*self, Self::Retrying(_, _))
    }
}

struct CommandProcessorInner {
    ui_sender: UISender,
    capi: Option<ClientApiConnection>,
    reconnect: bool,
    finished: bool,
    autoconnect: bool,
    autoreconnect: bool,
    server_addr: Option<SocketAddr>,
    connection_waker: Eventual,
    last_call_id: Option<u64>,
    enable_app_messages: bool,
}

#[derive(Clone)]
pub struct CommandProcessor {
    inner: Arc<Mutex<CommandProcessorInner>>,
}

impl CommandProcessor {
    pub fn new(ui_sender: UISender, settings: &Settings) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CommandProcessorInner {
                ui_sender,
                capi: None,
                reconnect: settings.autoreconnect,
                finished: false,
                autoconnect: settings.autoconnect,
                autoreconnect: settings.autoreconnect,
                server_addr: None,
                connection_waker: Eventual::new(),
                last_call_id: None,
                enable_app_messages: false,
            })),
        }
    }
    pub fn set_client_api_connection(&self, capi: ClientApiConnection) {
        self.inner.lock().capi = Some(capi);
    }
    fn inner(&self) -> MutexGuard<CommandProcessorInner> {
        self.inner.lock()
    }
    fn inner_mut(&self) -> MutexGuard<CommandProcessorInner> {
        self.inner.lock()
    }
    fn ui_sender(&self) -> UISender {
        self.inner.lock().ui_sender.clone()
    }
    fn capi(&self) -> ClientApiConnection {
        self.inner.lock().capi.as_ref().unwrap().clone()
    }

    fn word_split(line: &str) -> (String, Option<String>) {
        let trimmed = line.trim();
        if let Some(p) = trimmed.find(char::is_whitespace) {
            let first = trimmed[0..p].to_owned();
            let rest = trimmed[p..].trim_start().to_owned();
            (first, Some(rest))
        } else {
            (trimmed.to_owned(), None)
        }
    }

    pub fn cancel_command(&self) {
        trace!("CommandProcessor::cancel_command");
        let capi = self.capi();
        capi.cancel_all();
    }

    pub fn cmd_help(&self, _rest: Option<String>, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_help");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local(async move {
            let out = match capi.server_debug("help".to_owned()).await {
                Err(e) => {
                    error!("Server command 'debug help' failed: {}", e);
                    ui.send_callback(callback);
                    return;
                }
                Ok(v) => v,
            };

            ui.add_node_event(
                Level::Info,
                format!(
                    r#"Client Commands:
    exit/quit                           exit the client
    disconnect                          disconnect the client from the Veilid node 
    shutdown                            shut the server down
    change_log_level <layer> <level>    change the log level for a tracing layer
                                        layers include: 
                                            all, terminal, system, api, file, otlp
                                        levels include:
                                            error, warn, info, debug, trace
    enable [flag]                       set a flag
    disable [flag]                      unset a flag
                                        valid flags in include:
                                            app_messages
Server Debug Commands:
{}
"#,
                    indent_all_by(4, out)
                ),
            );
            ui.send_callback(callback);
        });
        Ok(())
    }

    pub fn cmd_exit(&self, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_exit");
        let ui = self.ui_sender();
        ui.send_callback(callback);
        ui.quit();
        Ok(())
    }

    pub fn cmd_shutdown(&self, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_shutdown");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local(async move {
            if let Err(e) = capi.server_shutdown().await {
                error!("Server command 'shutdown' failed to execute: {}", e);
            }
            ui.send_callback(callback);
        });
        Ok(())
    }

    pub fn cmd_disconnect(&self, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_disconnect");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local(async move {
            capi.disconnect().await;
            ui.send_callback(callback);
        });
        Ok(())
    }

    pub fn cmd_debug(&self, command_line: String, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_debug");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local(async move {
            match capi.server_debug(command_line).await {
                Ok(output) => {
                    ui.add_node_event(Level::Info, output);
                    ui.send_callback(callback);
                }
                Err(e) => {
                    ui.add_node_event(Level::Error, e.to_string());
                    ui.send_callback(callback);
                }
            }
        });
        Ok(())
    }

    pub fn cmd_change_log_level(
        &self,
        rest: Option<String>,
        callback: UICallback,
    ) -> Result<(), String> {
        trace!("CommandProcessor::cmd_change_log_level");
        let capi = self.capi();
        let ui = self.ui_sender();
        spawn_detached_local(async move {
            let (layer, rest) = Self::word_split(&rest.unwrap_or_default());
            let log_level = match convert_loglevel(&rest.unwrap_or_default()) {
                Ok(v) => v,
                Err(e) => {
                    ui.add_node_event(Level::Error, format!("Failed to change log level: {}", e));
                    ui.send_callback(callback);
                    return;
                }
            };

            match capi.server_change_log_level(layer, log_level).await {
                Ok(()) => {
                    ui.display_string_dialog("Success", "Log level changed", callback);
                }
                Err(e) => {
                    ui.display_string_dialog(
                        "Server command 'change_log_level' failed",
                        e.to_string(),
                        callback,
                    );
                }
            }
        });
        Ok(())
    }

    pub fn cmd_enable(&self, rest: Option<String>, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_enable");

        let ui = self.ui_sender();
        let this = self.clone();
        spawn_detached_local(async move {
            let flag = rest.clone().unwrap_or_default();
            match flag.as_str() {
                "app_messages" => {
                    this.inner.lock().enable_app_messages = true;
                    ui.add_node_event(Level::Info, format!("flag enabled: {}", flag));
                    ui.send_callback(callback);
                }
                _ => {
                    ui.add_node_event(Level::Error, format!("unknown flag: {}", flag));
                    ui.send_callback(callback);
                    return;
                }
            }
        });
        Ok(())
    }

    pub fn cmd_disable(&self, rest: Option<String>, callback: UICallback) -> Result<(), String> {
        trace!("CommandProcessor::cmd_disable");

        let ui = self.ui_sender();
        let this = self.clone();
        spawn_detached_local(async move {
            let flag = rest.clone().unwrap_or_default();
            match flag.as_str() {
                "app_messages" => {
                    this.inner.lock().enable_app_messages = false;
                    ui.add_node_event(Level::Info, format!("flag disabled: {}", flag));
                    ui.send_callback(callback);
                }
                _ => {
                    ui.add_node_event(Level::Error, format!("unknown flag: {}", flag));
                    ui.send_callback(callback);
                    return;
                }
            }
        });
        Ok(())
    }

    pub fn run_command(&self, command_line: &str, callback: UICallback) -> Result<(), String> {
        //
        let (cmd, rest) = Self::word_split(command_line);
        match cmd.as_str() {
            "help" => self.cmd_help(rest, callback),
            "exit" => self.cmd_exit(callback),
            "quit" => self.cmd_exit(callback),
            "disconnect" => self.cmd_disconnect(callback),
            "shutdown" => self.cmd_shutdown(callback),
            "change_log_level" => self.cmd_change_log_level(rest, callback),
            "enable" => self.cmd_enable(rest, callback),
            "disable" => self.cmd_disable(rest, callback),
            _ => self.cmd_debug(command_line.to_owned(), callback),
        }
    }

    pub async fn connection_manager(&self) {
        // Connect until we're done
        while !self.inner_mut().finished {
            // Wait for connection request
            if !self.inner().autoconnect {
                let waker = self.inner_mut().connection_waker.instance_clone(());
                waker.await;
            } else {
                self.inner_mut().autoconnect = false;
            }
            self.inner_mut().connection_waker.reset();
            // Loop while we want to keep the connection
            let mut first = true;
            while self.inner().reconnect {
                let server_addr_opt = self.inner_mut().server_addr;
                let server_addr = match server_addr_opt {
                    None => break,
                    Some(addr) => addr,
                };
                if first {
                    info!("Connecting to server at {}", server_addr);
                    self.set_connection_state(ConnectionState::Retrying(
                        server_addr,
                        SystemTime::now(),
                    ));
                } else {
                    debug!("Retrying connection to {}", server_addr);
                }
                let capi = self.capi();
                let res = capi.connect(server_addr).await;
                if res.is_ok() {
                    info!(
                        "Connection to server at {} terminated normally",
                        server_addr
                    );
                    break;
                }
                if !self.inner().autoreconnect {
                    info!("Connection to server lost.");
                    break;
                }

                self.set_connection_state(ConnectionState::Retrying(
                    server_addr,
                    SystemTime::now(),
                ));

                debug!("Connection lost, retrying in 2 seconds");
                {
                    let waker = self.inner_mut().connection_waker.instance_clone(());
                    let _ = timeout(2000, waker).await;
                }
                self.inner_mut().connection_waker.reset();
                first = false;
            }
            info!("Disconnected.");
            self.set_connection_state(ConnectionState::Disconnected);
            self.inner_mut().reconnect = true;
        }
    }

    // called by ui
    ////////////////////////////////////////////
    pub fn set_server_address(&self, server_addr: Option<SocketAddr>) {
        self.inner_mut().server_addr = server_addr;
    }
    pub fn get_server_address(&self) -> Option<SocketAddr> {
        self.inner().server_addr
    }
    // called by client_api_connection
    // calls into ui
    ////////////////////////////////////////////

    pub fn log_message(&self, log_level: Level, message: String) {
        self.inner().ui_sender.add_node_event(log_level, message);
    }

    pub fn update_attachment(&self, attachment: &json::JsonValue) {
        self.inner_mut().ui_sender.set_attachment_state(
            attachment["state"].as_str().unwrap_or_default().to_owned(),
            attachment["public_internet_ready"]
                .as_bool()
                .unwrap_or_default(),
            attachment["local_network_ready"]
                .as_bool()
                .unwrap_or_default(),
        );
    }

    pub fn update_network_status(&self, network: &json::JsonValue) {
        self.inner_mut().ui_sender.set_network_status(
            network["started"].as_bool().unwrap_or_default(),
            json_str_u64(&network["bps_down"]),
            json_str_u64(&network["bps_up"]),
            network["peers"]
                .members()
                .cloned()
                .collect::<Vec<json::JsonValue>>(),
        );
    }
    pub fn update_config(&self, config: &json::JsonValue) {
        self.inner_mut().ui_sender.set_config(&config["config"])
    }
    pub fn update_route(&self, route: &json::JsonValue) {
        let mut out = String::new();
        if route["dead_routes"].len() != 0 {
            out.push_str(&format!("Dead routes: {:?}", route["dead_routes"]));
        }
        if route["dead_routes"].len() != 0 {
            if !out.is_empty() {
                out.push_str("\n");
            }
            out.push_str(&format!(
                "Dead remote routes: {:?}",
                route["dead_remote_routes"]
            ));
        }
        if !out.is_empty() {
            self.inner().ui_sender.add_node_event(Level::Info, out);
        }
    }
    pub fn update_value_change(&self, value_change: &json::JsonValue) {
        let out = format!("Value change: {:?}", value_change.as_str().unwrap_or("???"));
        self.inner().ui_sender.add_node_event(Level::Info, out);
    }

    pub fn update_log(&self, log: &json::JsonValue) {
        let log_level =
            Level::from_str(log["log_level"].as_str().unwrap_or("error")).unwrap_or(Level::Error);
        self.inner().ui_sender.add_node_event(
            log_level,
            format!(
                "{}: {}{}",
                log["log_level"].as_str().unwrap_or("???"),
                log["message"].as_str().unwrap_or("???"),
                if let Some(bt) = log["backtrace"].as_str() {
                    format!("\nBacktrace:\n{}", bt)
                } else {
                    "".to_owned()
                }
            ),
        );
    }

    pub fn update_app_message(&self, msg: &json::JsonValue) {
        if !self.inner.lock().enable_app_messages {
            return;
        }

        let message = json_str_vec_u8(&msg["message"]);

        // check is message body is ascii printable
        let mut printable = true;
        for c in &message {
            if *c < 32 || *c > 126 {
                printable = false;
            }
        }

        let (message, truncated) = if message.len() > 64 {
            (&message[0..64], true)
        } else {
            (&message[..], false)
        };

        let strmsg = if printable {
            format!("\"{}\"", String::from_utf8_lossy(&message).to_string())
        } else {
            hex::encode(message)
        };

        self.inner().ui_sender.add_node_event(
            Level::Info,
            format!(
                "AppMessage ({:?}): {}{}",
                msg["sender"],
                strmsg,
                if truncated { "..." } else { "" }
            ),
        );
    }

    pub fn update_app_call(&self, call: &json::JsonValue) {
        if !self.inner.lock().enable_app_messages {
            return;
        }

        let message = json_str_vec_u8(&call["message"]);

        // check is message body is ascii printable
        let mut printable = true;
        for c in &message {
            if *c < 32 || *c > 126 {
                printable = false;
            }
        }

        let (message, truncated) = if message.len() > 64 {
            (&message[0..64], true)
        } else {
            (&message[..], false)
        };

        let strmsg = if printable {
            format!("\"{}\"", String::from_utf8_lossy(&message).to_string())
        } else {
            hex::encode(message)
        };

        let id = json_str_u64(&call["call_id"]);

        self.inner().ui_sender.add_node_event(
            Level::Info,
            format!(
                "AppCall ({:?}) id = {:016x} : {}{}",
                call["sender"],
                id,
                strmsg,
                if truncated { "..." } else { "" }
            ),
        );

        self.inner_mut().last_call_id = Some(id);
    }

    pub fn update_shutdown(&self) {
        // Do nothing with this, we'll process shutdown when rpc connection closes
    }

    // called by client_api_connection
    // calls into ui
    ////////////////////////////////////////////
    pub fn set_connection_state(&self, state: ConnectionState) {
        self.inner_mut().ui_sender.set_connection_state(state);
    }
    // called by ui
    ////////////////////////////////////////////
    pub fn start_connection(&self) {
        self.inner_mut().reconnect = true;
        self.inner_mut().connection_waker.resolve();
    }
    // pub fn stop_connection(&self) {
    //     self.inner_mut().reconnect = false;
    //     let mut capi = self.capi().clone();
    //     spawn_detached(async move {
    //         capi.disconnect().await;
    //     });
    // }
    pub fn cancel_reconnect(&self) {
        self.inner_mut().reconnect = false;
        self.inner_mut().connection_waker.resolve();
    }
    pub fn quit(&self) {
        self.inner_mut().finished = true;
        self.inner_mut().reconnect = false;
        self.inner_mut().connection_waker.resolve();
    }

    // called by ui
    // calls into client_api_connection
    ////////////////////////////////////////////
    pub fn attach(&self) {
        let capi = self.capi();

        spawn_detached_local(async move {
            if let Err(e) = capi.server_attach().await {
                error!("Server command 'attach' failed to execute: {}", e);
            }
        });
    }

    pub fn detach(&self) {
        let capi = self.capi();

        spawn_detached_local(async move {
            if let Err(e) = capi.server_detach().await {
                error!("Server command 'detach' failed to execute: {}", e);
            }
        });
    }
}
