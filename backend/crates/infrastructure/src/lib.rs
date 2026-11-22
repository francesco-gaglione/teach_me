//! Infrastructure layer: adapters implementing the domain ports.

pub mod anthropic;
pub mod audio;
pub mod audio_queue;
pub mod chat;
pub mod clock;
pub mod crypto;
pub mod documents;
pub mod event_hub;
pub mod fsrs;
pub mod fsrs_optimizer;
pub mod id;
pub mod jobs;
pub mod lessons;
pub mod llm_usage;
pub mod missions;
pub mod pdf;
pub mod preferences;
pub mod repositories;
pub mod resources;
pub mod reviews;
pub mod secret;
pub mod sqlite;
pub mod tts;
pub mod users;

pub use self::fsrs::FsrsScheduler;
pub use anthropic::{AnthropicConfig, AnthropicProvider, ApiAuth, RoleModel};
pub use audio::{FsClipStore, SqliteClipRepository};
pub use audio_queue::SqliteAudioQueue;
pub use chat::SqliteChatRepository;
pub use clock::SystemClock;
pub use crypto::{Argon2idHasher, OsTokenIssuer, ServerKey, ServerKeyError, XChaChaSecretCipher};
pub use documents::{LocalDocumentFiles, SqliteDocumentRepository};
pub use event_hub::{EventHub, EventHubConfig, StoredEvent, Subscription};
pub use fsrs_optimizer::FsrsOptimizer;
pub use id::UuidV7Generator;
pub use jobs::SqliteJobRepository;
pub use lessons::{
    SqliteAttemptRepository, SqliteHighlightRepository, SqliteLearningRepository,
    SqliteLessonRepository, SqliteStrokeRepository,
};
pub use llm_usage::SqliteLlmUsageRepository;
pub use missions::SqliteMissionRepository;
pub use pdf::PdfiumTextExtractor;
pub use preferences::SqlitePreferenceRepository;
pub use repositories::{SqliteFolderRepository, SqliteTopicRepository};
pub use resources::SqliteResourceRepository;
pub use reviews::SqliteReviewRepository;
pub use secret::Secret;
pub use tts::{Qwen3TtsBuildError, Qwen3TtsConfig, Qwen3TtsSynthesizer, parse_voices};
pub use users::{SqliteSessionRepository, SqliteUserRepository};
