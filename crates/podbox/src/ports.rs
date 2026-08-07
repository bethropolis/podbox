use std::net::{SocketAddr, TcpListener, UdpSocket};

/// A host port that is already occupied by another process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortConflict {
    /// The host port that could not be bound.
    pub port: u16,
    /// `tcp` or `udp`.
    pub proto: &'static str,
    /// The bind address that failed, e.g. `0.0.0.0:3000` or `[::]:3000`.
    pub bind: String,
}

impl std::fmt::Display for PortConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.proto, self.bind)
    }
}

/// Check every published host port for an existing listener on the host.
///
/// Mirrors how `podman --network pasta` binds published ports: a wildcard
/// bind is attempted on both IPv4 and IPv6 (dual-stack). Testing `0.0.0.0`
/// and `[::]` therefore catches listeners on either address family, including
/// IPv6-only sockets such as `[::1]:<port>`.
pub fn check_host_ports(ports: &[String]) -> Vec<PortConflict> {
    let mut conflicts = Vec::new();
    for spec in ports {
        let Some((bind_addrs, host_port)) = parse_port_spec(spec) else {
            continue;
        };
        for addr in bind_addrs {
            for proto in ["tcp", "udp"] {
                if addr_occupied(addr, proto) {
                    conflicts.push(PortConflict {
                        port: host_port,
                        proto,
                        bind: addr.to_string(),
                    });
                }
            }
        }
    }
    conflicts
}

/// Parse `hostPort:containerPort` or `ip:hostPort:containerPort` into the
/// host bind addresses to test plus the host port.
fn parse_port_spec(spec: &str) -> Option<(Vec<SocketAddr>, u16)> {
    let parts: Vec<&str> = spec.split(':').collect();
    let (ip, host_port_str) = match parts.as_slice() {
        [host_port, _container] => (None, host_port),
        [ip, host_port, _container] => (Some(*ip), host_port),
        _ => return None,
    };
    let host_port: u16 = host_port_str.parse().ok()?;
    let addrs = match ip {
        Some(ip) => vec![format!("{ip}:{host_port}").parse().ok()?],
        None => vec![
            format!("0.0.0.0:{host_port}").parse().ok()?,
            format!("[::]:{host_port}").parse().ok()?,
        ],
    };
    Some((addrs, host_port))
}

/// True if binding `addr` for `proto` fails (port already occupied).
fn addr_occupied(addr: SocketAddr, proto: &str) -> bool {
    match proto {
        "tcp" => TcpListener::bind(addr).is_err(),
        "udp" => UdpSocket::bind(addr).is_err(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wildcard_spec() {
        let (addrs, port) = parse_port_spec("3000:3000").unwrap();
        assert_eq!(port, 3000);
        assert_eq!(addrs.len(), 2);
        let ipv4: std::net::IpAddr = "0.0.0.0".parse().unwrap();
        assert_eq!(addrs[0].ip(), ipv4);
        assert!(addrs[1].is_ipv6());
    }

    #[test]
    fn parses_ip_spec() {
        let (addrs, port) = parse_port_spec("127.0.0.1:8080:80").unwrap();
        assert_eq!(port, 8080);
        assert_eq!(addrs.len(), 1);
        let ipv4: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        assert_eq!(addrs[0].ip(), ipv4);
    }

    #[test]
    fn rejects_malformed_spec() {
        assert!(parse_port_spec("not-a-port").is_none());
        assert!(parse_port_spec("a:b:c:d").is_none());
    }

    #[test]
    fn detects_existing_tcp_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let conflicts = check_host_ports(&[format!("{port}:{port}")]);
        assert!(
            conflicts.iter().any(|c| c.proto == "tcp" && c.port == port),
            "expected a TCP conflict for port {port}, got {conflicts:?}"
        );
    }

    #[test]
    fn detects_existing_udp_bind() {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = sock.local_addr().unwrap().port();
        let conflicts = check_host_ports(&[format!("{port}:{port}")]);
        assert!(
            conflicts.iter().any(|c| c.proto == "udp" && c.port == port),
            "expected a UDP conflict for port {port}, got {conflicts:?}"
        );
    }

    #[test]
    fn free_port_has_no_conflict() {
        let conflicts = check_host_ports(&["39999:39999".into()]);
        assert!(
            conflicts.is_empty(),
            "expected no conflicts for an unused port, got {conflicts:?}"
        );
    }
}
