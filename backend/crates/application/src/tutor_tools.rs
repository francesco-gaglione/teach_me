//! The Tutor's tools (phase 3): definitions, input validation and execution.
//!
//! Definitions and result texts are not written here: they come from
//! `backend/prompts/tutor/tools.json` and `tool_results.json` (owned by
//! `llm-engineer`). The same JSON Schema that the model sees is what validates
//! its input, so the prompt and the code cannot drift apart.

use std::sync::Arc;

use domain::llm::ToolDef;
use domain::{
    Clock, IdGenerator, JobId, Mission, MissionContent, MissionRepository, Preference,
    PreferenceId, PreferenceRepository, PreferenceScope, PreferenceText, RepositoryError,
    ScopeKind, Topic, TopicId, TopicRepository,
};
use serde_json::Value;
use time::OffsetDateTime;

use crate::error::{AppError, ConflictReason};
use crate::lessons::LessonService;
use crate::prompts::{TutorPrompts, render_template};
use crate::time_util::now_millis;

pub const SAVE_MISSION_DRAFT: &str = "save_mission_draft";
pub const PROPOSE_MISSION_CHANGE: &str = "propose_mission_change";
pub const SAVE_PREFERENCE: &str = "save_preference";
pub const REQUEST_LESSON: &str = "request_lesson";

/// Preferences kept per scope (global, or one topic). Past it the tool refuses,
/// so a looping model cannot grow the table without bound.
pub const MAX_PREFERENCES_PER_SCOPE: u64 = 50;

/// The name of a tool for logs: only the known ones, never model-made text.
#[must_use]
pub fn loggable_tool_name(name: &str) -> &'static str {
    match name {
        SAVE_MISSION_DRAFT => SAVE_MISSION_DRAFT,
        PROPOSE_MISSION_CHANGE => PROPOSE_MISSION_CHANGE,
        SAVE_PREFERENCE => SAVE_PREFERENCE,
        REQUEST_LESSON => REQUEST_LESSON,
        _ => "unknown",
    }
}

/// The prompt files are embedded in the binary and checked by tests, so a
/// malformed one is a build-time mistake, not a run-time condition.
#[derive(Debug, thiserror::Error)]
pub enum ToolKitError {
    #[error("tools.json: {0}")]
    Tools(String),
    #[error("tool_results.json: {0}")]
    Results(String),
}

/// Definitions and result texts, parsed once.
pub struct ToolKit {
    defs: Vec<(String, String, Value)>,
    results: Value,
}

impl ToolKit {
    pub fn parse(prompts: &TutorPrompts) -> Result<Self, ToolKitError> {
        let tools: Value =
            serde_json::from_str(prompts.tools).map_err(|e| ToolKitError::Tools(e.to_string()))?;
        let defs = tools["tools"]
            .as_array()
            .ok_or_else(|| ToolKitError::Tools("`tools` is not an array".into()))?
            .iter()
            .map(|t| {
                match (
                    t["name"].as_str(),
                    t["description"].as_str(),
                    t["input_schema"].is_object(),
                ) {
                    (Some(n), Some(d), true) => {
                        Ok((n.to_owned(), d.to_owned(), t["input_schema"].clone()))
                    }
                    _ => Err(ToolKitError::Tools(
                        "a tool lacks name, description or input_schema".into(),
                    )),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        for name in [
            SAVE_MISSION_DRAFT,
            PROPOSE_MISSION_CHANGE,
            SAVE_PREFERENCE,
            REQUEST_LESSON,
        ] {
            if !defs.iter().any(|(n, _, _)| n == name) {
                return Err(ToolKitError::Tools(format!("tool `{name}` is missing")));
            }
        }
        let results: Value = serde_json::from_str(prompts.tool_results)
            .map_err(|e| ToolKitError::Results(e.to_string()))?;
        if !results["results"].is_object() {
            return Err(ToolKitError::Results("`results` is not an object".into()));
        }
        Ok(Self { defs, results })
    }

    /// In the file's order, always: the tools are part of the cached prefix.
    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDef> {
        self.defs
            .iter()
            .map(|(name, description, schema)| ToolDef::Function {
                name: name.clone(),
                description: description.clone(),
                input_schema: schema.clone(),
            })
            .collect()
    }

    fn has_outcome(&self, tool: &str, key: &str) -> bool {
        self.results["results"][tool][key]["text"].is_string()
    }

    fn schema(&self, tool: &str) -> Option<&Value> {
        self.defs
            .iter()
            .find(|(n, _, _)| n == tool)
            .map(|(_, _, s)| s)
    }

    /// The text for `results[tool][key]` (or `common`), placeholders filled. A
    /// missing or inconsistent entry is a prompt/code mismatch: it is logged
    /// and the model gets the generic internal-error text.
    fn outcome(&self, tool: &str, key: &str, vars: &[(&str, &str)]) -> ToolRun {
        let entry = &self.results["results"][tool][key];
        let rendered = entry["text"].as_str().and_then(|text| {
            render_template(text, vars)
                .map_err(|e| tracing::error!(tool, key, error = %e, "tool result template"))
                .ok()
        });
        if let (Some(content), Some(is_error)) = (rendered, entry["is_error"].as_bool()) {
            return ToolRun::reply(content, is_error);
        }
        tracing::error!(tool, key, "tool result missing from tool_results.json");
        let fallback = self.results["results"]["common"]["internal_error"]["text"]
            .as_str()
            .unwrap_or("Errore interno: non è stato salvato nulla.");
        ToolRun::reply(fallback.to_owned(), true)
    }

    fn problem_line(&self, problem: &Problem) -> String {
        let (key, vars): (&str, Vec<(&str, String)>) = match problem {
            Problem::Missing { field } => ("missing_field", vec![("field", field.clone())]),
            Problem::WrongType { field, expected } => (
                "wrong_type",
                vec![
                    ("field", field.clone()),
                    ("expected", (*expected).to_owned()),
                ],
            ),
            Problem::Empty { field } => ("string_empty", vec![("field", field.clone())]),
            Problem::TooLong { field, actual, max } => (
                "string_too_long",
                vec![
                    ("field", field.clone()),
                    ("actual", actual.to_string()),
                    ("max", max.to_string()),
                ],
            ),
            Problem::ItemTooLong {
                field,
                index,
                actual,
                max,
            } => (
                "item_too_long",
                vec![
                    ("field", field.clone()),
                    ("index", index.to_string()),
                    ("actual", actual.to_string()),
                    ("max", max.to_string()),
                ],
            ),
            Problem::TooFew { field, actual, min } => (
                "too_few_items",
                vec![
                    ("field", field.clone()),
                    ("actual", actual.to_string()),
                    ("min", min.to_string()),
                ],
            ),
            Problem::TooMany { field, actual, max } => (
                "too_many_items",
                vec![
                    ("field", field.clone()),
                    ("actual", actual.to_string()),
                    ("max", max.to_string()),
                ],
            ),
            Problem::BadEnum { field, allowed } => (
                "bad_enum",
                vec![("field", field.clone()), ("allowed", allowed.clone())],
            ),
            Problem::Unknown { field } => ("unknown_field", vec![("field", field.clone())]),
        };
        let vars: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let template = self.results["results"]["problems"][key].as_str();
        template
            .and_then(|t| render_template(t, &vars).ok())
            .unwrap_or_else(|| format!("- `{}`: valore non valido.", problem.field()))
    }

    fn problems_text(&self, problems: &[Problem]) -> String {
        problems
            .iter()
            .map(|p| self.problem_line(p))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ---------- input validation, derived from the tool's JSON Schema ----------

/// One thing wrong with a tool input, attributed to a field.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    Missing {
        field: String,
    },
    WrongType {
        field: String,
        expected: &'static str,
    },
    Empty {
        field: String,
    },
    TooLong {
        field: String,
        actual: usize,
        max: usize,
    },
    ItemTooLong {
        field: String,
        index: usize,
        actual: usize,
        max: usize,
    },
    TooFew {
        field: String,
        actual: usize,
        min: usize,
    },
    TooMany {
        field: String,
        actual: usize,
        max: usize,
    },
    BadEnum {
        field: String,
        allowed: String,
    },
    Unknown {
        field: String,
    },
}

impl Problem {
    fn field(&self) -> &str {
        match self {
            Self::Missing { field }
            | Self::WrongType { field, .. }
            | Self::Empty { field }
            | Self::TooLong { field, .. }
            | Self::ItemTooLong { field, .. }
            | Self::TooFew { field, .. }
            | Self::TooMany { field, .. }
            | Self::BadEnum { field, .. }
            | Self::Unknown { field } => field,
        }
    }
}

fn usize_keyword(schema: &Value, key: &str) -> Option<usize> {
    schema[key].as_u64().and_then(|n| usize::try_from(n).ok())
}

/// Checks `value` against the subset of JSON Schema the tools use (`type`,
/// `required`, `properties`, `additionalProperties`, `enum`, `minLength`,
/// `maxLength`, `items`, `minItems`, `maxItems`). Strings are trimmed before
/// their length is judged; the trimmed value is what is returned.
fn check_object(schema: &Value, input: &Value, problems: &mut Vec<Problem>) -> Value {
    let Some(map) = input.as_object() else {
        return Value::Null;
    };
    let empty = serde_json::Map::new();
    let properties = schema["properties"].as_object().unwrap_or(&empty);
    let mut out = serde_json::Map::new();
    if schema["additionalProperties"] == Value::Bool(false) {
        for key in map.keys().filter(|k| !properties.contains_key(*k)) {
            problems.push(Problem::Unknown { field: key.clone() });
        }
    }
    for required in schema["required"].as_array().into_iter().flatten() {
        if let Some(name) = required.as_str().filter(|n| !map.contains_key(*n)) {
            problems.push(Problem::Missing {
                field: name.to_owned(),
            });
        }
    }
    for (name, prop) in properties {
        if let Some(value) = map.get(name)
            && let Some(clean) = check_value(name, prop, value, None, problems)
        {
            out.insert(name.clone(), clean);
        }
    }
    Value::Object(out)
}

fn check_value(
    field: &str,
    schema: &Value,
    value: &Value,
    item_index: Option<usize>,
    problems: &mut Vec<Problem>,
) -> Option<Value> {
    match schema["type"].as_str() {
        Some("string") => {
            let Some(text) = value.as_str() else {
                problems.push(Problem::WrongType {
                    field: field.to_owned(),
                    expected: "una stringa",
                });
                return None;
            };
            let text = text.trim();
            let len = text.chars().count();
            let min = usize_keyword(schema, "minLength").unwrap_or(0);
            if min > 0 && len == 0 {
                problems.push(Problem::Empty {
                    field: field.to_owned(),
                });
                return None;
            }
            if let Some(max) = usize_keyword(schema, "maxLength").filter(|m| len > *m) {
                problems.push(match item_index {
                    Some(i) => Problem::ItemTooLong {
                        field: field.to_owned(),
                        index: i + 1,
                        actual: len,
                        max,
                    },
                    None => Problem::TooLong {
                        field: field.to_owned(),
                        actual: len,
                        max,
                    },
                });
                return None;
            }
            if let Some(allowed) = schema["enum"].as_array()
                && !allowed.iter().any(|a| a.as_str() == Some(text))
            {
                problems.push(Problem::BadEnum {
                    field: field.to_owned(),
                    allowed: allowed
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", "),
                });
                return None;
            }
            Some(Value::String(text.to_owned()))
        }
        Some("array") => {
            let Some(items) = value.as_array() else {
                problems.push(Problem::WrongType {
                    field: field.to_owned(),
                    expected: "un array",
                });
                return None;
            };
            let mut ok = true;
            if let Some(min) = usize_keyword(schema, "minItems").filter(|m| items.len() < *m) {
                problems.push(Problem::TooFew {
                    field: field.to_owned(),
                    actual: items.len(),
                    min,
                });
                ok = false;
            }
            if let Some(max) = usize_keyword(schema, "maxItems").filter(|m| items.len() > *m) {
                problems.push(Problem::TooMany {
                    field: field.to_owned(),
                    actual: items.len(),
                    max,
                });
                ok = false;
            }
            let mut clean = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match check_value(field, &schema["items"], item, Some(i), problems) {
                    Some(v) => clean.push(v),
                    None => ok = false,
                }
            }
            ok.then_some(Value::Array(clean))
        }
        _ => Some(value.clone()),
    }
}

/// Reads a validated list of strings.
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i.as_str().map(str::to_owned))
        .collect()
}

// ---------- execution ----------

/// A tool call the model made.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// A draft to show as a card in the current message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub mission: Mission,
    pub replaces_version: Option<u32>,
}

/// What a tool did: the result text for the model and the effects the chat
/// layer must reflect (card, events).
#[derive(Debug, Default)]
pub struct ToolRun {
    pub content: String,
    pub is_error: bool,
    pub proposal: Option<Proposal>,
    pub preference: Option<Preference>,
    /// The generation job the message must point to (`lesson_request` part).
    pub lesson_job: Option<JobId>,
    pub topic_changed: bool,
}

impl ToolRun {
    fn reply(content: String, is_error: bool) -> Self {
        Self {
            content,
            is_error,
            ..Self::default()
        }
    }
}

/// Why a tool could not even produce a result for the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAbort {
    /// The topic was deleted meanwhile.
    Gone,
    /// Storage failed; the turn ends as `internal`.
    Storage,
}

/// State shared by the tools of one turn.
#[derive(Debug, Clone, Copy)]
pub struct TurnState {
    /// When the assistant message started. A draft created at or after this
    /// instant was saved by an earlier attempt of the same message.
    pub started_at: OffsetDateTime,
    /// A mission tool already produced a card in this turn.
    pub mission_tool_done: bool,
    /// `request_lesson` already produced the card of this turn.
    pub lesson_requested: bool,
}

pub struct TutorTools {
    kit: ToolKit,
    topics: Arc<dyn TopicRepository>,
    missions: Arc<dyn MissionRepository>,
    preferences: Arc<dyn PreferenceRepository>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    /// The use case behind `POST /topics/{id}/lessons`, so the tool cannot
    /// drift from the endpoint. Unset only in setups without lessons.
    lessons: Option<Arc<LessonService>>,
}

fn storage_abort(error: &RepositoryError) -> ToolAbort {
    match error {
        RepositoryError::NotFound | RepositoryError::MissingParent => ToolAbort::Gone,
        other => {
            tracing::error!(error = %other, "storage failure in a tool");
            ToolAbort::Storage
        }
    }
}

fn app_abort(error: &AppError) -> ToolAbort {
    match error {
        AppError::NotFound => ToolAbort::Gone,
        other => {
            tracing::error!(error = %other, "lesson request failed in a tool");
            ToolAbort::Storage
        }
    }
}

impl TutorTools {
    #[must_use]
    pub fn new(
        kit: ToolKit,
        topics: Arc<dyn TopicRepository>,
        missions: Arc<dyn MissionRepository>,
        preferences: Arc<dyn PreferenceRepository>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            kit,
            topics,
            missions,
            preferences,
            clock,
            ids,
            lessons: None,
        }
    }

    pub fn set_lessons(&mut self, lessons: Arc<LessonService>) {
        self.lessons = Some(lessons);
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDef> {
        self.kit.definitions()
    }

    pub async fn run(
        &self,
        topic_id: TopicId,
        call: &ToolCall,
        turn: &mut TurnState,
    ) -> Result<ToolRun, ToolAbort> {
        match call.name.as_str() {
            SAVE_MISSION_DRAFT => self.mission_tool(topic_id, call, turn, false).await,
            PROPOSE_MISSION_CHANGE => self.mission_tool(topic_id, call, turn, true).await,
            SAVE_PREFERENCE => self.preference_tool(topic_id, call).await,
            REQUEST_LESSON => self.lesson_tool(topic_id, call, turn).await,
            other => Ok(self
                .kit
                .outcome("common", "unknown_tool", &[("tool", other)])),
        }
    }

    /// Validates the input against the tool's schema. `Err` holds the ready
    /// result for the model.
    fn validated(&self, call: &ToolCall) -> Result<Value, Box<ToolRun>> {
        let name = call.name.as_str();
        let Some(schema) = self.kit.schema(name) else {
            return Err(Box::new(self.kit.outcome(
                "common",
                "unknown_tool",
                &[("tool", name)],
            )));
        };
        if !call.input.is_object() {
            let problems = self.kit.problems_text(&[Problem::WrongType {
                field: "input".into(),
                expected: "un oggetto",
            }]);
            return Err(Box::new(self.kit.outcome(
                "common",
                "invalid_input_shape",
                &[("tool", name), ("problems", &problems)],
            )));
        }
        let mut problems = Vec::new();
        let clean = check_object(schema, &call.input, &mut problems);
        if problems.is_empty() {
            Ok(clean)
        } else {
            let text = self.kit.problems_text(&problems);
            Err(Box::new(self.kit.outcome(
                name,
                "validation_error",
                &[("problems", &text)],
            )))
        }
    }

    async fn mission_tool(
        &self,
        topic_id: TopicId,
        call: &ToolCall,
        turn: &mut TurnState,
        change: bool,
    ) -> Result<ToolRun, ToolAbort> {
        let name = call.name.as_str();
        // One card per message: checked before anything else, nothing is stored.
        if turn.lesson_requested {
            return Ok(self
                .kit
                .outcome(name, "not_allowed_after_lesson_request", &[]));
        }
        let topic = self
            .topics
            .get_system(topic_id)
            .await
            .map_err(|e| storage_abort(&e))?
            .ok_or(ToolAbort::Gone)?;
        let confirmed = self
            .missions
            .current(topic_id)
            .await
            .map_err(|e| storage_abort(&e))?;

        // Admission depends on the data (is there a confirmed mission?), not
        // on the topic's state.
        match (&confirmed, change) {
            (Some(c), false) => {
                return Ok(self.kit.outcome(
                    name,
                    "not_allowed_confirmed",
                    &[("version", &c.version.to_string())],
                ));
            }
            (None, true) => return Ok(self.kit.outcome(name, "not_allowed_no_confirmed", &[])),
            _ => {}
        }
        if turn.mission_tool_done {
            return Ok(self.kit.outcome(name, "already_in_turn", &[]));
        }

        let clean = match self.validated(call) {
            Ok(v) => v,
            Err(run) => return Ok(*run),
        };
        let content = match MissionContent::parse(
            clean["why"].as_str().unwrap_or_default(),
            clean["starting_point"].as_str().unwrap_or_default(),
            &strings(&clean["success_criteria"]),
            &strings(&clean["constraints"]),
            &strings(&clean["out_of_scope"]),
        ) {
            Ok(c) => c,
            Err(issues) => {
                // The schema and the domain limits should agree; if they do
                // not, say so to the model in the generic format.
                tracing::warn!("tool schema accepted a mission the domain rejects");
                let text = issues
                    .iter()
                    .map(|i| format!("- `{}`: {}.", i.field, i.issue.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Ok(self
                    .kit
                    .outcome(name, "validation_error", &[("problems", &text)]));
            }
        };

        if let Some(c) = confirmed.as_ref().filter(|c| c.content == content) {
            return Ok(self
                .kit
                .outcome(name, "unchanged", &[("version", &c.version.to_string())]));
        }

        let replaces_version = confirmed.as_ref().map(|c| c.version);
        let open = self
            .missions
            .open_draft(topic_id)
            .await
            .map_err(|e| storage_abort(&e))?;
        if let Some(draft) = open.filter(|d| d.content == content) {
            // The same draft again. Normally nothing to do, but if it was saved
            // by an earlier attempt of this very message (a "Riprova" after a
            // failure), this attempt's message has no card yet and needs one.
            let own = draft.created_at >= turn.started_at;
            let version = draft.version.to_string();
            let mut run = if change {
                self.change_ok(&draft, replaces_version)
            } else {
                self.kit
                    .outcome(name, "unchanged", &[("version", &version)])
            };
            if change || own {
                turn.mission_tool_done = true;
                run.proposal = Some(Proposal {
                    mission: draft,
                    replaces_version,
                });
            }
            return Ok(run);
        }

        self.store_draft(topic, name, change, &content, replaces_version, turn)
            .await
    }

    /// Same effect as `POST /topics/{id}/lessons` (`lessons.md` §12): the
    /// admission rules live in [`LessonService::request_generation`].
    async fn lesson_tool(
        &self,
        topic_id: TopicId,
        call: &ToolCall,
        turn: &mut TurnState,
    ) -> Result<ToolRun, ToolAbort> {
        let name = call.name.as_str();
        if turn.mission_tool_done {
            return Ok(self.kit.outcome(name, "not_allowed_with_mission_tool", &[]));
        }
        // A second call in the turn: the first one already put the card in,
        // so no new job (even if the first has finished meanwhile) and an
        // answer that says the first call worked and the turn is over.
        if turn.lesson_requested {
            return Ok(self.kit.outcome(name, "already_in_turn", &[]));
        }
        if let Err(run) = self.validated(call) {
            return Ok(*run);
        }
        let Some(lessons) = &self.lessons else {
            tracing::error!("request_lesson called but lessons are not wired");
            return Ok(self.kit.outcome("common", "internal_error", &[]));
        };
        let topic = self
            .topics
            .get_system(topic_id)
            .await
            .map_err(|e| storage_abort(&e))?
            .ok_or(ToolAbort::Gone)?;
        let (key, job_id) = match lessons.request_generation(topic.user_id, topic_id).await {
            Ok(job) => ("ok", job.id),
            Err(AppError::Conflict(ConflictReason::JobActive)) => {
                match lessons.active_generation(topic.user_id, topic_id).await {
                    Ok(Some(job)) => ("already_running", job.id),
                    // The job ended between the two reads: the slot is free
                    // now, so try once more. A second failure is not ours to
                    // resolve.
                    Ok(None) => match lessons.request_generation(topic.user_id, topic_id).await {
                        Ok(job) => ("ok", job.id),
                        Err(e) => {
                            tracing::warn!(error = %e, "lesson request failed after a race retry");
                            return Ok(self.kit.outcome("common", "internal_error", &[]));
                        }
                    },
                    Err(e) => return Err(app_abort(&e)),
                }
            }
            Err(AppError::Conflict(ConflictReason::TopicNotReady)) => {
                return Ok(self.kit.outcome(name, "not_ready", &[]));
            }
            Err(AppError::LlmKeyMissing) => {
                return Ok(self.kit.outcome(name, "llm_key_missing", &[]));
            }
            Err(e) => return Err(app_abort(&e)),
        };
        turn.lesson_requested = true;
        let mut run = self.kit.outcome(name, key, &[]);
        run.lesson_job = Some(job_id);
        Ok(run)
    }

    /// Stores a new version. Only the first mission moves the topic along; a
    /// change proposed while a mission is confirmed leaves the state alone.
    async fn store_draft(
        &self,
        topic: Topic,
        name: &str,
        change: bool,
        content: &MissionContent,
        replaces_version: Option<u32>,
        turn: &mut TurnState,
    ) -> Result<ToolRun, ToolAbort> {
        let topic_id = topic.id;
        let now = now_millis(self.clock.as_ref());
        let mut moved = None;
        if !change {
            let mut projected: Topic = topic;
            match projected.propose_mission(now) {
                Ok(m) => moved = m,
                Err(e) => {
                    tracing::warn!(error = %e, "mission draft refused by the topic state");
                    return Ok(self.kit.outcome("common", "internal_error", &[]));
                }
            }
        }
        // Draft and state change are one transaction: a refusal (the topic
        // moved on meanwhile) stores nothing and the model is told so.
        let mission = match self
            .missions
            .insert_draft(topic_id, content, now, moved)
            .await
        {
            Ok(m) => m,
            Err(RepositoryError::Conflict) => {
                tracing::warn!("topic state changed while its draft was saved");
                return Ok(self.kit.outcome("common", "internal_error", &[]));
            }
            Err(e) => return Err(storage_abort(&e)),
        };
        let topic_changed = moved.is_some();
        turn.mission_tool_done = true;
        let mut run = if change {
            self.change_ok(&mission, replaces_version)
        } else {
            self.kit
                .outcome(name, "ok", &[("version", &mission.version.to_string())])
        };
        run.topic_changed = topic_changed;
        run.proposal = Some(Proposal {
            mission,
            replaces_version,
        });
        Ok(run)
    }

    /// `tool_results.json` may define `save_preference.limit_reached`; until it
    /// does, this text is used.
    fn limit_reached(&self, label: &str) -> ToolRun {
        if self.kit.has_outcome(SAVE_PREFERENCE, "limit_reached") {
            return self
                .kit
                .outcome(SAVE_PREFERENCE, "limit_reached", &[("scope_label", label)]);
        }
        ToolRun::reply(
            format!(
                "Preferenza NON salvata: ci sono già {MAX_PREFERENCES_PER_SCOPE} preferenze per \
                 {label}. Non è stato salvato nulla. Dillo alla persona in una frase: può \
                 eliminarne qualcuna dall'elenco delle preferenze."
            ),
            true,
        )
    }

    fn change_ok(&self, draft: &Mission, replaces_version: Option<u32>) -> ToolRun {
        self.kit.outcome(
            PROPOSE_MISSION_CHANGE,
            "ok",
            &[
                ("version", &draft.version.to_string()),
                (
                    "current_version",
                    &replaces_version.unwrap_or_default().to_string(),
                ),
            ],
        )
    }

    async fn preference_tool(
        &self,
        topic_id: TopicId,
        call: &ToolCall,
    ) -> Result<ToolRun, ToolAbort> {
        let clean = match self.validated(call) {
            Ok(v) => v,
            Err(run) => return Ok(*run),
        };
        let kind = clean["scope"]
            .as_str()
            .and_then(|s| s.parse::<ScopeKind>().ok());
        let text = clean["text"].as_str().map(PreferenceText::parse);
        let (Some(kind), Some(Ok(text))) = (kind, text) else {
            tracing::warn!("tool schema accepted a preference the domain rejects");
            return Ok(self.kit.outcome("common", "internal_error", &[]));
        };
        // The preference belongs to the owner of the topic being talked about.
        let owner = self
            .topics
            .get_system(topic_id)
            .await
            .map_err(|e| storage_abort(&e))?
            .ok_or(ToolAbort::Gone)?
            .user_id;
        let preference = Preference::new(
            PreferenceId::new(self.ids.new_id()),
            owner,
            kind,
            topic_id,
            text,
            now_millis(self.clock.as_ref()),
        );
        let label = match preference.scope {
            PreferenceScope::Global => "tutti gli argomenti",
            PreferenceScope::Topic(_) => "questo argomento",
        };
        let held = self
            .preferences
            .count(owner, preference.scope)
            .await
            .map_err(|e| storage_abort(&e))?;
        if held >= MAX_PREFERENCES_PER_SCOPE {
            return Ok(self.limit_reached(label));
        }
        match self.preferences.insert(&preference).await {
            Ok(()) => {
                let mut run = self
                    .kit
                    .outcome(SAVE_PREFERENCE, "ok", &[("scope_label", label)]);
                run.preference = Some(preference);
                Ok(run)
            }
            Err(RepositoryError::Duplicate) => {
                Ok(self
                    .kit
                    .outcome(SAVE_PREFERENCE, "duplicate", &[("scope_label", label)]))
            }
            Err(e) => Err(storage_abort(&e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn kit() -> ToolKit {
        ToolKit::parse(&TutorPrompts::embedded()).expect("embedded prompts parse")
    }

    fn problems(tool: &str, input: &Value) -> Vec<Problem> {
        let kit = kit();
        let mut out = Vec::new();
        check_object(kit.schema(tool).unwrap(), input, &mut out);
        out
    }

    fn lines(tool: &str, input: &Value) -> String {
        let kit = kit();
        kit.problems_text(&problems(tool, input))
    }

    #[test]
    fn embedded_prompts_define_the_four_tools_in_a_fixed_order() {
        let names: Vec<_> = kit()
            .definitions()
            .into_iter()
            .filter_map(|d| match d {
                ToolDef::Function { name, .. } => Some(name),
                _ => None,
            })
            .collect();
        assert_eq!(
            names,
            [
                SAVE_MISSION_DRAFT,
                PROPOSE_MISSION_CHANGE,
                REQUEST_LESSON,
                SAVE_PREFERENCE
            ]
        );
    }

    #[test]
    fn malformed_prompt_files_are_reported() {
        let mut p = TutorPrompts::embedded();
        p.tools = "{";
        assert!(matches!(ToolKit::parse(&p), Err(ToolKitError::Tools(_))));
        p.tools = r#"{"tools":[]}"#;
        assert!(matches!(ToolKit::parse(&p), Err(ToolKitError::Tools(_))));
        let mut p = TutorPrompts::embedded();
        p.tool_results = r#"{"results":1}"#;
        assert!(matches!(ToolKit::parse(&p), Err(ToolKitError::Results(_))));
    }

    #[test]
    fn a_valid_input_passes_and_is_trimmed() {
        let input = json!({
            "why": "  Per lavoro ",
            "starting_point": " Parto da zero ",
            "success_criteria": [" a "],
            "constraints": [],
            "out_of_scope": []
        });
        let kit = kit();
        let mut out = Vec::new();
        let clean = check_object(kit.schema(SAVE_MISSION_DRAFT).unwrap(), &input, &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(clean["why"], "Per lavoro");
        assert_eq!(clean["starting_point"], "Parto da zero");
        assert_eq!(clean["success_criteria"], json!(["a"]));
    }

    #[test]
    fn every_kind_of_problem_has_a_rendered_line() {
        let long = "x".repeat(201);
        let input = json!({
            "why": "x".repeat(501),
            "success_criteria": ["ok", long, "  ", 3, "a", "b"],
            "constraints": "no",
            "extra": 1
        });
        let text = lines(SAVE_MISSION_DRAFT, &input);
        for needle in [
            "`why` è lungo 501 caratteri, il massimo è 500",
            "`starting_point` manca",
            "`success_criteria`, voce 2 (partendo da 1) è lunga 201 caratteri, il massimo è 200",
            "`success_criteria` è vuoto",
            "`success_criteria` deve essere una stringa",
            "`success_criteria` ha 6 voci, il massimo è 5",
            "`constraints` deve essere un array",
            "`out_of_scope` manca",
            "`extra` non esiste",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
        assert!(!text.contains("valore non valido"), "{text}");

        let few = lines(
            SAVE_MISSION_DRAFT,
            &json!({"why": "w", "starting_point": "s", "success_criteria": [], "constraints": [], "out_of_scope": []}),
        );
        assert!(
            few.contains("`success_criteria` ha 0 voci, ne servono almeno 1"),
            "{few}"
        );
        let bad_enum = lines(SAVE_PREFERENCE, &json!({"scope": "x", "text": "t"}));
        assert!(bad_enum.contains("uno tra: global, topic"), "{bad_enum}");
    }

    #[test]
    fn lengths_are_counted_in_code_points_after_trimming() {
        let ok = json!({"scope": "topic", "text": format!("  {}  ", "è".repeat(300))});
        assert_eq!(problems(SAVE_PREFERENCE, &ok).len(), 0);
        let too_long = json!({"scope": "topic", "text": "è".repeat(301)});
        assert_eq!(problems(SAVE_PREFERENCE, &too_long).len(), 1);
    }

    #[test]
    fn the_schema_limits_match_the_domain_limits() {
        // The schema is what validates; the domain is the backstop. They must
        // not disagree, or valid-looking input would be refused later.
        let kit = kit();
        let s = kit.schema(SAVE_MISSION_DRAFT).unwrap();
        let p = &s["properties"];
        assert_eq!(p["why"]["maxLength"], domain::mission::WHY_MAX_CHARS);
        assert_eq!(
            p["starting_point"]["maxLength"],
            domain::mission::STARTING_POINT_MAX_CHARS
        );
        assert_eq!(p["starting_point"]["minLength"], 1);
        for tool in [SAVE_MISSION_DRAFT, PROPOSE_MISSION_CHANGE] {
            let required = kit.schema(tool).unwrap()["required"].as_array().unwrap();
            assert!(required.iter().any(|r| r == "starting_point"), "{tool}");
        }
        assert_eq!(
            p["success_criteria"]["items"]["maxLength"],
            domain::mission::ITEM_MAX_CHARS
        );
        assert_eq!(
            p["success_criteria"]["maxItems"],
            domain::mission::SUCCESS_CRITERIA_MAX_ITEMS
        );
        assert_eq!(p["success_criteria"]["minItems"], 1);
        assert_eq!(
            p["constraints"]["maxItems"],
            domain::mission::CONSTRAINTS_MAX_ITEMS
        );
        assert_eq!(
            p["out_of_scope"]["maxItems"],
            domain::mission::OUT_OF_SCOPE_MAX_ITEMS
        );
        let pref = kit.schema(SAVE_PREFERENCE).unwrap();
        assert_eq!(
            pref["properties"]["text"]["maxLength"],
            domain::preference::PREFERENCE_TEXT_MAX_CHARS
        );
    }

    #[test]
    fn a_missing_result_entry_falls_back_to_the_internal_error_text() {
        let run = kit().outcome("save_preference", "no_such_key", &[]);
        assert!(run.is_error);
        assert!(run.content.contains("Errore interno"), "{}", run.content);
    }
}
