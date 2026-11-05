//! Native public-service discovery and account-bound chat certificates. Private key material
//! remains in memory; it never enters account serialization, Debug output or a resource pack.
use super::{Account, Error, HttpService, Service};
use crate::session::chat::security::Roots;
use base64::Engine;
use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey, traits::PublicKeyParts};
use serde_json::Value;
use zeroize::{Zeroize, Zeroizing};
const DISCOVERY: &str = "https://discovery.minecraftservices.com/minecraft/client";
/// AccountProfileKeyPairManager.MINIMUM_PROFILE_KEY_REFRESH_INTERVAL in26.3.
pub(crate) const MINIMUM_REFRESH_INTERVAL_MILLIS: i64 = 60 * 60 * 1000;
const STAGE: &str = "Minecraft chat certificates";
/// Neither private-key storage nor this holder implements Debug or Serialize.
pub(crate) struct Certificate {
    pub owner: [u8; 16],
    pub expires_at: i64,
    pub refreshed_after: i64,
    pub public_der: Vec<u8>,
    pub signature: Vec<u8>,
    pub private: RsaPrivateKey,
}
pub(crate) struct Material {
    pub roots: Roots,
    pub certificate: Option<Certificate>,
}
/// Called as cancellable connection-owned preparation, never as part of a render frame.
/// Account stays owned by this future and zeroizes its tokens when preparation ends/drops.
pub(crate) async fn prepare(
    account: std::sync::Arc<Account>,
    private_allowed: bool,
) -> Result<Material, Error> {
    let http = HttpService::default();
    let metadata = http
        .request("Minecraft chat discovery", DISCOVERY, None, None, false)
        .await?;
    let roots_url = endpoint(&metadata, "authentication", "getPublicKeys")?;
    let certificate_url = endpoint(&metadata, "player", "getCertificates")?;
    let keys = http
        .request("Minecraft chat trust keys", &roots_url, None, None, false)
        .await?;
    let roots = parse_roots(&keys)?;
    if !private_allowed || !account.valid_at(super::unix_now()) {
        return Ok(Material {
            roots,
            certificate: None,
        });
    }
    // Certificate denial does not change identity or silently select an offline account.
    // Servers may permit unsigned chat; a secure-chat requirement is checked independently.
    let response = http
        .request_method(
            STAGE,
            &certificate_url,
            Some(&account.access_token),
            None,
            false,
            true,
        )
        .await;
    let certificate = match response {
        Ok(value) => match parse_certificate(
            value,
            account.profile.id.into_bytes(),
            &roots,
            now_millis()?,
        ) {
            Ok(certificate) => Some(certificate),
            Err(error) => {
                tracing::warn!(target:"java_auth",%error,"Java chat certificate unavailable");
                None
            }
        },
        Err(error) => {
            tracing::warn!(target:"java_auth",%error,"Java chat certificate unavailable");
            None
        }
    };
    Ok(Material { roots, certificate })
}
fn now_millis() -> Result<i64, Error> {
    let value = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Error::Service(STAGE))?
        .as_millis();
    i64::try_from(value).map_err(|_| Error::Service(STAGE))
}
fn endpoint(metadata: &Value, service: &str, name: &str) -> Result<String, Error> {
    let value = metadata["discovery"][service]["endpoints"][name]["uri"]
        .as_str()
        .ok_or(Error::Service(STAGE))?;
    let url = reqwest::Url::parse(value).map_err(|_| Error::Service(STAGE))?;
    // Only the official HTTPS service family may receive a Minecraft bearer token.
    // Discovery itself is fixed above and the shared HTTP client refuses redirects.
    let official = url.host_str().is_some_and(|host| {
        host == "minecraftservices.com" || host.ends_with(".minecraftservices.com")
    });
    if !official
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.fragment().is_some()
    {
        return Err(Error::Service(STAGE));
    }
    Ok(url.into())
}
fn parse_roots(value: &Value) -> Result<Roots, Error> {
    let keys = value["playerCertificateKeys"]
        .as_array()
        .ok_or(Error::Service(STAGE))?;
    let keys = keys
        .iter()
        .map(|key| decode(key["publicKey"].as_str().ok_or(Error::Service(STAGE))?))
        .collect::<Result<Vec<_>, _>>()?;
    Roots::from_service_keys(keys).map_err(|_| Error::Service(STAGE))
}
fn decode(value: &str) -> Result<Vec<u8>, Error> {
    // Reuse the protocol's already pinned base64 implementation instead of a second parser.
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| Error::Service(STAGE))
}
fn pem(value: &str, label: &str) -> Result<Zeroizing<Vec<u8>>, Error> {
    let prefix = format!("-----BEGIN {label}-----");
    let suffix = format!("-----END {label}-----");
    let body = value
        .trim()
        .strip_prefix(&prefix)
        .and_then(|s| s.strip_suffix(&suffix))
        .ok_or(Error::Service(STAGE))?;
    let body = Zeroizing::new(
        body.chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>(),
    );
    let mut out = Zeroizing::new(vec![0; base64::decoded_len_estimate(body.len())]);
    let count = base64::engine::general_purpose::STANDARD
        .decode_slice(body.as_bytes(), &mut *out)
        .map_err(|_| Error::Service(STAGE))?;
    out.truncate(count);
    Ok(out)
}
/// Zeroizes all remaining response strings, including on any early parsing error.
struct SecretResponse(Value);
impl Drop for SecretResponse {
    fn drop(&mut self) {
        fn erase(value: &mut Value) {
            match value {
                Value::String(s) => s.zeroize(),
                Value::Array(values) => values.iter_mut().for_each(erase),
                Value::Object(fields) => fields.values_mut().for_each(erase),
                _ => {}
            }
        }
        erase(&mut self.0)
    }
}
fn parse_certificate(
    value: Value,
    owner: [u8; 16],
    roots: &Roots,
    now: i64,
) -> Result<Certificate, Error> {
    let mut value = SecretResponse(value);
    let expires_at = chrono::DateTime::parse_from_rfc3339(
        value.0["expiresAt"].as_str().ok_or(Error::Service(STAGE))?,
    )
    .map_err(|_| Error::Service(STAGE))?
    .timestamp_millis();
    let refreshed_after = chrono::DateTime::parse_from_rfc3339(
        value.0["refreshedAfter"]
            .as_str()
            .ok_or(Error::Service(STAGE))?,
    )
    .map_err(|_| Error::Service(STAGE))?
    .timestamp_millis();
    if expires_at < now {
        return Err(Error::Service(STAGE));
    }
    let signature = decode(
        value.0["publicKeySignatureV2"]
            .as_str()
            .ok_or(Error::Service(STAGE))?,
    )?;
    let public_der = pem(
        value.0["keyPair"]["publicKey"]
            .as_str()
            .ok_or(Error::Service(STAGE))?,
        "RSA PUBLIC KEY",
    )?
    .to_vec();
    let key = roots
        .verify_profile(owner, expires_at, &public_der, &signature)
        .map_err(|_| Error::Service(STAGE))?;
    let private_pem = value.0["keyPair"]["privateKey"].take();
    let Value::String(private_pem) = private_pem else {
        return Err(Error::Service(STAGE));
    };
    let private_pem = Zeroizing::new(private_pem);
    let private = RsaPrivateKey::from_pkcs8_der(&pem(&private_pem, "RSA PRIVATE KEY")?)
        .map_err(|_| Error::Service(STAGE))?;
    if private.n() != key.n() || private.e() != key.e() {
        return Err(Error::Service(STAGE));
    }
    Ok(Certificate {
        owner,
        expires_at,
        refreshed_after,
        public_der,
        signature,
        private,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_never_sends_bearers_to_external_or_cleartext_endpoints() {
        for url in [
            "http://api.minecraftservices.com/player/certificates",
            "https://api.minecraftservices.com.evil.test/cert",
            "https://evil.test/cert",
            "https://user:pass@api.minecraftservices.com/cert",
            "https://api.minecraftservices.com:444/cert",
            "https://127.0.0.1/cert",
        ] {
            let metadata = serde_json::json!({"discovery":{"player":{"endpoints":{"getCertificates":{"uri":url}}}}});
            assert!(endpoint(&metadata, "player", "getCertificates").is_err());
        }
        let metadata = serde_json::json!({"discovery":{"player":{"endpoints":{"getCertificates":{"uri":"https://api.minecraftservices.com/player/certificates"}}}}});
        assert_eq!(
            endpoint(&metadata, "player", "getCertificates").unwrap(),
            "https://api.minecraftservices.com/player/certificates"
        );
        assert!(endpoint(&Value::Null, "player", "getCertificates").is_err());
    }
    #[test]
    fn service_certificate_pem_key_match_uuid_and_expiry() {
        use rand::SeedableRng;
        use rsa::{
            Pkcs1v15Sign,
            pkcs8::{EncodePrivateKey, EncodePublicKey},
        };
        use sha2::Digest;
        let mut rng = rand::rngs::StdRng::seed_from_u64(7822);
        let root = RsaPrivateKey::new(&mut rng, 1024).unwrap();
        let private = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let public = private.to_public_key().to_public_key_der().unwrap();
        let owner = [12; 16];
        // Independently known epoch for2099-01-01T00:00:00Z.
        let expires = 4_070_908_800_000i64;
        let mut payload = owner.to_vec();
        payload.extend(expires.to_be_bytes());
        payload.extend(public.as_bytes());
        let signature = root
            .sign(
                Pkcs1v15Sign::new::<sha1::Sha1>(),
                &sha1::Sha1::digest(payload),
            )
            .unwrap();
        let roots = Roots::from_service_keys([root
            .to_public_key()
            .to_public_key_der()
            .unwrap()
            .as_bytes()
            .to_vec()])
        .unwrap();
        let pem = |label: &str, bytes: &[u8]| {
            format!(
                "-----BEGIN {label}-----\n{}\n-----END {label}-----",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        };
        let mut data = serde_json::json!({"expiresAt":"2099-01-01T00:00:00Z","refreshedAfter":"2098-01-01T00:00:00Z","publicKeySignatureV2":base64::engine::general_purpose::STANDARD.encode(&signature),"keyPair":{"publicKey":pem("RSA PUBLIC KEY",public.as_bytes()),"privateKey":pem("RSA PRIVATE KEY",private.to_pkcs8_der().unwrap().as_bytes())}});
        let cert = parse_certificate(data.clone(), owner, &roots, expires - 1).unwrap();
        assert_eq!(cert.expires_at, expires);
        assert_eq!(cert.owner, owner);
        assert_eq!(cert.public_der, public.as_bytes());
        assert!(parse_certificate(data.clone(), [13; 16], &roots, expires - 1).is_err());
        assert!(parse_certificate(data.clone(), owner, &roots, expires + 1).is_err());
        data["keyPair"]["privateKey"] = Value::String(pem(
            "RSA PRIVATE KEY",
            root.to_pkcs8_der().unwrap().as_bytes(),
        ));
        assert!(parse_certificate(data, owner, &roots, expires - 1).is_err());
    }
}

pub(crate) fn now_millis_protocol() -> Result<i64, crate::ProtocolError> {
    now_millis().map_err(|_| crate::ProtocolError::Invalid("Java chat clock"))
}
