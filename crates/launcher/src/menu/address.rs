//! Saved-server address parsing and input bounds.

/// The default Bedrock server port displayed by launcher address fields.
pub const DEFAULT_PORT: &str = bridge::contract::DEFAULT_BEDROCK_PORT_TEXT;

/// Host and port of a saved `host:port`; a bare host gets the default port.
pub fn split_address(address: &str) -> (String, String) {
    let literal = address
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(address);
    if literal.parse::<std::net::Ipv6Addr>().is_ok() {
        return (literal.to_owned(), DEFAULT_PORT.to_owned());
    }
    match address.rsplit_once(':') {
        Some((host, port))
            if !host.is_empty() && !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) =>
        {
            (host.trim_matches(['[', ']']).to_owned(), port.to_owned())
        }
        _ => (address.to_owned(), DEFAULT_PORT.to_owned()),
    }
}

/// Maximum saved server name size in UTF-8 bytes.
pub const MAX_SERVER_NAME_BYTES: usize = 64;
/// Maximum saved server address size in UTF-8 bytes.
pub const MAX_SERVER_ADDRESS_BYTES: usize = 128;
