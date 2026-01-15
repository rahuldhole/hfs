use local_ip_address::list_afinet_netifas;
use std::net::IpAddr;

pub fn get_local_ips() -> Vec<String> {
    let mut ips = Vec::new();
    if let Ok(network_interfaces) = list_afinet_netifas() {
        for (_, ip) in network_interfaces {
            match ip {
                IpAddr::V4(ipv4) => {
                    if !ipv4.is_loopback() {
                        ips.push(ipv4.to_string());
                    }
                }
                _ => {} // Ignore IPv6 for simplicity as per requirements
            }
        }
    }
    ips
}
