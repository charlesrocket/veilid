use super::sockets::*;
use super::*;
use lazy_static::*;

lazy_static! {
    static ref BAD_PORTS: BTreeSet<u16> = BTreeSet::from([
        1,    // tcpmux
        7,    // echo
        9,    // discard
        11,   // systat
        13,   // daytime
        15,   // netstat
        17,   // qotd
        19,   // chargen
        20,   // ftp data
        21,   // ftp access
        22,   // ssh
        23,   // telnet
        25,   // smtp
        37,   // time
        42,   // name
        43,   // nicname
        53,   // domain
        77,   // priv-rjs
        79,   // finger
        87,   // ttylink
        95,   // supdup
        101,  // hostriame
        102,  // iso-tsap
        103,  // gppitnp
        104,  // acr-nema
        109,  // pop2
        110,  // pop3
        111,  // sunrpc
        113,  // auth
        115,  // sftp
        117,  // uucp-path
        119,  // nntp
        123,  // NTP
        135,  // loc-srv /epmap
        139,  // netbios
        143,  // imap2
        179,  // BGP
        389,  // ldap
        427,  // SLP (Also used by Apple Filing Protocol)
        465,  // smtp+ssl
        512,  // print / exec
        513,  // login
        514,  // shell
        515,  // printer
        526,  // tempo
        530,  // courier
        531,  // chat
        532,  // netnews
        540,  // uucp
        548,  // AFP (Apple Filing Protocol)
        556,  // remotefs
        563,  // nntp+ssl
        587,  // smtp (rfc6409)
        601,  // syslog-conn (rfc3195)
        636,  // ldap+ssl
        993,  // ldap+ssl
        995,  // pop3+ssl
        2049, // nfs
        3659, // apple-sasl / PasswordServer
        4045, // lockd
        6000, // X11
        6665, // Alternate IRC [Apple addition]
        6666, // Alternate IRC [Apple addition]
        6667, // Standard IRC [Apple addition]
        6668, // Alternate IRC [Apple addition]
        6669, // Alternate IRC [Apple addition]
        6697, // IRC + TLS
    ]);
}

impl Network {
    /////////////////////////////////////////////////////
    // Support for binding first on ports to ensure nobody binds ahead of us
    // or two copies of the app don't accidentally collide. This is tricky
    // because we use 'reuseaddr/port' and we can accidentally bind in front of ourselves :P

    fn bind_first_udp_port(&self, udp_port: u16) -> bool {
        let mut inner = self.inner.lock();
        if inner.bound_first_udp.contains_key(&udp_port) {
            return true;
        }
        // If the address is specified, only use the specified port and fail otherwise
        let mut bound_first_socket_v4 = None;
        let mut bound_first_socket_v6 = None;
        if let Ok(bfs4) =
            new_bound_first_udp_socket(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), udp_port))
        {
            if let Ok(bfs6) = new_bound_first_udp_socket(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                udp_port,
            )) {
                bound_first_socket_v4 = Some(bfs4);
                bound_first_socket_v6 = Some(bfs6);
            }
        }
        if let (Some(bfs4), Some(bfs6)) = (bound_first_socket_v4, bound_first_socket_v6) {
            cfg_if! {
                if #[cfg(windows)] {
                    // On windows, drop the socket. This is a race condition, but there's
                    // no way around it. This isn't for security anyway, it's to prevent multiple copies of the
                    // app from binding on the same port.
                    drop(bfs4);
                    drop(bfs6);
                    inner.bound_first_udp.insert(udp_port, None);
                } else {
                    inner.bound_first_udp.insert(udp_port, Some((bfs4, bfs6)));
                }
            }
            true
        } else {
            false
        }
    }

    fn bind_first_tcp_port(&self, tcp_port: u16) -> bool {
        let mut inner = self.inner.lock();
        if inner.bound_first_tcp.contains_key(&tcp_port) {
            return true;
        }
        // If the address is specified, only use the specified port and fail otherwise
        let mut bound_first_socket_v4 = None;
        let mut bound_first_socket_v6 = None;
        if let Ok(bfs4) =
            new_bound_first_tcp_socket(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), tcp_port))
        {
            if let Ok(bfs6) = new_bound_first_tcp_socket(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                tcp_port,
            )) {
                bound_first_socket_v4 = Some(bfs4);
                bound_first_socket_v6 = Some(bfs6);
            }
        }
        if let (Some(bfs4), Some(bfs6)) = (bound_first_socket_v4, bound_first_socket_v6) {
            cfg_if! {
                if #[cfg(windows)] {
                    // On windows, drop the socket. This is a race condition, but there's
                    // no way around it. This isn't for security anyway, it's to prevent multiple copies of the
                    // app from binding on the same port.
                    drop(bfs4);
                    drop(bfs6);
                    inner.bound_first_tcp.insert(tcp_port, None);
                } else {
                    inner.bound_first_tcp.insert(tcp_port, Some((bfs4, bfs6)));
                }
            }
            true
        } else {
            false
        }
    }

    pub(super) fn free_bound_first_ports(&self) {
        let mut inner = self.inner.lock();
        inner.bound_first_udp.clear();
        inner.bound_first_tcp.clear();
    }

    /////////////////////////////////////////////////////

    fn find_available_udp_port(&self) -> Result<u16, String> {
        // If the address is empty, iterate ports until we find one we can use.
        let mut udp_port = 5150u16;
        loop {
            if BAD_PORTS.contains(&udp_port) {
                continue;
            }
            if self.bind_first_udp_port(udp_port) {
                break;
            }
            if udp_port == 65535 {
                return Err("Could not find free udp port to listen on".to_owned());
            }
            udp_port += 1;
        }
        Ok(udp_port)
    }

    fn find_available_tcp_port(&self) -> Result<u16, String> {
        // If the address is empty, iterate ports until we find one we can use.
        let mut tcp_port = 5150u16;
        loop {
            if BAD_PORTS.contains(&tcp_port) {
                continue;
            }
            if self.bind_first_tcp_port(tcp_port) {
                break;
            }
            if tcp_port == 65535 {
                return Err("Could not find free tcp port to listen on".to_owned());
            }
            tcp_port += 1;
        }
        Ok(tcp_port)
    }

    async fn allocate_udp_port(
        &self,
        listen_address: String,
    ) -> Result<(u16, Vec<IpAddr>), String> {
        if listen_address.is_empty() {
            // If listen address is empty, find us a port iteratively
            let port = self.find_available_udp_port()?;
            let ip_addrs = vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            ];
            Ok((port, ip_addrs))
        } else {
            // If no address is specified, but the port is, use ipv4 and ipv6 unspecified
            // If the address is specified, only use the specified port and fail otherwise
            let sockaddrs = listen_address_to_socket_addrs(&listen_address)?;
            if sockaddrs.is_empty() {
                return Err(format!("No valid listen address: {}", listen_address));
            }
            let port = sockaddrs[0].port();
            if self.bind_first_udp_port(port) {
                Ok((port, sockaddrs.iter().map(|s| s.ip()).collect()))
            } else {
                Err("Could not find free udp port to listen on".to_owned())
            }
        }
    }

    async fn allocate_tcp_port(
        &self,
        listen_address: String,
    ) -> Result<(u16, Vec<IpAddr>), String> {
        if listen_address.is_empty() {
            // If listen address is empty, find us a port iteratively
            let port = self.find_available_tcp_port()?;
            let ip_addrs = vec![
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                IpAddr::V6(Ipv6Addr::UNSPECIFIED),
            ];
            Ok((port, ip_addrs))
        } else {
            // If no address is specified, but the port is, use ipv4 and ipv6 unspecified
            // If the address is specified, only use the specified port and fail otherwise
            let sockaddrs = listen_address_to_socket_addrs(&listen_address)?;
            if sockaddrs.is_empty() {
                return Err(format!("No valid listen address: {}", listen_address));
            }
            let port = sockaddrs[0].port();
            if self.bind_first_tcp_port(port) {
                Ok((port, sockaddrs.iter().map(|s| s.ip()).collect()))
            } else {
                Err("Could not find free tcp port to listen on".to_owned())
            }
        }
    }

    /////////////////////////////////////////////////////

    pub(super) async fn start_udp_listeners(&self) -> Result<(), String> {
        trace!("starting udp listeners");
        let routing_table = self.routing_table();
        let (listen_address, public_address, enable_local_peer_scope) = {
            let c = self.config.get();
            (
                c.network.protocol.udp.listen_address.clone(),
                c.network.protocol.udp.public_address.clone(),
                c.network.enable_local_peer_scope,
            )
        };

        // Pick out UDP port we're going to use everywhere
        // Keep sockets around until the end of this function
        // to keep anyone else from binding in front of us
        let (udp_port, ip_addrs) = self.allocate_udp_port(listen_address.clone()).await?;

        // Save the bound udp port for use later on
        self.inner.lock().udp_port = udp_port;

        // First, create outbound sockets
        // (unlike tcp where we create sockets for every connection)
        // and we'll add protocol handlers for them too
        self.create_udp_outbound_sockets().await?;

        // Now create udp inbound sockets for whatever interfaces we're listening on
        info!(
            "UDP: starting listeners on port {} at {:?}",
            udp_port, ip_addrs
        );
        let local_dial_info_list = self.create_udp_inbound_sockets(ip_addrs, udp_port).await?;
        let mut static_public = false;

        trace!("UDP: listener started on {:#?}", local_dial_info_list);

        // Register local dial info
        for di in &local_dial_info_list {
            // If the local interface address is global, or we are enabling local peer scope
            // register global dial info if no public address is specified
            if public_address.is_none() && (di.is_global() || enable_local_peer_scope) {
                routing_table.register_dial_info(
                    RoutingDomain::PublicInternet,
                    di.clone(),
                    DialInfoClass::Direct,
                )?;
                static_public = true;
            }

            // Register interface dial info as well since the address is on the local interface
            routing_table.register_dial_info(
                RoutingDomain::LocalNetwork,
                di.clone(),
                DialInfoClass::Direct,
            )?;
        }

        // Add static public dialinfo if it's configured
        if let Some(public_address) = public_address.as_ref() {
            // Resolve statically configured public dialinfo
            let mut public_sockaddrs = public_address
                .to_socket_addrs()
                .await
                .map_err(|e| format!("Unable to resolve address: {}\n{}", public_address, e))?;

            // Add all resolved addresses as public dialinfo
            for pdi_addr in &mut public_sockaddrs {
                let pdi = DialInfo::udp_from_socketaddr(pdi_addr);

                // Register the public address
                routing_table.register_dial_info(
                    RoutingDomain::PublicInternet,
                    pdi.clone(),
                    DialInfoClass::Direct,
                )?;

                // See if this public address is also a local interface address we haven't registered yet
                let is_interface_address = self.with_interface_addresses(|ip_addrs| {
                    for ip_addr in ip_addrs {
                        if pdi_addr.ip() == *ip_addr {
                            return true;
                        }
                    }
                    false
                });
                if !local_dial_info_list.contains(&pdi) && is_interface_address {
                    routing_table.register_dial_info(
                        RoutingDomain::LocalNetwork,
                        DialInfo::udp_from_socketaddr(pdi_addr),
                        DialInfoClass::Direct,
                    )?;
                }

                static_public = true;
            }
        }

        if static_public {
            self.inner
                .lock()
                .static_public_dialinfo
                .insert(ProtocolType::UDP);
        }

        // Now create tasks for udp listeners
        self.create_udp_listener_tasks().await
    }

    pub(super) async fn start_ws_listeners(&self) -> Result<(), String> {
        trace!("starting ws listeners");
        let routing_table = self.routing_table();
        let (listen_address, url, path, enable_local_peer_scope) = {
            let c = self.config.get();
            (
                c.network.protocol.ws.listen_address.clone(),
                c.network.protocol.ws.url.clone(),
                c.network.protocol.ws.path.clone(),
                c.network.enable_local_peer_scope,
            )
        };

        // Pick out TCP port we're going to use everywhere
        // Keep sockets around until the end of this function
        // to keep anyone else from binding in front of us
        let (ws_port, ip_addrs) = self.allocate_tcp_port(listen_address.clone()).await?;

        // Save the bound ws port for use later on
        self.inner.lock().ws_port = ws_port;

        trace!(
            "WS: starting listener on port {} at {:?}",
            ws_port,
            ip_addrs
        );
        let socket_addresses = self
            .start_tcp_listener(
                ip_addrs,
                ws_port,
                false,
                Box::new(|c, t, a| Box::new(WebsocketProtocolHandler::new(c, t, a))),
            )
            .await?;
        trace!("WS: listener started on {:#?}", socket_addresses);

        let mut static_public = false;
        let mut registered_addresses: HashSet<IpAddr> = HashSet::new();

        // Add static public dialinfo if it's configured
        if let Some(url) = url.as_ref() {
            let mut split_url = SplitUrl::from_str(url)?;
            if split_url.scheme.to_ascii_lowercase() != "ws" {
                return Err("WS URL must use 'ws://' scheme".to_owned());
            }
            split_url.scheme = "ws".to_owned();

            // Resolve static public hostnames
            let global_socket_addrs = split_url
                .host_port(80)
                .to_socket_addrs()
                .await
                .map_err(map_to_string)
                .map_err(logthru_net!(error))?;

            for gsa in global_socket_addrs {
                let pdi = DialInfo::try_ws(SocketAddress::from_socket_addr(gsa), url.clone())
                    .map_err(map_to_string)
                    .map_err(logthru_net!(error))?;

                routing_table.register_dial_info(
                    RoutingDomain::PublicInternet,
                    pdi.clone(),
                    DialInfoClass::Direct,
                )?;
                static_public = true;

                // See if this public address is also a local interface address
                let is_interface_address = self.with_interface_addresses(|ip_addrs| {
                    for ip_addr in ip_addrs {
                        if gsa.ip() == *ip_addr {
                            return true;
                        }
                    }
                    false
                });
                if !registered_addresses.contains(&gsa.ip()) && is_interface_address {
                    routing_table.register_dial_info(
                        RoutingDomain::LocalNetwork,
                        pdi,
                        DialInfoClass::Direct,
                    )?;
                }

                registered_addresses.insert(gsa.ip());
            }
        }

        for socket_address in socket_addresses {
            // Skip addresses we already did
            if registered_addresses.contains(&socket_address.to_ip_addr()) {
                continue;
            }
            // Build dial info request url
            let local_url = format!("ws://{}/{}", socket_address, path);
            let local_di = DialInfo::try_ws(socket_address, local_url)
                .map_err(map_to_string)
                .map_err(logthru_net!(error))?;

            if url.is_none() && (socket_address.address().is_global() || enable_local_peer_scope) {
                // Register public dial info
                routing_table.register_dial_info(
                    RoutingDomain::PublicInternet,
                    local_di.clone(),
                    DialInfoClass::Direct,
                )?;
                static_public = true;
            }

            // Register local dial info
            routing_table.register_dial_info(
                RoutingDomain::LocalNetwork,
                local_di,
                DialInfoClass::Direct,
            )?;
        }

        if static_public {
            self.inner
                .lock()
                .static_public_dialinfo
                .insert(ProtocolType::WS);
        }

        Ok(())
    }

    pub(super) async fn start_wss_listeners(&self) -> Result<(), String> {
        trace!("starting wss listeners");

        let routing_table = self.routing_table();
        let (listen_address, url) = {
            let c = self.config.get();
            (
                c.network.protocol.wss.listen_address.clone(),
                c.network.protocol.wss.url.clone(),
            )
        };

        // Pick out TCP port we're going to use everywhere
        // Keep sockets around until the end of this function
        // to keep anyone else from binding in front of us
        let (wss_port, ip_addrs) = self.allocate_tcp_port(listen_address.clone()).await?;

        // Save the bound wss port for use later on
        self.inner.lock().wss_port = wss_port;

        trace!(
            "WSS: starting listener on port {} at {:?}",
            wss_port,
            ip_addrs
        );
        let socket_addresses = self
            .start_tcp_listener(
                ip_addrs,
                wss_port,
                true,
                Box::new(|c, t, a| Box::new(WebsocketProtocolHandler::new(c, t, a))),
            )
            .await?;
        trace!("WSS: listener started on {:#?}", socket_addresses);

        // NOTE: No interface dial info for WSS, as there is no way to connect to a local dialinfo via TLS
        // If the hostname is specified, it is the public dialinfo via the URL. If no hostname
        // is specified, then TLS won't validate, so no local dialinfo is possible.
        // This is not the case with unencrypted websockets, which can be specified solely by an IP address

        let mut static_public = false;
        let mut registered_addresses: HashSet<IpAddr> = HashSet::new();

        // Add static public dialinfo if it's configured
        if let Some(url) = url.as_ref() {
            // Add static public dialinfo if it's configured
            let mut split_url = SplitUrl::from_str(url)?;
            if split_url.scheme.to_ascii_lowercase() != "wss" {
                return Err("WSS URL must use 'wss://' scheme".to_owned());
            }
            split_url.scheme = "wss".to_owned();

            // Resolve static public hostnames
            let global_socket_addrs = split_url
                .host_port(443)
                .to_socket_addrs()
                .await
                .map_err(map_to_string)
                .map_err(logthru_net!(error))?;

            for gsa in global_socket_addrs {
                let pdi = DialInfo::try_wss(SocketAddress::from_socket_addr(gsa), url.clone())
                    .map_err(map_to_string)
                    .map_err(logthru_net!(error))?;

                routing_table.register_dial_info(
                    RoutingDomain::PublicInternet,
                    pdi.clone(),
                    DialInfoClass::Direct,
                )?;
                static_public = true;

                // See if this public address is also a local interface address
                let is_interface_address = self.with_interface_addresses(|ip_addrs| {
                    for ip_addr in ip_addrs {
                        if gsa.ip() == *ip_addr {
                            return true;
                        }
                    }
                    false
                });
                if !registered_addresses.contains(&gsa.ip()) && is_interface_address {
                    routing_table.register_dial_info(
                        RoutingDomain::LocalNetwork,
                        pdi,
                        DialInfoClass::Direct,
                    )?;
                }

                registered_addresses.insert(gsa.ip());
            }
        } else {
            return Err("WSS URL must be specified due to TLS requirements".to_owned());
        }

        if static_public {
            self.inner
                .lock()
                .static_public_dialinfo
                .insert(ProtocolType::WSS);
        }

        Ok(())
    }

    pub(super) async fn start_tcp_listeners(&self) -> Result<(), String> {
        trace!("starting tcp listeners");

        let routing_table = self.routing_table();
        let (listen_address, public_address, enable_local_peer_scope) = {
            let c = self.config.get();
            (
                c.network.protocol.tcp.listen_address.clone(),
                c.network.protocol.tcp.public_address.clone(),
                c.network.enable_local_peer_scope,
            )
        };

        // Pick out TCP port we're going to use everywhere
        // Keep sockets around until the end of this function
        // to keep anyone else from binding in front of us
        let (tcp_port, ip_addrs) = self.allocate_tcp_port(listen_address.clone()).await?;

        // Save the bound tcp port for use later on
        self.inner.lock().tcp_port = tcp_port;

        trace!(
            "TCP: starting listener on port {} at {:?}",
            tcp_port,
            ip_addrs
        );
        let socket_addresses = self
            .start_tcp_listener(
                ip_addrs,
                tcp_port,
                false,
                Box::new(|_, _, a| Box::new(RawTcpProtocolHandler::new(a))),
            )
            .await?;
        trace!("TCP: listener started on {:#?}", socket_addresses);

        let mut static_public = false;
        let mut registered_addresses: HashSet<IpAddr> = HashSet::new();

        for socket_address in socket_addresses {
            let di = DialInfo::tcp(socket_address);

            // Register global dial info if no public address is specified
            if public_address.is_none() && (di.is_global() || enable_local_peer_scope) {
                routing_table.register_dial_info(
                    RoutingDomain::PublicInternet,
                    di.clone(),
                    DialInfoClass::Direct,
                )?;
                static_public = true;
            }
            // Register interface dial info
            routing_table.register_dial_info(
                RoutingDomain::LocalNetwork,
                di.clone(),
                DialInfoClass::Direct,
            )?;
            registered_addresses.insert(socket_address.to_ip_addr());
        }

        // Add static public dialinfo if it's configured
        if let Some(public_address) = public_address.as_ref() {
            // Resolve statically configured public dialinfo
            let mut public_sockaddrs = public_address
                .to_socket_addrs()
                .await
                .map_err(|e| format!("Unable to resolve address: {}\n{}", public_address, e))?;

            // Add all resolved addresses as public dialinfo
            for pdi_addr in &mut public_sockaddrs {
                // Skip addresses we already did
                if registered_addresses.contains(&pdi_addr.ip()) {
                    continue;
                }
                let pdi = DialInfo::tcp_from_socketaddr(pdi_addr);

                routing_table.register_dial_info(
                    RoutingDomain::PublicInternet,
                    pdi.clone(),
                    DialInfoClass::Direct,
                )?;
                static_public = true;

                // See if this public address is also a local interface address
                let is_interface_address = self.with_interface_addresses(|ip_addrs| {
                    for ip_addr in ip_addrs {
                        if pdi_addr.ip() == *ip_addr {
                            return true;
                        }
                    }
                    false
                });
                if is_interface_address {
                    routing_table.register_dial_info(
                        RoutingDomain::LocalNetwork,
                        pdi,
                        DialInfoClass::Direct,
                    )?;
                }
            }
        }

        if static_public {
            self.inner
                .lock()
                .static_public_dialinfo
                .insert(ProtocolType::TCP);
        }

        Ok(())
    }
}
