//! A fresh in-memory application per run, and the seeding of what a case
//! describes. Seeding goes through the repositories where one exists and
//! through SQL where the real flow would need an earlier model call (a
//! confirmed mission, a researched topic, a finished lesson).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use api::state::{AppOptions, AppState, AuthSetup};
use application::{ChatSettings, JobSettings, LessonSettings, ResearchSettings};
use domain::llm::LlmProvider;
use domain::{
    Answer, Attempt, AttemptId, AttemptRepository, ChatMessage, ChatRepository, Clock, Exercise,
    Grade, IdGenerator, LessonId, MessageId, MessageText, Outcome, TopicId, UserId, Verdict,
};
use infrastructure::sqlite::{self, Pool};
use infrastructure::{
    ServerKey, SqliteAttemptRepository, SqliteChatRepository, SystemClock, UuidV7Generator,
};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::macros::format_description;
use uuid::Uuid;

pub struct App {
    pub state: AppState,
    pub pool: Pool,
    pub topic: TopicId,
    stubs: HashMap<u32, LessonId>,
}

/// Settings the evals need that differ from a server's defaults.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    pub max_searches: u32,
    pub history_token_budget: usize,
    /// `[llm.roles.generator] max_fetches`: 0 leaves the Generator's web fetch off.
    pub max_fetches: u32,
    /// The provider can run the web fetch tool (`server_web_fetch`).
    pub web_fetch_capable: bool,
}

impl Tuning {
    /// Whether the suites that need the Generator's web fetch tool can run.
    #[must_use]
    pub fn web_fetch_available(&self) -> bool {
        self.max_fetches > 0 && self.web_fetch_capable
    }

    /// Why they cannot, or `None` when they can.
    #[must_use]
    pub fn web_fetch_unavailable_reason(&self) -> Option<String> {
        if self.max_fetches == 0 {
            Some(
                "the web fetch tool is off: set `[llm.roles.generator] max_fetches` to 1 or more (ADR 0009)"
                    .to_owned(),
            )
        } else if !self.web_fetch_capable {
            Some(
                "the provider is not declared able to run the web fetch tool (`server_web_fetch`; see config/teach_me.example.toml)"
                    .to_owned(),
            )
        } else {
            None
        }
    }
}

pub fn stamp(at: OffsetDateTime) -> String {
    at.to_offset(time::UtcOffset::UTC)
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .unwrap_or_default()
}

fn now() -> String {
    stamp(SystemClock.now())
}

/// A server's state over an empty in-memory database, with a topic titled
/// `title` in state `state`.
pub async fn new_app(
    llm: Arc<dyn LlmProvider>,
    tuning: Tuning,
    title: &str,
    state: &str,
    language: domain::ContentLanguage,
) -> anyhow::Result<App> {
    let pool = sqlite::connect("sqlite::memory:")
        .await
        .context("database")?;
    sqlite::migrate(&pool).await.context("migrations")?;
    let mut options = AppOptions {
        chat: ChatSettings {
            history_token_budget: tuning.history_token_budget,
            ..ChatSettings::default()
        },
        research: ResearchSettings {
            max_searches: tuning.max_searches,
            ..ResearchSettings::default()
        },
        jobs: JobSettings {
            job_timeout: Duration::from_mins(30),
            ..JobSettings::default()
        },
        lessons: LessonSettings {
            max_web_fetches: tuning.max_fetches,
            ..LessonSettings::default()
        },
        ..AppOptions::default()
    };
    options.sse_ping = Duration::from_secs(3600);
    // One user, `owner`, with a placeholder key: the use cases need a key to
    // exist, but the eval provider uses the environment's own (a fixed
    // credential of the adapter wins over the one on the request).
    let auth = AuthSetup::production(&ServerKey::from_bytes([7; 32]), "anthropic")
        .map_err(|e| anyhow!("{e}"))?;
    let app_state = AppState::new(
        Arc::new(SystemClock),
        Arc::new(UuidV7Generator),
        auth,
        &pool,
        llm,
        options,
    );
    app_state
        .account
        .set_llm_key(UserId::OWNER, "eval-placeholder-key")
        .await
        .map_err(|e| anyhow!("{e:?}"))?;
    let folder = app_state
        .folders
        .create(UserId::OWNER, Some("Evals"))
        .await
        .map_err(|e| anyhow!("{e:?}"))?;
    let topic = app_state
        .topics
        .create(UserId::OWNER, folder.folder.id, Some(title))
        .await
        .map_err(|e| anyhow!("{e:?}"))?;
    // The language is chosen at creation and drives the prompts, the fixed
    // texts and the checks of the run (ADR 0019).
    sqlx::query("UPDATE topics SET state = ?, language = ? WHERE id = ?")
        .bind(state)
        .bind(language.code())
        .bind(topic.id.to_string())
        .execute(&pool)
        .await?;
    Ok(App {
        state: app_state,
        pool,
        topic: topic.id,
        stubs: HashMap::new(),
    })
}

fn strings(v: Option<&Value>) -> String {
    let list: Vec<&str> = v
        .and_then(Value::as_array)
        .map_or_default(|a| a.iter().filter_map(Value::as_str).collect());
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
}

impl App {
    pub async fn set_state(&self, state: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE topics SET state = ? WHERE id = ?")
            .bind(state)
            .bind(self.topic.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// `mission` has the contract's shape: `version, why, success[], starting_point,
    /// constraints[], out_of_scope[]` (`success_criteria` is accepted as well).
    /// `starting_point` is a string, `null` or absent (stored as NULL: a version
    /// saved before contract v1.2).
    pub async fn mission(&self, status: &str, mission: &Value) -> anyhow::Result<u32> {
        let version = mission.get("version").and_then(Value::as_u64).unwrap_or(1);
        let at = now();
        let confirmed = (status == "confirmed").then(|| at.clone());
        sqlx::query(
            "INSERT INTO missions (topic_id, version, status, why, starting_point, \
             success_criteria, mission_constraints, out_of_scope, created_at, confirmed_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(self.topic.to_string())
        .bind(i64::try_from(version)?)
        .bind(status)
        .bind(
            mission
                .get("why")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .bind(mission.get("starting_point").and_then(Value::as_str))
        .bind(strings(
            mission
                .get("success")
                .or_else(|| mission.get("success_criteria")),
        ))
        .bind(strings(mission.get("constraints")))
        .bind(strings(mission.get("out_of_scope")))
        .bind(&at)
        .bind(confirmed)
        .execute(&self.pool)
        .await?;
        Ok(u32::try_from(version)?)
    }

    pub async fn preferences(&self, prefs: &[Value]) -> anyhow::Result<()> {
        for (i, p) in prefs.iter().enumerate() {
            let scope = p.get("scope").and_then(Value::as_str).unwrap_or("global");
            let (scope, topic) = if scope == "topic" {
                ("topic", Some(self.topic.to_string()))
            } else {
                ("global", None)
            };
            let text = p.get("text").and_then(Value::as_str).unwrap_or_default();
            sqlx::query(
                "INSERT INTO preferences (id, user_id, scope, topic_id, text, text_key, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(UuidV7Generator.new_id().to_string())
            .bind(UserId::OWNER.to_string())
            .bind(scope)
            .bind(topic)
            .bind(text)
            .bind(format!("{}-{i}", text.to_lowercase()))
            .bind(now())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Earlier chat turns: `{role, text}` pairs, a user message then the reply.
    pub async fn history(&self, history: &[Value]) -> anyhow::Result<()> {
        let repo = SqliteChatRepository::new(self.pool.clone());
        let ids = UuidV7Generator;
        let base = SystemClock.now() - time::Duration::hours(1);
        let mut pending: Option<ChatMessage> = None;
        for (i, m) in history.iter().enumerate() {
            let at = base + time::Duration::seconds(i64::try_from(i)? * 2);
            let text = m.get("text").and_then(Value::as_str).unwrap_or_default();
            if m.get("role").and_then(Value::as_str) == Some("user") {
                {
                    let text = MessageText::parse(text).map_err(|e| anyhow!("{e:?}"))?;
                    pending = Some(ChatMessage::user(
                        MessageId::new(ids.new_id()),
                        self.topic,
                        text,
                        at,
                    ));
                }
            } else {
                {
                    let user = pending
                        .take()
                        .context("the history must alternate user and assistant messages")?;
                    let mut reply = ChatMessage::assistant_streaming(
                        MessageId::new(ids.new_id()),
                        self.topic,
                        at,
                    );
                    reply.append_text(text, at);
                    reply.complete(at).map_err(|e| anyhow!("{e:?}"))?;
                    repo.insert_turn(&user, &reply)
                        .await
                        .map_err(|e| anyhow!("{e:?}"))?;
                }
            }
        }
        Ok(())
    }

    /// Knowledge resources with the ids the case gives them (Generator), or
    /// fresh ones (Researcher's existing sources).
    pub async fn resources(&self, items: &[Value]) -> anyhow::Result<()> {
        for (i, r) in items.iter().enumerate() {
            let id = r
                .get("resource_id")
                .and_then(Value::as_str)
                .map_or_else(|| UuidV7Generator.new_id().to_string(), str::to_owned);
            let s = |k: &str, d: &str| r.get(k).and_then(Value::as_str).unwrap_or(d).to_owned();
            // SQLite takes any text; the repository fails later, deep inside
            // the job, with an error that does not name the fixture. Refuse a
            // value the domain does not know here, with the reason.
            let (kind, source_type) = (s("kind", "knowledge"), s("source_type", "documentation"));
            let label = || format!("resource #{i} (`{}`) of the case", s("title", "Fonte"));
            kind.parse::<domain::ResourceKind>()
                .map_err(|_| anyhow!("{}: `kind` {kind:?} is not a resource kind", label()))?;
            source_type.parse::<domain::SourceType>().map_err(|_| {
                anyhow!(
                    "{}: `source_type` {source_type:?} is not a source type",
                    label()
                )
            })?;
            sqlx::query(
                "INSERT INTO resources (id, topic_id, position, kind, source_type, title, url, \
                 language, annotation, use_for, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(self.topic.to_string())
            .bind(i64::try_from(i)?)
            .bind(kind)
            .bind(source_type)
            .bind(s("title", "Fonte"))
            .bind(r.get("url").and_then(Value::as_str))
            .bind(s("language", "en"))
            .bind(s("annotation", "Fonte di prova."))
            .bind(s("use_for", "prova"))
            .bind(now())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Uploaded PDFs, ready: `{document_id, resource_id, title, language?,
    /// page_count?, sections: [{id, title, level, page_start, page_end}],
    /// pages: {"<n>": "text"}}`. Only the pages a case gives have text; the
    /// resource is the one the real import writes (`resource_draft`).
    pub async fn documents(&self, items: &[Value]) -> anyhow::Result<()> {
        for d in items {
            let s = |k: &str| {
                d.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let id = s("document_id");
            let resource_id = s("resource_id");
            let pages = d.get("pages").and_then(Value::as_object);
            let last_page = pages
                .into_iter()
                .flat_map(|p| p.keys().filter_map(|k| k.parse::<i64>().ok()))
                .max()
                .unwrap_or(1);
            let page_count = d
                .get("page_count")
                .and_then(Value::as_i64)
                .unwrap_or(last_page);
            let language = d.get("language").and_then(Value::as_str).unwrap_or("en");
            let title = s("title");
            let at = now();
            sqlx::query(
                "INSERT INTO documents (id, topic_id, resource_id, filename, title, status, \
                 size_bytes, page_count, language, created_at) \
                 VALUES (?, ?, ?, ?, ?, 'ready', 1000, ?, ?, ?)",
            )
            .bind(&id)
            .bind(self.topic.to_string())
            .bind(&resource_id)
            .bind(format!("{title}.pdf"))
            .bind(&title)
            .bind(page_count)
            .bind(language)
            .bind(&at)
            .execute(&self.pool)
            .await?;
            // The texts the real import writes (`domain::document::resource_draft`).
            let annotation = format!("PDF caricato da te, {page_count} pagine.");
            sqlx::query(
                "INSERT INTO resources (id, topic_id, position, kind, source_type, title, url, \
                 language, annotation, use_for, document_id, created_at) \
                 VALUES (?, ?, 0, 'document', 'pdf', ?, NULL, ?, ?, ?, ?, ?)",
            )
            .bind(&resource_id)
            .bind(self.topic.to_string())
            .bind(&title)
            .bind(language)
            .bind(&annotation)
            .bind("seguire il tuo PDF, citandone le pagine")
            .bind(&id)
            .bind(&at)
            .execute(&self.pool)
            .await?;
            for (position, section) in d
                .get("sections")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                let n = |k: &str| section.get(k).and_then(Value::as_i64).unwrap_or(1);
                sqlx::query(
                    "INSERT INTO document_sections (document_id, position, section_id, title, \
                     level, page_start, page_end) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(i64::try_from(position)?)
                .bind(
                    section
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
                .bind(
                    section
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
                .bind(n("level"))
                .bind(n("page_start"))
                .bind(n("page_end"))
                .execute(&self.pool)
                .await?;
            }
            for (page, text) in pages.into_iter().flatten() {
                sqlx::query(
                    "INSERT INTO document_pages (document_id, page, text) VALUES (?, ?, ?)",
                )
                .bind(&id)
                .bind(page.parse::<i64>()?)
                .bind(text.as_str().unwrap_or_default())
                .execute(&self.pool)
                .await?;
            }
        }
        Ok(())
    }

    /// A lesson with `number`. `document` and `plan` are stored as given.
    pub async fn lesson(
        &self,
        id: Option<&str>,
        number: u32,
        title: &str,
        objective: &str,
        document: &Value,
        plan: &Value,
    ) -> anyhow::Result<LessonId> {
        let id = id.map_or_else(
            || UuidV7Generator.new_id(),
            |s| Uuid::parse_str(s).unwrap_or_else(|_| UuidV7Generator.new_id()),
        );
        sqlx::query(
            "INSERT INTO lessons (id, topic_id, number, title, objective, estimated_minutes, \
             catalog_version, mission_version, document, plan, generation_job_id, created_at) \
             VALUES (?, ?, ?, ?, ?, 5, 1, 1, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(self.topic.to_string())
        .bind(i64::from(number))
        .bind(title)
        .bind(objective)
        .bind(document.to_string())
        .bind(plan.to_string())
        .bind(UuidV7Generator.new_id().to_string())
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(LessonId::new(id))
    }

    /// A placeholder lesson with `number`, made once; what records and
    /// glossary terms of earlier lessons hang from.
    pub async fn stub_lesson(&mut self, number: u32) -> anyhow::Result<LessonId> {
        if let Some(id) = self.stubs.get(&number) {
            return Ok(*id);
        }
        let doc = json!({"catalog_version": 1, "title": format!("Lezione {number}"),
            "objective": "Lezione di contesto.", "estimated_minutes": 5, "blocks": []});
        let id = self
            .lesson(
                None,
                number,
                &format!("Lezione {number}"),
                "Lezione di contesto.",
                &doc,
                &json!({}),
            )
            .await?;
        self.stubs.insert(number, id);
        Ok(id)
    }

    /// Terms of the Topic glossary: `{key, term, definition, avoid[]}`.
    pub async fn glossary(&mut self, terms: &[Value], lesson: LessonId) -> anyhow::Result<()> {
        for t in terms {
            let s = |k: &str| {
                t.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            sqlx::query(
                "INSERT INTO glossary_terms (id, topic_id, key, term, term_key, definition, avoid, \
                 lesson_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(UuidV7Generator.new_id().to_string())
            .bind(self.topic.to_string())
            .bind(s("key"))
            .bind(s("term"))
            .bind(domain::term_sort_key(&s("term")))
            .bind(s("definition"))
            .bind(strings(t.get("avoid")))
            .bind(lesson.to_string())
            .bind(now())
            .bind(now())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Active records: `{number, kind, lesson_number?, title, body}`. A record
    /// hangs from the lesson with its `lesson_number` (a placeholder when the
    /// case has no such lesson).
    pub async fn records(&mut self, records: &[Value], default_lesson: u32) -> anyhow::Result<()> {
        for r in records {
            let number = r.get("number").and_then(Value::as_u64).unwrap_or(1);
            let lesson_number = r
                .get("lesson_number")
                .and_then(Value::as_u64)
                .map_or(default_lesson, |n| {
                    u32::try_from(n).unwrap_or(default_lesson)
                });
            let lesson = self.lesson_numbered(lesson_number).await?;
            let s = |k: &str| {
                r.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            sqlx::query(
                "INSERT INTO learning_records (id, topic_id, number, kind, title, body, evidence, \
                 status, lesson_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'active', ?, ?)",
            )
            .bind(UuidV7Generator.new_id().to_string())
            .bind(self.topic.to_string())
            .bind(i64::try_from(number)?)
            .bind(
                r.get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("demonstrated"),
            )
            .bind(s("title"))
            .bind(s("body"))
            .bind(
                r.get("evidence")
                    .and_then(Value::as_str)
                    .unwrap_or("Dalla lezione."),
            )
            .bind(lesson.to_string())
            .bind(now())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn lesson_numbered(&mut self, number: u32) -> anyhow::Result<LessonId> {
        let found: Option<String> =
            sqlx::query_scalar("SELECT id FROM lessons WHERE topic_id = ? AND number = ?")
                .bind(self.topic.to_string())
                .bind(i64::from(number))
                .fetch_optional(&self.pool)
                .await?;
        match found {
            Some(id) => Ok(LessonId::new(Uuid::parse_str(&id)?)),
            None => self.stub_lesson(number).await,
        }
    }

    /// Attempts as the case lists them: `{block_id, attempt_number, answer, outcome, grade?}`.
    pub async fn attempts(
        &self,
        lesson: LessonId,
        exercises: &domain::LessonExercises,
        attempts: &[Value],
    ) -> anyhow::Result<()> {
        let repo = SqliteAttemptRepository::new(self.pool.clone());
        let at = SystemClock.now();
        for (i, a) in attempts.iter().enumerate() {
            let block = a
                .get("block_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let exercise: &Exercise = exercises
                .get(block)
                .with_context(|| format!("attempt on an unknown exercise `{block}`"))?;
            let answer: Answer = exercise
                .parse_answer(a.get("answer").unwrap_or(&Value::Null))
                .map_err(|k| anyhow!("answer of `{block}` is not valid: {k:?}"))?;
            let outcome = match a.get("outcome").and_then(Value::as_str) {
                Some("correct") => Outcome::Correct,
                Some("partial") => Outcome::Partial,
                Some("incorrect") => Outcome::Incorrect,
                _ => Outcome::Pending,
            };
            let grade = a.get("grade").filter(|g| !g.is_null()).map(|g| Grade {
                verdict: match g.get("verdict").and_then(Value::as_str) {
                    Some("correct") => Verdict::Correct,
                    Some("partial") => Verdict::Partial,
                    _ => Verdict::Incorrect,
                },
                feedback: g
                    .get("feedback")
                    .and_then(Value::as_str)
                    .unwrap_or("…")
                    .to_owned(),
                misconception: g
                    .get("misconception")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });
            let created = at + time::Duration::milliseconds(i64::try_from(i)?);
            let attempt = Attempt {
                id: AttemptId::new(UuidV7Generator.new_id()),
                lesson_id: lesson,
                block_id: block.to_owned(),
                kind: exercise.kind,
                attempt_number: u8::try_from(
                    a.get("attempt_number").and_then(Value::as_u64).unwrap_or(1),
                )?,
                answer,
                outcome,
                grade,
                answered_at: created,
                created_at: created,
            };
            repo.record(lesson, &[attempt])
                .await
                .map_err(|e| anyhow!("{e:?}"))?;
        }
        Ok(())
    }
}
