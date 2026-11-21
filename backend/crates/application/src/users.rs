//! Users, sessions and the LLM key (ADR 0008, `docs/contracts/users.md`).
//!
//! [`CreateUser`] is the one place an account is made. Today only the `user
//! add` command calls it; a registration endpoint would call the same use case.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use domain::user::{DEVICE_NAME_MAX_CHARS, LOGIN_MAX_FAILURES, LOGIN_WINDOW, USERNAME_MAX_CHARS};
use domain::{
    Clock, DisplayName, DocumentFiles, DocumentRepository, IdGenerator, IssueKind, NewUser,
    Password, PasswordHasher, RepositoryError, Secret, SecretCipher, Session, SessionId,
    SessionRepository, StoredPasswordHash, TokenIssuer, User, UserId, UserRepository, Username,
    ValidationIssue, parse_api_key,
};
use time::OffsetDateTime;

use crate::error::AppError;
use crate::llm_keys::key_context;
use crate::time_util::now_millis;

/// Input of [`CreateUser`]. Without a password one is generated.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub username: String,
    /// Defaults to the username.
    pub display_name: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug)]
pub struct CreatedUser {
    pub user: User,
    /// Set only when the password was generated: shown once, never stored.
    pub generated_password: Option<Secret>,
}

pub struct CreateUser {
    users: Arc<dyn UserRepository>,
    hasher: Arc<dyn PasswordHasher>,
    tokens: Arc<dyn TokenIssuer>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}

impl CreateUser {
    #[must_use]
    pub fn new(
        users: Arc<dyn UserRepository>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenIssuer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            users,
            hasher,
            tokens,
            clock,
            ids,
        }
    }

    pub async fn execute(&self, input: NewAccount) -> Result<CreatedUser, AppError> {
        let mut issues = Vec::new();
        let username = collect(&mut issues, Username::parse(&input.username));
        let display_name = collect(
            &mut issues,
            DisplayName::parse(input.display_name.as_deref().unwrap_or(&input.username)),
        );
        let (password, generated) = if let Some(raw) = input.password.as_deref() {
            (collect(&mut issues, Password::parse(raw, "password")), None)
        } else {
            let secret = self.tokens.temporary_password();
            let parsed = Password::parse(secret.expose(), "password").ok();
            (parsed, Some(secret))
        };
        let (Some(username), Some(display_name), Some(password)) =
            (username, display_name, password)
        else {
            return Err(AppError::Validation(issues));
        };
        let hash = self
            .hasher
            .hash(&password)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;
        let new = NewUser {
            id: UserId::new(self.ids.new_id()),
            username,
            display_name,
            password_hash: Some(hash),
            created_at: now_millis(self.clock.as_ref()),
        };
        self.users.insert(&new).await.map_err(|e| match e {
            RepositoryError::Duplicate => {
                AppError::validation(ValidationIssue::new("username", IssueKind::Duplicate))
            }
            other => other.into(),
        })?;
        let user = self.users.get(new.id).await?.ok_or(AppError::NotFound)?;
        Ok(CreatedUser {
            user,
            generated_password: generated,
        })
    }
}

fn collect<T>(issues: &mut Vec<ValidationIssue>, parsed: Result<T, ValidationIssue>) -> Option<T> {
    match parsed {
        Ok(v) => Some(v),
        Err(issue) => {
            issues.push(issue);
            None
        }
    }
}

/// Login attempts per name inside [`LOGIN_WINDOW`], in memory: a restart
/// forgets them, which only helps an attacker who can restart the server.
pub struct LoginLimiter {
    failures: Mutex<HashMap<String, Vec<OffsetDateTime>>>,
}

/// Names tracked at once. Past it the oldest entries go: a flood of invented
/// names cannot grow the map without bound.
const LIMITER_MAX_NAMES: usize = 10_000;
/// Shared limiter key of every name too long to exist.
const TOO_LONG_KEY: &str = "\u{0}too-long";

impl LoginLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            failures: Mutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<OffsetDateTime>>> {
        self.failures
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reserves an attempt for `name` before the password is checked, so a
    /// burst of parallel attempts cannot get past the limit; a success gives
    /// them back with [`Self::clear`]. `Err` holds the seconds to wait.
    pub(crate) fn reserve(&self, name: &str, now: OffsetDateTime) -> Result<(), u64> {
        let mut map = self.lock();
        if let Some(list) = map.get_mut(name) {
            list.retain(|at| now - *at < LOGIN_WINDOW);
            if list.len() >= LOGIN_MAX_FAILURES {
                let oldest = list.iter().min().copied().unwrap_or(now);
                let wait = (oldest + LOGIN_WINDOW - now).whole_seconds();
                return Err(u64::try_from(wait).unwrap_or(0).max(1));
            }
        } else if map.len() >= LIMITER_MAX_NAMES {
            // Full: drop the expired names, and if that is not enough the one
            // whose latest attempt is the oldest. The new name is always
            // tracked: the limit never disappears for the names that come.
            map.retain(|_, list| list.iter().any(|at| now - *at < LOGIN_WINDOW));
            if map.len() >= LIMITER_MAX_NAMES {
                let oldest = map
                    .iter()
                    .min_by_key(|(_, list)| list.iter().max().copied())
                    .map(|(k, _)| k.clone());
                if let Some(key) = oldest {
                    map.remove(&key);
                }
            }
        }
        map.entry(name.to_owned()).or_default().push(now);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn lock_len(&self) -> usize {
        self.lock().len()
    }

    #[cfg(test)]
    pub(crate) fn tracked_names(&self) -> usize {
        self.lock().len()
    }

    fn clear(&self, name: &str) {
        self.lock().remove(name);
    }
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct LoggedIn {
    pub token: Secret,
    pub expires_at: OffsetDateTime,
    pub user: User,
}

/// Who a valid token belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Authenticated {
    pub user_id: UserId,
    pub session_id: SessionId,
}

/// Longest password a login attempt is hashed at. Longer cannot be a real
/// password (the limit is 128) and would only cost CPU.
const LOGIN_PASSWORD_MAX_BYTES: usize = 1024;

pub struct AuthService {
    users: Arc<dyn UserRepository>,
    sessions: Arc<dyn SessionRepository>,
    hasher: Arc<dyn PasswordHasher>,
    tokens: Arc<dyn TokenIssuer>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    pub(crate) limiter: LoginLimiter,
    /// A hash nobody has the password of: verified when the name is unknown
    /// or has no password, so those logins cost what a real one costs.
    decoy: tokio::sync::OnceCell<StoredPasswordHash>,
}

impl AuthService {
    #[must_use]
    pub fn new(
        users: Arc<dyn UserRepository>,
        sessions: Arc<dyn SessionRepository>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenIssuer>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            users,
            sessions,
            hasher,
            tokens,
            clock,
            ids,
            limiter: LoginLimiter::new(),
            decoy: tokio::sync::OnceCell::new(),
        }
    }

    /// Computes the decoy hash now (at startup) instead of at the first
    /// login with an unknown name, which would then be slower than the others.
    pub async fn warm_up(&self) -> Result<(), AppError> {
        self.decoy_hash().await.map(|_| ())
    }

    async fn decoy_hash(&self) -> Result<StoredPasswordHash, AppError> {
        self.decoy
            .get_or_try_init(|| async {
                let random = self.tokens.temporary_password();
                let password = Password::parse(random.expose(), "password")
                    .map_err(|_| AppError::Storage("decoy password rejected".into()))?;
                self.hasher
                    .hash(&password)
                    .await
                    .map_err(|e| AppError::Storage(Box::new(e)))
            })
            .await
            .cloned()
    }

    /// Exactly one password verification happens for every attempt that is not
    /// blocked, whatever is wrong with it (unknown name, no password,
    /// disabled account, wrong password), and all end in the same error.
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device_name: Option<&str>,
    ) -> Result<LoggedIn, AppError> {
        let device = match device_name.map(str::trim).filter(|d| !d.is_empty()) {
            None => None,
            Some(d) if d.chars().count() > DEVICE_NAME_MAX_CHARS => {
                return Err(AppError::validation(ValidationIssue::new(
                    "device_name",
                    IssueKind::TooLong,
                )));
            }
            Some(d) => Some(d.to_owned()),
        };
        let now = now_millis(self.clock.as_ref());
        // A name longer than any real one cannot exist: it is counted under one
        // shared key, so what an attacker sends never sets the size of the map.
        let name = if username.trim().chars().count() > USERNAME_MAX_CHARS {
            TOO_LONG_KEY.to_owned()
        } else {
            Username::lookup_key(username)
        };
        if let Err(retry_after_secs) = self.limiter.reserve(&name, now) {
            return Err(AppError::RateLimited { retry_after_secs });
        }

        let found = if name == TOO_LONG_KEY {
            None
        } else {
            self.users.credentials(&name).await?
        };
        let candidate = if password.len() > LOGIN_PASSWORD_MAX_BYTES {
            None
        } else {
            Secret::verbatim(password)
        };
        let probe_used = candidate.is_none();
        let (hash, real) = match found.as_ref().and_then(|c| c.password_hash.clone()) {
            Some(hash) => (hash, true),
            None => (self.decoy_hash().await?, false),
        };
        // A password that cannot be real (empty, absurdly long) is still
        // verified, against a fixed probe: the cost does not tell it apart.
        let probe = candidate.unwrap_or_else(|| Secret::generated("probe"));
        let verified = self.hasher.verify(&probe, &hash).await && !probe_used;
        let user = found
            .map(|c| c.user)
            .filter(|u| verified && real && u.is_active());
        let Some(user) = user else {
            return Err(AppError::InvalidCredentials);
        };

        self.limiter.clear(&name);
        let token = self.tokens.new_session_token();
        let expires_at = Session::expiry_from(now);
        let session = Session {
            id: SessionId::new(self.ids.new_id()),
            user_id: user.id,
            token_digest: self.tokens.digest(token.expose()),
            device_name: device,
            created_at: now,
            last_seen_at: now,
            expires_at,
        };
        self.sessions.insert(&session).await?;
        // A step after the insert that fails must not leave a session nobody
        // was given the token of.
        match self.finish_login(user.id, now).await {
            Ok(user) => Ok(LoggedIn {
                token,
                expires_at,
                user,
            }),
            Err(e) => {
                if let Err(undo) = self.sessions.revoke(session.id).await {
                    tracing::warn!(error = %undo, "cannot remove the session of a failed login");
                }
                Err(e)
            }
        }
    }

    async fn finish_login(&self, user: UserId, now: OffsetDateTime) -> Result<User, AppError> {
        self.users.set_last_login(user, now).await?;
        // Housekeeping: expired sessions would otherwise stay forever.
        if let Err(e) = self.sessions.delete_expired(now).await {
            tracing::warn!(error = %e, "cannot remove expired sessions");
        }
        self.users.get(user).await?.ok_or(AppError::NotFound)
    }

    /// The owner of a token, or `Unauthorized` for a missing, expired or
    /// revoked session and for a disabled or deleted user. A valid use renews
    /// the session (at most once an hour).
    pub async fn authenticate(&self, token: &str) -> Result<Authenticated, AppError> {
        let digest = self.tokens.digest(token);
        let session = self
            .sessions
            .by_digest(&digest)
            .await?
            .ok_or(AppError::Unauthorized)?;
        let now = now_millis(self.clock.as_ref());
        if session.is_expired(now) {
            self.sessions.revoke(session.id).await?;
            return Err(AppError::Unauthorized);
        }
        let user = self
            .users
            .get(session.user_id)
            .await?
            .filter(User::is_active)
            .ok_or(AppError::Unauthorized)?;
        if session.needs_renewal(now)
            && let Err(e) = self
                .sessions
                .renew(session.id, now, Session::expiry_from(now))
                .await
        {
            tracing::warn!(error = %e, "cannot renew a session");
        }
        Ok(Authenticated {
            user_id: user.id,
            session_id: session.id,
        })
    }

    pub async fn logout(&self, session: SessionId) -> Result<(), AppError> {
        self.sessions.revoke(session).await?;
        Ok(())
    }

    /// Changes the password and revokes every other session of the user.
    pub async fn change_password(
        &self,
        user: UserId,
        keep: SessionId,
        current: &str,
        new: &str,
    ) -> Result<(), AppError> {
        let mut issues = Vec::new();
        let stored = self.users.password_hash(user).await?;
        let current_ok = match (Secret::verbatim(current), stored) {
            (Some(secret), Some(hash)) => self.hasher.verify(&secret, &hash).await,
            _ => false,
        };
        if !current_ok {
            issues.push(ValidationIssue::new("current_password", IssueKind::Wrong));
        }
        let new = collect(&mut issues, Password::parse(new, "new_password"));
        let (Some(new), true) = (new, current_ok) else {
            return Err(AppError::Validation(issues));
        };
        let hash = self
            .hasher
            .hash(&new)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;
        self.users.set_password_hash(user, &hash).await?;
        self.sessions.revoke_all(user, Some(keep)).await?;
        Ok(())
    }
}

/// What the signed-in user does to their own account.
pub struct AccountService {
    users: Arc<dyn UserRepository>,
    cipher: Arc<dyn SecretCipher>,
}

impl AccountService {
    #[must_use]
    pub fn new(users: Arc<dyn UserRepository>, cipher: Arc<dyn SecretCipher>) -> Self {
        Self { users, cipher }
    }

    pub async fn me(&self, user: UserId) -> Result<User, AppError> {
        self.users.get(user).await?.ok_or(AppError::NotFound)
    }

    /// Seals and stores the key. It is not checked with the provider: a bad
    /// key shows at first use.
    pub async fn set_llm_key(&self, user: UserId, raw: &str) -> Result<User, AppError> {
        let key = parse_api_key(raw).map_err(AppError::validation)?;
        store_key(self.users.as_ref(), self.cipher.as_ref(), user, &key).await?;
        self.me(user).await
    }

    pub async fn clear_llm_key(&self, user: UserId) -> Result<(), AppError> {
        self.users.set_llm_key(user, None).await?;
        Ok(())
    }
}

async fn store_key(
    users: &dyn UserRepository,
    cipher: &dyn SecretCipher,
    user: UserId,
    key: &Secret,
) -> Result<(), AppError> {
    let sealed = cipher
        .seal(&key_context(user), key)
        .map_err(|e| AppError::Storage(Box::new(e)))?;
    users.set_llm_key(user, Some(&sealed)).await?;
    Ok(())
}

/// What the operator does from the server (`user …` commands, startup).
pub struct UserAdmin {
    users: Arc<dyn UserRepository>,
    sessions: Arc<dyn SessionRepository>,
    documents: Arc<dyn DocumentRepository>,
    files: Arc<dyn DocumentFiles>,
    hasher: Arc<dyn PasswordHasher>,
    tokens: Arc<dyn TokenIssuer>,
    cipher: Arc<dyn SecretCipher>,
    clock: Arc<dyn Clock>,
}

/// A new password, and whether it was made up here (to be shown once).
#[derive(Debug)]
pub struct PasswordSet {
    pub generated: Option<Secret>,
}

impl UserAdmin {
    // One argument per injected port: a parameter struct would only rename them.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        users: Arc<dyn UserRepository>,
        sessions: Arc<dyn SessionRepository>,
        documents: Arc<dyn DocumentRepository>,
        files: Arc<dyn DocumentFiles>,
        hasher: Arc<dyn PasswordHasher>,
        tokens: Arc<dyn TokenIssuer>,
        cipher: Arc<dyn SecretCipher>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            users,
            sessions,
            documents,
            files,
            hasher,
            tokens,
            cipher,
            clock,
        }
    }

    pub async fn list(&self) -> Result<Vec<User>, AppError> {
        Ok(self.users.list().await?)
    }

    async fn find(&self, username: &str) -> Result<User, AppError> {
        let key = Username::lookup_key(username);
        self.users
            .credentials(&key)
            .await?
            .map(|c| c.user)
            .ok_or(AppError::NotFound)
    }

    /// Sets a password (generating one when `password` is `None`) and revokes
    /// every session of the user.
    pub async fn set_password(
        &self,
        username: &str,
        password: Option<&str>,
    ) -> Result<PasswordSet, AppError> {
        let user = self.find(username).await?;
        let (parsed, generated) = if let Some(raw) = password {
            (
                Password::parse(raw, "password").map_err(AppError::validation)?,
                None,
            )
        } else {
            let secret = self.tokens.temporary_password();
            let parsed =
                Password::parse(secret.expose(), "password").map_err(AppError::validation)?;
            (parsed, Some(secret))
        };
        let hash = self
            .hasher
            .hash(&parsed)
            .await
            .map_err(|e| AppError::Storage(Box::new(e)))?;
        self.users.set_password_hash(user.id, &hash).await?;
        self.sessions.revoke_all(user.id, None).await?;
        Ok(PasswordSet { generated })
    }

    pub async fn rename(&self, username: &str, new: &str) -> Result<(), AppError> {
        let user = self.find(username).await?;
        let new = Username::parse(new).map_err(AppError::validation)?;
        self.users.rename(user.id, &new).await.map_err(|e| match e {
            RepositoryError::Duplicate => {
                AppError::validation(ValidationIssue::new("username", IssueKind::Duplicate))
            }
            other => other.into(),
        })
    }

    pub async fn set_llm_key(&self, username: &str, raw: &str) -> Result<(), AppError> {
        let user = self.find(username).await?;
        let key = parse_api_key(raw).map_err(AppError::validation)?;
        store_key(self.users.as_ref(), self.cipher.as_ref(), user.id, &key).await
    }

    /// Disabling also revokes the sessions: the user is out at once.
    pub async fn set_disabled(&self, username: &str, disabled: bool) -> Result<(), AppError> {
        let user = self.find(username).await?;
        let at = disabled.then(|| now_millis(self.clock.as_ref()));
        self.users.set_disabled(user.id, at).await?;
        if disabled {
            self.sessions.revoke_all(user.id, None).await?;
        }
        Ok(())
    }

    /// Deletes the user with everything they own, then the files of their
    /// documents (the ids are read first: the rows go with the user).
    /// Returns how many files were cleaned up.
    pub async fn delete(&self, username: &str) -> Result<usize, AppError> {
        let user = self.find(username).await?;
        let documents = self.documents.ids_for_user(user.id).await?;
        if !self.users.delete(user.id).await? {
            return Err(AppError::NotFound);
        }
        for id in &documents {
            self.files.remove(*id).await;
        }
        Ok(documents.len())
    }

    /// First start after the upgrade: the key of the configuration becomes
    /// the key of `owner` if `owner` has none. Whether it was stored.
    pub async fn adopt_configured_key(&self, key: &Secret) -> Result<bool, AppError> {
        let sealed = self
            .cipher
            .seal(&key_context(UserId::OWNER), key)
            .map_err(|e| AppError::Storage(Box::new(e)))?;
        Ok(self
            .users
            .set_llm_key_if_missing(UserId::OWNER, &sealed)
            .await?)
    }
}
