use super::*;
use alloc::collections::btree_map::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressFilterError {
    CountExceeded,
    RateExceeded,
}
impl fmt::Display for AddressFilterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                Self::CountExceeded => "Count exceeded",
                Self::RateExceeded => "Rate exceeded",
            }
        )
    }
}
impl std::error::Error for AddressFilterError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressNotInTableError {}
impl fmt::Display for AddressNotInTableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Address not in table")
    }
}
impl std::error::Error for AddressNotInTableError {}

#[derive(Debug)]
pub struct ConnectionLimits {
    max_connections_per_ip4: usize,
    max_connections_per_ip6_prefix: usize,
    max_connections_per_ip6_prefix_size: usize,
    max_connection_frequency_per_min: usize,
    conn_count_by_ip4: BTreeMap<Ipv4Addr, usize>,
    conn_count_by_ip6_prefix: BTreeMap<Ipv6Addr, usize>,
    conn_timestamps_by_ip4: BTreeMap<Ipv4Addr, Vec<u64>>,
    conn_timestamps_by_ip6_prefix: BTreeMap<Ipv6Addr, Vec<u64>>,
}

impl ConnectionLimits {
    pub fn new(config: VeilidConfig) -> Self {
        let c = config.get();
        Self {
            max_connections_per_ip4: c.network.max_connections_per_ip4 as usize,
            max_connections_per_ip6_prefix: c.network.max_connections_per_ip6_prefix as usize,
            max_connections_per_ip6_prefix_size: c.network.max_connections_per_ip6_prefix_size
                as usize,
            max_connection_frequency_per_min: c.network.max_connection_frequency_per_min as usize,
            conn_count_by_ip4: BTreeMap::new(),
            conn_count_by_ip6_prefix: BTreeMap::new(),
            conn_timestamps_by_ip4: BTreeMap::new(),
            conn_timestamps_by_ip6_prefix: BTreeMap::new(),
        }
    }

    // Converts an ip to a ip block by applying a netmask
    // to the host part of the ip address
    // ipv4 addresses are treated as single hosts
    // ipv6 addresses are treated as prefix allocated blocks
    fn ip_to_ipblock(&self, addr: IpAddr) -> IpAddr {
        match addr {
            IpAddr::V4(_) => addr,
            IpAddr::V6(v6) => {
                let mut hostlen = 128usize.saturating_sub(self.max_connections_per_ip6_prefix_size);
                let mut out = v6.octets();
                for i in (0..16).rev() {
                    if hostlen >= 8 {
                        out[i] = 0xFF;
                        hostlen -= 8;
                    } else {
                        out[i] |= !(0xFFu8 << hostlen);
                        break;
                    }
                }
                IpAddr::V6(Ipv6Addr::from(out))
            }
        }
    }

    fn purge_old_timestamps(&mut self, cur_ts: u64) {
        // v4
        {
            let mut dead_keys = Vec::<Ipv4Addr>::new();
            for (key, value) in &mut self.conn_timestamps_by_ip4 {
                value.retain(|v| {
                    // keep timestamps that are less than a minute away
                    cur_ts.saturating_sub(*v) < 60_000_000u64
                });
                if value.is_empty() {
                    dead_keys.push(*key);
                }
            }
            for key in dead_keys {
                self.conn_timestamps_by_ip4.remove(&key);
            }
        }
        // v6
        {
            let mut dead_keys = Vec::<Ipv6Addr>::new();
            for (key, value) in &mut self.conn_timestamps_by_ip6_prefix {
                value.retain(|v| {
                    // keep timestamps that are less than a minute away
                    cur_ts.saturating_sub(*v) < 60_000_000u64
                });
                if value.is_empty() {
                    dead_keys.push(*key);
                }
            }
            for key in dead_keys {
                self.conn_timestamps_by_ip6_prefix.remove(&key);
            }
        }
    }

    pub fn add(&mut self, addr: IpAddr) -> Result<(), AddressFilterError> {
        let ipblock = self.ip_to_ipblock(addr);
        let ts = intf::get_timestamp();

        self.purge_old_timestamps(ts);

        match ipblock {
            IpAddr::V4(v4) => {
                // See if we have too many connections from this ip block
                let cnt = &mut *self.conn_count_by_ip4.entry(v4).or_default();
                assert!(*cnt <= self.max_connections_per_ip4);
                if *cnt == self.max_connections_per_ip4 {
                    warn!("address filter count exceeded: {:?}", v4);
                    return Err(AddressFilterError::CountExceeded);
                }
                // See if this ip block has connected too frequently
                let tstamps = &mut self.conn_timestamps_by_ip4.entry(v4).or_default();
                tstamps.retain(|v| {
                    // keep timestamps that are less than a minute away
                    ts.saturating_sub(*v) < 60_000_000u64
                });
                assert!(tstamps.len() <= self.max_connection_frequency_per_min);
                if tstamps.len() == self.max_connection_frequency_per_min {
                    warn!("address filter rate exceeded: {:?}", v4);
                    return Err(AddressFilterError::RateExceeded);
                }

                // If it's okay, add the counts and timestamps
                *cnt += 1;
                tstamps.push(ts);
            }
            IpAddr::V6(v6) => {
                // See if we have too many connections from this ip block
                let cnt = &mut *self.conn_count_by_ip6_prefix.entry(v6).or_default();
                assert!(*cnt <= self.max_connections_per_ip6_prefix);
                if *cnt == self.max_connections_per_ip6_prefix {
                    warn!("address filter count exceeded: {:?}", v6);
                    return Err(AddressFilterError::CountExceeded);
                }
                // See if this ip block has connected too frequently
                let tstamps = &mut self.conn_timestamps_by_ip6_prefix.entry(v6).or_default();
                assert!(tstamps.len() <= self.max_connection_frequency_per_min);
                if tstamps.len() == self.max_connection_frequency_per_min {
                    warn!("address filter rate exceeded: {:?}", v6);
                    return Err(AddressFilterError::RateExceeded);
                }

                // If it's okay, add the counts and timestamps
                *cnt += 1;
                tstamps.push(ts);
            }
        }
        Ok(())
    }

    pub fn remove(&mut self, addr: IpAddr) -> Result<(), AddressNotInTableError> {
        let ipblock = self.ip_to_ipblock(addr);

        let ts = intf::get_timestamp();
        self.purge_old_timestamps(ts);

        match ipblock {
            IpAddr::V4(v4) => {
                match self.conn_count_by_ip4.entry(v4) {
                    Entry::Vacant(_) => {
                        return Err(AddressNotInTableError {});
                    }
                    Entry::Occupied(mut o) => {
                        let cnt = o.get_mut();
                        assert!(*cnt > 0);
                        if *cnt == 0 {
                            self.conn_count_by_ip4.remove(&v4);
                        } else {
                            *cnt -= 1;
                        }
                    }
                };
            }
            IpAddr::V6(v6) => {
                match self.conn_count_by_ip6_prefix.entry(v6) {
                    Entry::Vacant(_) => {
                        return Err(AddressNotInTableError {});
                    }
                    Entry::Occupied(mut o) => {
                        let cnt = o.get_mut();
                        assert!(*cnt > 0);
                        if *cnt == 0 {
                            self.conn_count_by_ip6_prefix.remove(&v6);
                        } else {
                            *cnt -= 1;
                        }
                    }
                };
            }
        }
        Ok(())
    }
}
