//! Ports: traits implemented by the infrastructure layer.

mod audio;
mod clock;
mod crypto;
mod events;
mod id_generator;
pub mod llm;
mod parameter_optimizer;
mod repository;
mod review_scheduler;

pub use audio::{AudioQueue, ClipRepository, ClipStore, SpeechSynthesizer, StoredFile};
pub use clock::Clock;
pub use crypto::{CryptoError, PasswordHasher, SecretCipher, TokenIssuer};
pub use events::TopicEventPublisher;
pub use id_generator::IdGenerator;
pub use parameter_optimizer::{OptimizerError, ParameterOptimizer};
pub use repository::{
    AttemptRepository, ChatRepository, DocumentRepository, FolderRepository, HighlightRepository,
    JobRepository, LearningRepository, LessonRepository, LlmUsageRepository, MissionRepository,
    PreferenceRepository, RepositoryError, ResourceRepository, ReviewMigrationRepository,
    ReviewOptimizationRepository, ReviewRepository, SessionRepository, StrokeRepository,
    TopicRepository, UserRepository,
};
pub use review_scheduler::{ReviewScheduler, SchedulerError};
