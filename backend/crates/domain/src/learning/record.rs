use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use super::UnknownLearningValue;
use super::review::CardSchedule;
use crate::ids::{GlossaryTermId, LearningRecordId, LessonId, ReviewCardId, TopicId};

/// Limits of what one lesson may add (`lessons.md` §6.1).
pub const MAX_RECORDS_PER_LESSON: usize = 5;
pub const MAX_TERMS_PER_LESSON: usize = 8;
pub const MAX_CARDS_PER_LESSON: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// The learner used something non-trivial correctly.
    Demonstrated,
    /// The learner believed something wrong and now sees why.
    MisconceptionCorrected,
}

impl RecordKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Demonstrated => "demonstrated",
            Self::MisconceptionCorrected => "misconception_corrected",
        }
    }
}

impl FromStr for RecordKind {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "demonstrated" => Ok(Self::Demonstrated),
            "misconception_corrected" => Ok(Self::MisconceptionCorrected),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordStatus {
    Active,
    Superseded,
}

impl RecordStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
        }
    }
}

impl FromStr for RecordStatus {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

/// A structured `learning-records/NNNN-slug.md` of the `teach` skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningRecord {
    pub id: LearningRecordId,
    pub topic_id: TopicId,
    /// 1, 2, 3... in the topic, assigned when stored.
    pub number: u32,
    pub kind: RecordKind,
    pub title: String,
    pub body: String,
    pub evidence: String,
    pub status: RecordStatus,
    pub superseded_by: Option<LearningRecordId>,
    pub lesson_id: LessonId,
    pub created_at: OffsetDateTime,
}

/// A voice of the topic's `GLOSSARY.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossaryTerm {
    pub id: GlossaryTermId,
    pub topic_id: TopicId,
    pub key: String,
    pub term: String,
    pub definition: String,
    pub avoid: Vec<String>,
    /// The lesson that added or last revised it.
    pub lesson_id: LessonId,
    /// The lesson that first added it; kept when the term is revised. `None`
    /// only for terms revised before the field existed (origin unknown).
    pub first_lesson_id: Option<LessonId>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Ordering key of the glossary: the term, case-insensitive.
#[must_use]
pub fn term_sort_key(term: &str) -> String {
    term.to_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStatus {
    Active,
    /// Not offered for review (see [`SuspensionReason`]).
    Suspended,
}

impl CardStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }
}

impl FromStr for CardStatus {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

/// Why a card is suspended (`reviews.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspensionReason {
    /// The learner suspended it; they can reactivate it.
    User,
    /// Its record was superseded: the card never comes back.
    Superseded,
}

impl SuspensionReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Superseded => "superseded",
        }
    }
}

impl FromStr for SuspensionReason {
    type Err = UnknownLearningValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Self::User),
            "superseded" => Ok(Self::Superseded),
            other => Err(UnknownLearningValue(other.to_owned())),
        }
    }
}

/// The last suspension of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suspension {
    pub reason: SuspensionReason,
    pub at: OffsetDateTime,
}

/// What a review card was made from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardOrigin {
    Record(LearningRecordId),
    Term(GlossaryTermId),
}

/// Front and back of a review card, and when to ask for it again.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewCard {
    pub id: ReviewCardId,
    pub topic_id: TopicId,
    pub lesson_id: LessonId,
    pub origin: CardOrigin,
    pub front: String,
    pub back: String,
    pub status: CardStatus,
    /// Set exactly when `status` is `Suspended`.
    pub suspension: Option<Suspension>,
    pub schedule: CardSchedule,
    pub created_at: OffsetDateTime,
    /// Derived on read from the card's votes (`reviews.md` §1.3), never stored.
    pub to_consolidate: bool,
}

/// A new record of a completion, before its number exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRecord {
    pub id: LearningRecordId,
    pub kind: RecordKind,
    pub title: String,
    pub body: String,
    pub evidence: String,
    /// `number`s of active records this one replaces.
    pub supersedes: Vec<u32>,
}

/// A new or revised term: stored on the existing row when the topic already
/// has the `key` (then `id` is ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermUpsert {
    pub id: GlossaryTermId,
    pub key: String,
    pub term: String,
    pub definition: String,
    pub avoid: Vec<String>,
}

/// Which element of a [`CompletionSave`] a card belongs to, by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardTarget {
    Record(usize),
    Term(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCard {
    pub id: ReviewCardId,
    pub target: CardTarget,
    pub front: String,
    pub back: String,
    /// Block id of the exercise the learner never passed, when the card is
    /// born "to consolidate".
    pub unpassed_block: Option<String>,
}

/// Everything a lesson completion stores, atomically (`lessons.md` §6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionSave {
    pub topic_id: TopicId,
    pub lesson_id: LessonId,
    pub at: OffsetDateTime,
    pub records: Vec<NewRecord>,
    pub terms: Vec<TermUpsert>,
    pub cards: Vec<NewCard>,
}

/// What a lesson's completion produced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompletionSummary {
    pub record_count: u32,
    pub term_count: u32,
    pub review_card_count: u32,
}

/// Keyset position of the record list (`number`, newest first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordCursor {
    pub number: u32,
}

/// Keyset position of the glossary (term case-folded, then key).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TermCursor {
    pub sort_key: String,
    pub key: String,
}

impl TermCursor {
    #[must_use]
    pub fn after(term: &GlossaryTerm) -> Self {
        Self {
            sort_key: term_sort_key(&term.term),
            key: term.key.clone(),
        }
    }
}

impl fmt::Display for RecordKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a lesson's completion left in the topic, for the lesson view.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LessonOutputs {
    pub records: Vec<LearningRecord>,
    pub terms: Vec<GlossaryTerm>,
    pub review_card_count: u32,
}
