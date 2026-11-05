//! Native profile-certificate and chat-signature primitives. Roots must come from
//! authenticated Minecraft Services discovery, never from a server's player list.
use super::window::{SIGNATURE_BYTES, Signature, WINDOW};
use crate::ProtocolError as Error;
use rsa::{Pkcs1v15Sign, RsaPublicKey, pkcs8::DecodePublicKey, traits::PublicKeyParts};
use sha2::Digest;

/// Public trust material received through the application's authenticated service boundary.
/// This is not constructed from remote player certificates or server resource packs.
pub(crate) struct Roots(Vec<RsaPublicKey>);
impl Roots {
    pub(crate) fn from_service_keys(
        keys: impl IntoIterator<Item = Vec<u8>>,
    ) -> Result<Self, Error> {
        let keys = keys
            .into_iter()
            .map(|der| {
                RsaPublicKey::from_public_key_der(&der)
                    .map_err(|_| Error::Invalid("Minecraft chat service public key"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() {
            return Err(Error::Invalid(
                "Minecraft chat certificate trust set is empty",
            ));
        }
        Ok(Self(keys))
    }
    /// Validates the UUID-bound v2 profile certificate before granting message authority.
    pub(crate) fn verify_profile(
        &self,
        uuid: [u8; 16],
        expires_at: i64,
        der: &[u8],
        signature: &[u8],
    ) -> Result<RsaPublicKey, Error> {
        let key = RsaPublicKey::from_public_key_der(der)
            .map_err(|_| Error::Invalid("Java profile public key"))?;
        if key.size() != SIGNATURE_BYTES {
            return Err(Error::Invalid("Java message-signature key size"));
        }
        let mut payload = Vec::with_capacity(24 + der.len());
        payload.extend(uuid);
        payload.extend(expires_at.to_be_bytes());
        payload.extend(der);
        let digest = sha1::Sha1::digest(payload);
        if !self.0.iter().any(|root| {
            root.verify(Pkcs1v15Sign::new::<sha1::Sha1>(), &digest, signature)
                .is_ok()
        }) {
            return Err(Error::Invalid("untrusted Java chat profile certificate"));
        }
        Ok(key)
    }
}
/// The exact signed body, separate from wire-only global indexes and unsigned decorations.
pub(crate) struct Body<'a> {
    pub sender: [u8; 16],
    pub session: [u8; 16],
    pub index: i32,
    pub text: &'a str,
    pub timestamp_millis: i64,
    pub salt: i64,
    pub last_seen: &'a [Signature],
}
impl Body<'_> {
    pub(crate) fn payload(&self) -> Result<Vec<u8>, Error> {
        if self.index < 0
            || self.last_seen.len() > WINDOW
            || self.text.encode_utf16().count() > super::PLAYER_MAX_UTF16
        {
            return Err(Error::Invalid("Java signed message body bounds"));
        }
        let mut bytes =
            Vec::with_capacity(64 + self.text.len() + self.last_seen.len() * SIGNATURE_BYTES);
        bytes.extend(1i32.to_be_bytes());
        bytes.extend(self.sender);
        bytes.extend(self.session);
        bytes.extend(self.index.to_be_bytes());
        bytes.extend(self.salt.to_be_bytes());
        // Instant.getEpochSecond floors negative milliseconds; truncating division is wrong.
        bytes.extend(self.timestamp_millis.div_euclid(1000).to_be_bytes());
        bytes.extend((self.text.len() as i32).to_be_bytes());
        bytes.extend(self.text.as_bytes());
        bytes.extend((self.last_seen.len() as i32).to_be_bytes());
        for signature in self.last_seen {
            bytes.extend(signature);
        }
        Ok(bytes)
    }
    pub(crate) fn verify(&self, key: &RsaPublicKey, signature: &Signature) -> Result<(), Error> {
        key.verify(
            Pkcs1v15Sign::new::<sha2::Sha256>(),
            &sha2::Sha256::digest(self.payload()?),
            signature,
        )
        .map_err(|_| Error::Invalid("invalid Java player message signature"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rsa::{RsaPrivateKey, pkcs8::EncodePublicKey};
    #[test]
    fn native_signed_body_digest_and_negative_epoch_floor() {
        let mut a = [0; SIGNATURE_BYTES];
        a[0] = 1;
        let mut b = [0; SIGNATURE_BYTES];
        b[0] = 2;
        let body = Body {
            sender: uuid::Uuid::from_u128((1 << 64) | 2).into_bytes(),
            session: uuid::Uuid::from_u128((3 << 64) | 4).into_bytes(),
            index: 2,
            text: "Hello 夜",
            timestamp_millis: 1700000000123,
            salt: 42,
            last_seen: &[a, b],
        };
        // ProbeSigning invokes official26.3 PlayerChatMessage.updateSignature. This expected
        // digest is independent of this encoder and includes both packed-cache-expanded signatures.
        assert_eq!(
            sha2::Sha256::digest(body.payload().unwrap()).as_slice(),
            &[
                74, 87, 162, 103, 226, 187, 3, 28, 51, 197, 236, 171, 23, 147, 39, 188, 39, 247,
                16, 233, 46, 146, 150, 16, 155, 106, 63, 189, 191, 205, 123, 9
            ]
        );
        let body = Body {
            timestamp_millis: -1,
            last_seen: &[],
            ..body
        };
        let payload = body.payload().unwrap();
        assert_eq!(&payload[48..56], &(-1i64).to_be_bytes());
    }
    #[test]
    fn certificate_uuid_expiry_root_and_message_tampering_are_rejected() {
        // Disposable synthetic keys test validation; they do not claim Mojang-issued authority.
        let mut rng = rand::rngs::StdRng::seed_from_u64(4512);
        let root = RsaPrivateKey::new(&mut rng, 1024).unwrap();
        let private = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let public = private.to_public_key();
        let der = public.to_public_key_der().unwrap();
        let owner = [7; 16];
        let expires = 5000i64;
        let mut certificate = owner.to_vec();
        certificate.extend(expires.to_be_bytes());
        certificate.extend(der.as_bytes());
        let signature = root
            .sign(
                Pkcs1v15Sign::new::<sha1::Sha1>(),
                &sha1::Sha1::digest(certificate),
            )
            .unwrap();
        let roots = Roots::from_service_keys([root
            .to_public_key()
            .to_public_key_der()
            .unwrap()
            .as_bytes()
            .to_vec()])
        .unwrap();
        assert_eq!(
            roots
                .verify_profile(owner, expires, der.as_bytes(), &signature)
                .unwrap(),
            public
        );
        assert!(
            roots
                .verify_profile([8; 16], expires, der.as_bytes(), &signature)
                .is_err()
        );
        assert!(
            roots
                .verify_profile(owner, expires + 1, der.as_bytes(), &signature)
                .is_err()
        );
        let body = Body {
            sender: owner,
            session: [9; 16],
            index: 0,
            text: "original",
            timestamp_millis: 100,
            salt: 3,
            last_seen: &[],
        };
        let signature: Signature = private
            .sign(
                Pkcs1v15Sign::new::<sha2::Sha256>(),
                &sha2::Sha256::digest(body.payload().unwrap()),
            )
            .unwrap()
            .try_into()
            .unwrap();
        body.verify(&public, &signature).unwrap();
        let changed = Body {
            text: "forged",
            ..body
        };
        assert!(changed.verify(&public, &signature).is_err());
    }
}
