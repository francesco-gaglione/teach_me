//! Ports for what must not live in the domain or the use cases: password
//! hashing, sealing a secret, and the randomness behind tokens.

use async_trait::async_trait;

use crate::secret::Secret;
use crate::user::{Password, SealedSecret, StoredPasswordHash, TokenDigest};

/// A cryptographic operation failed. Carries no detail on purpose: it can sit
/// next to a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CryptoError {
    #[error("the cryptographic operation failed")]
    Failed,
    /// A sealed value that cannot be opened: another server key, or tampering.
    #[error("the sealed value cannot be opened")]
    Unreadable,
}

/// Slow, salted password hashing (Argon2id in the adapter). Both methods are
/// CPU-heavy: adapters run them off the async runtime.
#[async_trait]
pub trait PasswordHasher: Send + Sync {
    async fn hash(&self, password: &Password) -> Result<StoredPasswordHash, CryptoError>;
    /// `false` for a wrong password and for a hash that cannot be read.
    async fn verify(&self, password: &Secret, hash: &StoredPasswordHash) -> bool;
}

/// Authenticated encryption of a secret with the server's key (ADR 0008 §6).
/// `context` is bound to the ciphertext (the owner's id), so a sealed key
/// copied to another row does not open.
pub trait SecretCipher: Send + Sync {
    fn seal(&self, context: &[u8], secret: &Secret) -> Result<SealedSecret, CryptoError>;
    fn open(&self, context: &[u8], sealed: &SealedSecret) -> Result<Secret, CryptoError>;
}

/// Randomness for credentials.
pub trait TokenIssuer: Send + Sync {
    /// A new opaque session token (256 random bits).
    fn new_session_token(&self) -> Secret;
    /// What is stored instead of the token; the same token gives the same digest.
    fn digest(&self, token: &str) -> TokenDigest;
    /// A password that satisfies the rules, for `user add` without one.
    fn temporary_password(&self) -> Secret;
}
