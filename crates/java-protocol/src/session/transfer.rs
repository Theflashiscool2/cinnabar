//! Server-directed socket replacement, distinct from same-socket configuration.
use super::*;

pub(super) fn display(address: &Address) -> String {
    if address.host.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{}]:{}", address.host, address.port)
    } else {
        format!("{}:{}", address.host, address.port)
    }
}
pub(super) fn destination(r: &mut Reader<'_>) -> Result<Address, ProtocolError> {
    let host = r.string(255)?;
    let port = u16::try_from(r.var()?).map_err(|_| ProtocolError::Invalid("transfer port"))?;
    Reader(r.0).finish()?;
    if host.is_empty() || host.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(ProtocolError::InvalidHostname);
    }
    let address = Address {
        host,
        port,
        explicit_port: true,
    };
    // Keep the ordinary parser's hostname rules and never perform SRV routing for a
    // port explicitly provided by a transfer packet. IPv6 brackets are presentation only.
    display(&address).parse()
}

impl Connection {
    pub(super) fn store_cookie(&mut self, r: &mut Reader<'_>) -> Result<(), Error> {
        let key = r.string(32767)?;
        let value = r.bytes(5120)?;
        let retained: usize = self
            .cookies
            .iter()
            .filter(|(k, _)| *k != &key)
            .map(|(k, v)| k.len() + v.len())
            .sum();
        if retained + key.len() + value.len() > 65536 {
            return Err(ProtocolError::Invalid("cookie budget").into());
        }
        self.cookies.insert(key, value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(host: &str, port: i32) -> Vec<u8> {
        let mut b = Vec::new();
        codec::write_string(host, 255, &mut b).unwrap();
        codec::write_varint(port, &mut b);
        b
    }
    #[test]
    fn explicit_destination_preserves_port_and_ipv6_without_srv() {
        for (host, port) in [
            ("example.test", 25567),
            ("::1", 25565),
            ("127.0.0.1", 25566),
        ] {
            let a = destination(&mut Reader(&packet(host, port))).unwrap();
            assert_eq!(a.host, host);
            assert_eq!(a.port, port as u16);
            assert!(a.explicit_port);
        }
    }
    #[test]
    fn malformed_destinations_and_trailing_data_never_become_connections() {
        for (host, port) in [
            ("", 1),
            (" bad.test", 1),
            ("x.test/path", 1),
            ("x.test", -1),
            ("x.test", 65536),
        ] {
            assert!(destination(&mut Reader(&packet(host, port))).is_err());
        }
        let mut b = packet("example.test", 25565);
        b.push(0);
        assert!(destination(&mut Reader(&b)).is_err());
        for end in 0..b.len() - 1 {
            assert!(destination(&mut Reader(&b[..end])).is_err());
        }
    }
}
