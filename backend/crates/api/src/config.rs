//! Configuration: optional TOML file, overridden by `TEACH_ME_*` variables.
//!
//! Environment access goes through a lookup closure so tests never touch the
//! process environment.

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use domain::llm::LlmRole;
use infrastructure::anthropic::{OPENROUTER_BASE_URL, openrouter_model_id};
use infrastructure::{
    AnthropicConfig, ApiAuth, Qwen3TtsConfig, RoleModel, Secret, ServerKey, ServerKeyError,
    parse_voices,
};
use serde::Deserialize;

use crate::state::{DEFAULT_MAX_SSE_PER_USER, DEFAULT_STORAGE_QUOTA_BYTES};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_DATABASE_URL: &str = "sqlite://teach_me.db";
const DEFAULT_BODY_LIMIT_BYTES: usize = 1024 * 1024;
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;
const DEFAULT_JOB_MAX_CONCURRENT: usize = 2;
const DEFAULT_JOB_TIMEOUT_SECS: u64 = 900;
const DEFAULT_AUDIO_DIR: &str = "/audio";
const DEFAULT_TTS_VOICE: &str = "vivian";
const DEFAULT_TTS_VOICES: &str =
    "vivian:Vivian,serena:Serena,ryan:Ryan,aiden:Aiden,eric:Eric,dylan:Dylan";
const DEFAULT_PROVIDER: &str = "anthropic";
/// Provider kinds: both speak the Anthropic Messages API (`OpenRouter` through
/// its Anthropic-compatible endpoint), so one adapter serves them.
const PROVIDER_KINDS: [&str; 2] = ["anthropic", "openrouter"];
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// Default history budget for chat roles (`llm-port.md` §9).
const DEFAULT_HISTORY_TOKEN_BUDGET: usize = 8000;
/// Web searches the Researcher may run per research (`llm-port.md` §12.8).
const DEFAULT_RESEARCHER_MAX_SEARCHES: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    ReadFile {
        path: String,
        source: std::io::Error,
    },
    // Neither the snippet nor toml's message: both can echo the offending value (the token).
    #[error("invalid config file {path}: invalid TOML syntax or value type{location}")]
    ParseFile { path: String, location: String },
    #[error("invalid value for {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
    #[error(
        "TEACH_ME_SECRET_KEY is required: 32 random bytes in base64 (generate one with `openssl rand -base64 32`); it encrypts the users' LLM keys"
    )]
    MissingSecretKey,
    #[error("TEACH_ME_SECRET_KEY is not valid: {0} (generate one with `openssl rand -base64 32`)")]
    InvalidSecretKey(ServerKeyError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    /// Seals the users' LLM keys in the database (`TEACH_ME_SECRET_KEY`).
    pub secret_key: ServerKey,
    /// `TEACH_ME_API_TOKEN` / `api_token` was set: no longer used, the binary
    /// warns about it.
    pub legacy_api_token_present: bool,
    pub log_format: LogFormat,
    /// OTLP/HTTP endpoint for traces (`OTEL_EXPORTER_OTLP_ENDPOINT` /
    /// `[telemetry] otlp_endpoint`); `None` (unset or empty) keeps export off.
    pub otlp_endpoint: Option<String>,
    pub body_limit_bytes: usize,
    pub request_timeout_secs: u64,
    /// Bytes of uploaded documents per user (`TEACH_ME_USER_STORAGE_QUOTA_BYTES`);
    /// 0 = unlimited.
    pub user_storage_quota_bytes: u64,
    /// Input + output tokens per user per UTC day (`TEACH_ME_USER_DAILY_LLM_TOKENS`);
    /// 0 = no budget.
    pub user_daily_llm_tokens: u64,
    /// Concurrent SSE streams per user (`TEACH_ME_MAX_SSE_PER_USER`).
    pub max_sse_per_user: usize,
    /// Jobs running at once, all users together (`TEACH_ME_JOB_MAX_CONCURRENT`).
    pub job_max_concurrent: usize,
    /// Time budget of one job run (`TEACH_ME_JOB_TIMEOUT_SECS`).
    pub job_timeout_secs: u64,
    /// Where uploads wait for their import (`<data_dir>/uploads`). By default
    /// the directory of the database file.
    pub data_dir: PathBuf,
    /// Directory with the PDF engine library (`libpdfium.so`); `None` looks
    /// in the system's library path.
    pub pdfium_dir: Option<PathBuf>,
    pub llm: LlmConfig,
    /// Where the clips of the read-aloud feature are stored (`TEACH_ME_AUDIO_DIR`).
    pub audio_dir: PathBuf,
    /// The speech engine; `None` (no `TEACH_ME_TTS_URL`) turns read-aloud off.
    pub tts: Option<Qwen3TtsConfig>,
}

/// Models and provider settings (ADR 0004).
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub anthropic: AnthropicConfig,
    /// `anthropic` or `openrouter`: the service every user brings a key for.
    pub provider_kind: String,
    /// Estimated tokens of chat history sent to the Tutor.
    pub tutor_history_token_budget: usize,
    /// `max_uses` of the Researcher's web search.
    pub researcher_max_searches: u32,
    /// Pages the Generator may read per lesson (ADR 0009); 0 = tool off.
    pub generator_max_fetches: u32,
}

/// File representation: everything optional, unknown keys rejected to catch typos.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    bind_addr: Option<String>,
    database_url: Option<String>,
    api_token: Option<String>,
    log_format: Option<LogFormat>,
    body_limit_bytes: Option<usize>,
    request_timeout_secs: Option<u64>,
    data_dir: Option<String>,
    pdfium_dir: Option<String>,
    llm: Option<FileLlm>,
    telemetry: Option<FileTelemetry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTelemetry {
    otlp_endpoint: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLlm {
    default_provider: Option<String>,
    providers: Option<HashMap<String, FileProvider>>,
    roles: Option<HashMap<String, FileRole>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileProvider {
    kind: Option<String>,
    base_url: Option<String>,
    api_version: Option<String>,
    /// Allowed only in a private file; the environment wins.
    api_key: Option<String>,
    connect_timeout_secs: Option<u64>,
    request_timeout_secs: Option<u64>,
    stream_idle_timeout_secs: Option<u64>,
    server_tool_timeout_secs: Option<u64>,
    web_search_tool: Option<String>,
    web_fetch_tool: Option<String>,
    /// Declares whether the provider runs the web fetch tool; the default is
    /// yes for Anthropic and no through `OpenRouter` (ADR 0009).
    server_web_fetch: Option<bool>,
    refusal_fallback: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRole {
    provider: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    max_output_tokens: Option<u32>,
    history_token_budget: Option<usize>,
    max_searches: Option<u32>,
    max_fetches: Option<u32>,
}

impl Config {
    /// Takes the LLM key out of the configuration. The server never calls the
    /// provider with it (each request carries its owner's key): it only
    /// becomes the key of `owner` at start. The provider is built from what is
    /// left, which has no key, so a request without a key of its own cannot
    /// run on this one.
    pub fn take_configured_llm_key(&mut self) -> Option<Secret> {
        self.llm.anthropic.api_key.take()
    }

    /// Loads the file (if given) and applies environment overrides.
    pub fn load(
        file: Option<&Path>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let raw = match file {
            Some(path) => {
                let text =
                    std::fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
                        path: path.display().to_string(),
                        source,
                    })?;
                toml::from_str(&text).map_err(|e| ConfigError::ParseFile {
                    path: path.display().to_string(),
                    location: e.span().map_or_default(|sp| line_col(&text, sp.start)),
                })?
            }
            None => FileConfig::default(),
        };
        Self::merge(raw, &env)
    }

    fn merge(file: FileConfig, env: &impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        // Empty variables count as unset so `TEACH_ME_X=` does not shadow the file.
        let var = |name: &str| env(name).filter(|v| !v.is_empty());

        let bind = var("TEACH_ME_BIND_ADDR")
            .or(file.bind_addr)
            .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_owned());
        let bind_addr = bind.parse().map_err(|e| ConfigError::Invalid {
            name: "TEACH_ME_BIND_ADDR",
            reason: format!("`{bind}`: {e}"),
        })?;

        let log_format = match var("TEACH_ME_LOG_FORMAT").as_deref() {
            None => file.log_format.unwrap_or(LogFormat::Text),
            Some("text") => LogFormat::Text,
            Some("json") => LogFormat::Json,
            Some(other) => {
                return Err(ConfigError::Invalid {
                    name: "TEACH_ME_LOG_FORMAT",
                    reason: format!("`{other}` is not one of: text, json"),
                });
            }
        };

        // The standard OpenTelemetry variable wins; an empty value switches export off.
        let otlp_endpoint = var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .or_else(|| file.telemetry.and_then(|t| t.otlp_endpoint))
            .map(|e| e.trim().to_owned())
            .filter(|e| !e.is_empty());

        let body_limit_bytes = parse_num(
            var("TEACH_ME_BODY_LIMIT_BYTES"),
            file.body_limit_bytes,
            "TEACH_ME_BODY_LIMIT_BYTES",
            DEFAULT_BODY_LIMIT_BYTES,
        )?;
        let request_timeout_secs = parse_num(
            var("TEACH_ME_REQUEST_TIMEOUT_SECS"),
            file.request_timeout_secs,
            "TEACH_ME_REQUEST_TIMEOUT_SECS",
            DEFAULT_REQUEST_TIMEOUT_SECS,
        )?;

        let limits = ResourceLimits::from_env(&var)?;

        // Environment only: the key must stay out of the files that get
        // backed up with the database.
        let secret_key = ServerKey::from_base64(
            &var("TEACH_ME_SECRET_KEY").ok_or(ConfigError::MissingSecretKey)?,
        )
        .map_err(ConfigError::InvalidSecretKey)?;
        let legacy_api_token_present = var("TEACH_ME_API_TOKEN").or(file.api_token).is_some();

        let llm = merge_llm(file.llm.unwrap_or_default(), &var)?;

        let database_url = var("TEACH_ME_DATABASE_URL")
            .or(file.database_url)
            .unwrap_or_else(|| DEFAULT_DATABASE_URL.to_owned());
        let data_dir = var("TEACH_ME_DATA_DIR")
            .or(file.data_dir)
            .map_or_else(|| data_dir_of(&database_url), PathBuf::from);
        let pdfium_dir = var("TEACH_ME_PDFIUM_DIR")
            .or(file.pdfium_dir)
            .map(PathBuf::from);

        Ok(Self {
            bind_addr,
            database_url,
            secret_key,
            legacy_api_token_present,
            log_format,
            otlp_endpoint,
            body_limit_bytes,
            request_timeout_secs,
            user_storage_quota_bytes: limits.storage_quota_bytes,
            user_daily_llm_tokens: limits.daily_llm_tokens,
            max_sse_per_user: limits.max_sse_per_user,
            job_max_concurrent: limits.job_max_concurrent,
            job_timeout_secs: limits.job_timeout_secs,
            data_dir,
            pdfium_dir,
            llm,
            audio_dir: var("TEACH_ME_AUDIO_DIR")
                .map_or_else(|| PathBuf::from(DEFAULT_AUDIO_DIR), PathBuf::from),
            tts: merge_tts(&var)?,
        })
    }
}

/// The directory of the SQLite file named by `url` (`sqlite://dir/x.db`,
/// `sqlite:x.db?mode=rwc`); the working directory for a bare file name or an
/// in-memory database.
fn data_dir_of(url: &str) -> PathBuf {
    let path = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))
        .unwrap_or(url);
    let path = path.split('?').next().unwrap_or(path);
    match Path::new(path).parent() {
        Some(dir) if !dir.as_os_str().is_empty() && !path.starts_with(':') => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// `TEACH_ME_TTS_URL` (unset = read-aloud off), `TEACH_ME_TTS_VOICES` (`id:Label,...`),
/// `TEACH_ME_TTS_DEFAULT_VOICE`, `TEACH_ME_TTS_MODEL`.
fn merge_tts(var: &impl Fn(&str) -> Option<String>) -> Result<Option<Qwen3TtsConfig>, ConfigError> {
    let Some(url) = var("TEACH_ME_TTS_URL") else {
        return Ok(None);
    };
    let voices =
        parse_voices(&var("TEACH_ME_TTS_VOICES").unwrap_or_else(|| DEFAULT_TTS_VOICES.to_owned()));
    let default_voice =
        var("TEACH_ME_TTS_DEFAULT_VOICE").unwrap_or_else(|| DEFAULT_TTS_VOICE.to_owned());
    let mut config = Qwen3TtsConfig::new(url, voices, default_voice);
    if let Some(model) = var("TEACH_ME_TTS_MODEL") {
        config.model = model;
    }
    config.validate().map_err(|e| {
        invalid(
            "TEACH_ME_TTS_URL/TEACH_ME_TTS_VOICES/TEACH_ME_TTS_DEFAULT_VOICE",
            e.to_string(),
        )
    })?;
    Ok(Some(config))
}

fn invalid(name: &'static str, reason: impl Into<String>) -> ConfigError {
    ConfigError::Invalid {
        name,
        reason: reason.into(),
    }
}

fn parse_role(key: &str) -> Option<LlmRole> {
    LlmRole::ALL.into_iter().find(|r| r.as_str() == key)
}

#[allow(clippy::too_many_lines)] // one linear pass over the file; splitting hides the flow
/// File + environment -> provider and role settings. Compiled defaults fill
/// whatever the file leaves out.
fn merge_llm(
    file: FileLlm,
    var: &impl Fn(&str) -> Option<String>,
) -> Result<LlmConfig, ConfigError> {
    let default_provider = var("TEACH_ME_LLM_PROVIDER")
        .or(file.default_provider)
        .unwrap_or_else(|| DEFAULT_PROVIDER.to_owned());
    let mut providers = file.providers.unwrap_or_default();
    for (name, provider) in &providers {
        let kind = provider.kind.as_deref().unwrap_or(DEFAULT_PROVIDER);
        if !PROVIDER_KINDS.contains(&kind) {
            return Err(invalid(
                "llm.providers",
                format!(
                    "provider `{name}`: kind must be one of {}",
                    PROVIDER_KINDS.join(", ")
                ),
            ));
        }
    }
    if providers.len() > 1 {
        return Err(invalid(
            "llm.providers",
            "only one provider can be configured for now",
        ));
    }
    let provider = match providers.remove(&default_provider) {
        Some(p) => p,
        // A provider named after a kind needs no section: its defaults suffice
        // (sections of other providers are then ignored).
        None if PROVIDER_KINDS.contains(&default_provider.as_str()) => FileProvider {
            kind: Some(default_provider.clone()),
            ..FileProvider::default()
        },
        None => {
            return Err(invalid(
                "llm.default_provider",
                format!("`{default_provider}` is not defined under [llm.providers]"),
            ));
        }
    };

    let mut anthropic = AnthropicConfig::default();
    // Without `kind`, a section named after a kind is that kind.
    let kind = provider.kind.as_deref().unwrap_or(
        if PROVIDER_KINDS.contains(&default_provider.as_str()) {
            default_provider.as_str()
        } else {
            DEFAULT_PROVIDER
        },
    );
    let openrouter = kind == "openrouter";
    if openrouter {
        anthropic.auth = ApiAuth::Bearer;
        OPENROUTER_BASE_URL.clone_into(&mut anthropic.base_url);
    }
    // Deliberately NOT `ANTHROPIC_BASE_URL`: other tools on the development
    // machine set it for their own purposes (ADR 0004).
    let base_url_var = if openrouter {
        "TEACH_ME_OPENROUTER_BASE_URL"
    } else {
        "TEACH_ME_ANTHROPIC_BASE_URL"
    };
    if let Some(url) = var(base_url_var).or(provider.base_url) {
        anthropic.base_url = url;
    }
    if let Some(version) = provider.api_version {
        anthropic.api_version = version;
    }
    let env_key = if openrouter {
        var("TEACH_ME_OPENROUTER_API_KEY").or_else(|| var("OPENROUTER_API_KEY"))
    } else {
        var("TEACH_ME_ANTHROPIC_API_KEY").or_else(|| var("ANTHROPIC_API_KEY"))
    };
    anthropic.api_key = env_key.or(provider.api_key).and_then(|k| Secret::new(&k));
    for (value, name, target) in [
        (
            provider.connect_timeout_secs,
            "llm connect_timeout_secs",
            &mut anthropic.connect_timeout,
        ),
        (
            provider.request_timeout_secs,
            "llm request_timeout_secs",
            &mut anthropic.request_timeout,
        ),
        (
            provider.stream_idle_timeout_secs,
            "llm stream_idle_timeout_secs",
            &mut anthropic.stream_idle_timeout,
        ),
        (
            provider.server_tool_timeout_secs,
            "llm server_tool_timeout_secs",
            &mut anthropic.server_tool_timeout,
        ),
    ] {
        if let Some(secs) = value {
            if secs == 0 {
                return Err(invalid(name, "must be greater than zero"));
            }
            *target = std::time::Duration::from_secs(secs);
        }
    }
    if let Some(tool) = provider.web_search_tool.filter(|t| !t.trim().is_empty()) {
        anthropic.web_search_tool = tool;
    }
    if let Some(tool) = provider.web_fetch_tool.filter(|t| !t.trim().is_empty()) {
        anthropic.web_fetch_tool = tool;
    }
    // Not proven through OpenRouter (ADR 0009): off there unless declared.
    anthropic.server_web_fetch = provider.server_web_fetch.unwrap_or(!openrouter);
    // Anthropic-only beta: never sent through OpenRouter.
    anthropic.refusal_fallback = !openrouter && provider.refusal_fallback.unwrap_or(false);

    let mut history_budget = DEFAULT_HISTORY_TOKEN_BUDGET;
    let mut max_searches = DEFAULT_RESEARCHER_MAX_SEARCHES;
    // Off until the real test of ADR 0009 is passed.
    let mut max_fetches = 0;
    for (key, role_file) in file.roles.unwrap_or_default() {
        let role = parse_role(&key)
            .ok_or_else(|| invalid("llm.roles", format!("unknown role `{key}`")))?;
        if role_file
            .provider
            .as_deref()
            .is_some_and(|p| p != default_provider)
        {
            return Err(invalid(
                "llm.roles",
                format!("role `{key}`: provider is not defined under [llm.providers]"),
            ));
        }
        let entry: &mut RoleModel = anthropic
            .roles
            .get_mut(&role)
            .ok_or_else(|| invalid("llm.roles", format!("unknown role `{key}`")))?;
        if let Some(model) = role_file.model.filter(|m| !m.trim().is_empty()) {
            entry.model = model;
        }
        if let Some(effort) = role_file.effort {
            if !EFFORTS.contains(&effort.as_str()) {
                return Err(invalid(
                    "llm.roles",
                    format!("role `{key}`: effort must be one of {}", EFFORTS.join(", ")),
                ));
            }
            entry.effort = Some(effort);
        }
        if let Some(max) = role_file.max_output_tokens {
            if max == 0 {
                return Err(invalid(
                    "llm.roles",
                    format!("role `{key}`: max_output_tokens must be greater than zero"),
                ));
            }
            entry.max_output_tokens = max;
        }
        if let (LlmRole::Tutor, Some(budget)) = (role, role_file.history_token_budget) {
            if budget == 0 {
                return Err(invalid(
                    "llm.roles",
                    "history_token_budget must be greater than zero",
                ));
            }
            history_budget = budget;
        }
        if let (LlmRole::Researcher, Some(searches)) = (role, role_file.max_searches) {
            if searches == 0 {
                return Err(invalid(
                    "llm.roles",
                    "max_searches must be greater than zero",
                ));
            }
            max_searches = searches;
        }
        if let (LlmRole::Generator, Some(fetches)) = (role, role_file.max_fetches) {
            max_fetches = fetches;
        }
    }
    // Per-role model overrides from the environment win over the file.
    for role in LlmRole::ALL {
        let name = format!("TEACH_ME_LLM_ROLES_{}_MODEL", role.as_str().to_uppercase());
        if let (Some(model), Some(entry)) = (var(&name), anthropic.roles.get_mut(&role)) {
            entry.model = model;
        }
    }

    // Last, so overrides may use either the Anthropic id or the OpenRouter slug.
    if openrouter {
        for entry in anthropic.roles.values_mut() {
            entry.model = openrouter_model_id(&entry.model);
        }
    }

    Ok(LlmConfig {
        anthropic,
        provider_kind: kind.to_owned(),
        tutor_history_token_budget: history_budget,
        researcher_max_searches: max_searches,
        generator_max_fetches: max_fetches,
    })
}

/// " (line L, column C)" for a byte offset.
fn line_col(text: &str, offset: usize) -> String {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    format!(" (line {line}, column {col})")
}

/// The resource limits of `api-conventions.md` section 12 (environment only).
struct ResourceLimits {
    storage_quota_bytes: u64,
    daily_llm_tokens: u64,
    max_sse_per_user: usize,
    job_max_concurrent: usize,
    job_timeout_secs: u64,
}

impl ResourceLimits {
    fn from_env(var: &impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        Ok(Self {
            storage_quota_bytes: parse_env_num(
                var("TEACH_ME_USER_STORAGE_QUOTA_BYTES"),
                "TEACH_ME_USER_STORAGE_QUOTA_BYTES",
                DEFAULT_STORAGE_QUOTA_BYTES,
                true,
            )?,
            daily_llm_tokens: parse_env_num(
                var("TEACH_ME_USER_DAILY_LLM_TOKENS"),
                "TEACH_ME_USER_DAILY_LLM_TOKENS",
                0,
                true,
            )?,
            max_sse_per_user: parse_env_num(
                var("TEACH_ME_MAX_SSE_PER_USER"),
                "TEACH_ME_MAX_SSE_PER_USER",
                DEFAULT_MAX_SSE_PER_USER,
                false,
            )?,
            job_max_concurrent: parse_env_num(
                var("TEACH_ME_JOB_MAX_CONCURRENT"),
                "TEACH_ME_JOB_MAX_CONCURRENT",
                DEFAULT_JOB_MAX_CONCURRENT,
                false,
            )?,
            job_timeout_secs: parse_env_num(
                var("TEACH_ME_JOB_TIMEOUT_SECS"),
                "TEACH_ME_JOB_TIMEOUT_SECS",
                DEFAULT_JOB_TIMEOUT_SECS,
                false,
            )?,
        })
    }
}

/// An environment-only number. Zero is refused unless `allow_zero` (where it
/// means "off" or "unlimited").
fn parse_env_num<T: std::str::FromStr + PartialOrd + Default>(
    env: Option<String>,
    name: &'static str,
    default: T,
    allow_zero: bool,
) -> Result<T, ConfigError>
where
    T::Err: fmt::Display,
{
    let value = match env {
        Some(raw) => raw.trim().parse::<T>().map_err(|e| ConfigError::Invalid {
            name,
            reason: format!("`{raw}`: {e}"),
        })?,
        None => default,
    };
    if !allow_zero && value <= T::default() {
        return Err(ConfigError::Invalid {
            name,
            reason: "must be greater than zero".to_owned(),
        });
    }
    Ok(value)
}

fn parse_num<T: std::str::FromStr + PartialOrd + Default>(
    env: Option<String>,
    file: Option<T>,
    name: &'static str,
    default: T,
) -> Result<T, ConfigError>
where
    T::Err: fmt::Display,
{
    let value = match env {
        Some(raw) => raw.parse::<T>().map_err(|e| ConfigError::Invalid {
            name,
            reason: format!("`{raw}`: {e}"),
        })?,
        None => file.unwrap_or(default),
    };
    if value <= T::default() {
        return Err(ConfigError::Invalid {
            name,
            reason: "must be greater than zero".to_owned(),
        });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// base64 of 32 bytes (`0123456789abcdef` twice).
    const KEY: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn the_data_dir_follows_the_database_unless_set() {
        for (url, dir) in [
            ("sqlite:///data/teach_me.db", "/data"),
            ("sqlite://teach_me.db", "."),
            ("sqlite://db/x.db?mode=rwc", "db"),
            ("sqlite::memory:", "."),
        ] {
            assert_eq!(data_dir_of(url), PathBuf::from(dir), "{url}");
        }
        let base = [("TEACH_ME_SECRET_KEY", KEY)];
        let mut all = base.to_vec();
        all.push(("TEACH_ME_DATA_DIR", "/srv/tm"));
        all.push(("TEACH_ME_PDFIUM_DIR", "/opt/pdfium"));
        let c = Config::load(None, env(&all)).unwrap();
        assert_eq!(c.data_dir, PathBuf::from("/srv/tm"));
        assert_eq!(c.pdfium_dir, Some(PathBuf::from("/opt/pdfium")));
        let c = Config::load(None, env(&base)).unwrap();
        assert_eq!(c.pdfium_dir, None);
    }

    #[test]
    fn defaults_apply_with_only_token() {
        let c = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", KEY)])).unwrap();
        assert_eq!(c.bind_addr.to_string(), "127.0.0.1:8080");
        assert_eq!(c.log_format, LogFormat::Text);
        assert_eq!(c.body_limit_bytes, 1024 * 1024);
        assert_eq!(c.request_timeout_secs, 30);
    }

    #[test]
    fn the_secret_key_is_required_and_checked() {
        assert!(matches!(
            Config::load(None, env(&[])),
            Err(ConfigError::MissingSecretKey)
        ));
        let err = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", "not base64!")])).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidSecretKey(ServerKeyError::NotBase64)
        ));
        let short = "c2hvcnQ="; // "short"
        let err = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", short)])).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::InvalidSecretKey(ServerKeyError::WrongLength)
        ));
        assert!(!format!("{err}").contains(short));
        let c = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", KEY)])).unwrap();
        assert!(!format!("{c:?}").contains(KEY));
    }

    #[test]
    fn the_configured_llm_key_is_taken_out_before_the_provider_is_built() {
        let mut c = Config::load(
            None,
            env(&[
                ("TEACH_ME_SECRET_KEY", KEY),
                ("TEACH_ME_ANTHROPIC_API_KEY", "sk-from-config"),
            ]),
        )
        .unwrap();
        let taken = c.take_configured_llm_key().unwrap();
        assert_eq!(taken.expose(), "sk-from-config");
        assert!(
            c.llm.anthropic.api_key.is_none(),
            "what builds the provider has no key"
        );
        assert!(c.take_configured_llm_key().is_none());
    }

    #[test]
    fn the_old_api_token_is_accepted_and_reported() {
        let both = [("TEACH_ME_SECRET_KEY", KEY), ("TEACH_ME_API_TOKEN", "x")];
        let c = Config::load(None, env(&both)).unwrap();
        assert!(c.legacy_api_token_present);
        let c = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", KEY)])).unwrap();
        assert!(!c.legacy_api_token_present);
    }

    #[test]
    fn env_wins_over_file() {
        let dir = std::env::temp_dir().join(format!("teach_me_cfg_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.toml");
        std::fs::write(
            &path,
            "bind_addr = \"0.0.0.0:9000\"\nlog_format = \"json\"\n",
        )
        .unwrap();
        let c = Config::load(
            Some(&path),
            env(&[
                ("TEACH_ME_BIND_ADDR", "127.0.0.1:1234"),
                ("TEACH_ME_SECRET_KEY", KEY),
            ]),
        )
        .unwrap();
        assert_eq!(c.bind_addr.port(), 1234);
        assert_eq!(c.log_format, LogFormat::Json);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn read_aloud_is_off_without_a_tts_url() {
        let c = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", KEY)])).unwrap();
        assert!(c.tts.is_none());
        assert_eq!(c.audio_dir, PathBuf::from("/audio"));
    }

    #[test]
    fn the_tts_settings_have_defaults_and_can_be_overridden() {
        let base = [
            ("TEACH_ME_SECRET_KEY", KEY),
            ("TEACH_ME_TTS_URL", "http://tts:8080"),
        ];
        let tts = Config::load(None, env(&base)).unwrap().tts.unwrap();
        assert_eq!(tts.default_voice, "vivian");
        assert_eq!(tts.voices.len(), 6);
        assert_eq!(tts.voices[0].label, "Vivian");

        let mut set = base.to_vec();
        set.extend([
            ("TEACH_ME_TTS_VOICES", "x:Xavier, y:Yara"),
            ("TEACH_ME_TTS_DEFAULT_VOICE", "y"),
            ("TEACH_ME_TTS_MODEL", "other"),
            ("TEACH_ME_AUDIO_DIR", "/srv/audio"),
        ]);
        let c = Config::load(None, env(&set)).unwrap();
        let tts = c.tts.unwrap();
        assert_eq!(tts.default_voice, "y");
        assert_eq!(tts.model, "other");
        assert_eq!(tts.voices.len(), 2);
        assert_eq!(c.audio_dir, PathBuf::from("/srv/audio"));
    }

    #[test]
    fn a_wrong_tts_setting_stops_the_start() {
        let base = [
            ("TEACH_ME_SECRET_KEY", KEY),
            ("TEACH_ME_TTS_URL", "http://tts:8080"),
        ];
        for extra in [
            ("TEACH_ME_TTS_DEFAULT_VOICE", "ghost"),
            ("TEACH_ME_TTS_VOICES", "a:A,a:B"),
            ("TEACH_ME_TTS_VOICES", "a:"),
            ("TEACH_ME_TTS_URL", "not a url"),
        ] {
            let mut set = base.to_vec();
            set.push(extra);
            // The later pair wins in `env`, so `TEACH_ME_TTS_URL` is replaced and the rest added.
            let err = Config::load(None, env(&set)).unwrap_err();
            assert!(
                matches!(err, ConfigError::Invalid { .. }),
                "{extra:?}: {err}"
            );
        }
    }

    #[test]
    fn the_resource_limits_have_the_contract_defaults_and_can_be_overridden() {
        let base = [("TEACH_ME_SECRET_KEY", KEY)];
        let c = Config::load(None, env(&base)).unwrap();
        assert_eq!(c.user_storage_quota_bytes, 1_073_741_824);
        assert_eq!(c.user_daily_llm_tokens, 0);
        assert_eq!(c.max_sse_per_user, 8);
        assert_eq!(c.job_max_concurrent, 2);
        assert_eq!(c.job_timeout_secs, 900);

        let c = Config::load(
            None,
            env(&[
                base[0],
                ("TEACH_ME_USER_STORAGE_QUOTA_BYTES", "0"),
                ("TEACH_ME_USER_DAILY_LLM_TOKENS", "500000"),
                ("TEACH_ME_MAX_SSE_PER_USER", "3"),
                ("TEACH_ME_JOB_MAX_CONCURRENT", "4"),
                ("TEACH_ME_JOB_TIMEOUT_SECS", "60"),
            ]),
        )
        .unwrap();
        assert_eq!(c.user_storage_quota_bytes, 0);
        assert_eq!(c.user_daily_llm_tokens, 500_000);
        assert_eq!(c.max_sse_per_user, 3);
        assert_eq!(c.job_max_concurrent, 4);
        assert_eq!(c.job_timeout_secs, 60);
    }

    #[test]
    fn the_job_and_stream_limits_must_be_positive_and_numbers() {
        for pair in [
            ("TEACH_ME_JOB_MAX_CONCURRENT", "0"),
            ("TEACH_ME_JOB_TIMEOUT_SECS", "0"),
            ("TEACH_ME_MAX_SSE_PER_USER", "0"),
            ("TEACH_ME_USER_DAILY_LLM_TOKENS", "lots"),
            ("TEACH_ME_USER_STORAGE_QUOTA_BYTES", "-1"),
        ] {
            assert!(
                matches!(
                    Config::load(None, env(&[("TEACH_ME_SECRET_KEY", KEY), pair])),
                    Err(ConfigError::Invalid { .. })
                ),
                "{pair:?}"
            );
        }
    }

    #[test]
    fn invalid_values_are_reported() {
        let base = [("TEACH_ME_SECRET_KEY", KEY)];
        let bad = [
            ("TEACH_ME_BIND_ADDR", "nope"),
            ("TEACH_ME_LOG_FORMAT", "xml"),
            ("TEACH_ME_BODY_LIMIT_BYTES", "0"),
        ];
        for pair in bad {
            let mut all = base.to_vec();
            all.push(pair);
            assert!(matches!(
                Config::load(None, env(&all)),
                Err(ConfigError::Invalid { .. })
            ));
        }
    }

    #[test]
    fn toml_type_error_does_not_leak_value() {
        let dir = std::env::temp_dir().join(format!("teach_me_cfg_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.toml");
        std::fs::write(&path, "api_token = 123456789012345678901234567890123\n").unwrap();
        let err = Config::load(Some(&path), env(&[("TEACH_ME_SECRET_KEY", KEY)]))
            .unwrap_err()
            .to_string();
        assert!(!err.contains("1234567890"), "{err}");
        assert!(err.contains("line 1"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn load_toml(text: &str, pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let dir = std::env::temp_dir().join(format!("teach_me_cfg_{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("c.toml");
        std::fs::write(&path, text).unwrap();
        let mut all = vec![("TEACH_ME_SECRET_KEY", KEY)];
        all.extend_from_slice(pairs);
        let result = Config::load(Some(&path), env(&all));
        std::fs::remove_dir_all(dir).unwrap();
        result
    }

    #[test]
    fn llm_defaults_follow_the_adr() {
        let c = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", KEY)])).unwrap();
        let roles = &c.llm.anthropic.roles;
        assert_eq!(roles[&LlmRole::Tutor].model, "claude-sonnet-5-5");
        assert_eq!(roles[&LlmRole::Planner].model, "claude-opus-5-5");
        assert_eq!(roles[&LlmRole::Grader].model, "claude-haiku-5-5");
        assert!(c.llm.anthropic.api_key.is_none());
        assert_eq!(c.llm.anthropic.base_url, "https://api.anthropic.com");
        assert_eq!(c.llm.tutor_history_token_budget, 8000);
        assert!(!c.llm.anthropic.refusal_fallback);
    }

    #[test]
    fn api_key_prefers_the_teach_me_variable_and_ignores_anthropic_base_url() {
        let both = [
            ("TEACH_ME_SECRET_KEY", KEY),
            ("TEACH_ME_ANTHROPIC_API_KEY", "key-one"),
            ("ANTHROPIC_API_KEY", "key-two"),
            ("ANTHROPIC_BASE_URL", "https://elsewhere.example"),
        ];
        let c = Config::load(None, env(&both)).unwrap();
        assert_eq!(c.llm.anthropic.api_key.unwrap().expose(), "key-one");
        assert_eq!(c.llm.anthropic.base_url, "https://api.anthropic.com");

        let fallback = [
            ("TEACH_ME_SECRET_KEY", KEY),
            ("ANTHROPIC_API_KEY", "key-two"),
        ];
        let c = Config::load(None, env(&fallback)).unwrap();
        assert_eq!(c.llm.anthropic.api_key.unwrap().expose(), "key-two");

        let own_url = [
            ("TEACH_ME_SECRET_KEY", KEY),
            ("TEACH_ME_ANTHROPIC_BASE_URL", "http://127.0.0.1:9"),
        ];
        let c = Config::load(None, env(&own_url)).unwrap();
        assert_eq!(c.llm.anthropic.base_url, "http://127.0.0.1:9");
    }

    #[test]
    fn openrouter_uses_its_key_url_bearer_auth_and_slugs() {
        let vars = [
            ("TEACH_ME_SECRET_KEY", KEY),
            ("TEACH_ME_LLM_PROVIDER", "openrouter"),
            ("TEACH_ME_ANTHROPIC_API_KEY", "anthropic-key"),
            ("OPENROUTER_API_KEY", "or-key"),
            ("TEACH_ME_LLM_ROLES_TUTOR_MODEL", "claude-haiku-5-5"),
            (
                "TEACH_ME_LLM_ROLES_GRADER_MODEL",
                "~anthropic/claude-haiku-latest",
            ),
        ];
        let c = Config::load(None, env(&vars)).unwrap();
        let a = &c.llm.anthropic;
        assert_eq!(a.api_key.as_ref().unwrap().expose(), "or-key");
        assert_eq!(a.auth, ApiAuth::Bearer);
        assert_eq!(a.base_url, "https://openrouter.ai/api");
        assert_eq!(a.roles[&LlmRole::Tutor].model, "anthropic/claude-haiku-5.5");
        assert_eq!(
            a.roles[&LlmRole::Planner].model,
            "anthropic/claude-opus-5.5"
        );
        assert_eq!(
            a.roles[&LlmRole::Grader].model,
            "~anthropic/claude-haiku-latest"
        );

        let c = load_toml(
            "[llm]\ndefault_provider = \"or\"\n[llm.providers.or]\nkind = \"openrouter\"\n",
            &[("TEACH_ME_OPENROUTER_API_KEY", "or-key-2")],
        )
        .unwrap();
        assert_eq!(c.llm.anthropic.api_key.unwrap().expose(), "or-key-2");
        assert_eq!(c.llm.anthropic.auth, ApiAuth::Bearer);

        // An Anthropic section in the file does not block the env switch, and
        // a section named `openrouter` without `kind` is OpenRouter.
        for text in [
            "[llm.providers.anthropic]\nstream_idle_timeout_secs = 5\n",
            "[llm]\ndefault_provider = \"openrouter\"\n[llm.providers.openrouter]\nrefusal_fallback = true\n",
        ] {
            let c = load_toml(text, &[("TEACH_ME_LLM_PROVIDER", "openrouter")]).unwrap();
            assert_eq!(c.llm.anthropic.auth, ApiAuth::Bearer, "{text}");
            assert_eq!(c.llm.anthropic.base_url, "https://openrouter.ai/api");
            assert!(!c.llm.anthropic.refusal_fallback);
        }

        let plain = Config::load(None, env(&[("TEACH_ME_SECRET_KEY", KEY)])).unwrap();
        assert_eq!(plain.llm.anthropic.auth, ApiAuth::XApiKey);
    }

    #[test]
    fn llm_file_overrides_only_what_it_names() {
        let c = load_toml(
            "[llm.providers.anthropic]\nrefusal_fallback = true\nstream_idle_timeout_secs = 5\n\
             [llm.roles.tutor]\nmodel = \"claude-haiku-5-5\"\neffort = \"high\"\nhistory_token_budget = 1234\n",
            &[("TEACH_ME_LLM_ROLES_PLANNER_MODEL", "claude-opus-5")],
        )
        .unwrap();
        let roles = &c.llm.anthropic.roles;
        assert_eq!(roles[&LlmRole::Tutor].model, "claude-haiku-5-5");
        assert_eq!(roles[&LlmRole::Tutor].effort.as_deref(), Some("high"));
        assert_eq!(roles[&LlmRole::Tutor].max_output_tokens, 16000);
        assert_eq!(roles[&LlmRole::Planner].model, "claude-opus-5");
        assert_eq!(roles[&LlmRole::Generator].model, "claude-opus-5-5");
        assert_eq!(c.llm.tutor_history_token_budget, 1234);
        assert!(c.llm.anthropic.refusal_fallback);
        assert_eq!(c.llm.anthropic.stream_idle_timeout.as_secs(), 5);
    }

    #[test]
    fn generator_defaults_to_low_effort_and_a_32k_cap_both_overridable() {
        let c = load_toml("", &[]).unwrap();
        let roles = &c.llm.anthropic.roles;
        assert_eq!(roles[&LlmRole::Generator].effort.as_deref(), Some("low"));
        assert_eq!(roles[&LlmRole::Generator].max_output_tokens, 32_000);
        // The Planner keeps `medium`.
        assert_eq!(roles[&LlmRole::Planner].effort.as_deref(), Some("medium"));
        // The Planner's cap has this one source, overridable from the file.
        assert_eq!(roles[&LlmRole::Planner].max_output_tokens, 8_000);
        let c = load_toml("[llm.roles.planner]\nmax_output_tokens = 12000\n", &[]).unwrap();
        assert_eq!(
            c.llm.anthropic.roles[&LlmRole::Planner].max_output_tokens,
            12_000
        );

        let c = load_toml(
            "[llm.roles.generator]\neffort = \"medium\"\nmax_output_tokens = 48000\n",
            &[],
        )
        .unwrap();
        let generator = &c.llm.anthropic.roles[&LlmRole::Generator];
        assert_eq!(generator.effort.as_deref(), Some("medium"));
        assert_eq!(generator.max_output_tokens, 48_000);
        assert!(load_toml("[llm.roles.generator]\nmax_output_tokens = 0\n", &[]).is_err());
    }

    #[test]
    fn the_shipped_example_and_deploy_files_say_what_the_defaults_say() {
        // They are documentation that must not drift from the compiled defaults.
        let deploy = include_str!("../../../deploy/teach_me.toml");
        let c = load_toml(deploy, &[]).unwrap();
        let generator = &c.llm.anthropic.roles[&LlmRole::Generator];
        assert_eq!(generator.effort.as_deref(), Some("low"));
        assert_eq!(generator.max_output_tokens, 32_000);
        let example = include_str!("../../../config/teach_me.example.toml");
        assert!(example.contains("max_output_tokens = 32000"));
        assert!(example.contains("effort = \"low\""));
    }

    #[test]
    fn web_fetch_is_off_by_default_and_declared_per_provider() {
        let c = load_toml("", &[]).unwrap();
        assert_eq!(c.llm.generator_max_fetches, 0);
        assert_eq!(c.llm.anthropic.web_fetch_tool, "web_fetch_20250910");
        assert!(c.llm.anthropic.server_web_fetch);

        let c = load_toml(
            "[llm.providers.anthropic]\nweb_fetch_tool = \"web_fetch_20250910\"\n\
             [llm.roles.generator]\nmax_fetches = 3\n",
            &[],
        )
        .unwrap();
        assert_eq!(c.llm.generator_max_fetches, 3);

        // Through OpenRouter the capability is off until declared.
        let c = load_toml("[llm]\ndefault_provider = \"openrouter\"\n", &[]).unwrap();
        assert!(!c.llm.anthropic.server_web_fetch);
        let c = load_toml(
            "[llm]\ndefault_provider = \"openrouter\"\n[llm.providers.openrouter]\nserver_web_fetch = true\n",
            &[],
        )
        .unwrap();
        assert!(c.llm.anthropic.server_web_fetch);
        assert!(load_toml("[llm.roles.generator]\nmax_fetches = -1\n", &[]).is_err());
    }

    #[test]
    fn researcher_search_settings_have_defaults_and_overrides() {
        let c = load_toml("", &[]).unwrap();
        assert_eq!(c.llm.researcher_max_searches, 4);
        assert_eq!(c.llm.anthropic.server_tool_timeout.as_secs(), 300);
        assert_eq!(c.llm.anthropic.web_search_tool, "web_search_20250305");

        let c = load_toml(
            "[llm.providers.anthropic]\nserver_tool_timeout_secs = 90\nweb_search_tool = \"web_search_20260209\"\n\
             [llm.roles.researcher]\nmax_searches = 6\n",
            &[],
        )
        .unwrap();
        assert_eq!(c.llm.researcher_max_searches, 6);
        assert_eq!(c.llm.anthropic.server_tool_timeout.as_secs(), 90);
        assert_eq!(c.llm.anthropic.web_search_tool, "web_search_20260209");
        assert!(load_toml("[llm.roles.researcher]\nmax_searches = 0\n", &[]).is_err());
        assert!(
            load_toml(
                "[llm.providers.anthropic]\nserver_tool_timeout_secs = 0\n",
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn llm_file_mistakes_are_errors_at_startup() {
        for bad in [
            "[llm.roles.wizard]\nmodel = \"x\"\n",
            "[llm.roles.tutor]\neffort = \"extreme\"\n",
            "[llm.roles.tutor]\nprovider = \"other\"\n",
            "[llm]\ndefault_provider = \"other\"\n",
            "[llm.providers.anthropic]\nkind = \"openai_compatible\"\n",
            "[llm.providers.anthropic]\nrequest_timeout_secs = 0\n",
            "[llm.providers.anthropic]\nunknown_key = 1\n",
        ] {
            assert!(load_toml(bad, &[]).is_err(), "{bad}");
        }
    }

    #[test]
    fn llm_config_debug_hides_the_key() {
        let c = Config::load(
            None,
            env(&[
                ("TEACH_ME_SECRET_KEY", KEY),
                ("TEACH_ME_ANTHROPIC_API_KEY", "sk-secret-value"),
            ]),
        )
        .unwrap();
        assert!(!format!("{c:?}").contains("sk-secret-value"));
    }
}
