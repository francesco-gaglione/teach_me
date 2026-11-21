//! In-memory implementations of the ports, for use case tests.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use domain::llm::{
    LlmCapabilities, LlmError, LlmEvent, LlmEventStream, LlmProvider, LlmRequest, LlmRole,
    StopReason, StructuredOutput, Usage,
};
use domain::{
    Attempt, AttemptId, AttemptRepository, CardOrigin, CardStatus, CardTarget, ChatCursor,
    ChatErrorCode, ChatMessage, ChatRepository, Clock, CompletionSave, CompletionSummary,
    ComponentRequest, ContentLanguage, Document, DocumentId, DocumentRepository, DocumentSection,
    DocumentStatus, DocumentView, Folder, FolderCursor, FolderId, FolderRepository, FolderSummary,
    GeneratedLesson, GlossaryTerm, Grade, Highlight, HighlightRepository, IdGenerator,
    ImportedDocument, Job, JobCursor, JobError, JobId, JobKind, JobRepository, JobStatus,
    LearningRecord, LearningRepository, Lesson, LessonCursor, LessonId, LessonOutputs,
    LessonRepository, LessonStatus, MessageError, MessageId, MessageStatus, Mission,
    MissionContent, MissionCursor, MissionRepository, MissionStatus, NewFolder, Outcome,
    Preference, PreferenceCursor, PreferenceId, PreferenceRepository, PreferenceScope,
    RecordCursor, RecordStatus, RepositoryError, ResearchOutcome, Resource, ResourceRepository,
    ReviewCard, StateChange, StoredLesson, Stroke, StrokeRepository, TermCursor, Topic,
    TopicCursor, TopicEvent, TopicEventPayload, TopicEventPublisher, TopicId, TopicRepository,
    TopicResources, TopicState, UserId,
};
use std::collections::VecDeque;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

pub struct TestClock(pub Mutex<OffsetDateTime>);

impl TestClock {
    pub fn new(at: OffsetDateTime) -> Self {
        Self(Mutex::new(at))
    }

    pub fn set(&self, at: OffsetDateTime) {
        *self.0.lock().unwrap() = at;
    }
}

impl Clock for TestClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap()
    }
}

#[derive(Default)]
pub struct SeqIds(AtomicU64);

impl IdGenerator for SeqIds {
    fn new_id(&self) -> Uuid {
        Uuid::from_u128(u128::from(self.0.fetch_add(1, Ordering::SeqCst)) + 1)
    }
}

/// Folders and topics share one store so cascade and counts behave like SQL.
#[derive(Default)]
pub struct Store {
    pub folders: Vec<Folder>,
    pub topics: Vec<Topic>,
    pub messages: Vec<ChatMessage>,
    pub missions: Vec<Mission>,
    pub preferences: Vec<Preference>,
    pub jobs: Vec<Job>,
    /// Per topic, as the repository would return them.
    pub resources: Vec<(TopicId, TopicResources)>,
    /// Makes `ResourceRepository::replace` fail, like a storage outage would.
    pub fail_replace: bool,
    /// How many times storing a finished job still fails (storage outage).
    pub fail_finishing_jobs: u32,
    /// Makes `transition_state` fail, like a storage outage would.
    pub fail_transitions: bool,
    pub lessons: Vec<LessonRow>,
    pub attempts: Vec<Attempt>,
    pub highlights: Vec<(LessonId, Vec<Highlight>)>,
    pub strokes: Vec<(LessonId, Vec<Stroke>)>,
    pub records: Vec<LearningRecord>,
    pub terms: Vec<GlossaryTerm>,
    pub cards: Vec<ReviewCard>,
    pub completions: Vec<(LessonId, CompletionSummary)>,
    /// Makes `save_completion` and `insert_generated` fail (storage outage).
    pub fail_learning_writes: bool,
    pub documents: Vec<DocumentRow>,
}

/// A stored document with its pages.
pub struct DocumentRow {
    pub document: Document,
    pub sections: Vec<DocumentSection>,
    pub pages: Vec<String>,
}

/// A stored lesson with what the API never shows.
pub struct LessonRow {
    pub lesson: Lesson,
    pub document: serde_json::Value,
    pub plan: serde_json::Value,
    pub job_id: JobId,
    pub requests: Vec<ComponentRequest>,
}

#[derive(Default)]
pub struct MemRepo(pub Mutex<Store>);

impl Store {
    /// Whether the topic exists and is the user's.
    pub fn owns(&self, user: UserId, topic: TopicId) -> bool {
        self.topics
            .iter()
            .any(|t| t.id == topic && t.user_id == user)
    }
}

#[async_trait]
impl FolderRepository for MemRepo {
    async fn insert(&self, folder: NewFolder) -> Result<FolderSummary, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let key = folder.name.unique_key();
        if s.folders
            .iter()
            .any(|f| f.user_id == folder.user_id && f.name.unique_key() == key)
        {
            return Err(RepositoryError::Duplicate);
        }
        let position = s
            .folders
            .iter()
            .filter(|f| f.user_id == folder.user_id)
            .map(|f| f.position + 1)
            .max()
            .unwrap_or(0);
        let f = Folder {
            id: folder.id,
            user_id: folder.user_id,
            name: folder.name,
            position,
            created_at: folder.created_at,
            updated_at: folder.created_at,
        };
        s.folders.push(f.clone());
        Ok(FolderSummary {
            folder: f,
            topic_count: 0,
        })
    }

    async fn get(
        &self,
        user: UserId,
        id: FolderId,
    ) -> Result<Option<FolderSummary>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.folders
            .iter()
            .find(|f| f.id == id && f.user_id == user)
            .map(|f| FolderSummary {
                folder: f.clone(),
                topic_count: s.topics.iter().filter(|t| t.folder_id == id).count() as u64,
            }))
    }

    async fn list(
        &self,
        user: UserId,
        after: Option<FolderCursor>,
        limit: u32,
    ) -> Result<Vec<FolderSummary>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .folders
            .iter()
            .filter(|f| f.user_id == user)
            .cloned()
            .collect();
        v.sort_by_key(|f| (f.position, f.id));
        Ok(v.into_iter()
            .filter(|f| after.is_none_or(|c| (f.position, f.id) > (c.position, c.id)))
            .take(limit as usize)
            .map(|f| FolderSummary {
                topic_count: s.topics.iter().filter(|t| t.folder_id == f.id).count() as u64,
                folder: f,
            })
            .collect())
    }

    async fn update(&self, folder: &Folder) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let key = folder.name.unique_key();
        if s.folders
            .iter()
            .any(|f| f.id != folder.id && f.user_id == folder.user_id && f.name.unique_key() == key)
        {
            return Err(RepositoryError::Duplicate);
        }
        let slot = s
            .folders
            .iter_mut()
            .find(|f| f.id == folder.id && f.user_id == folder.user_id)
            .ok_or(RepositoryError::NotFound)?;
        *slot = folder.clone();
        Ok(())
    }

    async fn delete(&self, user: UserId, id: FolderId) -> Result<bool, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.folders.iter().any(|f| f.id == id && f.user_id == user) {
            return Ok(false);
        }
        let before = s.folders.len();
        s.folders.retain(|f| f.id != id);
        let gone: Vec<_> = s
            .topics
            .iter()
            .filter(|t| t.folder_id == id)
            .map(|t| t.id)
            .collect();
        s.topics.retain(|t| t.folder_id != id);
        s.messages.retain(|m| !gone.contains(&m.topic_id));
        s.missions.retain(|m| !gone.contains(&m.topic_id));
        s.jobs.retain(|j| !gone.contains(&j.topic_id));
        s.preferences
            .retain(|p| p.scope.topic_id().is_none_or(|t| !gone.contains(&t)));
        Ok(s.folders.len() != before)
    }
}

#[async_trait]
impl TopicRepository for MemRepo {
    async fn insert(&self, topic: &Topic) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s
            .folders
            .iter()
            .any(|f| f.id == topic.folder_id && f.user_id == topic.user_id)
        {
            return Err(RepositoryError::MissingParent);
        }
        let key = topic.title.unique_key();
        if s.topics
            .iter()
            .any(|t| t.folder_id == topic.folder_id && t.title.unique_key() == key)
        {
            return Err(RepositoryError::Duplicate);
        }
        s.topics.push(topic.clone());
        Ok(())
    }

    async fn get(&self, user: UserId, id: TopicId) -> Result<Option<Topic>, RepositoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .topics
            .iter()
            .find(|t| t.id == id && t.user_id == user)
            .cloned())
    }

    async fn get_system(&self, id: TopicId) -> Result<Option<Topic>, RepositoryError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .topics
            .iter()
            .find(|t| t.id == id)
            .cloned())
    }

    async fn list_by_folder(
        &self,
        user: UserId,
        folder_id: FolderId,
        after: Option<TopicCursor>,
        limit: u32,
    ) -> Result<Vec<Topic>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .topics
            .iter()
            .filter(|t| t.folder_id == folder_id && t.user_id == user)
            .cloned()
            .collect();
        v.sort_by_key(|t| (t.created_at, t.id));
        Ok(v.into_iter()
            .filter(|t| after.is_none_or(|c| (t.created_at, t.id) > (c.created_at, c.id)))
            .take(limit as usize)
            .collect())
    }

    async fn update(&self, topic: &Topic) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s
            .folders
            .iter()
            .any(|f| f.id == topic.folder_id && f.user_id == topic.user_id)
        {
            return Err(RepositoryError::MissingParent);
        }
        let key = topic.title.unique_key();
        if s.topics.iter().any(|t| {
            t.id != topic.id && t.folder_id == topic.folder_id && t.title.unique_key() == key
        }) {
            return Err(RepositoryError::Duplicate);
        }
        let slot = s
            .topics
            .iter_mut()
            .find(|t| t.id == topic.id && t.user_id == topic.user_id)
            .ok_or(RepositoryError::NotFound)?;
        *slot = topic.clone();
        Ok(())
    }

    async fn transition_state(
        &self,
        id: TopicId,
        from: TopicState,
        to: TopicState,
        at: OffsetDateTime,
    ) -> Result<bool, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if s.fail_transitions {
            return Err(RepositoryError::Storage("scripted outage".into()));
        }
        match s.topics.iter_mut().find(|t| t.id == id && t.state == from) {
            Some(t) => {
                t.state = to;
                t.updated_at = at;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn ids_in_folder(
        &self,
        user: UserId,
        folder_id: FolderId,
    ) -> Result<Vec<TopicId>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.topics
            .iter()
            .filter(|t| t.folder_id == folder_id && t.user_id == user)
            .map(|t| t.id)
            .collect())
    }

    async fn delete(&self, user: UserId, id: TopicId) -> Result<bool, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.owns(user, id) {
            return Ok(false);
        }
        let before = s.topics.len();
        s.topics.retain(|t| t.id != id);
        s.messages.retain(|m| m.topic_id != id);
        s.missions.retain(|m| m.topic_id != id);
        s.jobs.retain(|j| j.topic_id != id);
        s.preferences.retain(|p| p.scope.topic_id() != Some(id));
        Ok(s.topics.len() != before)
    }
}

#[async_trait]
impl ChatRepository for MemRepo {
    async fn insert_turn(
        &self,
        user: &ChatMessage,
        assistant: &ChatMessage,
    ) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.topics.iter().any(|t| t.id == user.topic_id) {
            return Err(RepositoryError::MissingParent);
        }
        if s.messages
            .iter()
            .any(|m| m.topic_id == user.topic_id && m.status == MessageStatus::Streaming)
        {
            return Err(RepositoryError::Conflict);
        }
        s.messages.push(user.clone());
        s.messages.push(assistant.clone());
        Ok(())
    }

    async fn get(
        &self,
        topic_id: TopicId,
        id: MessageId,
    ) -> Result<Option<ChatMessage>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.messages
            .iter()
            .find(|m| m.topic_id == topic_id && m.id == id)
            .cloned())
    }

    async fn latest(&self, topic_id: TopicId) -> Result<Option<ChatMessage>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.messages
            .iter()
            .filter(|m| m.topic_id == topic_id)
            .max_by_key(|m| (m.created_at, m.id))
            .cloned())
    }

    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<ChatCursor>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .messages
            .iter()
            .filter(|m| m.topic_id == topic_id)
            .cloned()
            .collect();
        v.sort_by_key(|m| std::cmp::Reverse((m.created_at, m.id)));
        Ok(v.into_iter()
            .filter(|m| after.is_none_or(|c| (m.created_at, m.id) < (c.created_at, c.id)))
            .take(limit as usize)
            .collect())
    }

    async fn save(&self, message: &ChatMessage) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let slot = s
            .messages
            .iter_mut()
            .find(|m| m.id == message.id)
            .ok_or(RepositoryError::NotFound)?;
        *slot = message.clone();
        Ok(())
    }

    async fn restart_failed(&self, message: &ChatMessage) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let slot = s
            .messages
            .iter_mut()
            .find(|m| m.id == message.id)
            .ok_or(RepositoryError::NotFound)?;
        if slot.status != MessageStatus::Failed {
            return Err(RepositoryError::Conflict);
        }
        *slot = message.clone();
        Ok(())
    }

    async fn fail_streaming(
        &self,
        error: &(dyn Fn(ContentLanguage) -> MessageError + Sync),
        at: OffsetDateTime,
    ) -> Result<u64, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let languages: Vec<_> = s.topics.iter().map(|t| (t.id, t.language)).collect();
        let mut n = 0;
        for m in s
            .messages
            .iter_mut()
            .filter(|m| m.status == MessageStatus::Streaming)
        {
            let language = languages
                .iter()
                .find(|(id, _)| *id == m.topic_id)
                .map_or_else(ContentLanguage::default, |(_, l)| *l);
            m.status = MessageStatus::Failed;
            m.error = Some(error(language));
            m.updated_at = at;
            n += 1;
        }
        Ok(n)
    }
}

/// The topic move that goes with a mission write: all or nothing, like the
/// SQL transaction. `fail_transitions` makes it fail as a storage outage would.
fn move_topic(
    s: &mut Store,
    topic_id: TopicId,
    change: Option<StateChange>,
    at: OffsetDateTime,
) -> Result<(), RepositoryError> {
    let Some(change) = change else { return Ok(()) };
    if s.fail_transitions {
        return Err(RepositoryError::Storage("scripted outage".into()));
    }
    match s
        .topics
        .iter_mut()
        .find(|t| t.id == topic_id && t.state == change.from)
    {
        Some(t) => {
            t.state = change.to;
            t.updated_at = at;
            Ok(())
        }
        None => Err(RepositoryError::Conflict),
    }
}

#[async_trait]
impl MissionRepository for MemRepo {
    async fn insert_draft(
        &self,
        topic_id: TopicId,
        content: &MissionContent,
        at: OffsetDateTime,
        topic_change: Option<StateChange>,
    ) -> Result<Mission, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.topics.iter().any(|t| t.id == topic_id) {
            return Err(RepositoryError::MissingParent);
        }
        move_topic(&mut s, topic_id, topic_change, at)?;
        for m in s
            .missions
            .iter_mut()
            .filter(|m| m.topic_id == topic_id && m.status == MissionStatus::Draft)
        {
            m.supersede(at).unwrap();
        }
        let version = s
            .missions
            .iter()
            .filter(|m| m.topic_id == topic_id)
            .map(|m| m.version)
            .max()
            .unwrap_or(0)
            + 1;
        let mission = Mission::draft(topic_id, version, content.clone(), at);
        s.missions.push(mission.clone());
        Ok(mission)
    }

    async fn current(&self, topic_id: TopicId) -> Result<Option<Mission>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.missions
            .iter()
            .find(|m| m.topic_id == topic_id && m.status == MissionStatus::Confirmed)
            .cloned())
    }

    async fn open_draft(&self, topic_id: TopicId) -> Result<Option<Mission>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.missions
            .iter()
            .find(|m| m.topic_id == topic_id && m.status == MissionStatus::Draft)
            .cloned())
    }

    async fn confirm(
        &self,
        mission: &Mission,
        superseded_at: OffsetDateTime,
        topic_change: Option<StateChange>,
        follow_up: Option<&Job>,
    ) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let open = s.missions.iter().any(|m| {
            m.topic_id == mission.topic_id
                && m.version == mission.version
                && m.status == MissionStatus::Draft
        });
        if !open {
            return Err(RepositoryError::Conflict);
        }
        if let Some(job) = follow_up
            && s.jobs.iter().any(|j| active_twin(j, job))
        {
            return Err(RepositoryError::Duplicate);
        }
        move_topic(&mut s, mission.topic_id, topic_change, superseded_at)?;
        if let Some(job) = follow_up {
            s.jobs.push(job.clone());
        }
        for m in s
            .missions
            .iter_mut()
            .filter(|m| m.topic_id == mission.topic_id)
        {
            if m.status == MissionStatus::Confirmed {
                m.supersede(superseded_at).unwrap();
            } else if m.version == mission.version {
                *m = mission.clone();
            }
        }
        Ok(())
    }

    async fn list_versions(
        &self,
        topic_id: TopicId,
        after: Option<MissionCursor>,
        limit: u32,
    ) -> Result<Vec<Mission>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .missions
            .iter()
            .filter(|m| m.topic_id == topic_id && after.is_none_or(|c| m.version < c.version))
            .cloned()
            .collect();
        v.sort_by_key(|m| std::cmp::Reverse(m.version));
        v.truncate(limit as usize);
        Ok(v)
    }
}

/// `other` occupies the (topic, kind) slot `job` wants.
fn active_twin(other: &Job, job: &Job) -> bool {
    other.id != job.id
        && other.topic_id == job.topic_id
        && other.kind == job.kind
        && other.lesson_id == job.lesson_id
        && other.status.is_active()
}

#[async_trait]
impl JobRepository for MemRepo {
    async fn insert(&self, job: &Job) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.topics.iter().any(|t| t.id == job.topic_id) {
            return Err(RepositoryError::MissingParent);
        }
        if job.status.is_active() && s.jobs.iter().any(|j| active_twin(j, job)) {
            return Err(RepositoryError::Conflict);
        }
        s.jobs.push(job.clone());
        Ok(())
    }

    async fn get(
        &self,
        user: UserId,
        topic_id: TopicId,
        id: JobId,
    ) -> Result<Option<Job>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.jobs
            .iter()
            .filter(|_| s.owns(user, topic_id))
            .find(|j| j.topic_id == topic_id && j.id == id)
            .cloned())
    }

    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<JobCursor>,
        limit: u32,
    ) -> Result<Vec<Job>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .jobs
            .iter()
            .filter(|j| {
                j.topic_id == topic_id
                    && after.is_none_or(|c| (j.created_at, j.id) < (c.created_at, c.id))
            })
            .cloned()
            .collect();
        v.sort_by_key(|j| std::cmp::Reverse((j.created_at, j.id)));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn latest(
        &self,
        topic_id: TopicId,
        kind: JobKind,
        lesson_id: Option<LessonId>,
    ) -> Result<Option<Job>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.jobs
            .iter()
            .filter(|j| j.topic_id == topic_id && j.kind == kind && j.lesson_id == lesson_id)
            .max_by_key(|j| (j.created_at, j.id))
            .cloned())
    }

    async fn next_queued(&self, kinds: &[JobKind]) -> Result<Option<Job>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.jobs
            .iter()
            .filter(|j| j.status == JobStatus::Queued && kinds.contains(&j.kind))
            .min_by_key(|j| (j.created_at, j.id))
            .cloned())
    }

    async fn running(&self) -> Result<Vec<Job>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.jobs
            .iter()
            .filter(|j| j.status == JobStatus::Running)
            .cloned()
            .collect())
    }

    async fn update(&self, job: &Job, expected: JobStatus) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !job.status.is_active() && expected == JobStatus::Running && s.fail_finishing_jobs > 0 {
            s.fail_finishing_jobs -= 1;
            return Err(RepositoryError::Storage("scripted".into()));
        }
        if job.status.is_active() && s.jobs.iter().any(|j| active_twin(j, job)) {
            return Err(RepositoryError::Conflict);
        }
        let slot = s
            .jobs
            .iter_mut()
            .find(|j| j.id == job.id)
            .ok_or(RepositoryError::NotFound)?;
        if slot.status != expected {
            return Err(RepositoryError::Conflict);
        }
        *slot = job.clone();
        Ok(())
    }
}

#[async_trait]
impl ResourceRepository for MemRepo {
    async fn get(&self, topic_id: TopicId) -> Result<TopicResources, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.resources
            .iter()
            .find(|(t, _)| *t == topic_id)
            .map_or_default(|(_, r)| r.clone()))
    }

    async fn replace(
        &self,
        outcome: &ResearchOutcome,
        at: OffsetDateTime,
    ) -> Result<bool, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if s.fail_replace {
            return Err(RepositoryError::Storage("scripted".into()));
        }
        let topic = s
            .topics
            .iter_mut()
            .find(|t| t.id == outcome.topic_id)
            .ok_or(RepositoryError::NotFound)?;
        let moved = topic.state == TopicState::Researching;
        if moved {
            topic.state = TopicState::Ready;
            topic.updated_at = at;
        }
        let set = TopicResources {
            resources: outcome
                .resources
                .iter()
                .map(|(id, d)| Resource {
                    id: *id,
                    topic_id: outcome.topic_id,
                    kind: d.kind,
                    source_type: d.source_type,
                    title: d.title.clone(),
                    url: d.url.clone(),
                    language: d.language.clone(),
                    annotation: d.annotation.clone(),
                    use_for: d.use_for.clone(),
                    document_id: None,
                    created_at: at,
                })
                .collect(),
            gaps: outcome.gaps.clone(),
            communities_skipped: outcome.communities_skipped,
            research: Some(outcome.info),
        };
        // Uploaded documents are not replaced by a research.
        let mut set = set;
        if let Some((_, old)) = s.resources.iter().find(|(t, _)| *t == outcome.topic_id) {
            let mut kept: Vec<Resource> = old
                .resources
                .iter()
                .filter(|r| r.kind == domain::ResourceKind::Document)
                .cloned()
                .collect();
            kept.append(&mut set.resources);
            set.resources = kept;
        }
        s.resources.retain(|(t, _)| *t != outcome.topic_id);
        s.resources.push((outcome.topic_id, set));
        Ok(moved)
    }
}

#[async_trait]
impl PreferenceRepository for MemRepo {
    async fn insert(&self, preference: &Preference) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if let Some(t) = preference.scope.topic_id()
            && !s.topics.iter().any(|x| x.id == t)
        {
            return Err(RepositoryError::MissingParent);
        }
        let key = preference.text.unique_key();
        if s.preferences.iter().any(|p| {
            p.user_id == preference.user_id
                && p.scope == preference.scope
                && p.text.unique_key() == key
        }) {
            return Err(RepositoryError::Duplicate);
        }
        s.preferences.push(preference.clone());
        Ok(())
    }

    async fn count(&self, user: UserId, scope: PreferenceScope) -> Result<u64, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.preferences
            .iter()
            .filter(|p| p.user_id == user && p.scope == scope)
            .count() as u64)
    }

    async fn get(
        &self,
        user: UserId,
        id: PreferenceId,
    ) -> Result<Option<Preference>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.preferences
            .iter()
            .find(|p| p.id == id && p.user_id == user)
            .cloned())
    }

    async fn list(
        &self,
        user: UserId,
        topic: Option<TopicId>,
        after: Option<PreferenceCursor>,
        limit: u32,
    ) -> Result<Vec<Preference>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .preferences
            .iter()
            .filter(|p| p.user_id == user)
            .filter(|p| topic.is_none_or(|t| p.scope.topic_id().is_none_or(|x| x == t)))
            .filter(|p| after.is_none_or(|c| (p.created_at, p.id) > (c.created_at, c.id)))
            .cloned()
            .collect();
        v.sort_by_key(|p| (p.created_at, p.id));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn latest_for_topic(
        &self,
        user: UserId,
        topic_id: TopicId,
        limit: u32,
    ) -> Result<Vec<Preference>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .preferences
            .iter()
            .filter(|p| p.user_id == user)
            .filter(|p| p.scope.topic_id().is_none_or(|x| x == topic_id))
            .cloned()
            .collect();
        v.sort_by_key(|p| std::cmp::Reverse((p.created_at, p.id)));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn delete(&self, user: UserId, id: PreferenceId) -> Result<bool, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let before = s.preferences.len();
        s.preferences.retain(|p| !(p.id == id && p.user_id == user));
        Ok(s.preferences.len() != before)
    }
}

/// Records everything published, in order.
#[derive(Default)]
pub struct RecordingEvents {
    pub events: Mutex<Vec<TopicEvent>>,
    /// Payloads sent to every open stream (`publish_to_listeners`).
    pub broadcasts: Mutex<Vec<TopicEventPayload>>,
    pub closed: Mutex<Vec<TopicId>>,
}

impl RecordingEvents {
    pub fn snapshot(&self) -> Vec<TopicEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl TopicEventPublisher for RecordingEvents {
    fn publish(&self, event: TopicEvent) -> Option<u64> {
        let mut events = self.events.lock().unwrap();
        events.push(event);
        Some(events.len() as u64)
    }

    fn latest_event_id(&self, _topic_id: TopicId) -> u64 {
        self.events.lock().unwrap().len() as u64
    }

    fn publish_to_listeners(&self, _user: UserId, _at: OffsetDateTime, payload: TopicEventPayload) {
        self.broadcasts.lock().unwrap().push(payload);
    }

    fn topic_closed(&self, topic_id: TopicId) {
        self.closed.lock().unwrap().push(topic_id);
    }
}

/// One scripted step of a reply.
#[derive(Clone)]
pub enum Step {
    Delta(&'static str),
    /// A provider failure on the open stream, by the code it maps to.
    Fail(ChatErrorCode),
    Event(LlmEvent),
    /// A complete tool call; a script with one ends in `Finished(ToolUse)`.
    Tool {
        id: &'static str,
        name: &'static str,
        input: serde_json::Value,
    },
    Err(LlmError),
    /// Waits until the test releases it (keeps a reply open on demand).
    Wait(Arc<tokio::sync::Notify>),
    /// Do not append `Finished`: the stream just ends.
    NoFinish,
    Panic,
}

/// Fake LLM: each call to `stream` consumes the next script, and records the
/// request it received. A script that does not end in a failure gets
/// `Finished(EndTurn)` appended.
#[derive(Default)]
pub struct FakeLlm {
    scripts: AsyncMutex<VecDeque<Vec<Step>>>,
    /// Errors returned by `stream` itself, one per call, before any script.
    pub start_errors: Mutex<VecDeque<LlmError>>,
    pub requests: Mutex<Vec<LlmRequest>>,
    /// Makes the fake report no `server_web_fetch` capability.
    pub without_web_fetch: std::sync::atomic::AtomicBool,
}

impl FakeLlm {
    pub fn with(scripts: Vec<Vec<Step>>) -> Arc<Self> {
        Arc::new(Self {
            scripts: AsyncMutex::new(scripts.into()),
            ..Self::default()
        })
    }
}

struct ScriptStream(VecDeque<Step>);

#[async_trait]
impl LlmEventStream for ScriptStream {
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>> {
        loop {
            match self.0.pop_front()? {
                Step::Delta(text) => {
                    return Some(Ok(LlmEvent::TextDelta {
                        block: 0,
                        text: text.to_owned(),
                    }));
                }
                Step::Fail(code) => {
                    self.0.clear();
                    return Some(Err(match code {
                        ChatErrorCode::UpstreamFailed => LlmError::Unavailable,
                        ChatErrorCode::RateLimited => LlmError::RateLimited { retry_after: None },
                        ChatErrorCode::Internal => LlmError::InvalidRequest("scripted".into()),
                        ChatErrorCode::LlmKeyMissing | ChatErrorCode::LlmKeyRejected => {
                            LlmError::Authentication
                        }
                        ChatErrorCode::LlmCreditExhausted => LlmError::CreditExhausted,
                    }));
                }
                Step::Event(e) => return Some(Ok(e)),
                Step::Tool { id, name, input } => {
                    return Some(Ok(LlmEvent::ToolCallFinished {
                        block: 1,
                        id: id.to_owned(),
                        name: name.to_owned(),
                        input,
                    }));
                }
                Step::Err(e) => {
                    self.0.clear();
                    return Some(Err(e));
                }
                Step::Wait(n) => n.notified().await,
                Step::NoFinish => {}
                Step::Panic => panic!("scripted panic"),
            }
        }
    }
}

#[async_trait]
impl LlmProvider for FakeLlm {
    fn capabilities(&self, _role: LlmRole) -> LlmCapabilities {
        LlmCapabilities {
            tools: true,
            parallel_tool_calls: false,
            forced_tool_choice: false,
            structured_output: StructuredOutput::Emulated,
            streaming_tool_input: false,
            prompt_caching: false,
            max_cache_breakpoints: 0,
            context_window_tokens: 100_000,
            max_output_tokens: 4096,
            effort: Some("low".to_owned()),
            server_web_search: true,
            server_web_fetch: !self
                .without_web_fetch
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError> {
        self.requests.lock().unwrap().push(req);
        if let Some(e) = self.start_errors.lock().unwrap().pop_front() {
            return Err(e);
        }
        let mut script = self.scripts.lock().await.pop_front().unwrap_or_default();
        // A script that finishes by itself (e.g. with `pause_turn`) is left alone.
        let fails = script.iter().any(|s| {
            matches!(
                s,
                Step::Fail(_)
                    | Step::Err(_)
                    | Step::Panic
                    | Step::NoFinish
                    | Step::Event(LlmEvent::Finished { .. })
            )
        });
        let stop_reason = if script.iter().any(|s| matches!(s, Step::Tool { .. })) {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        };
        if !fails {
            script.push(Step::Event(LlmEvent::Finished {
                stop_reason,
                usage: Usage::default(),
            }));
        }
        Ok(Box::new(ScriptStream(script.into())))
    }
}

#[async_trait]
impl LessonRepository for MemRepo {
    async fn insert_generated(&self, new: &GeneratedLesson) -> Result<Lesson, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if s.fail_learning_writes {
            return Err(RepositoryError::Storage("scripted".into()));
        }
        if !s.topics.iter().any(|t| t.id == new.topic_id) {
            return Err(RepositoryError::MissingParent);
        }
        if s.lessons.iter().any(|l| l.job_id == new.job_id) {
            return Err(RepositoryError::Conflict);
        }
        let number = s
            .lessons
            .iter()
            .filter(|l| l.lesson.topic_id == new.topic_id)
            .map(|l| l.lesson.number)
            .max()
            .unwrap_or(0)
            + 1;
        let lesson = Lesson {
            quality_warnings: new.quality_warnings.clone(),
            id: new.id,
            topic_id: new.topic_id,
            number,
            title: new.meta.title.clone(),
            objective: new.meta.objective.clone(),
            estimated_minutes: new.meta.estimated_minutes,
            status: LessonStatus::NotStarted,
            catalog_version: new.meta.catalog_version,
            mission_version: new.mission_version,
            created_at: new.created_at,
            completed_at: None,
        };
        s.lessons.push(LessonRow {
            lesson: lesson.clone(),
            document: new.document.clone(),
            plan: new.plan.clone(),
            job_id: new.job_id,
            requests: new.component_requests.clone(),
        });
        Ok(lesson)
    }

    async fn by_generation_job(&self, job_id: JobId) -> Result<Option<Lesson>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.lessons
            .iter()
            .find(|l| l.job_id == job_id)
            .map(|l| l.lesson.clone()))
    }

    async fn exists(&self, user: UserId, id: LessonId) -> Result<bool, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.lessons
            .iter()
            .any(|l| l.lesson.id == id && s.owns(user, l.lesson.topic_id)))
    }

    async fn get(
        &self,
        user: UserId,
        id: LessonId,
    ) -> Result<Option<StoredLesson>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.lessons
            .iter()
            .find(|l| l.lesson.id == id && s.owns(user, l.lesson.topic_id))
            .map(|l| StoredLesson {
                lesson: l.lesson.clone(),
                document: l.document.clone(),
                plan: l.plan.clone(),
            }))
    }

    async fn list(
        &self,
        topic_id: TopicId,
        after: Option<LessonCursor>,
        limit: u32,
    ) -> Result<Vec<Lesson>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .lessons
            .iter()
            .map(|l| &l.lesson)
            .filter(|l| l.topic_id == topic_id && after.is_none_or(|c| l.number < c.number))
            .cloned()
            .collect();
        v.sort_by_key(|l| std::cmp::Reverse(l.number));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn list_in_progress(
        &self,
        user: UserId,
        after: Option<domain::InProgressCursor>,
        limit: u32,
    ) -> Result<Vec<domain::InProgressLesson>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<domain::InProgressLesson> = s
            .lessons
            .iter()
            .filter(|l| {
                l.lesson.status == domain::LessonStatus::InProgress
                    && s.owns(user, l.lesson.topic_id)
            })
            .map(|l| domain::InProgressLesson {
                lesson: l.lesson.clone(),
                topic_title: s
                    .topics
                    .iter()
                    .find(|t| t.id == l.lesson.topic_id)
                    .map_or_else(String::new, |t| t.title.as_str().to_owned()),
                last_activity_at: s
                    .attempts
                    .iter()
                    .filter(|a| a.lesson_id == l.lesson.id)
                    .map(|a| a.created_at)
                    .max()
                    .unwrap_or(l.lesson.created_at),
                plan: l.plan.clone(),
            })
            .collect();
        v.sort_by(|a, b| {
            b.last_activity_at
                .cmp(&a.last_activity_at)
                .then(a.lesson.id.cmp(&b.lesson.id))
        });
        v.retain(|i| {
            after.is_none_or(|c| {
                i.last_activity_at < c.last_activity_at
                    || (i.last_activity_at == c.last_activity_at && i.lesson.id > c.id)
            })
        });
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn all_in_topic(&self, topic_id: TopicId) -> Result<Vec<Lesson>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .lessons
            .iter()
            .map(|l| l.lesson.clone())
            .filter(|l| l.topic_id == topic_id)
            .collect();
        v.sort_by_key(|l| l.number);
        Ok(v)
    }

    async fn plans(
        &self,
        topic_id: TopicId,
    ) -> Result<Vec<(LessonId, serde_json::Value)>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.lessons
            .iter()
            .filter(|l| l.lesson.topic_id == topic_id)
            .map(|l| (l.lesson.id, l.plan.clone()))
            .collect())
    }

    async fn complete(
        &self,
        id: LessonId,
        at: OffsetDateTime,
        job: &Job,
    ) -> Result<Lesson, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let index = s
            .lessons
            .iter()
            .position(|l| l.lesson.id == id)
            .ok_or(RepositoryError::NotFound)?;
        if s.lessons[index].lesson.is_completed() || s.jobs.iter().any(|j| active_twin(j, job)) {
            return Err(RepositoryError::Conflict);
        }
        s.jobs.push(job.clone());
        let lesson = &mut s.lessons[index].lesson;
        lesson.status = LessonStatus::Completed;
        lesson.completed_at = Some(at);
        Ok(lesson.clone())
    }

    async fn component_requests(
        &self,
        lesson_id: LessonId,
    ) -> Result<Vec<ComponentRequest>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.lessons
            .iter()
            .find(|l| l.lesson.id == lesson_id)
            .map_or_default(|l| l.requests.clone()))
    }

    async fn component_request_groups(
        &self,
        _user: UserId,
        after: Option<domain::ComponentRequestCursor>,
        limit: u32,
    ) -> Result<Vec<domain::ComponentRequestGroup>, RepositoryError> {
        use domain::{ComponentRequestExample, ComponentRequestGroup};
        let s = self.0.lock().unwrap();
        let mut groups: Vec<ComponentRequestGroup> = Vec::new();
        for row in &s.lessons {
            let topic_title = s
                .topics
                .iter()
                .find(|t| t.id == row.lesson.topic_id)
                .map_or_default(|t| t.title.as_str().to_owned());
            for r in &row.requests {
                let example = ComponentRequestExample {
                    purpose: r.purpose.clone(),
                    lesson_id: row.lesson.id,
                    lesson_title: row.lesson.title.clone(),
                    topic_title: topic_title.clone(),
                    requested_at: r.created_at,
                };
                if let Some(g) = groups.iter_mut().find(|g| g.wanted == r.wanted) {
                    g.count += 1;
                    g.first_requested_at = g.first_requested_at.min(r.created_at);
                    g.last_requested_at = g.last_requested_at.max(r.created_at);
                    g.catalog_version = g.catalog_version.max(row.lesson.catalog_version);
                    g.examples.push(example);
                } else {
                    groups.push(ComponentRequestGroup {
                        wanted: r.wanted.clone(),
                        count: 1,
                        first_requested_at: r.created_at,
                        last_requested_at: r.created_at,
                        catalog_version: row.lesson.catalog_version,
                        examples: vec![example],
                    });
                }
            }
        }
        for g in &mut groups {
            g.examples
                .sort_by_key(|e| std::cmp::Reverse(e.requested_at));
            g.examples.truncate(domain::COMPONENT_REQUEST_EXAMPLES);
        }
        groups.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then(b.last_requested_at.cmp(&a.last_requested_at))
                .then(a.wanted.cmp(&b.wanted))
        });
        Ok(groups
            .into_iter()
            .filter(|g| {
                after.as_ref().is_none_or(|c| {
                    (
                        std::cmp::Reverse(g.count),
                        std::cmp::Reverse(g.last_requested_at),
                        &g.wanted,
                    ) > (
                        std::cmp::Reverse(c.count),
                        std::cmp::Reverse(c.last_requested_at),
                        &c.wanted,
                    )
                })
            })
            .take(limit as usize)
            .collect())
    }
}

#[async_trait]
impl StrokeRepository for MemRepo {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Stroke>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.strokes
            .iter()
            .find(|(id, _)| *id == lesson_id)
            .map_or_default(|(_, v)| v.clone()))
    }

    async fn replace(
        &self,
        lesson_id: LessonId,
        strokes: &[Stroke],
    ) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.lessons.iter().any(|l| l.lesson.id == lesson_id) {
            return Err(RepositoryError::NotFound);
        }
        s.strokes.retain(|(id, _)| *id != lesson_id);
        s.strokes.push((lesson_id, strokes.to_vec()));
        Ok(())
    }
}

#[async_trait]
impl HighlightRepository for MemRepo {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Highlight>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.highlights
            .iter()
            .find(|(id, _)| *id == lesson_id)
            .map_or_default(|(_, h)| h.clone()))
    }

    async fn replace(
        &self,
        lesson_id: LessonId,
        highlights: &[Highlight],
    ) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.lessons.iter().any(|l| l.lesson.id == lesson_id) {
            return Err(RepositoryError::NotFound);
        }
        s.highlights.retain(|(id, _)| *id != lesson_id);
        s.highlights.push((lesson_id, highlights.to_vec()));
        Ok(())
    }
}

#[async_trait]
impl AttemptRepository for MemRepo {
    async fn list(&self, lesson_id: LessonId) -> Result<Vec<Attempt>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .attempts
            .iter()
            .filter(|a| a.lesson_id == lesson_id)
            .cloned()
            .collect();
        v.sort_by_key(|a| (a.created_at, a.id));
        Ok(v)
    }

    async fn record(
        &self,
        lesson_id: LessonId,
        attempts: &[Attempt],
    ) -> Result<Vec<Attempt>, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let row = s
            .lessons
            .iter_mut()
            .find(|l| l.lesson.id == lesson_id)
            .ok_or(RepositoryError::NotFound)?;
        if row.lesson.is_completed() {
            return Err(RepositoryError::Conflict);
        }
        if row.lesson.status == LessonStatus::NotStarted && !attempts.is_empty() {
            row.lesson.status = LessonStatus::InProgress;
        }
        let mut stored = Vec::new();
        let before = s.attempts.len();
        for a in attempts {
            let existing = s.attempts.iter().find(|x| {
                x.lesson_id == lesson_id
                    && x.block_id == a.block_id
                    && x.attempt_number == a.attempt_number
            });
            if let Some(x) = existing {
                stored.push(x.clone());
            } else {
                if a.attempt_number == 2 {
                    let first = s
                        .attempts
                        .iter()
                        .find(|x| {
                            x.lesson_id == lesson_id
                                && x.block_id == a.block_id
                                && x.attempt_number == 1
                        })
                        .map(|x| x.outcome);
                    if first.is_none_or(|o| o == Outcome::Correct) {
                        s.attempts.truncate(before);
                        return Err(RepositoryError::RuleViolated);
                    }
                }
                s.attempts.push(a.clone());
                stored.push(a.clone());
            }
        }
        Ok(stored)
    }

    async fn set_grade(&self, id: AttemptId, grade: &Grade) -> Result<Attempt, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let a = s
            .attempts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(RepositoryError::NotFound)?;
        if a.outcome == Outcome::Pending {
            a.outcome = grade.verdict.outcome();
            a.grade = Some(grade.clone());
        }
        Ok(a.clone())
    }
}

#[async_trait]
impl LearningRepository for MemRepo {
    async fn list_records(
        &self,
        topic_id: TopicId,
        active_only: bool,
        after: Option<RecordCursor>,
        limit: u32,
    ) -> Result<Vec<LearningRecord>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .records
            .iter()
            .filter(|r| {
                r.topic_id == topic_id
                    && (!active_only || r.status == RecordStatus::Active)
                    && after.is_none_or(|c| r.number < c.number)
            })
            .cloned()
            .collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.number));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn list_terms(
        &self,
        topic_id: TopicId,
        after: Option<TermCursor>,
        limit: u32,
    ) -> Result<Vec<GlossaryTerm>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut v: Vec<_> = s
            .terms
            .iter()
            .filter(|t| {
                t.topic_id == topic_id && after.as_ref().is_none_or(|c| TermCursor::after(t) > *c)
            })
            .cloned()
            .collect();
        v.sort_by_key(|t| (domain::term_sort_key(&t.term), t.key.clone()));
        v.truncate(limit as usize);
        Ok(v)
    }

    async fn lesson_outputs(&self, lesson_id: LessonId) -> Result<LessonOutputs, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut records: Vec<_> = s
            .records
            .iter()
            .filter(|r| r.lesson_id == lesson_id)
            .cloned()
            .collect();
        records.sort_by_key(|r| r.number);
        let mut terms: Vec<_> = s
            .terms
            .iter()
            .filter(|t| t.lesson_id == lesson_id)
            .cloned()
            .collect();
        terms.sort_by_key(|t| (domain::term_sort_key(&t.term), t.key.clone()));
        let cards = s.cards.iter().filter(|c| c.lesson_id == lesson_id).count();
        Ok(LessonOutputs {
            records,
            terms,
            review_card_count: u32::try_from(cards).unwrap_or(u32::MAX),
        })
    }

    async fn review_cards(&self, topic_id: TopicId) -> Result<Vec<ReviewCard>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.cards
            .iter()
            .filter(|c| c.topic_id == topic_id)
            .cloned()
            .collect())
    }

    #[allow(clippy::too_many_lines)] // mirrors the SQL transaction step by step
    async fn save_completion(
        &self,
        save: &CompletionSave,
    ) -> Result<CompletionSummary, RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if s.fail_learning_writes {
            return Err(RepositoryError::Storage("scripted".into()));
        }
        if s.completions.iter().any(|(l, _)| *l == save.lesson_id) {
            return Err(RepositoryError::Conflict);
        }
        // Work on copies and commit at the end: all or nothing.
        let mut records = s.records.clone();
        let mut terms = s.terms.clone();
        let mut cards = s.cards.clone();
        let mut next = records
            .iter()
            .filter(|r| r.topic_id == save.topic_id)
            .map(|r| r.number)
            .max()
            .unwrap_or(0);
        let mut record_ids = Vec::new();
        #[allow(clippy::explicit_counter_loop)]
        for new in &save.records {
            next += 1;
            record_ids.push(new.id);
            for number in &new.supersedes {
                let old = records
                    .iter_mut()
                    .find(|r| {
                        r.topic_id == save.topic_id
                            && r.number == *number
                            && r.status == RecordStatus::Active
                    })
                    .ok_or(RepositoryError::Conflict)?;
                old.status = RecordStatus::Superseded;
                old.superseded_by = Some(new.id);
                let old_id = old.id;
                for card in &mut cards {
                    if card.origin == CardOrigin::Record(old_id) {
                        card.status = CardStatus::Suspended;
                        card.suspension = Some(domain::Suspension {
                            reason: domain::SuspensionReason::Superseded,
                            at: save.at,
                        });
                    }
                }
            }
            records.push(LearningRecord {
                id: new.id,
                topic_id: save.topic_id,
                number: next,
                kind: new.kind,
                title: new.title.clone(),
                body: new.body.clone(),
                evidence: new.evidence.clone(),
                status: RecordStatus::Active,
                superseded_by: None,
                lesson_id: save.lesson_id,
                created_at: save.at,
            });
        }
        let mut term_ids = Vec::new();
        for up in &save.terms {
            if let Some(t) = terms
                .iter_mut()
                .find(|t| t.topic_id == save.topic_id && t.key == up.key)
            {
                t.term.clone_from(&up.term);
                t.definition.clone_from(&up.definition);
                t.avoid.clone_from(&up.avoid);
                t.lesson_id = save.lesson_id;
                t.updated_at = save.at;
                term_ids.push(t.id);
            } else {
                term_ids.push(up.id);
                terms.push(GlossaryTerm {
                    id: up.id,
                    topic_id: save.topic_id,
                    key: up.key.clone(),
                    term: up.term.clone(),
                    definition: up.definition.clone(),
                    avoid: up.avoid.clone(),
                    lesson_id: save.lesson_id,
                    first_lesson_id: Some(save.lesson_id),
                    created_at: save.at,
                    updated_at: save.at,
                });
            }
        }
        for card in &save.cards {
            let origin = match card.target {
                CardTarget::Record(i) => CardOrigin::Record(record_ids[i]),
                CardTarget::Term(i) => CardOrigin::Term(term_ids[i]),
            };
            cards.push(ReviewCard {
                id: card.id,
                topic_id: save.topic_id,
                lesson_id: save.lesson_id,
                origin,
                front: card.front.clone(),
                back: card.back.clone(),
                status: CardStatus::Active,
                suspension: None,
                schedule: domain::CardSchedule::initial(save.at),
                created_at: save.at,
                to_consolidate: card.unpassed_block.is_some(),
            });
        }
        let summary = CompletionSummary {
            record_count: u32::try_from(save.records.len()).unwrap_or(u32::MAX),
            term_count: u32::try_from(save.terms.len()).unwrap_or(u32::MAX),
            review_card_count: u32::try_from(save.cards.len()).unwrap_or(u32::MAX),
        };
        s.records = records;
        s.terms = terms;
        s.cards = cards;
        s.completions.push((save.lesson_id, summary));
        Ok(summary)
    }

    async fn completion_summary(
        &self,
        lesson_id: LessonId,
    ) -> Result<Option<CompletionSummary>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.completions
            .iter()
            .find(|(l, _)| *l == lesson_id)
            .map(|(_, c)| *c))
    }
}

impl MemRepo {
    /// The document as the import job says (see the SQLite adapter).
    fn derived(s: &Store, row: &DocumentRow) -> Document {
        let mut d = row.document.clone();
        if d.status == DocumentStatus::Ready {
            return d;
        }
        match s
            .jobs
            .iter()
            .find(|j| j.document_id == Some(d.id))
            .map(|j| (j.status, j.error.clone()))
        {
            Some((JobStatus::Failed, error)) => {
                d.status = DocumentStatus::Failed;
                d.error = error.or(d.error);
            }
            Some(_) => {
                d.status = DocumentStatus::Importing;
                d.error = None;
            }
            None => {}
        }
        d
    }
}

#[async_trait]
impl DocumentRepository for MemRepo {
    async fn insert_with_job(&self, document: &Document, job: &Job) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s.topics.iter().any(|t| t.id == document.topic_id) {
            return Err(RepositoryError::MissingParent);
        }
        let count = s
            .documents
            .iter()
            .filter(|d| d.document.topic_id == document.topic_id)
            .count();
        if count >= domain::document::MAX_DOCUMENTS_PER_TOPIC
            || s.jobs.iter().any(|j| active_twin(j, job))
        {
            return Err(RepositoryError::Conflict);
        }
        s.jobs.push(job.clone());
        s.documents.push(DocumentRow {
            document: document.clone(),
            sections: Vec::new(),
            pages: Vec::new(),
        });
        Ok(())
    }

    async fn get(&self, user: UserId, id: DocumentId) -> Result<Option<Document>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.documents
            .iter()
            .find(|d| d.document.id == id && s.owns(user, d.document.topic_id))
            .map(|d| Self::derived(&s, d)))
    }

    async fn list(&self, topic_id: TopicId) -> Result<Vec<DocumentView>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let mut rows: Vec<&DocumentRow> = s
            .documents
            .iter()
            .filter(|d| d.document.topic_id == topic_id)
            .collect();
        rows.sort_by_key(|d| std::cmp::Reverse((d.document.created_at, d.document.id)));
        Ok(rows
            .into_iter()
            .map(|d| {
                let document = Self::derived(&s, d);
                let sections = if document.status == DocumentStatus::Ready {
                    d.sections.clone()
                } else {
                    Vec::new()
                };
                DocumentView { document, sections }
            })
            .collect())
    }

    async fn mark_importing(&self, id: DocumentId) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if let Some(d) = s.documents.iter_mut().find(|d| d.document.id == id)
            && d.document.status != DocumentStatus::Ready
        {
            d.document.status = DocumentStatus::Importing;
            d.document.error = None;
        }
        Ok(())
    }

    async fn complete(
        &self,
        import: &ImportedDocument,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        let topic = {
            let d = s
                .documents
                .iter_mut()
                .find(|d| d.document.id == import.document_id)
                .ok_or(RepositoryError::NotFound)?;
            d.document.status = DocumentStatus::Ready;
            d.document.error = None;
            d.document.title.clone_from(&import.title);
            d.document.language.clone_from(&import.language);
            d.document.page_count = Some(u32::try_from(import.pages.len()).unwrap());
            d.document.resource_id = Some(import.resource_id);
            d.sections.clone_from(&import.sections);
            d.pages.clone_from(&import.pages);
            d.document.topic_id
        };
        let resource = Resource {
            id: import.resource_id,
            topic_id: topic,
            kind: import.resource.kind,
            source_type: import.resource.source_type,
            title: import.resource.title.clone(),
            url: None,
            language: import.resource.language.clone(),
            annotation: import.resource.annotation.clone(),
            use_for: import.resource.use_for.clone(),
            document_id: Some(import.document_id),
            created_at: at,
        };
        if let Some((_, set)) = s.resources.iter_mut().find(|(t, _)| *t == topic) {
            set.resources
                .retain(|r| r.document_id != Some(import.document_id));
            set.resources.insert(0, resource);
        } else {
            s.resources.push((
                topic,
                TopicResources {
                    resources: vec![resource],
                    ..TopicResources::default()
                },
            ));
        }
        Ok(())
    }

    async fn fail(&self, id: DocumentId, error: &JobError) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if let Some(d) = s.documents.iter_mut().find(|d| d.document.id == id)
            && d.document.status != DocumentStatus::Ready
        {
            d.document.status = DocumentStatus::Failed;
            d.document.error = Some(error.clone());
        }
        Ok(())
    }

    async fn ids_for_user(&self, user: UserId) -> Result<Vec<DocumentId>, RepositoryError> {
        let s = self.0.lock().unwrap();
        Ok(s.documents
            .iter()
            .filter(|d| s.owns(user, d.document.topic_id))
            .map(|d| d.document.id)
            .collect())
    }

    async fn delete(&self, user: UserId, id: DocumentId) -> Result<(), RepositoryError> {
        let mut s = self.0.lock().unwrap();
        if !s
            .documents
            .iter()
            .any(|d| d.document.id == id && s.owns(user, d.document.topic_id))
        {
            return Err(RepositoryError::NotFound);
        }
        if s.jobs
            .iter()
            .any(|j| j.document_id == Some(id) && j.status.is_active())
        {
            return Err(RepositoryError::Conflict);
        }
        s.documents.retain(|d| d.document.id != id);
        s.jobs.retain(|j| j.document_id != Some(id));
        for (_, set) in &mut s.resources {
            set.resources.retain(|r| r.document_id != Some(id));
        }
        Ok(())
    }

    async fn pages(
        &self,
        id: DocumentId,
        first: u32,
        last: u32,
    ) -> Result<Vec<(u32, String)>, RepositoryError> {
        let s = self.0.lock().unwrap();
        let row = s
            .documents
            .iter()
            .find(|d| d.document.id == id)
            .ok_or(RepositoryError::NotFound)?;
        Ok(row
            .pages
            .iter()
            .enumerate()
            .map(|(i, t)| (u32::try_from(i + 1).unwrap(), t.clone()))
            .filter(|(p, _)| (first..=last).contains(p))
            .collect())
    }
}

// ---------- users, sessions and credentials ----------

use domain::{
    CryptoError, NewUser, Password, PasswordHasher, SealedSecret, Secret, SecretCipher, Session,
    SessionId, SessionRepository, StoredPasswordHash, TokenDigest, TokenIssuer, User,
    UserCredentials, UserRepository, Username,
};

struct UserRow {
    user: User,
    password_hash: Option<StoredPasswordHash>,
    llm_key: Option<SealedSecret>,
}

/// Users and sessions in memory.
#[derive(Default)]
pub struct MemUsers {
    /// Makes `set_last_login` fail, like a storage outage would.
    pub fail_last_login: std::sync::atomic::AtomicBool,
    users: Mutex<Vec<UserRow>>,
    pub sessions: Mutex<Vec<Session>>,
}

impl MemUsers {
    pub fn with_owner() -> Arc<Self> {
        let repo = Arc::new(Self::default());
        repo.users.lock().unwrap().push(UserRow {
            user: User {
                id: UserId::OWNER,
                username: Username::from_stored("owner".into()),
                display_name: domain::DisplayName::parse("owner").unwrap(),
                has_llm_key: false,
                tts_voice: None,
                disabled_at: None,
                last_login_at: None,
                created_at: OffsetDateTime::UNIX_EPOCH,
            },
            password_hash: None,
            llm_key: None,
        });
        repo
    }

    /// A second user, for tests with more than one.
    pub fn add_plain(&self, id: UserId) {
        let mut users = self.users.lock().unwrap();
        if users.iter().any(|r| r.user.id == id) {
            return;
        }
        users.push(UserRow {
            user: User {
                id,
                username: Username::from_stored(format!("user-{}", id.as_uuid().as_u128())),
                display_name: domain::DisplayName::parse("user").unwrap(),
                has_llm_key: false,
                tts_voice: None,
                disabled_at: None,
                last_login_at: None,
                created_at: OffsetDateTime::UNIX_EPOCH,
            },
            password_hash: None,
            llm_key: None,
        });
    }

    /// Gives the user a key as `AccountService` would store it.
    pub fn give_key(&self, user: UserId, cipher: &FakeCipher, key: &str) {
        let sealed = cipher
            .seal(&crate::key_context(user), &Secret::new(key).unwrap())
            .unwrap();
        let mut users = self.users.lock().unwrap();
        let row = users.iter_mut().find(|r| r.user.id == user).unwrap();
        row.llm_key = Some(sealed);
        row.user.has_llm_key = true;
    }
}

#[async_trait]
impl UserRepository for MemUsers {
    async fn insert(&self, user: &NewUser) -> Result<(), RepositoryError> {
        let mut users = self.users.lock().unwrap();
        if users.iter().any(|r| r.user.username == user.username) {
            return Err(RepositoryError::Duplicate);
        }
        users.push(UserRow {
            user: User {
                id: user.id,
                username: user.username.clone(),
                display_name: user.display_name.clone(),
                has_llm_key: false,
                tts_voice: None,
                disabled_at: None,
                last_login_at: None,
                created_at: user.created_at,
            },
            password_hash: user.password_hash.clone(),
            llm_key: None,
        });
        Ok(())
    }

    async fn get(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        let users = self.users.lock().unwrap();
        Ok(users
            .iter()
            .find(|r| r.user.id == id)
            .map(|r| r.user.clone()))
    }

    async fn credentials(&self, name: &str) -> Result<Option<UserCredentials>, RepositoryError> {
        let users = self.users.lock().unwrap();
        Ok(users
            .iter()
            .find(|r| r.user.username.as_str() == name)
            .map(|r| UserCredentials {
                user: r.user.clone(),
                password_hash: r.password_hash.clone(),
            }))
    }

    async fn by_username(&self, name: &Username) -> Result<Option<User>, RepositoryError> {
        let users = self.users.lock().unwrap();
        Ok(users
            .iter()
            .find(|r| &r.user.username == name)
            .map(|r| r.user.clone()))
    }

    async fn list(&self) -> Result<Vec<User>, RepositoryError> {
        let users = self.users.lock().unwrap();
        let mut all: Vec<User> = users.iter().map(|r| r.user.clone()).collect();
        all.sort_by(|a, b| a.username.as_str().cmp(b.username.as_str()));
        Ok(all)
    }

    async fn password_hash(
        &self,
        id: UserId,
    ) -> Result<Option<StoredPasswordHash>, RepositoryError> {
        let users = self.users.lock().unwrap();
        Ok(users
            .iter()
            .find(|r| r.user.id == id)
            .and_then(|r| r.password_hash.clone()))
    }

    async fn set_password_hash(
        &self,
        id: UserId,
        hash: &StoredPasswordHash,
    ) -> Result<(), RepositoryError> {
        let mut users = self.users.lock().unwrap();
        let row = users
            .iter_mut()
            .find(|r| r.user.id == id)
            .ok_or(RepositoryError::NotFound)?;
        row.password_hash = Some(hash.clone());
        Ok(())
    }

    async fn rename(&self, id: UserId, name: &Username) -> Result<(), RepositoryError> {
        let mut users = self.users.lock().unwrap();
        if users
            .iter()
            .any(|r| &r.user.username == name && r.user.id != id)
        {
            return Err(RepositoryError::Duplicate);
        }
        let row = users
            .iter_mut()
            .find(|r| r.user.id == id)
            .ok_or(RepositoryError::NotFound)?;
        row.user.username = name.clone();
        Ok(())
    }

    async fn set_disabled(
        &self,
        id: UserId,
        at: Option<OffsetDateTime>,
    ) -> Result<(), RepositoryError> {
        let mut users = self.users.lock().unwrap();
        let row = users
            .iter_mut()
            .find(|r| r.user.id == id)
            .ok_or(RepositoryError::NotFound)?;
        row.user.disabled_at = at;
        Ok(())
    }

    async fn llm_key(&self, id: UserId) -> Result<Option<SealedSecret>, RepositoryError> {
        let users = self.users.lock().unwrap();
        Ok(users
            .iter()
            .find(|r| r.user.id == id)
            .and_then(|r| r.llm_key.clone()))
    }

    async fn set_llm_key(
        &self,
        id: UserId,
        key: Option<&SealedSecret>,
    ) -> Result<(), RepositoryError> {
        let mut users = self.users.lock().unwrap();
        let row = users
            .iter_mut()
            .find(|r| r.user.id == id)
            .ok_or(RepositoryError::NotFound)?;
        row.llm_key = key.cloned();
        row.user.has_llm_key = key.is_some();
        Ok(())
    }

    async fn set_llm_key_if_missing(
        &self,
        id: UserId,
        key: &SealedSecret,
    ) -> Result<bool, RepositoryError> {
        let mut users = self.users.lock().unwrap();
        let row = users
            .iter_mut()
            .find(|r| r.user.id == id)
            .ok_or(RepositoryError::NotFound)?;
        if row.llm_key.is_some() {
            return Ok(false);
        }
        row.llm_key = Some(key.clone());
        row.user.has_llm_key = true;
        Ok(true)
    }

    async fn set_tts_voice(&self, id: UserId, voice: &str) -> Result<(), RepositoryError> {
        let mut users = self.users.lock().unwrap();
        let row = users
            .iter_mut()
            .find(|r| r.user.id == id)
            .ok_or(RepositoryError::NotFound)?;
        row.user.tts_voice = Some(voice.to_owned());
        Ok(())
    }

    async fn set_last_login(&self, id: UserId, at: OffsetDateTime) -> Result<(), RepositoryError> {
        if self.fail_last_login.load(Ordering::SeqCst) {
            return Err(RepositoryError::Storage("scripted outage".into()));
        }
        let mut users = self.users.lock().unwrap();
        let row = users
            .iter_mut()
            .find(|r| r.user.id == id)
            .ok_or(RepositoryError::NotFound)?;
        row.user.last_login_at = Some(at);
        Ok(())
    }

    async fn delete(&self, id: UserId) -> Result<bool, RepositoryError> {
        let mut users = self.users.lock().unwrap();
        let before = users.len();
        users.retain(|r| r.user.id != id);
        self.sessions.lock().unwrap().retain(|s| s.user_id != id);
        Ok(users.len() != before)
    }
}

#[async_trait]
impl SessionRepository for MemUsers {
    async fn insert(&self, session: &Session) -> Result<(), RepositoryError> {
        self.sessions.lock().unwrap().push(session.clone());
        Ok(())
    }

    async fn by_digest(&self, digest: &TokenDigest) -> Result<Option<Session>, RepositoryError> {
        Ok(self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .find(|s| &s.token_digest == digest)
            .cloned())
    }

    async fn renew(
        &self,
        id: SessionId,
        last_seen_at: OffsetDateTime,
        expires_at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        if let Some(s) = self
            .sessions
            .lock()
            .unwrap()
            .iter_mut()
            .find(|s| s.id == id)
        {
            s.last_seen_at = last_seen_at;
            s.expires_at = expires_at;
        }
        Ok(())
    }

    async fn revoke(&self, id: SessionId) -> Result<bool, RepositoryError> {
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.len();
        sessions.retain(|s| s.id != id);
        Ok(sessions.len() != before)
    }

    async fn revoke_all(
        &self,
        user: UserId,
        keep: Option<SessionId>,
    ) -> Result<u64, RepositoryError> {
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.len();
        sessions.retain(|s| s.user_id != user || Some(s.id) == keep);
        Ok((before - sessions.len()) as u64)
    }

    async fn delete_expired(&self, now: OffsetDateTime) -> Result<u64, RepositoryError> {
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.len();
        sessions.retain(|s| s.expires_at > now);
        Ok((before - sessions.len()) as u64)
    }
}

/// "Hashes" by prefixing, and counts how often a password was verified, so a
/// test can tell that every login attempt costs one verification.
#[derive(Default)]
pub struct FakeHasher {
    pub verifications: AtomicU64,
    pub hashes: AtomicU64,
}

#[async_trait]
impl PasswordHasher for FakeHasher {
    async fn hash(&self, password: &Password) -> Result<StoredPasswordHash, CryptoError> {
        self.hashes.fetch_add(1, Ordering::SeqCst);
        Ok(StoredPasswordHash::new(format!(
            "hash:{}",
            password.expose()
        )))
    }

    async fn verify(&self, password: &Secret, hash: &StoredPasswordHash) -> bool {
        self.verifications.fetch_add(1, Ordering::SeqCst);
        // The real one takes a while: let the other attempts run meanwhile.
        tokio::task::yield_now().await;
        hash.as_str() == format!("hash:{}", password.expose())
    }
}

/// Reversible, bound to the context like the real one.
#[derive(Default)]
pub struct FakeCipher;

impl SecretCipher for FakeCipher {
    fn seal(&self, context: &[u8], secret: &Secret) -> Result<SealedSecret, CryptoError> {
        let mut bytes = context.to_vec();
        bytes.push(b'|');
        bytes.extend_from_slice(secret.expose().as_bytes());
        Ok(SealedSecret::new(bytes))
    }

    fn open(&self, context: &[u8], sealed: &SealedSecret) -> Result<Secret, CryptoError> {
        let bytes = sealed.as_bytes();
        let rest = bytes
            .strip_prefix(context)
            .and_then(|r| r.strip_prefix(b"|"))
            .ok_or(CryptoError::Unreadable)?;
        let text = String::from_utf8(rest.to_vec()).map_err(|_| CryptoError::Unreadable)?;
        Secret::new(&text).ok_or(CryptoError::Unreadable)
    }
}

/// Counting tokens: `tok-1`, `tok-2`, ...; the digest is the token itself.
#[derive(Default)]
pub struct FakeTokens(AtomicU64);

impl TokenIssuer for FakeTokens {
    fn new_session_token(&self) -> Secret {
        Secret::generated(&format!(
            "tok-{}",
            self.0.fetch_add(1, Ordering::SeqCst) + 1
        ))
    }

    fn digest(&self, token: &str) -> TokenDigest {
        TokenDigest::new(token.as_bytes().to_vec())
    }

    fn temporary_password(&self) -> Secret {
        Secret::generated("temporary-password-0001")
    }
}

/// Keys for the tests that call the model: the owner has one.
pub fn owner_keys() -> Arc<crate::LlmKeys> {
    let users = MemUsers::with_owner();
    let cipher = Arc::new(FakeCipher);
    users.give_key(UserId::OWNER, &cipher, "sk-owner");
    Arc::new(crate::LlmKeys::new(users, cipher))
}
