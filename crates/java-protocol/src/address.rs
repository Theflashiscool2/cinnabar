//! Address identity stays distinct from DNS/SRV routing.
use crate::{ProtocolError, status::DEFAULT_PORT};
#[cfg(not(target_os = "ios"))]
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::rr::rdata::SRV;
use rand::Rng;
use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

#[cfg(any(target_os = "ios", test))]
#[path = "address/apple_dns.rs"]
mod apple_dns;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub host: String,
    pub port: u16,
    pub explicit_port: bool,
}
impl FromStr for Address {
    type Err = ProtocolError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let s = input.trim();
        if s.is_empty()
            || s.chars().any(|c| c.is_control() || c.is_whitespace())
            || s.contains(['/', '@', '\\'])
        {
            return Err(ProtocolError::InvalidHostname);
        }
        let (host, port) = if let Some(s) = s.strip_prefix('[') {
            let (host, tail) = s.split_once(']').ok_or(ProtocolError::InvalidHostname)?;
            if host.parse::<std::net::Ipv6Addr>().is_err() {
                return Err(ProtocolError::InvalidHostname);
            }
            (
                host,
                if tail.is_empty() {
                    None
                } else {
                    Some(
                        tail.strip_prefix(':')
                            .ok_or(ProtocolError::InvalidHostname)?,
                    )
                },
            )
        } else if s.parse::<IpAddr>().is_ok() {
            (s, None)
        } else if let Some((host, port)) = s.rsplit_once(':') {
            if host.contains(':') {
                return Err(ProtocolError::InvalidHostname);
            }
            (host, Some(port))
        } else {
            (s, None)
        };
        if host.is_empty() || host.len() > 255 || host.contains(['[', ']']) {
            return Err(ProtocolError::InvalidHostname);
        }
        let explicit_port = port.is_some();
        let port = port
            .map(|p| p.parse::<u16>().ok().filter(|p| *p != 0))
            .unwrap_or(Some(DEFAULT_PORT))
            .ok_or(ProtocolError::InvalidHostname)?;
        Ok(Self {
            host: host.to_owned(),
            port,
            explicit_port,
        })
    }
}
/// Honors explicit ports; SRV changes only the socket destination, never the handshake host.
pub async fn resolve(address: &Address) -> Result<Vec<SocketAddr>, std::io::Error> {
    if let Ok(ip) = address.host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, address.port)]);
    }
    // An explicit port disables SRV. Use the OS resolver, including iOS's network-aware
    // getaddrinfo implementation, rather than requiring a Unix resolv.conf file.
    if address.explicit_port {
        return tokio::net::lookup_host((address.host.as_str(), address.port))
            .await
            .map(|addresses| addresses.collect());
    }
    let records = srv_records(&address.host).await?;
    let (host, port) = srv_destination(address, records)?;
    let sockets: Vec<_> = tokio::net::lookup_host((host.as_str(), port))
        .await?
        .collect();
    if sockets.is_empty() {
        return Err(std::io::Error::other("Java hostname has no addresses"));
    }
    Ok(sockets)
}

/// iOS supplies DNS through its system service, not /etc/resolv.conf.
#[cfg(target_os = "ios")]
async fn srv_records(host: &str) -> Result<Vec<SRV>, std::io::Error> {
    apple_dns::lookup(&format!("_minecraft._tcp.{host}")).await
}

#[cfg(not(target_os = "ios"))]
async fn srv_records(host: &str) -> Result<Vec<SRV>, std::io::Error> {
    let resolver = TokioResolver::builder_tokio()
        .map_err(std::io::Error::other)?
        .build();
    match resolver.srv_lookup(format!("_minecraft._tcp.{host}")).await {
        Ok(records) => Ok(records.iter().cloned().collect()),
        Err(e) if e.is_no_records_found() => Ok(Vec::new()),
        Err(e) => Err(std::io::Error::other(e)),
    }
}

/// Preserve virtual-host identity while selecting the lowest-priority SRV group.
fn srv_destination(
    address: &Address,
    mut records: Vec<SRV>,
) -> Result<(String, u16), std::io::Error> {
    records.sort_by_key(|r| r.priority());
    if let Some(first) = records.first() {
        let priority = first.priority();
        records.retain(|r| r.priority() == priority);
        let total: u32 = records.iter().map(|r| u32::from(r.weight())).sum();
        let selected = if total == 0 {
            rand::thread_rng().gen_range(0..records.len())
        } else {
            let mut draw = rand::thread_rng().gen_range(0..total);
            records
                .iter()
                .position(|r| {
                    let w = u32::from(r.weight());
                    if draw < w {
                        true
                    } else {
                        draw -= w;
                        false
                    }
                })
                .unwrap_or(0)
        };
        let record = &records[selected];
        let host = record.target().to_utf8();
        let port = record.port();
        if host == "." || port == 0 {
            return Err(std::io::Error::other("Java service unavailable via SRV"));
        }
        return Ok((host, port));
    }
    Ok((address.host.clone(), address.port))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srv_routes_socket_without_rewriting_identity_or_hiding_unavailability() {
        let address: Address = "virtual.test".parse().unwrap();
        let record =
            |priority, port, host: &str| SRV::new(priority, 1, port, host.parse().unwrap());
        assert_eq!(
            srv_destination(&address, vec![]).unwrap(),
            ("virtual.test".into(), DEFAULT_PORT)
        );
        assert_eq!(
            srv_destination(
                &address,
                vec![
                    record(20, 25565, "other.test."),
                    record(10, 25566, "route.test.")
                ]
            )
            .unwrap(),
            ("route.test.".into(), 25566)
        );
        assert_eq!(address.host, "virtual.test");
        assert!(srv_destination(&address, vec![record(0, 25565, ".")]).is_err());
        assert!(srv_destination(&address, vec![record(0, 0, "route.test.")]).is_err());
    }
    #[tokio::test]
    async fn explicit_hostname_port_uses_system_addresses_without_srv() {
        let address: Address = "localhost:25566".parse().unwrap();
        let sockets = resolve(&address).await.unwrap();
        assert!(!sockets.is_empty());
        assert!(
            sockets
                .iter()
                .all(|socket| socket.ip().is_loopback() && socket.port() == 25566)
        );
        assert_eq!(address.host, "localhost");
    }
    #[test]
    fn literal_hostname_and_explicit_port_identity() {
        for (input, host, port, explicit) in [
            ("example.test", "example.test", DEFAULT_PORT, false),
            ("example.test:25566", "example.test", 25566, true),
            ("[::1]", "::1", DEFAULT_PORT, false),
            ("[::1]:25566", "::1", 25566, true),
            ("::1", "::1", DEFAULT_PORT, false),
            ("127.0.0.1:25565", "127.0.0.1", 25565, true),
        ] {
            let parsed: Address = input.parse().unwrap();
            assert_eq!(
                (parsed.host.as_str(), parsed.port, parsed.explicit_port),
                (host, port, explicit)
            );
        }
        for invalid in [
            "",
            "[::1",
            "example.test:0",
            "host:65536",
            "host:bad",
            "user@host",
            "https://host",
            "a\0b",
            "[wrong]",
        ] {
            assert!(invalid.parse::<Address>().is_err(), "{invalid:?}");
        }
    }
}
