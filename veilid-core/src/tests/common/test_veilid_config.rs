#![allow(clippy::bool_assert_comparison)]

use crate::*;

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        use std::fs::File;
        use std::io::prelude::*;
        use std::path::PathBuf;

        static CERTFILE: &str = r#"-----BEGIN CERTIFICATE-----
MIIDbzCCAlegAwIBAgIRALB/PvRpqN55Pk7L33NNsvcwDQYJKoZIhvcNAQELBQAw
FDESMBAGA1UEAwwJTm9jdGVtIENBMB4XDTIwMDkwODIxMDkwMFoXDTMwMDkwNjIx
MDkwMFowHDEaMBgGA1UEAwwRKi5ub2N0ZW0uaW50ZXJuYWwwggEiMA0GCSqGSIb3
DQEBAQUAA4IBDwAwggEKAoIBAQDRbAtA2dIlTPaQUN43/bdGi2wuDzCXk36TcfOr
YoxGsyJV6QpcIdmtrPN2WbkuDmA/G+0BUcQPvBfA/pFRHQElrzMhGR23Mp6IK7YR
pomUa1DQSJyMw/WM9V0+tidp5tJSeUCB+qKhLBrztD5XXjdhU6WA1J0y26XQoBqs
RZbPV8mce4LxVaQptkf4NB4/jnr3M1/FWEri60xBw3blWGaLP6gza3vqAr8pqEY4
zXU4q+egLbRIOwxwBJ0/vcyO6BdSzA1asWJCddXQJkUQrLl3OQ+44FMsAFyzCOiK
DVoqD2z4IJvIRT6TH8OcYvrotytlsNXS4ja9r32tTR1/DxUrAgMBAAGjgbMwgbAw
CQYDVR0TBAIwADAdBgNVHQ4EFgQUhjP4CArB3wWGHfavf7mRxaYshKMwRAYDVR0j
BD0wO4AUKAOv10AaiIUHgOtx0Mk6ZaZ/tGWhGKQWMBQxEjAQBgNVBAMMCU5vY3Rl
bSBDQYIJAISVWafozd3RMBMGA1UdJQQMMAoGCCsGAQUFBwMBMAsGA1UdDwQEAwIF
oDAcBgNVHREEFTATghEqLm5vY3RlbS5pbnRlcm5hbDANBgkqhkiG9w0BAQsFAAOC
AQEAMfVGtpXdkxflSQY2DzIUXLp9cZQnu4A8gww8iaLAg5CIUijP71tb2JJ+SsRx
W3p14YMhOYtswIvGTtXWzMgfAivwrxCcJefnqDAG9yviWoA0CSQe21nRjEqN6nyh
CS2BIkOcNNf10TD9sNo7z6IIXNjok7/F031JvH6pBgZ8Bq4IE/ANIuAvxwslPrqT
80qnWtAc5TzNNR1CT+fyZwMEpeW5fMZQnrSyUMsNv06Jydl/7IkGvlmbwihZOg95
Vty37pyzrXU5s/DY1zi5aYoFiK7/4bNEy9mRL9ero+kCvQfea0Yt2rITKQkCYvKu
MQTNaSyo6GTifW5InckkQIsnTQ==
-----END CERTIFICATE-----"#;

        static KEYFILE: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDRbAtA2dIlTPaQ
UN43/bdGi2wuDzCXk36TcfOrYoxGsyJV6QpcIdmtrPN2WbkuDmA/G+0BUcQPvBfA
/pFRHQElrzMhGR23Mp6IK7YRpomUa1DQSJyMw/WM9V0+tidp5tJSeUCB+qKhLBrz
tD5XXjdhU6WA1J0y26XQoBqsRZbPV8mce4LxVaQptkf4NB4/jnr3M1/FWEri60xB
w3blWGaLP6gza3vqAr8pqEY4zXU4q+egLbRIOwxwBJ0/vcyO6BdSzA1asWJCddXQ
JkUQrLl3OQ+44FMsAFyzCOiKDVoqD2z4IJvIRT6TH8OcYvrotytlsNXS4ja9r32t
TR1/DxUrAgMBAAECggEBAMIAK+CUqCbjyBliwKjvwWN5buqwKZyRBxXB3y/qJ/aq
pWkea/lzZjqMWDFP5sryiFiOHx00yMKmxP6FFMsmalSlm2DS6oM2QkP08kIhm5vB
WmjIizWfpo5BEnMwvQxOxpGeP5LpQtS5jfIrDAFVh0oC+fOBgmqFrXK5jlv+Tzmc
9PzoF5lgy8CHw3NxuScJpEhA1vTzu5N7sTdiTDKqY1ph2+RFlf30oyx4whoRVpIC
w8vp3WbLu/yAGuN5S14mYJW2Qgi8/rVCDStROEKOeB99mt1MG5lX7iuagzS/95Lr
2m1Nya0+7hkkpq6Y3Wqne9H0NLasJK8PU8ZaEc6BwTkCgYEA8iLVBrt4W/Cc5hry
8LWCMX8P25z7WIRYswnPvqwTwE0f6Q1ddWIaR9GPWUHgoRC4Z0b0MKolwo9s8RPE
GBuTOCy8ArSgYb1jNpsanGIWg6mZZgfylKdMdCMXMAAYF1/sTXeqCDY+FSCzEAvZ
hzppcCpiKV7Pa9aOo7o3/IeUBZcCgYEA3WmyvscG27R18XASJYL8Y4DuFvvnTHMp
YnxJIoS1+0TnUD2QqXUnXKbnTioWs7t990YAjbsHvK4fVsbnkuEm/as0oYbC8vU1
W3XN0HrpiacGcYIcXU4AY4XvY8t3y76FycJAT9Q6QztVofI5DmXV+8qsyrEegUys
wPIkkumCJ40CgYBKT3hTPZudk8WDNQgT6ZCQQi+Kta3Jp6xVHhC8srDJFqJRcsGY
8ceg/OZifT5EEA6X24W7naxC/qNvhSJsR6Ix3kDBD9AczvOw4X8UOWIxfA5Q6uV+
y61CAzbti0nZep3Z1HzBUmxRLZzmssxKnRmYy9keWzOLI+jYxKDEBpPd9wKBgAY1
pquvDUQwJXal+/xNViK8RPEkE3KTcD+w2KQ9MJVhc1NOxrXZ8Uap76bDi2tzAK9k
qTNQYYErKPnYDjqSUfOfT5SQIPuLYPm1rhYAvHf91TJtwbnkLCKeaP5VgICYUUw9
RGx4uUGVcmteTbdXp86t+naczQw3SEkJAXmVTu8pAoGATF7xXifMUSL1v43Ybrmc
RikQyDecRspMYLOCNmPWI2PPz6MAjm8jDCsXK52HUK4mUqrd/W3rqnl+TrJsXOnH
Ww6tESPaF1kCVyV2Jx/5m8qsE9y5Bds7eMo2JF8vnAKFX6t4KwZiyHBymj6uelNc
wFAbkZY9eS/x6P7qrpd7dUA=
-----END PRIVATE KEY-----"#;
    }
}

cfg_if! {

    if #[cfg(target_arch = "wasm32")] {
        pub fn get_table_store_path() -> String {
            String::new()
        }
        pub fn get_block_store_path() -> String {
            String::new()
        }
        pub fn get_protected_store_path() -> String {
            String::new()
        }
        pub fn get_certfile_path() -> String {
            String::new()
        }
        pub fn get_keyfile_path() -> String {
            String::new()
        }
    }
    else {

        fn get_data_dir() -> PathBuf {
            cfg_if! {
                if #[cfg(target_os = "android")] {
                    PathBuf::from(crate::intf::android::get_files_dir())
                } else {
                    use directories::*;

                    if let Some(my_proj_dirs) = ProjectDirs::from("org", "Veilid", "VeilidCoreTests") {
                        PathBuf::from(my_proj_dirs.data_local_dir())
                    } else {
                        PathBuf::from("./")
                    }
                }
            }
        }

        pub fn get_table_store_path() -> String {
            let mut out = get_data_dir();
            std::fs::create_dir_all(&out).unwrap();

            out.push("table_store");

            out.into_os_string().into_string().unwrap()
        }

        pub fn get_block_store_path() -> String {
            let mut out = get_data_dir();
            std::fs::create_dir_all(&out).unwrap();

            out.push("block_store");

            out.into_os_string().into_string().unwrap()
        }

        pub fn get_protected_store_path() -> String {
            let mut out = get_data_dir();
            std::fs::create_dir_all(&out).unwrap();

            out.push("protected_store");

            out.into_os_string().into_string().unwrap()
        }

        pub fn get_certfile_path() -> String {
            let mut out = get_data_dir();
            std::fs::create_dir_all(&out).unwrap();

            out.push("cert.pem");
            // Initialize certfile
            if !out.exists() {
                debug!("creating certfile at {:?}", out);
                File::create(&out).unwrap().write_all(CERTFILE.as_bytes()).unwrap();
            }

            out.into_os_string().into_string().unwrap()
        }

        pub fn get_keyfile_path() -> String {
            let mut out = get_data_dir();
            std::fs::create_dir_all(&out).unwrap();

            out.push("key.pem");

            // Initialize keyfile
            if !out.exists() {
                debug!("creating keyfile at {:?}", out);
                File::create(&out).unwrap().write_all(KEYFILE.as_bytes()).unwrap();
            }

            out.into_os_string().into_string().unwrap()
        }
    }
}

fn update_callback(_update: VeilidUpdate) {
    // println!("update_callback: {:?}", update);
}

pub fn setup_veilid_core() -> (UpdateCallback, ConfigCallback) {
    (Arc::new(update_callback), Arc::new(config_callback))
}

fn config_callback(key: String) -> ConfigCallbackReturn {
    match key.as_str() {
        "program_name" => Ok(Box::new(String::from("Veilid"))),
        "namespace" => Ok(Box::new(String::from(""))),
        "capabilities.protocol_udp" => Ok(Box::new(true)),
        "capabilities.protocol_connect_tcp" => Ok(Box::new(true)),
        "capabilities.protocol_accept_tcp" => Ok(Box::new(true)),
        "capabilities.protocol_connect_ws" => Ok(Box::new(true)),
        "capabilities.protocol_accept_ws" => Ok(Box::new(true)),
        "capabilities.protocol_connect_wss" => Ok(Box::new(true)),
        "capabilities.protocol_accept_wss" => Ok(Box::new(true)),
        "table_store.directory" => Ok(Box::new(get_table_store_path())),
        "table_store.delete" => Ok(Box::new(false)),
        "block_store.directory" => Ok(Box::new(get_block_store_path())),
        "block_store.delete" => Ok(Box::new(false)),
        "protected_store.allow_insecure_fallback" => Ok(Box::new(true)),
        "protected_store.always_use_insecure_storage" => Ok(Box::new(false)),
        "protected_store.insecure_fallback_directory" => Ok(Box::new(get_protected_store_path())),
        "protected_store.delete" => Ok(Box::new(false)),
        "network.connection_initial_timeout_ms" => Ok(Box::new(2_000u32)),
        "network.connection_inactivity_timeout_ms" => Ok(Box::new(60_000u32)),
        "network.max_connections_per_ip4" => Ok(Box::new(8u32)),
        "network.max_connections_per_ip6_prefix" => Ok(Box::new(8u32)),
        "network.max_connections_per_ip6_prefix_size" => Ok(Box::new(56u32)),
        "network.max_connection_frequency_per_min" => Ok(Box::new(8u32)),
        "network.client_whitelist_timeout_ms" => Ok(Box::new(300_000u32)),
        "network.reverse_connection_receipt_time_ms" => Ok(Box::new(5_000u32)),
        "network.hole_punch_receipt_time_ms" => Ok(Box::new(5_000u32)),
        "network.node_id" => Ok(Box::new(Option::<TypedKey>::None)),
        "network.node_id_secret" => Ok(Box::new(Option::<SecretKey>::None)),
        "network.bootstrap" => Ok(Box::new(Vec::<String>::new())),
        "network.routing_table.limit_over_attached" => Ok(Box::new(64u32)),
        "network.routing_table.limit_fully_attached" => Ok(Box::new(32u32)),
        "network.routing_table.limit_attached_strong" => Ok(Box::new(16u32)),
        "network.routing_table.limit_attached_good" => Ok(Box::new(8u32)),
        "network.routing_table.limit_attached_weak" => Ok(Box::new(4u32)),
        "network.rpc.concurrency" => Ok(Box::new(2u32)),
        "network.rpc.queue_size" => Ok(Box::new(1024u32)),
        "network.rpc.max_timestamp_behind_ms" => Ok(Box::new(Some(10_000u32))),
        "network.rpc.max_timestamp_ahead_ms" => Ok(Box::new(Some(10_000u32))),
        "network.rpc.timeout_ms" => Ok(Box::new(10_000u32)),
        "network.rpc.max_route_hop_count" => Ok(Box::new(4u8)),
        "network.rpc.default_route_hop_count" => Ok(Box::new(1u8)),
        "network.dht.resolve_node_timeout_ms" => Ok(Box::new(Option::<u32>::None)),
        "network.dht.resolve_node_count" => Ok(Box::new(20u32)),
        "network.dht.resolve_node_fanout" => Ok(Box::new(3u32)),
        "network.dht.max_find_node_count" => Ok(Box::new(20u32)),
        "network.dht.get_value_timeout_ms" => Ok(Box::new(Option::<u32>::None)),
        "network.dht.get_value_count" => Ok(Box::new(20u32)),
        "network.dht.get_value_fanout" => Ok(Box::new(3u32)),
        "network.dht.set_value_timeout_ms" => Ok(Box::new(Option::<u32>::None)),
        "network.dht.set_value_count" => Ok(Box::new(20u32)),
        "network.dht.set_value_fanout" => Ok(Box::new(5u32)),
        "network.dht.min_peer_count" => Ok(Box::new(20u32)),
        "network.dht.min_peer_refresh_time_ms" => Ok(Box::new(2_000u32)),
        "network.dht.validate_dial_info_receipt_time_ms" => Ok(Box::new(5_000u32)),
        "network.upnp" => Ok(Box::new(false)),
        "network.detect_address_changes" => Ok(Box::new(true)),
        "network.restricted_nat_retries" => Ok(Box::new(3u32)),
        "network.tls.certificate_path" => Ok(Box::new(get_certfile_path())),
        "network.tls.private_key_path" => Ok(Box::new(get_keyfile_path())),
        "network.tls.connection_initial_timeout_ms" => Ok(Box::new(2_000u32)),
        "network.application.https.enabled" => Ok(Box::new(false)),
        "network.application.https.listen_address" => Ok(Box::new("".to_owned())),
        "network.application.https.path" => Ok(Box::new(String::from("app"))),
        "network.application.https.url" => Ok(Box::new(Option::<String>::None)),
        "network.application.http.enabled" => Ok(Box::new(false)),
        "network.application.http.listen_address" => Ok(Box::new("".to_owned())),
        "network.application.http.path" => Ok(Box::new(String::from("app"))),
        "network.application.http.url" => Ok(Box::new(Option::<String>::None)),
        "network.protocol.udp.enabled" => Ok(Box::new(true)),
        "network.protocol.udp.socket_pool_size" => Ok(Box::new(16u32)),
        "network.protocol.udp.listen_address" => Ok(Box::new("".to_owned())),
        "network.protocol.udp.public_address" => Ok(Box::new(Option::<String>::None)),
        "network.protocol.tcp.connect" => Ok(Box::new(true)),
        "network.protocol.tcp.listen" => Ok(Box::new(true)),
        "network.protocol.tcp.max_connections" => Ok(Box::new(32u32)),
        "network.protocol.tcp.listen_address" => Ok(Box::new("".to_owned())),
        "network.protocol.tcp.public_address" => Ok(Box::new(Option::<String>::None)),
        "network.protocol.ws.connect" => Ok(Box::new(false)),
        "network.protocol.ws.listen" => Ok(Box::new(false)),
        "network.protocol.ws.max_connections" => Ok(Box::new(16u32)),
        "network.protocol.ws.listen_address" => Ok(Box::new("".to_owned())),
        "network.protocol.ws.path" => Ok(Box::new(String::from("ws"))),
        "network.protocol.ws.url" => Ok(Box::new(Option::<String>::None)),
        "network.protocol.wss.connect" => Ok(Box::new(false)),
        "network.protocol.wss.listen" => Ok(Box::new(false)),
        "network.protocol.wss.max_connections" => Ok(Box::new(16u32)),
        "network.protocol.wss.listen_address" => Ok(Box::new("".to_owned())),
        "network.protocol.wss.path" => Ok(Box::new(String::from("ws"))),
        "network.protocol.wss.url" => Ok(Box::new(Option::<String>::None)),
        _ => {
            let err = format!("config key '{}' doesn't exist", key);
            debug!("{}", err);
            Err(VeilidAPIError::internal(err))
        }
    }
}

pub fn get_config() -> VeilidConfig {
    let mut vc = VeilidConfig::new();
    match vc.setup(Arc::new(config_callback), Arc::new(update_callback)) {
        Ok(()) => (),
        Err(e) => {
            error!("Error: {}", e);
            unreachable!();
        }
    };
    vc
}

pub async fn test_config() {
    let mut vc = VeilidConfig::new();
    match vc.setup(Arc::new(config_callback), Arc::new(update_callback)) {
        Ok(()) => (),
        Err(e) => {
            error!("Error: {}", e);
            unreachable!();
        }
    }

    let inner = vc.get();
    assert_eq!(inner.program_name, String::from("Veilid"));
    assert_eq!(inner.namespace, String::from(""));
    assert_eq!(inner.capabilities.protocol_udp, true);
    assert_eq!(inner.capabilities.protocol_connect_tcp, true);
    assert_eq!(inner.capabilities.protocol_accept_tcp, true);
    assert_eq!(inner.capabilities.protocol_connect_ws, true);
    assert_eq!(inner.capabilities.protocol_accept_ws, true);
    assert_eq!(inner.capabilities.protocol_connect_wss, true);
    assert_eq!(inner.capabilities.protocol_accept_wss, true);
    assert_eq!(inner.table_store.directory, get_table_store_path());
    assert_eq!(inner.table_store.delete, false);
    assert_eq!(inner.block_store.directory, get_block_store_path());
    assert_eq!(inner.block_store.delete, false);
    assert_eq!(inner.protected_store.allow_insecure_fallback, true);
    assert_eq!(inner.protected_store.always_use_insecure_storage, false);
    assert_eq!(
        inner.protected_store.insecure_fallback_directory,
        get_protected_store_path()
    );
    assert_eq!(inner.protected_store.delete, false);
    assert_eq!(inner.network.connection_initial_timeout_ms, 2_000u32);
    assert_eq!(inner.network.connection_inactivity_timeout_ms, 60_000u32);
    assert_eq!(inner.network.max_connections_per_ip4, 8u32);
    assert_eq!(inner.network.max_connections_per_ip6_prefix, 8u32);
    assert_eq!(inner.network.max_connections_per_ip6_prefix_size, 56u32);
    assert_eq!(inner.network.max_connection_frequency_per_min, 8u32);
    assert_eq!(inner.network.client_whitelist_timeout_ms, 300_000u32);
    assert_eq!(inner.network.reverse_connection_receipt_time_ms, 5_000u32);
    assert_eq!(inner.network.hole_punch_receipt_time_ms, 5_000u32);
    assert!(inner.network.node_id.is_none());
    assert!(inner.network.node_id_secret.is_none());
    assert_eq!(inner.network.bootstrap, Vec::<String>::new());
    assert_eq!(inner.network.rpc.concurrency, 2u32);
    assert_eq!(inner.network.rpc.queue_size, 1024u32);
    assert_eq!(inner.network.rpc.timeout_ms, 10_000u32);
    assert_eq!(inner.network.rpc.max_route_hop_count, 4u8);
    assert_eq!(inner.network.rpc.default_route_hop_count, 1u8);
    assert_eq!(inner.network.routing_table.limit_over_attached, 64u32);
    assert_eq!(inner.network.routing_table.limit_fully_attached, 32u32);
    assert_eq!(inner.network.routing_table.limit_attached_strong, 16u32);
    assert_eq!(inner.network.routing_table.limit_attached_good, 8u32);
    assert_eq!(inner.network.routing_table.limit_attached_weak, 4u32);

    assert_eq!(
        inner.network.dht.resolve_node_timeout_ms,
        Option::<u32>::None
    );
    assert_eq!(inner.network.dht.resolve_node_count, 20u32);
    assert_eq!(inner.network.dht.resolve_node_fanout, 3u32);
    assert_eq!(inner.network.dht.get_value_timeout_ms, Option::<u32>::None);
    assert_eq!(inner.network.dht.get_value_count, 20u32);
    assert_eq!(inner.network.dht.get_value_fanout, 3u32);
    assert_eq!(inner.network.dht.set_value_timeout_ms, Option::<u32>::None);
    assert_eq!(inner.network.dht.set_value_count, 20u32);
    assert_eq!(inner.network.dht.set_value_fanout, 5u32);
    assert_eq!(inner.network.dht.min_peer_count, 20u32);
    assert_eq!(inner.network.dht.min_peer_refresh_time_ms, 2_000u32);
    assert_eq!(
        inner.network.dht.validate_dial_info_receipt_time_ms,
        5_000u32
    );

    assert_eq!(inner.network.upnp, false);
    assert_eq!(inner.network.detect_address_changes, true);
    assert_eq!(inner.network.restricted_nat_retries, 3u32);
    assert_eq!(inner.network.tls.certificate_path, get_certfile_path());
    assert_eq!(inner.network.tls.private_key_path, get_keyfile_path());
    assert_eq!(inner.network.tls.connection_initial_timeout_ms, 2_000u32);

    assert_eq!(inner.network.application.https.enabled, false);
    assert_eq!(inner.network.application.https.listen_address, "");
    assert_eq!(inner.network.application.https.path, "app");
    assert_eq!(inner.network.application.https.url, None);
    assert_eq!(inner.network.application.http.enabled, false);
    assert_eq!(inner.network.application.http.listen_address, "");
    assert_eq!(inner.network.application.http.path, "app");
    assert_eq!(inner.network.application.http.url, None);
    assert_eq!(inner.network.protocol.udp.enabled, true);
    assert_eq!(inner.network.protocol.udp.socket_pool_size, 16u32);
    assert_eq!(inner.network.protocol.udp.listen_address, "");
    assert_eq!(inner.network.protocol.udp.public_address, None);
    assert_eq!(inner.network.protocol.tcp.connect, true);
    assert_eq!(inner.network.protocol.tcp.listen, true);
    assert_eq!(inner.network.protocol.tcp.max_connections, 32u32);
    assert_eq!(inner.network.protocol.tcp.listen_address, "");
    assert_eq!(inner.network.protocol.tcp.public_address, None);
    assert_eq!(inner.network.protocol.ws.connect, false);
    assert_eq!(inner.network.protocol.ws.listen, false);
    assert_eq!(inner.network.protocol.ws.max_connections, 16u32);
    assert_eq!(inner.network.protocol.ws.listen_address, "");
    assert_eq!(inner.network.protocol.ws.path, "ws");
    assert_eq!(inner.network.protocol.ws.url, None);
    assert_eq!(inner.network.protocol.wss.connect, false);
    assert_eq!(inner.network.protocol.wss.listen, false);
    assert_eq!(inner.network.protocol.wss.max_connections, 16u32);
    assert_eq!(inner.network.protocol.wss.listen_address, "");
    assert_eq!(inner.network.protocol.wss.path, "ws");
    assert_eq!(inner.network.protocol.wss.url, None);
}

pub async fn test_all() {
    test_config().await;
}
