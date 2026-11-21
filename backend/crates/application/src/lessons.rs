//! The user's side of lessons: asking for one, listing, opening, closing, and
//! reading the glossary and the learning records (`docs/contracts/lessons.md`).

use std::sync::Arc;

use domain::{
    Attempt, AttemptRepository, Clock, ContentLanguage, DocumentRepository, DocumentView,
    GlossaryTerm, Highlight, HighlightRepository, InProgressCursor, Job, JobKind, LearningRecord,
    LearningRepository, Lesson, LessonCursor, LessonId, LessonOutputs, LessonRepository, Page,
    PageRequest, RecordCursor, RepositoryError, Stroke, StrokeRepository, TermCursor, TopicId,
    TopicRepository, TopicState, UserId, client_view,
};
use serde_json::Value;

use crate::audio_queue::AudioPreparer;
use crate::document_context::{DocumentSource, document_sources};
use crate::error::{AppError, ConflictReason};
use crate::jobs::JobService;
use crate::llm_keys::LlmKeys;
use crate::paging::paginate;
use crate::time_util::now_millis;

/// A lesson of a list with the documents it was built from.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonItem {
    pub lesson: Lesson,
    pub document_sources: Vec<DocumentSource>,
}

/// A row of `GET /lessons?status=in_progress`.
#[derive(Debug, Clone, PartialEq)]
pub struct InProgressItem {
    pub item: LessonItem,
    pub topic_title: String,
    pub last_activity_at: time::OffsetDateTime,
}

/// What `GET /lessons/{id}` returns.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonView {
    pub lesson: Lesson,
    /// Language of the topic, for the SDUI blocks without `lang` (ADR 0019).
    pub language: ContentLanguage,
    pub document_sources: Vec<DocumentSource>,
    /// The document without the fields only the backend may see.
    pub document: Value,
    pub attempts: Vec<Attempt>,
    /// The learner's highlights, in document order.
    pub highlights: Vec<Highlight>,
    /// The learner's handwritten strokes, in drawing order.
    pub strokes: Vec<Stroke>,
    /// `Some` once the lesson is completed.
    pub completion: Option<CompletionView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionView {
    /// The latest `lesson_completion` job of the lesson.
    pub job: Option<Job>,
    pub outputs: LessonOutputs,
}

pub struct LessonService {
    topics: Arc<dyn TopicRepository>,
    lessons: Arc<dyn LessonRepository>,
    attempts: Arc<dyn AttemptRepository>,
    highlights: Arc<dyn HighlightRepository>,
    strokes: Arc<dyn StrokeRepository>,
    learning: Arc<dyn LearningRepository>,
    jobs: Arc<JobService>,
    keys: Arc<LlmKeys>,
    clock: Arc<dyn Clock>,
    /// `None` when read-aloud is off.
    audio: Option<Arc<AudioPreparer>>,
    /// `None`: lessons show no document sources.
    documents: Option<Arc<dyn DocumentRepository>>,
}

impl LessonService {
    #[must_use]
    #[allow(clippy::too_many_arguments)] // one port per thing the view and the closing read
    pub fn new(
        topics: Arc<dyn TopicRepository>,
        lessons: Arc<dyn LessonRepository>,
        attempts: Arc<dyn AttemptRepository>,
        highlights: Arc<dyn HighlightRepository>,
        strokes: Arc<dyn StrokeRepository>,
        learning: Arc<dyn LearningRepository>,
        jobs: Arc<JobService>,
        keys: Arc<LlmKeys>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            topics,
            lessons,
            attempts,
            highlights,
            strokes,
            learning,
            jobs,
            keys,
            clock,
            audio: None,
            documents: None,
        }
    }

    /// Lets lessons show the documents they were built from.
    #[must_use]
    pub fn with_documents(mut self, documents: Arc<dyn DocumentRepository>) -> Self {
        self.documents = Some(documents);
        self
    }

    async fn topic_documents(&self, topic_id: TopicId) -> Result<Vec<DocumentView>, AppError> {
        match &self.documents {
            Some(d) => Ok(d.list(topic_id).await?),
            None => Ok(Vec::new()),
        }
    }

    /// Lessons in progress of every topic of the user, most recently worked on
    /// first. Documents are read once per topic on the page.
    pub async fn list_in_progress(
        &self,
        user: UserId,
        request: PageRequest<InProgressCursor>,
    ) -> Result<Page<InProgressItem, InProgressCursor>, AppError> {
        let page = paginate(
            request,
            |after, limit| async move { self.lessons.list_in_progress(user, after, limit).await },
            InProgressCursor::after,
        )
        .await
        .map_err(AppError::from)?;
        let mut docs: std::collections::HashMap<TopicId, Vec<DocumentView>> =
            std::collections::HashMap::new();
        let mut items = Vec::with_capacity(page.items.len());
        for i in page.items {
            let topic = i.lesson.topic_id;
            if let std::collections::hash_map::Entry::Vacant(e) = docs.entry(topic) {
                e.insert(self.topic_documents(topic).await?);
            }
            items.push(InProgressItem {
                item: LessonItem {
                    document_sources: document_sources(&i.plan, &docs[&topic]),
                    lesson: i.lesson,
                },
                topic_title: i.topic_title,
                last_activity_at: i.last_activity_at,
            });
        }
        Ok(Page {
            items,
            next: page.next,
        })
    }

    /// Opening a lesson then queues its missing audio (`audio.md` §1).
    #[must_use]
    pub fn with_audio(mut self, audio: Arc<AudioPreparer>) -> Self {
        self.audio = Some(audio);
        self
    }

    async fn require_topic(&self, user: UserId, id: TopicId) -> Result<domain::Topic, AppError> {
        self.topics.get(user, id).await?.ok_or(AppError::NotFound)
    }

    /// Queues a `lesson_generation` job. The topic must be `ready` and have
    /// no generation queued or running.
    pub async fn request_generation(
        &self,
        user: UserId,
        topic_id: TopicId,
    ) -> Result<Job, AppError> {
        let topic = self.require_topic(user, topic_id).await?;
        if topic.state != TopicState::Ready {
            return Err(AppError::Conflict(ConflictReason::TopicNotReady));
        }
        self.jobs
            .enqueue(user, topic_id, JobKind::LessonGeneration)
            .await
    }

    /// The queued or running `lesson_generation` job of the topic, if any.
    pub async fn active_generation(
        &self,
        user: UserId,
        topic_id: TopicId,
    ) -> Result<Option<Job>, AppError> {
        self.require_topic(user, topic_id).await?;
        Ok(self
            .jobs
            .latest(topic_id, JobKind::LessonGeneration)
            .await?
            .filter(|j| j.status.is_active()))
    }

    /// Newest lesson first.
    pub async fn list(
        &self,
        user: UserId,
        topic_id: TopicId,
        request: PageRequest<LessonCursor>,
    ) -> Result<Page<LessonItem, LessonCursor>, AppError> {
        self.require_topic(user, topic_id).await?;
        let page = paginate(
            request,
            |after, limit| async move { self.lessons.list(topic_id, after, limit).await },
            LessonCursor::after,
        )
        .await
        .map_err(AppError::from)?;
        // One read of the plans and one of the documents for the whole page.
        let plans: std::collections::HashMap<_, _> =
            self.lessons.plans(topic_id).await?.into_iter().collect();
        let docs = self.topic_documents(topic_id).await?;
        let items = page
            .items
            .into_iter()
            .map(|lesson| LessonItem {
                document_sources: plans
                    .get(&lesson.id)
                    .map_or_else(Vec::new, |p| document_sources(p, &docs)),
                lesson,
            })
            .collect();
        Ok(Page {
            items,
            next: page.next,
        })
    }

    /// The lesson row, to answer 404 before anything else.
    pub async fn get(&self, user: UserId, lesson_id: LessonId) -> Result<Lesson, AppError> {
        Ok(self
            .lessons
            .get(user, lesson_id)
            .await?
            .ok_or(AppError::NotFound)?
            .lesson)
    }

    pub async fn view(&self, user: UserId, lesson_id: LessonId) -> Result<LessonView, AppError> {
        let stored = self
            .lessons
            .get(user, lesson_id)
            .await?
            .ok_or(AppError::NotFound)?;
        // Opening the lesson asks for its audio. Whatever goes wrong here is
        // logged: the lesson itself must still open.
        let topic = self
            .topics
            .get(user, stored.lesson.topic_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if let Some(audio) = &self.audio {
            let prepared = audio
                .prepare(user, lesson_id, &stored.document, topic.language, true)
                .await;
            if let Err(e) = prepared {
                tracing::warn!(lesson = %lesson_id, error = %e, "could not queue the audio of a lesson");
            }
        }
        let attempts = self.attempts.list(lesson_id).await?;
        let highlights = self.highlights.list(lesson_id).await?;
        let strokes = self.strokes.list(lesson_id).await?;
        let completion = if stored.lesson.is_completed() {
            Some(CompletionView {
                job: self
                    .jobs
                    .latest_matching(
                        stored.lesson.topic_id,
                        JobKind::LessonCompletion,
                        Some(lesson_id),
                    )
                    .await?,
                outputs: self.learning.lesson_outputs(lesson_id).await?,
            })
        } else {
            None
        };
        let docs = self.topic_documents(stored.lesson.topic_id).await?;
        Ok(LessonView {
            language: topic.language,
            document_sources: document_sources(&stored.plan, &docs),
            document: client_view(&stored.document),
            lesson: stored.lesson,
            attempts,
            highlights,
            strokes,
            completion,
        })
    }

    /// Marks the lesson completed and queues its `lesson_completion` job, in
    /// one transaction.
    pub async fn complete(&self, user: UserId, lesson_id: LessonId) -> Result<Job, AppError> {
        let stored = self
            .lessons
            .get(user, lesson_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if stored.lesson.is_completed() {
            return Err(AppError::Conflict(ConflictReason::LessonCompleted));
        }
        // Closing a lesson records it with the model: no key, no completion
        // (and no half-closed lesson).
        self.keys.require(user).await?;
        let job =
            self.jobs
                .new_lesson_job(stored.lesson.topic_id, lesson_id, JobKind::LessonCompletion);
        self.lessons
            .complete(lesson_id, now_millis(self.clock.as_ref()), &job)
            .await
            .map_err(|e| match e {
                RepositoryError::Conflict => AppError::Conflict(ConflictReason::LessonCompleted),
                other => other.into(),
            })?;
        self.jobs.announce(&job);
        Ok(job)
    }

    /// Alphabetical by term.
    pub async fn glossary(
        &self,
        user: UserId,
        topic_id: TopicId,
        request: PageRequest<TermCursor>,
    ) -> Result<Page<GlossaryTerm, TermCursor>, AppError> {
        self.require_topic(user, topic_id).await?;
        paginate(
            request,
            |after, limit| async move { self.learning.list_terms(topic_id, after, limit).await },
            TermCursor::after,
        )
        .await
        .map_err(AppError::from)
    }

    /// Newest first; `active_only` hides the superseded ones.
    pub async fn records(
        &self,
        user: UserId,
        topic_id: TopicId,
        active_only: bool,
        request: PageRequest<RecordCursor>,
    ) -> Result<Page<LearningRecord, RecordCursor>, AppError> {
        self.require_topic(user, topic_id).await?;
        paginate(
            request,
            |after, limit| async move {
                self.learning
                    .list_records(topic_id, active_only, after, limit)
                    .await
            },
            |r: &LearningRecord| RecordCursor { number: r.number },
        )
        .await
        .map_err(AppError::from)
    }
}
