//! RSA PKCS#1 v1.5 and signed SHA-1 server hashes, as required by Java login.
use crate::{ProtocolError, wire};
use rsa::{Pkcs1v15Encrypt, RsaPublicKey, pkcs8::DecodePublicKey, rand_core::RngCore};
use sha1::{Digest, Sha1};

pub fn response(
    server_id: &str,
    public_key: &[u8],
    challenge: &[u8],
) -> Result<([u8; 16], Vec<u8>, String), ProtocolError> {
    let key = RsaPublicKey::from_public_key_der(public_key)
        .map_err(|_| ProtocolError::Invalid("RSA public key"))?;
    let mut random = rsa::rand_core::OsRng;
    let mut secret = [0; 16];
    random.fill_bytes(&mut secret);
    let mut response = Vec::new();
    wire::bytes(
        &key.encrypt(&mut random, Pkcs1v15Encrypt, &secret)
            .map_err(|_| ProtocolError::Invalid("RSA encryption"))?,
        &mut response,
    )?;
    wire::bytes(
        &key.encrypt(&mut random, Pkcs1v15Encrypt, challenge)
            .map_err(|_| ProtocolError::Invalid("RSA challenge"))?,
        &mut response,
    )?;
    let hash = server_hash(server_id.as_bytes(), &secret, public_key);
    Ok((secret, response, hash))
}
fn server_hash(server_id: &[u8], secret: &[u8], public_key: &[u8]) -> String {
    let mut digest = Sha1::new();
    digest.update(server_id);
    digest.update(secret);
    digest.update(public_key);
    let mut bytes: [u8; 20] = digest.finalize().into();
    let negative = bytes[0] & 128 != 0;
    if negative {
        let mut carry = true;
        for byte in bytes.iter_mut().rev() {
            *byte = !*byte;
            if carry {
                let (v, c) = byte.overflowing_add(1);
                *byte = v;
                carry = c;
            }
        }
    }
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let number = hex.trim_start_matches('0');
    format!(
        "{}{}",
        if negative { "-" } else { "" },
        if number.is_empty() { "0" } else { number }
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_signed_sha1_vectors() {
        assert_eq!(
            server_hash(b"Notch", b"", b""),
            "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48"
        );
        assert_eq!(
            server_hash(b"jeb_", b"", b""),
            "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1"
        );
    }
}

#[cfg(test)]
mod rsa_tests {
    use super::*;
    use rsa::{RsaPrivateKey, pkcs8::EncodePublicKey};
    #[test]
    fn encrypted_response_is_independently_decryptable() {
        let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024).unwrap();
        let public = RsaPublicKey::from(&private).to_public_key_der().unwrap();
        let (secret, encoded, _) = response("", public.as_bytes(), &[1, 2, 3, 4]).unwrap();
        let mut r = crate::wire::Reader(&encoded);
        assert_eq!(
            private
                .decrypt(Pkcs1v15Encrypt, &r.bytes(256).unwrap())
                .unwrap(),
            secret
        );
        assert_eq!(
            private
                .decrypt(Pkcs1v15Encrypt, &r.bytes(256).unwrap())
                .unwrap(),
            [1, 2, 3, 4]
        );
        r.finish().unwrap();
    }
}
