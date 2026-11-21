//! Adapters for the credential ports: `Argon2id` password hashing,
//! `XChaCha20Poly1305` sealing of the users' LLM keys, and the randomness
//! behind session tokens. All of the `RustCrypto` types stay in this file.
//!
//! Crates and why (ADR 0008): `argon2` (the password-hashing crate of the
//! `RustCrypto` project, PHC strings, `Argon2id`), `chacha20poly1305` (the AEAD of
//! the same family; `XChaCha20` has a 192-bit nonce, large enough to pick every
//! nonce at random without tracking a counter), `sha2` (token digests),
//! `getrandom` (the operating system's random source).

use std::fmt;

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher as _, PasswordVerifier as _};
use argon2::{Algorithm, Argon2, Params, Version};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};
use domain::{
    CryptoError, Password, PasswordHasher, SealedSecret, Secret, SecretCipher, StoredPasswordHash,
    TokenDigest, TokenIssuer,
};
use sha2::{Digest, Sha256};

/// Argon2id costs: the OWASP Password Storage Cheat Sheet's first
/// recommendation (19 MiB of memory, 2 iterations, 1 lane), which is also
/// the `argon2` crate's default. Fixed here so an update of the crate cannot
/// change them silently; hashes carry their own parameters, so raising them
/// later does not invalidate stored ones.
pub const ARGON2_MEMORY_KIB: u32 = 19 * 1024;
pub const ARGON2_ITERATIONS: u32 = 2;
pub const ARGON2_LANES: u32 = 1;

/// Hashes or verifications running at once: each takes 19 MiB, and a burst of
/// logins must not be able to exhaust the memory of the host.
pub const ARGON2_CONCURRENCY: usize = 3;

pub struct Argon2idHasher {
    argon2: Argon2<'static>,
    slots: std::sync::Arc<tokio::sync::Semaphore>,
}

impl Argon2idHasher {
    #[must_use]
    pub fn new() -> Self {
        // The constants are valid, so `with_costs` cannot fail for them.
        Self::with_costs(ARGON2_MEMORY_KIB, ARGON2_ITERATIONS, ARGON2_LANES).unwrap_or_else(|_| {
            Self {
                argon2: Argon2::default(),
                slots: std::sync::Arc::new(tokio::sync::Semaphore::new(ARGON2_CONCURRENCY)),
            }
        })
    }

    /// Custom costs, for tests that hash many passwords. Production code uses
    /// [`Argon2idHasher::new`].
    pub fn with_costs(memory_kib: u32, iterations: u32, lanes: u32) -> Result<Self, CryptoError> {
        let params =
            Params::new(memory_kib, iterations, lanes, None).map_err(|_| CryptoError::Failed)?;
        Ok(Self {
            argon2: Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
            slots: std::sync::Arc::new(tokio::sync::Semaphore::new(ARGON2_CONCURRENCY)),
        })
    }
}

impl Default for Argon2idHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PasswordHasher for Argon2idHasher {
    async fn hash(&self, password: &Password) -> Result<StoredPasswordHash, CryptoError> {
        let argon2 = self.argon2.clone();
        let password = password.clone();
        let _slot = self
            .slots
            .acquire()
            .await
            .map_err(|_| CryptoError::Failed)?;
        tokio::task::spawn_blocking(move || {
            let hash: PasswordHash = argon2
                .hash_password(password.expose().as_bytes())
                .map_err(|_| CryptoError::Failed)?;
            Ok(StoredPasswordHash::new(hash.to_string()))
        })
        .await
        .map_err(|_| CryptoError::Failed)?
    }

    async fn verify(&self, password: &Secret, hash: &StoredPasswordHash) -> bool {
        let argon2 = self.argon2.clone();
        let password = password.clone();
        let hash = hash.clone();
        let Ok(_slot) = self.slots.acquire().await else {
            return false;
        };
        tokio::task::spawn_blocking(move || {
            let Ok(parsed) = PasswordHash::new(hash.as_str()) else {
                return false;
            };
            argon2
                .verify_password(password.expose().as_bytes(), &parsed)
                .is_ok()
        })
        .await
        .unwrap_or(false)
    }
}

/// The server key (`TEACH_ME_SECRET_KEY`): 32 bytes, never printed.
#[derive(Clone)]
pub struct ServerKey([u8; 32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ServerKeyError {
    #[error("the key is not valid base64")]
    NotBase64,
    #[error("the key must be exactly 32 bytes")]
    WrongLength,
}

impl ServerKey {
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Standard or URL-safe base64, with or without padding.
    pub fn from_base64(raw: &str) -> Result<Self, ServerKeyError> {
        let raw = raw.trim();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(raw)
            .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(raw))
            .or_else(|_| URL_SAFE_NO_PAD.decode(raw.trim_end_matches('=')))
            .map_err(|_| ServerKeyError::NotBase64)?;
        let bytes: [u8; 32] = decoded
            .try_into()
            .map_err(|_| ServerKeyError::WrongLength)?;
        Ok(Self(bytes))
    }
}

impl fmt::Debug for ServerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// Sealed layout: `[version: 1][nonce: 24][ciphertext + tag]`.
const SEAL_VERSION: u8 = 1;
const NONCE_LEN: usize = 24;

pub struct XChaChaSecretCipher {
    cipher: XChaCha20Poly1305,
}

impl XChaChaSecretCipher {
    pub fn new(key: &ServerKey) -> Result<Self, CryptoError> {
        let cipher = XChaCha20Poly1305::new_from_slice(&key.0).map_err(|_| CryptoError::Failed)?;
        Ok(Self { cipher })
    }
}

impl SecretCipher for XChaChaSecretCipher {
    fn seal(&self, context: &[u8], secret: &Secret) -> Result<SealedSecret, CryptoError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| CryptoError::Failed)?;
        let ciphertext = self
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: secret.expose().as_bytes(),
                    aad: context,
                },
            )
            .map_err(|_| CryptoError::Failed)?;
        let mut sealed = Vec::with_capacity(1 + NONCE_LEN + ciphertext.len());
        sealed.push(SEAL_VERSION);
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);
        Ok(SealedSecret::new(sealed))
    }

    fn open(&self, context: &[u8], sealed: &SealedSecret) -> Result<Secret, CryptoError> {
        let bytes = sealed.as_bytes();
        let (&version, rest) = bytes.split_first().ok_or(CryptoError::Unreadable)?;
        if version != SEAL_VERSION || rest.len() < NONCE_LEN {
            return Err(CryptoError::Unreadable);
        }
        let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| CryptoError::Unreadable)?;
        let plaintext = self
            .cipher
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: context,
                },
            )
            .map_err(|_| CryptoError::Unreadable)?;
        let text = String::from_utf8(plaintext).map_err(|_| CryptoError::Unreadable)?;
        Secret::verbatim(&text).ok_or(CryptoError::Unreadable)
    }
}

/// Session tokens and temporary passwords from the OS random source.
pub struct OsTokenIssuer;

/// Session tokens start with this, so a leaked one is recognisable.
const TOKEN_PREFIX: &str = "tm_";

impl OsTokenIssuer {
    fn random_bytes<const N: usize>() -> [u8; N] {
        let mut bytes = [0u8; N];
        // The OS source failing means the machine is unusable for anything
        // secure; refusing to continue is the only safe answer, and the
        // alternative (a predictable token) is worse than a crash.
        getrandom::fill(&mut bytes).expect("the operating system's random source failed");
        bytes
    }
}

impl TokenIssuer for OsTokenIssuer {
    fn new_session_token(&self) -> Secret {
        let token = format!(
            "{TOKEN_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(Self::random_bytes::<32>())
        );
        Secret::generated(&token)
    }

    fn digest(&self, token: &str) -> TokenDigest {
        TokenDigest::new(Sha256::digest(token.as_bytes()).to_vec())
    }

    fn temporary_password(&self) -> Secret {
        // 18 random bytes = 24 URL-safe characters: above the 10-character
        // minimum, typeable enough for a one-time hand-over.
        let password = URL_SAFE_NO_PAD.encode(Self::random_bytes::<18>());
        Secret::generated(&password)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher(byte: u8) -> XChaChaSecretCipher {
        XChaChaSecretCipher::new(&ServerKey::from_bytes([byte; 32])).unwrap()
    }

    fn secret(raw: &str) -> Secret {
        Secret::new(raw).unwrap()
    }

    #[test]
    fn seal_then_open_returns_the_secret() {
        let c = cipher(7);
        let sealed = c.seal(b"user-1", &secret("sk-or-abc")).unwrap();
        assert_eq!(c.open(b"user-1", &sealed).unwrap().expose(), "sk-or-abc");
    }

    #[test]
    fn the_stored_bytes_do_not_contain_the_plaintext() {
        let sealed = cipher(7).seal(b"u", &secret("sk-or-abcdefgh")).unwrap();
        let hay = sealed.as_bytes();
        assert!(!hay.windows(8).any(|w| w == b"abcdefgh"));
    }

    #[test]
    fn every_sealing_uses_a_fresh_nonce() {
        let c = cipher(7);
        let a = c.seal(b"u", &secret("same")).unwrap();
        let b = c.seal(b"u", &secret("same")).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn another_key_another_context_or_tampering_do_not_open() {
        let sealed = cipher(7).seal(b"user-1", &secret("k")).unwrap();
        assert_eq!(
            cipher(8).open(b"user-1", &sealed),
            Err(CryptoError::Unreadable)
        );
        assert_eq!(
            cipher(7).open(b"user-2", &sealed),
            Err(CryptoError::Unreadable)
        );
        let mut bytes = sealed.as_bytes().to_vec();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert_eq!(
            cipher(7).open(b"user-1", &SealedSecret::new(bytes)),
            Err(CryptoError::Unreadable)
        );
        assert_eq!(
            cipher(7).open(b"user-1", &SealedSecret::new(vec![1, 2, 3])),
            Err(CryptoError::Unreadable)
        );
        assert_eq!(
            cipher(7).open(b"user-1", &SealedSecret::new(vec![])),
            Err(CryptoError::Unreadable)
        );
    }

    #[test]
    fn server_key_parses_base64_of_32_bytes_only() {
        let ok = base64::engine::general_purpose::STANDARD.encode([5u8; 32]);
        assert!(ServerKey::from_base64(&ok).is_ok());
        assert!(ServerKey::from_base64(&format!(" {ok}\n")).is_ok());
        assert_eq!(
            ServerKey::from_base64("not base64 !!").unwrap_err(),
            ServerKeyError::NotBase64
        );
        let short = base64::engine::general_purpose::STANDARD.encode([5u8; 31]);
        assert_eq!(
            ServerKey::from_base64(&short).unwrap_err(),
            ServerKeyError::WrongLength
        );
        assert_eq!(
            format!("{:?}", ServerKey::from_base64(&ok).unwrap()),
            "[redacted]"
        );
    }

    #[tokio::test]
    async fn argon2id_hash_verifies_and_is_salted() {
        let hasher = Argon2idHasher::with_costs(8, 1, 1).unwrap();
        let password = Password::parse("correct horse battery", "password").unwrap();
        let a = hasher.hash(&password).await.unwrap();
        let b = hasher.hash(&password).await.unwrap();
        assert_ne!(a, b, "a random salt per hash");
        assert!(a.as_str().starts_with("$argon2id$v=19$"));
        assert!(hasher.verify(&secret("correct horse battery"), &a).await);
        assert!(!hasher.verify(&secret("wrong horse battery"), &a).await);
        assert!(
            !hasher
                .verify(&secret("x"), &StoredPasswordHash::new("garbage".into()))
                .await
        );
    }

    #[tokio::test]
    async fn no_more_than_the_allowed_verifications_run_at_once() {
        let hasher = std::sync::Arc::new(Argon2idHasher::with_costs(8, 1, 1).unwrap());
        let password = Password::parse("correct horse battery", "password").unwrap();
        let hash = hasher.hash(&password).await.unwrap();
        // Hold every slot: a verification must wait, and go on when one frees.
        let held = hasher.slots.clone().acquire_many_owned(3).await.unwrap();
        let (h, hs) = (hasher.clone(), hash.clone());
        let task =
            tokio::spawn(async move { h.verify(&secret("correct horse battery"), &hs).await });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished(), "waits while every slot is taken");
        drop(held);
        assert!(task.await.unwrap());
    }

    #[test]
    fn production_costs_are_the_documented_ones() {
        let hasher = Argon2idHasher::new();
        assert_eq!(hasher.argon2.params().m_cost(), 19_456);
        assert_eq!(hasher.argon2.params().t_cost(), 2);
        assert_eq!(hasher.argon2.params().p_cost(), 1);
    }

    #[test]
    fn tokens_are_long_unique_and_digested_without_being_stored() {
        let issuer = OsTokenIssuer;
        let a = issuer.new_session_token();
        let b = issuer.new_session_token();
        assert_ne!(a, b);
        assert!(a.expose().starts_with("tm_"));
        assert_eq!(a.expose().len(), 3 + 43, "256 bits in base64url");
        assert_eq!(issuer.digest(a.expose()), issuer.digest(a.expose()));
        assert_ne!(issuer.digest(a.expose()), issuer.digest(b.expose()));
        assert_eq!(issuer.digest(a.expose()).as_bytes().len(), 32);
        assert!(!format!("{:?}", issuer.digest(a.expose())).contains(a.expose()));
    }

    #[test]
    fn temporary_passwords_satisfy_the_rules() {
        let p = OsTokenIssuer.temporary_password();
        assert!(Password::parse(p.expose(), "password").is_ok());
    }
}
