use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use api::app::{Limits, build_router};
use api::config::Config;
use api::state::{AppOptions, AppState, AudioSetup, AuthSetup, DocumentSetup, NoPdfEngine};
use api::telemetry;
use api::users_cli::{self, AdminTools, StdConsole};
use application::{
    AudioCleaner, AudioSettings, ChatSettings, LessonSettings, MigrateReviewModel,
    OptimizeReviewParameters, ResearchSettings, RetryPolicy, RetryingLlm, SystemRetryEnv,
};
use domain::JobKind;
use infrastructure::{
    AnthropicProvider, FsClipStore, FsrsOptimizer, FsrsScheduler, LocalDocumentFiles,
    PdfiumTextExtractor, Qwen3TtsSynthesizer, SqliteReviewRepository, SystemClock, UuidV7Generator,
    sqlite,
};
use tokio::net::TcpListener;

const DEFAULT_CONFIG_PATH: &str = "config/teach_me.toml";

#[tokio::main]
#[allow(clippy::too_many_lines)] // the composition root: one block per wire
async fn main() -> anyhow::Result<()> {
    // `teach-me user …` manages users and exits; anything else starts the server.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match args.split_first() {
        Some((first, rest)) if first == "user" => Some(rest.to_vec()),
        _ => None,
    };
    let config_path = std::env::var("TEACH_ME_CONFIG")
        .ok()
        .filter(|p| !p.is_empty())
        .or_else(|| {
            Path::new(DEFAULT_CONFIG_PATH)
                .exists()
                .then(|| DEFAULT_CONFIG_PATH.to_owned())
        });
    let mut config = Config::load(config_path.as_deref().map(Path::new), |k| {
        std::env::var(k).ok()
    })
    .context("invalid configuration")?;

    let telemetry_guard =
        telemetry::init(config.log_format, config.otlp_endpoint.as_deref()).context("telemetry")?;
    if telemetry_guard.exporting() {
        tracing::info!("exporting traces over OTLP");
    }

    let pool = sqlite::connect(&config.database_url)
        .await
        .context("database")?;
    sqlite::migrate(&pool).await.context("database")?;

    if config.legacy_api_token_present {
        tracing::warn!(
            "TEACH_ME_API_TOKEN / `api_token` is no longer used: sign in with a user name and \
             password (`teach-me user passwd owner` sets the owner's); remove it from the configuration"
        );
    }
    let auth_setup = AuthSetup::production(&config.secret_key, &config.llm.provider_kind)
        .map_err(|e| anyhow::anyhow!("cannot set up the credential adapters: {e}"))?;
    // The configured LLM key is not used by the API any more: users bring
    // their own. It becomes the owner's key at first start, and the provider
    // is built without it.
    let configured_key = config.take_configured_llm_key();
    let anthropic = AnthropicProvider::new(config.llm.anthropic.clone()).context("LLM provider")?;
    let llm = Arc::new(RetryingLlm::new(
        Arc::new(anthropic),
        RetryPolicy::default(),
        Arc::new(SystemRetryEnv),
    ));

    // Uploads wait in `<data_dir>/uploads`; a partial file left by a crash is
    // removed at startup.
    let files = Arc::new(LocalDocumentFiles::new(config.data_dir.join("uploads")));
    files.prepare().await.context("upload directory")?;

    let tools = AdminTools::new(
        &pool,
        &auth_setup,
        files.clone(),
        Arc::new(SystemClock),
        Arc::new(UuidV7Generator),
    );
    if let Some(args) = command {
        let mut console = StdConsole;
        return match users_cli::parse(&args) {
            Ok(cmd) => users_cli::run(cmd, &tools, &mut console)
                .await
                .map_err(|e| anyhow::anyhow!(e)),
            Err(e) => Err(anyhow::anyhow!("{e}\n\n{}", users_cli::USAGE)),
        };
    }
    if let Some(key) = configured_key
        && tools
            .admin
            .adopt_configured_key(&key)
            .await
            .context("storing the configured LLM key for the owner")?
    {
        tracing::info!("the configured LLM key is now the owner's key");
    }
    // Before the first request, so the due list and the previews read cards
    // already on the current model. A card this leaves behind (it failed) is
    // not unsafe: a vote or a preview on it replays its votes first.
    migrate_review_model(&pool).await;
    let extractor = pdf_engine(config.pdfium_dir.as_deref());

    let audio = audio_setup(&config).await?;
    let state = AppState::with_audio(
        Arc::new(SystemClock),
        Arc::new(UuidV7Generator),
        auth_setup,
        &pool,
        llm,
        AppOptions {
            chat: ChatSettings {
                history_token_budget: config.llm.tutor_history_token_budget,
                ..ChatSettings::default()
            },
            research: ResearchSettings {
                max_searches: config.llm.researcher_max_searches,
                ..ResearchSettings::default()
            },
            lessons: LessonSettings {
                max_web_fetches: config.llm.generator_max_fetches,
                ..LessonSettings::default()
            },
            ..AppOptions::default()
        },
        DocumentSetup { extractor, files },
        audio,
    );
    // The decoy hash of the login is computed now, not at the first unknown name.
    state.auth.warm_up().await.context("preparing the login")?;
    // No generation survives a restart: settle what the last run left open.
    let interrupted = state
        .chat
        .recover_interrupted()
        .await
        .context("recovering interrupted replies")?;
    if interrupted > 0 {
        tracing::warn!(count = interrupted, "marked interrupted replies as failed");
    }
    // Kinds without a handler are not run: their jobs wait in the queue,
    // visible as `queued`.
    let runner = Arc::new(state.job_runner(vec![
        (JobKind::Research, state.research_handler()),
        (JobKind::LessonGeneration, state.generation_handler()),
        (JobKind::LessonCompletion, state.completion_handler()),
        (JobKind::DocumentImport, state.import_handler()),
    ]));
    let recovery = runner
        .recover_interrupted()
        .await
        .context("recovering interrupted jobs")?;
    if recovery.requeued + recovery.failed > 0 {
        tracing::warn!(
            requeued = recovery.requeued,
            failed = recovery.failed,
            "settled jobs interrupted by the last run"
        );
    }
    let runner_task = tokio::spawn(runner.run());
    // The voice samples are made after all lesson work (they wait for an idle
    // queue); the list is worked out again at every start.
    if let Some(audio) = &state.audio
        && let Err(e) = audio.prepare_samples().await
    {
        tracing::warn!(error = %e, "could not queue the voice samples");
    }
    // One worker makes the clips of the read-aloud feature, one at a time.
    let audio_task = state
        .audio_worker()
        .map(|worker| tokio::spawn(worker.run()));
    // The clip cache is cleaned once a day, from a few minutes after the start.
    // Plain `abort` at shutdown is enough: a pass cut short loses nothing.
    let cleanup_task = state
        .audio
        .as_ref()
        .map(|audio| tokio::spawn(Arc::new(AudioCleaner::new(audio.cache().clone())).run()));
    // Personal review weights: a first pass shortly after the start, then one
    // a day. It waits before its first pass, so it never delays the start.
    let optimizer = Arc::new(FsrsOptimizer::new());
    let weights_task =
        tokio::spawn(review_weights_optimizer(&pool, &state, optimizer.clone()).run());
    let events = state.events.clone();
    let limits = Limits {
        body_bytes: config.body_limit_bytes,
        request_timeout: Duration::from_secs(config.request_timeout_secs),
        ..Limits::default()
    };
    let app = build_router(state, limits);

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("cannot bind {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, version = env!("CARGO_PKG_VERSION"), "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            // Open SSE streams would otherwise hold the shutdown forever.
            events.close_all();
        })
        .await
        .context("server error")?;
    // Jobs still running are settled by the next startup's recovery.
    runner_task.abort();
    if let Some(task) = audio_task {
        // Its running entry is queued again by the next startup.
        task.abort();
    }
    if let Some(task) = cleanup_task {
        task.abort();
    }
    // A training in flight runs on a blocking thread, which `abort` cannot
    // reach and which the runtime would wait for at exit: ask it to stop. It
    // reports "interrupted", which is not recorded, so the next pass makes the
    // attempt again. A rebuild of the cards cut short is resumed by the next
    // pass too: cards not yet rebuilt carry the stamp of the old weights.
    optimizer.stop();
    weights_task.abort();
    tracing::info!("shutdown complete");
    telemetry_guard.shutdown();
    Ok(())
}

/// Moves the review cards still on the previous memory model to the current one
/// by replaying their votes. Never stops the start: a card that fails stays as
/// it is and is tried again at the next start.
async fn migrate_review_model(pool: &sqlite::Pool) {
    let migration = MigrateReviewModel::new(
        Arc::new(SqliteReviewRepository::new(pool.clone())),
        Arc::new(FsrsScheduler::new()),
    );
    let started = std::time::Instant::now();
    match migration.run().await {
        Ok(report) => {
            for (card, error) in &report.failed {
                tracing::error!(%card, %error, "could not migrate the review card; it stays on the previous model");
            }
            tracing::info!(
                migrated = report.migrated,
                failed = report.failed.len(),
                elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                "review cards migrated to the current model"
            );
        }
        Err(error) => {
            tracing::error!(%error, "could not list the review cards to migrate; they stay as they are");
        }
    }
}

/// The background task that trains personal review weights (ADR 0014).
fn review_weights_optimizer(
    pool: &sqlite::Pool,
    state: &AppState,
    optimizer: Arc<FsrsOptimizer>,
) -> Arc<OptimizeReviewParameters> {
    let repo = Arc::new(SqliteReviewRepository::new(pool.clone()));
    Arc::new(OptimizeReviewParameters::new(
        repo.clone(),
        repo,
        Arc::new(FsrsScheduler::new()),
        optimizer,
        state.clock.clone(),
    ))
}

/// The speech engine and the clip directory, or `None` when read-aloud is off
/// (no `TEACH_ME_TTS_URL`): then no directory is created either.
async fn audio_setup(config: &Config) -> anyhow::Result<Option<AudioSetup>> {
    let Some(tts) = config.tts.clone() else {
        tracing::info!("read-aloud is off (TEACH_ME_TTS_URL is not set)");
        return Ok(None);
    };
    let synthesizer = Qwen3TtsSynthesizer::new(tts).context("speech engine")?;
    let store = FsClipStore::new(config.audio_dir.clone());
    store
        .prepare()
        .await
        .with_context(|| format!("audio directory {}", config.audio_dir.display()))?;
    Ok(Some(AudioSetup {
        synthesizer: Arc::new(synthesizer),
        store: Arc::new(store),
        settings: AudioSettings::default(),
    }))
}

/// Without the PDF engine the backend still starts: imports fail with
/// `internal` and say why in the log.
fn pdf_engine(library_dir: Option<&Path>) -> Arc<dyn domain::PdfTextExtractor> {
    match PdfiumTextExtractor::bind(library_dir) {
        Ok(engine) => Arc::new(engine),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "PDF engine not found (libpdfium): uploaded documents cannot be imported; \
                 set TEACH_ME_PDFIUM_DIR to the directory with the library"
            );
            Arc::new(NoPdfEngine)
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { () = ctrl_c => {}, () = terminate => {} }
    tracing::info!("shutdown signal received");
}
