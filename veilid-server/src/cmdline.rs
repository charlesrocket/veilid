use crate::settings::*;
use clap::{Arg, ArgMatches, Command};
use std::ffi::OsStr;
use std::path::Path;
use std::str::FromStr;
use veilid_core::{DHTKey, DHTKeySecret};

fn do_clap_matches(default_config_path: &OsStr) -> Result<clap::ArgMatches, clap::Error> {
    let matches = Command::new("veilid-server")
        .version("0.1")
        .about("Veilid Server")
        .color(clap::ColorChoice::Auto)
        .arg(
            Arg::new("daemon")
                .long("daemon")
                .short('d')
                .help("Run in daemon mode in the background"),
        )
        .arg(
            Arg::new("foreground")
                .long("foreground")
                .short('f')
                .conflicts_with("daemon")
                .help("Run in the foreground"),
        )
        .arg(
            Arg::new("config-file")
                .short('c')
                .long("config-file")
                .takes_value(true)
                .value_name("FILE")
                .default_value_os(default_config_path)
                .allow_invalid_utf8(true)
                .help("Specify a configuration file to use"),
        )
        .arg(
            Arg::new("set-config")
                .short('s')
                .long("set-config")
                .takes_value(true)
                .multiple_occurrences(true)
                .help("Specify configuration value to set (key in dot format, value in json format), eg: logging.api.enabled=true")
        )
        .arg(
            Arg::new("attach")
                .long("attach")
                .takes_value(true)
                .value_name("BOOL")
                .possible_values(&["false", "true"])
                .help("Automatically attach the server to the Veilid network"),
        )
        // Dev options
        .arg(
            Arg::new("debug")
                .long("debug")
                .help("Turn on debug logging on the terminal"),
        )
        .arg(
            Arg::new("trace")
                .long("trace")
                .conflicts_with("debug")
                .help("Turn on trace logging on the terminal"),
        )
        .arg(
            Arg::new("otlp")
                .long("otlp")
                .takes_value(true)
                .value_name("endpoint")
                .default_missing_value("localhost:4317")
                .help("Turn on OpenTelemetry tracing"),
        )
        .arg(
            Arg::new("subnode-index")
                .long("subnode-index")
                .takes_value(true)
                .help("Run as an extra daemon on the same machine for testing purposes, specify a number greater than zero to offset the listening ports"),
        )
        .arg(
            Arg::new("generate-dht-key")
                .long("generate-dht-key")
                .help("Only generate a new dht key and print it"),
        )
        .arg(
            Arg::new("set-node-id")
                .long("set-node-id")
                .takes_value(true)
                .value_name("ID")
                .help("Set the node id and secret key")
                .long_help("To specify both node id and secret key on the command line, use a ID:SECRET syntax with a colon, like:\n  zsVXz5aTU98vZxwTcDmvpcnO5g1B2jRO3wpdNiDrRgw:gJzQLmzuBvA-dFvEmLcYvLoO5bh7hzCWFzfpJHapZKg\nIf no colon is used, the node id is specified, and a prompt appears to enter the secret key interactively.")
        )
        .arg(
            Arg::new("delete-protected-store")
                .long("delete-protected-store")
                .help("Delete the entire contents of the protected store (DANGER, NO UNDO!)"),
        )
        .arg(
            Arg::new("delete-table-store")
                .long("delete-table-store")
                .help("Delete the entire contents of the table store (DANGER, NO UNDO!)"),
        )
        .arg(
            Arg::new("delete-block-store")
                .long("delete-block-store")
                .help("Delete the entire contents of the block store (DANGER, NO UNDO!)"),
        )
        .arg(
            Arg::new("dump-config")
                .long("dump-config")
                .help("Instead of running the server, print the configuration it would use to the console"),
        )
        .arg(
            Arg::new("dump-txt-record")
                .long("dump-txt-record")
                .help("Prints the bootstrap TXT record for this node and then quits")
        )
        .arg(
            Arg::new("bootstrap")
                .long("bootstrap")
                .takes_value(true)
                .value_name("BOOTSTRAP_LIST")
                .help("Specify a list of bootstrap hostnames to use")
        )
        .arg(
            Arg::new("bootstrap-nodes")
                .conflicts_with("bootstrap")
                .long("bootstrap-nodes")
                .takes_value(true)
                .value_name("BOOTSTRAP_NODE_LIST")
                .help("Specify a list of bootstrap node dialinfos to use"),
        )
        .arg(
            Arg::new("local")
                .long("local")
                .help("Enable local peer scope")
        );

    #[cfg(debug_assertions)]
    let matches = matches.arg(
        Arg::new("wait-for-debug")
            .long("wait-for-debug")
            .help("Wait for debugger to attach"),
    );

    Ok(matches.get_matches())
}

pub fn process_command_line() -> Result<(Settings, ArgMatches), String> {
    // Get command line options
    let default_config_path = Settings::get_default_config_path();
    let matches = do_clap_matches(default_config_path.as_os_str())
        .map_err(|e| format!("failed to parse command line: {}", e))?;

    // Check for one-off commands
    #[cfg(debug_assertions)]
    if matches.occurrences_of("wait-for-debug") != 0 {
        use bugsalot::debugger;
        debugger::wait_until_attached(None).expect("state() not implemented on this platform");
    }

    // Attempt to load configuration
    let settings_path = if let Some(config_file) = matches.value_of_os("config-file") {
        if Path::new(config_file).exists() {
            Some(config_file)
        } else {
            None
        }
    } else {
        None
    };

    let settings =
        Settings::new(settings_path).map_err(|e| format!("configuration is invalid: {}", e))?;

    // write lock the settings
    let mut settingsrw = settings.write();

    // Set config from command line
    if matches.occurrences_of("daemon") != 0 {
        settingsrw.daemon.enabled = true;
        settingsrw.logging.terminal.enabled = false;
    }
    if matches.occurrences_of("foreground") != 0 {
        settingsrw.daemon.enabled = false;
    }
    if matches.occurrences_of("subnode-index") != 0 {
        let subnode_index = match matches.value_of("subnode-index") {
            Some(x) => x
                .parse()
                .map_err(|e| format!("couldn't parse subnode index: {}", e))?,
            None => {
                return Err("value not specified for subnode-index".to_owned());
            }
        };
        if subnode_index == 0 {
            return Err("value of subnode_index should be between 1 and 65535".to_owned());
        }
        settingsrw.testing.subnode_index = subnode_index;
    }

    if matches.occurrences_of("debug") != 0 {
        settingsrw.logging.terminal.enabled = true;
        settingsrw.logging.terminal.level = LogLevel::Debug;
    }
    if matches.occurrences_of("trace") != 0 {
        settingsrw.logging.terminal.enabled = true;
        settingsrw.logging.terminal.level = LogLevel::Trace;
    }
    if matches.occurrences_of("otlp") != 0 {
        settingsrw.logging.otlp.enabled = true;
        settingsrw.logging.otlp.grpc_endpoint = NamedSocketAddrs::from_str(
            &matches
                .value_of("otlp")
                .expect("should not be null because of default missing value")
                .to_string(),
        )
        .map_err(|e| format!("failed to parse OTLP address: {}", e))?;
        settingsrw.logging.otlp.level = LogLevel::Trace;
    }
    if matches.is_present("attach") {
        settingsrw.auto_attach = !matches!(matches.value_of("attach"), Some("true"));
    }
    if matches.is_present("local") {
        settingsrw.core.network.enable_local_peer_scope = true;
    }
    if matches.occurrences_of("delete-protected-store") != 0 {
        settingsrw.core.protected_store.delete = true;
    }
    if matches.occurrences_of("delete-block-store") != 0 {
        settingsrw.core.block_store.delete = true;
    }
    if matches.occurrences_of("delete-table-store") != 0 {
        settingsrw.core.table_store.delete = true;
    }
    if matches.occurrences_of("dump-txt-record") != 0 {
        // Turn off terminal logging so we can be interactive
        settingsrw.logging.terminal.enabled = false;
    }
    if let Some(v) = matches.value_of("set-node-id") {
        // Turn off terminal logging so we can be interactive
        settingsrw.logging.terminal.enabled = false;

        // Split or get secret
        let (k, s) = if let Some((k, s)) = v.split_once(':') {
            let k = DHTKey::try_decode(k)?;
            let s = DHTKeySecret::try_decode(s)?;
            (k, s)
        } else {
            let k = DHTKey::try_decode(v)?;
            let buffer = rpassword::prompt_password("Enter secret key (will not echo): ")
                .map_err(|e| e.to_string())?;
            let buffer = buffer.trim().to_string();
            let s = DHTKeySecret::try_decode(&buffer)?;
            (k, s)
        };
        settingsrw.core.network.node_id = k;
        settingsrw.core.network.node_id_secret = s;
    }

    if matches.occurrences_of("bootstrap") != 0 {
        let bootstrap_list = match matches.value_of("bootstrap") {
            Some(x) => {
                println!("Overriding bootstrap list with: ");
                let mut out: Vec<String> = Vec::new();
                for x in x.split(',') {
                    let x = x.trim().to_string();
                    println!("    {}", x);
                    out.push(x);
                }
                out
            }
            None => {
                return Err("value not specified for bootstrap".to_owned());
            }
        };
        settingsrw.core.network.bootstrap = bootstrap_list;
    }

    if matches.occurrences_of("bootstrap-nodes") != 0 {
        let bootstrap_list = match matches.value_of("bootstrap-nodes") {
            Some(x) => {
                println!("Overriding bootstrap node list with: ");
                let mut out: Vec<ParsedNodeDialInfo> = Vec::new();
                for x in x.split(',') {
                    let x = x.trim();
                    println!("    {}", x);
                    out.push(ParsedNodeDialInfo::from_str(x).map_err(|e| {
                        format!(
                            "unable to parse dial info in bootstrap node list: {} for {}",
                            e, x
                        )
                    })?);
                }
                out
            }
            None => {
                return Err("value not specified for bootstrap node list".to_owned());
            }
        };
        settingsrw.core.network.bootstrap_nodes = bootstrap_list;
    }
    drop(settingsrw);

    // Set specific config settings
    if let Some(set_configs) = matches.values_of("set-config") {
        for set_config in set_configs {
            if let Some((k, v)) = set_config.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                settings.set(k, v)?;
            }
        }
    }

    // Apply subnode index if we're testing
    settings
        .apply_subnode_index()
        .map_err(|_| "failed to apply subnode index".to_owned())?;

    Ok((settings, matches))
}
