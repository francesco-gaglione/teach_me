//! Users, sessions, the LLM key and how the job runner uses it, over fakes.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use async_trait::async_trait;
use domain::llm::{LlmError, LlmProvider, LlmRequest, LlmRole};
use domain::{
    Document, DocumentFiles, DocumentId, DocumentStatus, IssueKind, Job, JobErrorCode, JobKind,
    JobRepository, JobResult, Secret, SessionRepository, Topic, TopicId, TopicTitle, UserId,
    UserRepository,
};
use time::macros::datetime;
use uuid::Uuid;

use crate::error::AppError;
use crate::fakes::{
    FakeCipher, FakeHasher, FakeLlm, FakeTokens, MemRepo, MemUsers, RecordingEvents, SeqIds,
    TestClock,
};
use crate::{
    AccountService, AuthService, CreateUser, JobContext, JobFailure, JobHandler, JobRunner,
    JobService, JobSettings, KeyScopedLlm, LlmKeys, NewAccount, UserAdmin, with_key,
};

const T0: time::OffsetDateTime = datetime!(2026-11-10 10:00 UTC);

struct Env {
    users: Arc<MemUsers>,
    hasher: Arc<FakeHasher>,
    cipher: Arc<FakeCipher>,
    clock: Arc<TestClock>,
    auth: AuthService,
    create: CreateUser,
    account: AccountService,
    keys: Arc<LlmKeys>,
}

fn env() -> Env {
    let users = MemUsers::with_owner();
    let hasher = Arc::new(FakeHasher::default());
    let cipher = Arc::new(FakeCipher);
    let tokens = Arc::new(FakeTokens::default());
    let clock = Arc::new(TestClock::new(T0));
    let ids = Arc::new(SeqIds::default());
    Env {
        auth: AuthService::new(
            users.clone(),
            users.clone(),
            hasher.clone(),
            tokens.clone(),
            clock.clone(),
            ids.clone(),
        ),
        create: CreateUser::new(users.clone(), hasher.clone(), tokens, clock.clone(), ids),
        account: AccountService::new(users.clone(), cipher.clone()),
        keys: Arc::new(LlmKeys::new(users.clone(), cipher.clone())),
        users,
        hasher,
        cipher,
        clock,
    }
}

fn account(name: &str, password: &str) -> NewAccount {
    NewAccount {
        username: name.into(),
        display_name: None,
        password: Some(password.into()),
    }
}

fn fields(err: AppError) -> Vec<(&'static str, IssueKind)> {
    match err {
        AppError::Validation(v) => v.into_iter().map(|i| (i.field, i.issue)).collect(),
        other => panic!("expected a validation error, got {other:?}"),
    }
}

// ---------- CreateUser ----------

#[tokio::test]
async fn create_user_validates_stores_and_generates_a_password() {
    let e = env();
    let made = e
        .create
        .execute(account("Ann.Lee", "long enough pw"))
        .await
        .unwrap();
    assert_eq!(made.user.username.as_str(), "ann.lee");
    assert_eq!(made.user.display_name.as_str(), "Ann.Lee");
    assert!(made.generated_password.is_none());

    let bad = e.create.execute(account("a", "short")).await.unwrap_err();
    assert_eq!(
        fields(bad),
        [
            ("username", IssueKind::TooShort),
            ("password", IssueKind::TooShort)
        ]
    );
    let dup = e
        .create
        .execute(account("ANN.LEE", "long enough pw"))
        .await
        .unwrap_err();
    assert_eq!(fields(dup), [("username", IssueKind::Duplicate)]);

    let generated = e
        .create
        .execute(NewAccount {
            username: "bob".into(),
            display_name: Some("Bob B.".into()),
            password: None,
        })
        .await
        .unwrap();
    let pw = generated.generated_password.expect("a generated password");
    assert!(e.auth.login("bob", pw.expose(), None).await.is_ok());
}

// ---------- login ----------

#[tokio::test]
async fn every_failed_login_costs_one_verification_and_looks_the_same() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    e.create
        .execute(account("off", "correct horse"))
        .await
        .unwrap();
    let off = e
        .users
        .by_username(&domain::Username::parse("off").unwrap())
        .await
        .unwrap()
        .unwrap();
    e.users.set_disabled(off.id, Some(T0)).await.unwrap();

    let attempts = [
        ("ann", "wrong password"),   // wrong password
        ("nobody", "correct horse"), // unknown name
        ("off", "correct horse"),    // disabled, right password
        ("owner", "correct horse"),  // an account without a password
        ("ann", ""),                 // nothing to check
    ];
    for (name, password) in attempts {
        let before = e.hasher.verifications.load(Ordering::SeqCst);
        let err = e.auth.login(name, password, None).await.unwrap_err();
        assert!(
            matches!(err, AppError::InvalidCredentials),
            "{name}: {err:?}"
        );
        assert_eq!(
            e.hasher.verifications.load(Ordering::SeqCst) - before,
            1,
            "{name}: one verification, whatever is wrong"
        );
    }
    // ...and the success costs the same.
    let before = e.hasher.verifications.load(Ordering::SeqCst);
    e.auth
        .login("ann", "correct horse", Some("Tablet"))
        .await
        .unwrap();
    assert_eq!(e.hasher.verifications.load(Ordering::SeqCst) - before, 1);
}

#[tokio::test]
async fn five_failures_in_fifteen_minutes_block_the_name_even_for_the_right_password() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    for _ in 0..5 {
        assert!(matches!(
            e.auth.login("ann", "nope nope nope", None).await,
            Err(AppError::InvalidCredentials)
        ));
    }
    match e.auth.login("ann", "correct horse", None).await {
        Err(AppError::RateLimited { retry_after_secs }) => {
            assert!((1..=900).contains(&retry_after_secs), "{retry_after_secs}");
        }
        other => panic!("expected the block, got {other:?}"),
    }
    // Another name is not affected; an unknown name is blocked the same way.
    e.create
        .execute(account("bob", "correct horse"))
        .await
        .unwrap();
    assert!(e.auth.login("bob", "correct horse", None).await.is_ok());
    for _ in 0..5 {
        let _ = e.auth.login("ghost", "x", None).await;
    }
    assert!(matches!(
        e.auth.login("ghost", "x", None).await,
        Err(AppError::RateLimited { .. })
    ));

    e.clock.set(T0 + time::Duration::minutes(15));
    assert!(e.auth.login("ann", "correct horse", None).await.is_ok());
}

#[tokio::test]
async fn a_success_forgets_the_earlier_failures() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    for _ in 0..4 {
        let _ = e.auth.login("ann", "bad bad bad bad", None).await;
    }
    e.auth.login("ann", "correct horse", None).await.unwrap();
    for _ in 0..4 {
        assert!(matches!(
            e.auth.login("ann", "bad bad bad bad", None).await,
            Err(AppError::InvalidCredentials)
        ));
    }
}

#[tokio::test]
async fn login_validates_the_device_name() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    let err = e
        .auth
        .login("ann", "correct horse", Some(&"d".repeat(61)))
        .await
        .unwrap_err();
    assert_eq!(fields(err), [("device_name", IssueKind::TooLong)]);
}

// ---------- sessions ----------

#[tokio::test]
async fn a_session_expires_after_ninety_idle_days_and_a_request_renews_it() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    let login = e.auth.login("ann", "correct horse", None).await.unwrap();
    assert_eq!(login.expires_at, T0 + time::Duration::days(90));
    let token = login.token.expose().to_owned();

    // 80 days later: valid, and the use pushes the expiry out.
    e.clock.set(T0 + time::Duration::days(80));
    let who = e.auth.authenticate(&token).await.unwrap();
    assert_eq!(who.user_id, login.user.id);
    // 89 days after that is still inside the renewed window...
    e.clock.set(T0 + time::Duration::days(80 + 89));
    assert!(e.auth.authenticate(&token).await.is_ok());
    // ...and 90 idle days are not.
    e.clock.set(T0 + time::Duration::days(80 + 89 + 90));
    assert!(matches!(
        e.auth.authenticate(&token).await,
        Err(AppError::Unauthorized)
    ));
    assert!(matches!(
        e.auth.authenticate(&token).await,
        Err(AppError::Unauthorized)
    ));
}

#[tokio::test]
async fn logout_unknown_tokens_and_disabled_users_are_unauthorized() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    let one = e.auth.login("ann", "correct horse", None).await.unwrap();
    let two = e.auth.login("ann", "correct horse", None).await.unwrap();
    let who = e.auth.authenticate(one.token.expose()).await.unwrap();
    e.auth.logout(who.session_id).await.unwrap();
    assert!(matches!(
        e.auth.authenticate(one.token.expose()).await,
        Err(AppError::Unauthorized)
    ));
    assert!(matches!(
        e.auth.authenticate("tok-unknown").await,
        Err(AppError::Unauthorized)
    ));
    // The other device is untouched, until the user is disabled.
    assert!(e.auth.authenticate(two.token.expose()).await.is_ok());
    e.users.set_disabled(two.user.id, Some(T0)).await.unwrap();
    assert!(matches!(
        e.auth.authenticate(two.token.expose()).await,
        Err(AppError::Unauthorized)
    ));
}

#[tokio::test]
async fn changing_the_password_revokes_the_other_sessions_only() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    let phone = e.auth.login("ann", "correct horse", None).await.unwrap();
    let tablet = e.auth.login("ann", "correct horse", None).await.unwrap();
    let tablet_session = e.auth.authenticate(tablet.token.expose()).await.unwrap();

    let wrong = e
        .auth
        .change_password(
            tablet.user.id,
            tablet_session.session_id,
            "not it at all",
            "new password 1",
        )
        .await
        .unwrap_err();
    assert_eq!(fields(wrong), [("current_password", IssueKind::Wrong)]);
    let short = e
        .auth
        .change_password(
            tablet.user.id,
            tablet_session.session_id,
            "correct horse",
            "short",
        )
        .await
        .unwrap_err();
    assert_eq!(fields(short), [("new_password", IssueKind::TooShort)]);
    // Nothing happened yet.
    assert!(e.auth.authenticate(phone.token.expose()).await.is_ok());

    e.auth
        .change_password(
            tablet.user.id,
            tablet_session.session_id,
            "correct horse",
            "new password 1",
        )
        .await
        .unwrap();
    assert!(matches!(
        e.auth.authenticate(phone.token.expose()).await,
        Err(AppError::Unauthorized)
    ));
    assert!(e.auth.authenticate(tablet.token.expose()).await.is_ok());
    assert!(matches!(
        e.auth.login("ann", "correct horse", None).await,
        Err(AppError::InvalidCredentials)
    ));
    assert!(e.auth.login("ann", "new password 1", None).await.is_ok());
}

// ---------- the LLM key ----------

#[tokio::test]
async fn the_key_is_stored_sealed_never_in_the_clear() {
    let e = env();
    assert!(!e.account.me(UserId::OWNER).await.unwrap().has_llm_key);
    assert!(matches!(
        e.keys.require(UserId::OWNER).await,
        Err(AppError::LlmKeyMissing)
    ));

    let me = e
        .account
        .set_llm_key(UserId::OWNER, "  sk-or-secret \n")
        .await
        .unwrap();
    assert!(me.has_llm_key);
    assert_eq!(
        e.keys.require(UserId::OWNER).await.unwrap().expose(),
        "sk-or-secret"
    );
    let stored = e.users.llm_key(UserId::OWNER).await.unwrap().unwrap();
    assert_ne!(stored.as_bytes(), b"sk-or-secret");
    // Bound to its owner: the same bytes do not open for someone else.
    assert!(
        e.cipher_open_for(UserId::new(Uuid::from_u128(9)), &stored)
            .is_none()
    );

    let bad = e
        .account
        .set_llm_key(UserId::OWNER, "   ")
        .await
        .unwrap_err();
    assert_eq!(fields(bad), [("api_key", IssueKind::Empty)]);
    let long = e
        .account
        .set_llm_key(UserId::OWNER, &"k".repeat(501))
        .await
        .unwrap_err();
    assert_eq!(fields(long), [("api_key", IssueKind::TooLong)]);

    e.account.clear_llm_key(UserId::OWNER).await.unwrap();
    assert!(matches!(
        e.keys.require(UserId::OWNER).await,
        Err(AppError::LlmKeyMissing)
    ));
}

impl Env {
    fn cipher_open_for(&self, user: UserId, sealed: &domain::SealedSecret) -> Option<Secret> {
        use domain::SecretCipher;
        self.cipher.open(&crate::key_context(user), sealed).ok()
    }
}

#[tokio::test]
async fn a_key_that_cannot_be_opened_counts_as_missing() {
    let e = env();
    e.users
        .set_llm_key(
            UserId::OWNER,
            Some(&domain::SealedSecret::new(b"garbage".to_vec())),
        )
        .await
        .unwrap();
    assert!(matches!(
        e.keys.require(UserId::OWNER).await,
        Err(AppError::LlmKeyMissing)
    ));
}

#[tokio::test]
async fn requests_carry_the_scoped_key_and_never_one_by_default() {
    let llm = FakeLlm::with(vec![vec![], vec![]]);
    let scoped = KeyScopedLlm::new(llm.clone());
    scoped
        .complete(LlmRequest::new(LlmRole::Tutor, "t"))
        .await
        .ok();
    with_key(UserId::OWNER, Secret::new("sk-a").unwrap(), async {
        scoped
            .complete(LlmRequest::new(LlmRole::Tutor, "t"))
            .await
            .ok();
    })
    .await;
    let seen = llm.requests.lock().unwrap();
    assert_eq!(
        seen[0].api_key, None,
        "outside a scope: no key, the adapter refuses"
    );
    assert_eq!(seen[1].api_key.as_ref().map(Secret::expose), Some("sk-a"));
}

// ---------- the job runner and the owner's key ----------

/// A handler that calls the model, like the Researcher does.
struct CallsModel(Arc<dyn LlmProvider>);

#[async_trait]
impl JobHandler for CallsModel {
    fn needs_llm(&self) -> bool {
        true
    }

    async fn run(&self, _ctx: &JobContext) -> Result<JobResult, JobFailure> {
        match self
            .0
            .complete(LlmRequest::new(LlmRole::Researcher, "r"))
            .await
        {
            Ok(_) => Ok(JobResult::Research {
                resource_count: 0,
                gap_count: 0,
            }),
            Err(LlmError::Authentication) => Err(JobFailure(JobErrorCode::LlmKeyRejected)),
            Err(_) => Err(JobFailure(JobErrorCode::UpstreamFailed)),
        }
    }
}

struct Jobs {
    repo: Arc<MemRepo>,
    users: Arc<MemUsers>,
    llm: Arc<FakeLlm>,
    service: Arc<JobService>,
    runner: JobRunner,
    a: TopicId,
    b: TopicId,
}

const USER_B: UserId = UserId::new(Uuid::from_u128(0xb));

fn jobs_env() -> Jobs {
    let repo = Arc::new(MemRepo::default());
    let users = MemUsers::with_owner();
    let cipher = Arc::new(FakeCipher);
    let keys = Arc::new(LlmKeys::new(users.clone(), cipher.clone()));
    let clock = Arc::new(TestClock::new(T0));
    let ids = Arc::new(SeqIds::default());
    let (a, b) = (
        TopicId::new(Uuid::from_u128(0xa1)),
        TopicId::new(Uuid::from_u128(0xb1)),
    );
    {
        let mut s = repo.0.lock().unwrap();
        for (topic, user) in [(a, UserId::OWNER), (b, USER_B)] {
            let folder = domain::FolderId::new(Uuid::from_u128(topic.as_uuid().as_u128() + 1));
            s.folders.push(domain::Folder {
                id: folder,
                user_id: user,
                name: domain::FolderName::parse("F").unwrap(),
                position: 0,
                created_at: T0,
                updated_at: T0,
            });
            s.topics.push(Topic::new(
                topic,
                user,
                folder,
                TopicTitle::parse("T").unwrap(),
                T0,
            ));
        }
    }
    let llm = FakeLlm::with(vec![]);
    let scoped: Arc<dyn LlmProvider> = Arc::new(KeyScopedLlm::new(llm.clone()));
    let service = Arc::new(JobService::new(
        repo.clone(),
        repo.clone(),
        Arc::new(RecordingEvents::default()),
        clock,
        ids,
        keys,
    ));
    let runner = JobRunner::new(
        service.clone(),
        JobSettings {
            max_concurrent: 1,
            job_timeout: Duration::from_secs(10),
            max_attempts: 3,
            store_retry_delay: Duration::from_millis(1),
        },
    )
    .with_handler(JobKind::Research, Arc::new(CallsModel(scoped)));
    Jobs {
        repo,
        users,
        llm,
        service,
        runner,
        a,
        b,
    }
}

impl Jobs {
    fn give_key(&self, user: UserId, key: &str) {
        if user != UserId::OWNER {
            // The fake repository only knows `owner`: add the other user.
            self.users.add_plain(user);
        }
        self.users.give_key(user, &FakeCipher, key);
    }

    async fn queue(&self, topic: TopicId) -> Job {
        let job = self.service.new_job(topic, JobKind::Research);
        self.repo.insert(&job).await.unwrap();
        job
    }

    fn outcome(&self, job: &Job) -> Job {
        self.repo
            .0
            .lock()
            .unwrap()
            .jobs
            .iter()
            .find(|j| j.id == job.id)
            .cloned()
            .unwrap()
    }
}

#[tokio::test]
async fn a_job_runs_with_the_key_of_the_owner_of_its_topic() {
    let j = jobs_env();
    j.give_key(UserId::OWNER, "sk-of-a");
    j.give_key(USER_B, "sk-of-b");
    let for_b = j.queue(j.b).await;
    let for_a = j.queue(j.a).await;
    assert!(j.runner.run_next().await.unwrap());
    assert!(j.runner.run_next().await.unwrap());
    let keys: Vec<Option<String>> = j
        .llm
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.api_key.as_ref().map(|k| k.expose().to_owned()))
        .collect();
    // The queue is oldest first: B's job ran first, with B's key.
    assert_eq!(keys, [Some("sk-of-b".into()), Some("sk-of-a".into())]);
    assert_eq!(j.outcome(&for_b).status, domain::JobStatus::Succeeded);
    assert_eq!(j.outcome(&for_a).status, domain::JobStatus::Succeeded);
}

#[tokio::test]
async fn a_job_of_an_owner_without_a_key_fails_with_llm_key_missing_and_never_calls_the_model() {
    let j = jobs_env();
    j.give_key(UserId::OWNER, "sk-of-a");
    j.users.add_plain(USER_B); // no key
    let for_b = j.queue(j.b).await;
    assert!(j.runner.run_next().await.unwrap());
    let done = j.outcome(&for_b);
    assert_eq!(done.status, domain::JobStatus::Failed);
    assert_eq!(done.error.unwrap().code, JobErrorCode::LlmKeyMissing);
    assert!(j.llm.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_provider_refusal_is_llm_key_rejected() {
    let j = jobs_env();
    j.give_key(UserId::OWNER, "sk-bad");
    j.llm
        .start_errors
        .lock()
        .unwrap()
        .push_back(LlmError::Authentication);
    let job = j.queue(j.a).await;
    assert!(j.runner.run_next().await.unwrap());
    assert_eq!(
        j.outcome(&job).error.unwrap().code,
        JobErrorCode::LlmKeyRejected
    );
}

#[tokio::test]
async fn starting_or_retrying_llm_work_without_a_key_is_refused_before_anything_is_created() {
    let j = jobs_env();
    // enqueue: 409, no job.
    let err = j
        .service
        .enqueue(UserId::OWNER, j.a, JobKind::Research)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::LlmKeyMissing), "{err:?}");
    assert_eq!(j.repo.0.lock().unwrap().jobs.len(), 0);

    // A failed job cannot be retried without a key either; with one it can.
    let mut failed = j.service.new_job(j.a, JobKind::Research);
    failed.start(T0).unwrap();
    failed
        .fail(JobErrorCode::LlmKeyMissing, domain::ContentLanguage::It, T0)
        .unwrap();
    j.repo.0.lock().unwrap().jobs.push(failed.clone());
    let err = j
        .service
        .retry(UserId::OWNER, j.a, failed.id)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::LlmKeyMissing), "{err:?}");
    j.give_key(UserId::OWNER, "sk-late");
    let retried = j
        .service
        .retry(UserId::OWNER, j.a, failed.id)
        .await
        .unwrap();
    assert_eq!(retried.status, domain::JobStatus::Queued);
    // Another user's job is simply not found.
    let err = j.service.retry(USER_B, j.a, failed.id).await.unwrap_err();
    assert!(matches!(err, AppError::NotFound), "{err:?}");
}

#[tokio::test]
async fn importing_a_document_needs_no_key() {
    assert!(!JobKind::DocumentImport.needs_llm());
    assert!(JobKind::Research.needs_llm());
    assert!(JobKind::LessonGeneration.needs_llm());
    assert!(JobKind::LessonCompletion.needs_llm());
}

// ---------- the operator's side ----------

struct Files(std::sync::Mutex<Vec<DocumentId>>);

#[async_trait]
impl DocumentFiles for Files {
    fn path(&self, id: DocumentId) -> std::path::PathBuf {
        format!("/uploads/{id}").into()
    }
    fn partial_path(&self, id: DocumentId) -> std::path::PathBuf {
        format!("/uploads/{id}.part").into()
    }
    async fn remove(&self, id: DocumentId) {
        self.0.lock().unwrap().push(id);
    }
}

fn admin(e: &Env, repo: Arc<MemRepo>, files: Arc<Files>) -> UserAdmin {
    UserAdmin::new(
        e.users.clone(),
        e.users.clone(),
        repo,
        files,
        e.hasher.clone(),
        Arc::new(FakeTokens::default()),
        e.cipher.clone(),
        e.clock.clone(),
    )
}

#[tokio::test]
async fn set_password_and_disable_revoke_every_session() {
    let e = env();
    let admin = admin(
        &e,
        Arc::new(MemRepo::default()),
        Arc::new(Files(std::sync::Mutex::default())),
    );
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    let s = e.auth.login("ann", "correct horse", None).await.unwrap();

    let set = admin
        .set_password("ANN", Some("another long one"))
        .await
        .unwrap();
    assert!(set.generated.is_none());
    assert!(matches!(
        e.auth.authenticate(s.token.expose()).await,
        Err(AppError::Unauthorized)
    ));
    let s = e.auth.login("ann", "another long one", None).await.unwrap();

    admin.set_disabled("ann", true).await.unwrap();
    assert!(matches!(
        e.auth.authenticate(s.token.expose()).await,
        Err(AppError::Unauthorized)
    ));
    assert!(matches!(
        e.auth.login("ann", "another long one", None).await,
        Err(AppError::InvalidCredentials)
    ));
    admin.set_disabled("ann", false).await.unwrap();
    assert!(e.auth.login("ann", "another long one", None).await.is_ok());

    // `owner` has no password until the operator sets one.
    let made = admin.set_password("owner", None).await.unwrap();
    let pw = made.generated.unwrap();
    assert!(e.auth.login("owner", pw.expose(), None).await.is_ok());
    assert!(matches!(
        admin.set_password("ghost", None).await,
        Err(AppError::NotFound)
    ));
}

#[tokio::test]
async fn rename_and_the_configured_key_adoption() {
    let e = env();
    let admin = admin(
        &e,
        Arc::new(MemRepo::default()),
        Arc::new(Files(std::sync::Mutex::default())),
    );
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    assert_eq!(
        fields(admin.rename("ann", "owner").await.unwrap_err()),
        [("username", IssueKind::Duplicate)]
    );
    admin.rename("ann", "anna").await.unwrap();
    assert!(e.auth.login("anna", "correct horse", None).await.is_ok());

    let key = Secret::new("sk-from-config").unwrap();
    assert!(admin.adopt_configured_key(&key).await.unwrap());
    assert_eq!(
        e.keys.require(UserId::OWNER).await.unwrap().expose(),
        "sk-from-config"
    );
    // Not twice, and never over a key the owner chose.
    e.account
        .set_llm_key(UserId::OWNER, "sk-chosen")
        .await
        .unwrap();
    assert!(!admin.adopt_configured_key(&key).await.unwrap());
    assert_eq!(
        e.keys.require(UserId::OWNER).await.unwrap().expose(),
        "sk-chosen"
    );
}

#[tokio::test]
async fn deleting_a_user_removes_their_data_and_their_files() {
    let e = env();
    let repo = Arc::new(MemRepo::default());
    let files = Arc::new(Files(std::sync::Mutex::default()));
    let admin = admin(&e, repo.clone(), files.clone());
    let made = e
        .create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    let ann = made.user.id;
    let topic = TopicId::new(Uuid::from_u128(0xa1));
    let doc = DocumentId::new(Uuid::from_u128(0xd1));
    {
        let mut s = repo.0.lock().unwrap();
        let folder = domain::FolderId::new(Uuid::from_u128(0xf1));
        s.folders.push(domain::Folder {
            id: folder,
            user_id: ann,
            name: domain::FolderName::parse("F").unwrap(),
            position: 0,
            created_at: T0,
            updated_at: T0,
        });
        s.topics.push(Topic::new(
            topic,
            ann,
            folder,
            TopicTitle::parse("T").unwrap(),
            T0,
        ));
        s.documents.push(crate::fakes::DocumentRow {
            document: Document::importing(doc, topic, "a.pdf".into(), 10, T0),
            sections: vec![],
            pages: vec![],
        });
    }
    assert_eq!(
        DocumentStatus::Importing,
        repo.0.lock().unwrap().documents[0].document.status
    );
    let cleaned = admin.delete("ann").await.unwrap();
    assert_eq!(cleaned, 1);
    assert_eq!(*files.0.lock().unwrap(), [doc]);
    assert!(e.users.get(ann).await.unwrap().is_none());
    assert!(matches!(admin.delete("ann").await, Err(AppError::NotFound)));
    let _ = (JobKind::Research, e.users.sessions.lock().unwrap().len());
    let _ = SessionRepository::delete_expired(&*e.users, T0).await;
}

// ---------- hardening of the login ----------

#[tokio::test]
async fn a_name_longer_than_any_real_one_never_enters_the_limiter_with_its_size() {
    let e = env();
    let huge = "n".repeat(1_000_000);
    for _ in 0..5 {
        assert!(matches!(
            e.auth.login(&huge, "x", None).await,
            Err(AppError::InvalidCredentials)
        ));
    }
    // One shared entry, however many such names come; then they are blocked.
    for i in 0..50 {
        let _ = e.auth.login(&format!("{i}{huge}"), "x", None).await;
    }
    assert_eq!(e.auth.limiter.tracked_names(), 1);
    assert!(matches!(
        e.auth.login(&huge, "x", None).await,
        Err(AppError::RateLimited { .. })
    ));
    // Real names are not affected, and none was looked up in the database.
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    assert!(e.auth.login("ann", "correct horse", None).await.is_ok());
}

#[test]
fn a_full_limiter_still_limits_every_new_name() {
    let limiter = crate::LoginLimiter::new();
    for i in 0..10_000 {
        limiter
            .reserve(
                &format!("invented-{i}"),
                T0 + time::Duration::milliseconds(i),
            )
            .unwrap();
    }
    let now = T0 + time::Duration::seconds(30);
    for _ in 0..5 {
        limiter.reserve("the-real-user", now).unwrap();
    }
    assert!(
        limiter.reserve("the-real-user", now).is_err(),
        "the new name is limited too"
    );
    assert!(limiter.lock_len() <= 10_000, "and the map did not grow");
}

#[tokio::test]
async fn a_parallel_burst_on_one_name_cannot_get_past_five_attempts() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    let hasher = e.hasher.clone();
    let before = hasher.verifications.load(Ordering::SeqCst);
    let auth = Arc::new(e.auth);
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..40 {
        let auth = auth.clone();
        tasks.spawn(async move { auth.login("ann", "wrong wrong wrong", None).await });
    }
    let (mut wrong, mut blocked) = (0, 0);
    while let Some(done) = tasks.join_next().await {
        match done.unwrap() {
            Err(AppError::InvalidCredentials) => wrong += 1,
            Err(AppError::RateLimited { .. }) => blocked += 1,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!((wrong, blocked), (5, 35));
    assert_eq!(hasher.verifications.load(Ordering::SeqCst) - before, 5);
}

#[tokio::test]
async fn the_decoy_is_computed_at_startup_not_at_the_first_unknown_login() {
    let e = env();
    assert_eq!(e.hasher.hashes.load(Ordering::SeqCst), 0);
    e.auth.warm_up().await.unwrap();
    assert_eq!(e.hasher.hashes.load(Ordering::SeqCst), 1);
    let _ = e.auth.login("nobody", "whatever it is", None).await;
    let _ = e.auth.login("nobody-else", "whatever it is", None).await;
    assert_eq!(
        e.hasher.hashes.load(Ordering::SeqCst),
        1,
        "never computed again"
    );
}

#[tokio::test]
async fn a_login_that_fails_after_the_session_was_stored_leaves_no_session() {
    let e = env();
    e.create
        .execute(account("ann", "correct horse"))
        .await
        .unwrap();
    e.users.fail_last_login.store(true, Ordering::SeqCst);
    assert!(matches!(
        e.auth.login("ann", "correct horse", None).await,
        Err(AppError::Storage(_))
    ));
    assert_eq!(e.users.sessions.lock().unwrap().len(), 0);
    e.users.fail_last_login.store(false, Ordering::SeqCst);
    assert!(e.auth.login("ann", "correct horse", None).await.is_ok());
    assert_eq!(e.users.sessions.lock().unwrap().len(), 1);
}
