//! Users, credentials and sessions (ADR 0008, `docs/contracts/users.md`).
//!
//! Nothing here knows how a password is hashed or a key encrypted: those are
//! ports ([`crate::ports::PasswordHasher`], [`crate::ports::SecretCipher`]).

use std::fmt;

use time::OffsetDateTime;

use crate::ids::{SessionId, UserId};
use crate::secret::Secret;
use crate::validation::{IssueKind, ValidationIssue, normalize_text};

pub const USERNAME_MIN_CHARS: usize = 3;
pub const USERNAME_MAX_CHARS: usize = 32;
pub const DISPLAY_NAME_MAX_CHARS: usize = 60;
pub const PASSWORD_MIN_CHARS: usize = 10;
pub const PASSWORD_MAX_CHARS: usize = 128;
pub const API_KEY_MAX_CHARS: usize = 500;
pub const DEVICE_NAME_MAX_CHARS: usize = 60;

/// Login name: 3-32 characters from `a-z 0-9 . _ -`, stored in lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Username(String);

impl Username {
    pub fn parse(raw: &str) -> Result<Self, ValidationIssue> {
        let key = Self::lookup_key(raw);
        let issue = |kind| ValidationIssue::new("username", kind);
        if key.is_empty() {
            return Err(issue(IssueKind::Empty));
        }
        let chars = key.chars().count();
        if chars > USERNAME_MAX_CHARS {
            return Err(issue(IssueKind::TooLong));
        }
        if chars < USERNAME_MIN_CHARS {
            return Err(issue(IssueKind::TooShort));
        }
        let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c);
        if !key.chars().all(allowed) {
            return Err(issue(IssueKind::Invalid));
        }
        Ok(Self(key))
    }

    /// How a name typed at login is compared: trimmed and lowercased, without
    /// the format rules (a name that could never exist is just unknown).
    #[must_use]
    pub fn lookup_key(raw: &str) -> String {
        raw.trim().to_lowercase()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Rebuilds a name read from storage, where it was validated on the way in.
    #[must_use]
    pub fn from_stored(stored: String) -> Self {
        Self(stored)
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Name shown in the interface: trimmed, 1-60 code points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

impl DisplayName {
    pub fn parse(raw: &str) -> Result<Self, ValidationIssue> {
        normalize_text(raw, DISPLAY_NAME_MAX_CHARS, "display_name").map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A password that passed the rules (10-128 characters, nothing else). Never
/// printed. Not trimmed: spaces count.
#[derive(Clone)]
pub struct Password(Secret);

impl Password {
    /// `field` names the input in the error (`password`, `new_password`).
    pub fn parse(raw: &str, field: &'static str) -> Result<Self, ValidationIssue> {
        let chars = raw.chars().count();
        if chars < PASSWORD_MIN_CHARS {
            return Err(ValidationIssue::new(field, IssueKind::TooShort));
        }
        if chars > PASSWORD_MAX_CHARS {
            return Err(ValidationIssue::new(field, IssueKind::TooLong));
        }
        // Not empty: it has at least `PASSWORD_MIN_CHARS` characters.
        Secret::verbatim(raw)
            .map(Self)
            .ok_or(ValidationIssue::new(field, IssueKind::Empty))
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose()
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// An LLM API key as the user typed it: 1-500 characters after trimming.
pub fn parse_api_key(raw: &str) -> Result<Secret, ValidationIssue> {
    let key = Secret::new(raw).ok_or(ValidationIssue::new("api_key", IssueKind::Empty))?;
    if key.expose().chars().count() > API_KEY_MAX_CHARS {
        return Err(ValidationIssue::new("api_key", IssueKind::TooLong));
    }
    Ok(key)
}

/// A password hash in the format the adapter chose (a PHC string). Opaque to
/// the domain; kept out of `Debug` so it never reaches a log.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredPasswordHash(String);

impl StoredPasswordHash {
    #[must_use]
    pub fn new(raw: String) -> Self {
        Self(raw)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for StoredPasswordHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// An LLM key sealed by [`crate::ports::SecretCipher`], as stored.
#[derive(Clone, PartialEq, Eq)]
pub struct SealedSecret(Vec<u8>);

impl SealedSecret {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SealedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[sealed]")
    }
}

/// What the API and the commands know about a user. Neither the password hash
/// nor the key leave the repository through this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub username: Username,
    pub display_name: DisplayName,
    pub has_llm_key: bool,
    /// The voice the user chose for read-aloud, as saved. It may no longer be
    /// in the server's list: the effective voice is worked out by the
    /// application (`audio.md` §4.2).
    pub tts_voice: Option<String>,
    pub disabled_at: Option<OffsetDateTime>,
    pub last_login_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

impl User {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.disabled_at.is_none()
    }
}

/// A user to be stored. No password means the account cannot log in yet (the
/// initial `owner` until `user passwd`).
#[derive(Debug, Clone)]
pub struct NewUser {
    pub id: UserId,
    pub username: Username,
    pub display_name: DisplayName,
    pub password_hash: Option<StoredPasswordHash>,
    pub created_at: OffsetDateTime,
}

/// A user with what is needed to check a password.
#[derive(Debug, Clone)]
pub struct UserCredentials {
    pub user: User,
    pub password_hash: Option<StoredPasswordHash>,
}

/// The SHA-256-sized digest of a session token, as stored. The token itself
/// is never stored.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TokenDigest(Vec<u8>);

impl TokenDigest {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for TokenDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[digest]")
    }
}

/// A login on one device. Expires `SESSION_IDLE_DAYS` days after the last
/// request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub user_id: UserId,
    pub token_digest: TokenDigest,
    pub device_name: Option<String>,
    pub created_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
}

pub const SESSION_IDLE_DAYS: i64 = 90;

/// A request renews the session only when the last renewal is older than this,
/// so a busy client does not write on every call. Negligible against 90 days.
pub const SESSION_RENEW_AFTER: time::Duration = time::Duration::hours(1);

/// Failed logins allowed per name inside [`LOGIN_WINDOW`].
pub const LOGIN_MAX_FAILURES: usize = 5;
pub const LOGIN_WINDOW: time::Duration = time::Duration::minutes(15);

impl Session {
    #[must_use]
    pub fn expiry_from(at: OffsetDateTime) -> OffsetDateTime {
        at + time::Duration::days(SESSION_IDLE_DAYS)
    }

    #[must_use]
    pub fn is_expired(&self, now: OffsetDateTime) -> bool {
        now >= self.expires_at
    }

    #[must_use]
    pub fn needs_renewal(&self, now: OffsetDateTime) -> bool {
        now - self.last_seen_at >= SESSION_RENEW_AFTER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(raw: &str) -> IssueKind {
        Username::parse(raw).unwrap_err().issue
    }

    #[test]
    fn username_is_lowercased_and_trimmed() {
        assert_eq!(
            Username::parse("  Ann.Lee_9 ").unwrap().as_str(),
            "ann.lee_9"
        );
    }

    #[test]
    fn username_rules() {
        assert_eq!(issue(""), IssueKind::Empty);
        assert_eq!(issue("ab"), IssueKind::TooShort);
        assert!(Username::parse("abc").is_ok());
        assert!(Username::parse(&"a".repeat(32)).is_ok());
        assert_eq!(issue(&"a".repeat(33)), IssueKind::TooLong);
        assert_eq!(issue("with space"), IssueKind::Invalid);
        assert_eq!(issue("èmile"), IssueKind::Invalid);
        assert_eq!(issue("a/b"), IssueKind::Invalid);
    }

    #[test]
    fn password_rules_count_characters_not_bytes() {
        assert_eq!(
            Password::parse("123456789", "password").unwrap_err().issue,
            IssueKind::TooShort
        );
        assert!(Password::parse("1234567890", "password").is_ok());
        assert!(Password::parse(&"è".repeat(10), "password").is_ok());
        assert_eq!(
            Password::parse(&"a".repeat(129), "new_password")
                .unwrap_err()
                .field,
            "new_password"
        );
        assert!(Password::parse(&"a".repeat(128), "password").is_ok());
    }

    #[test]
    fn password_keeps_its_spaces_and_hides_itself() {
        let p = Password::parse("  spaced pass  ", "password").unwrap();
        assert_eq!(p.expose(), "  spaced pass  ");
        assert_eq!(format!("{p:?}"), "[redacted]");
    }

    #[test]
    fn api_key_is_trimmed_and_bounded() {
        assert_eq!(parse_api_key("  sk-1 \n").unwrap().expose(), "sk-1");
        assert_eq!(parse_api_key("  ").unwrap_err().issue, IssueKind::Empty);
        assert!(parse_api_key(&"k".repeat(500)).is_ok());
        assert_eq!(
            parse_api_key(&"k".repeat(501)).unwrap_err().issue,
            IssueKind::TooLong
        );
    }

    #[test]
    fn session_expires_after_ninety_idle_days() {
        let t = time::macros::datetime!(2026-11-10 10:00 UTC);
        let s = Session {
            id: SessionId::new(uuid::Uuid::nil()),
            user_id: UserId::OWNER,
            token_digest: TokenDigest::new(vec![1]),
            device_name: None,
            created_at: t,
            last_seen_at: t,
            expires_at: Session::expiry_from(t),
        };
        assert!(!s.is_expired(t + time::Duration::days(89)));
        assert!(s.is_expired(t + time::Duration::days(90)));
        assert!(!s.needs_renewal(t + time::Duration::minutes(59)));
        assert!(s.needs_renewal(t + time::Duration::hours(1)));
    }
}
