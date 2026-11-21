//! Chat messages exchanged with the Tutor inside a topic.

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use crate::ContentLanguage;
use crate::ids::{JobId, MessageId, TopicId};
use crate::mission::Mission;
use crate::validation::{ValidationIssue, normalize_text};

pub const MESSAGE_TEXT_MAX_CHARS: usize = 8000;

/// Validated text typed by the user: trimmed, 1-8000 code points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageText(String);

impl MessageText {
    pub fn parse(raw: &str) -> Result<Self, ValidationIssue> {
        normalize_text(raw, MESSAGE_TEXT_MAX_CHARS, "text").map(Self)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

impl MessageRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    Streaming,
    Complete,
    Failed,
}

impl MessageStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
}

/// Content of a message: text, or a native part the app renders as a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagePart {
    Text(String),
    /// Snapshot of the draft the Tutor proposed (`mission.md` §3). It never
    /// changes afterwards: the app compares versions to know whether the draft
    /// is still open.
    MissionProposal {
        mission: Mission,
        /// Confirmed version the draft would replace, if any.
        replaces_version: Option<u32>,
    },
    /// Reference to the `lesson_generation` job the Tutor started or found
    /// already running (`lessons.md` §12). Not a snapshot: the job holds the
    /// state.
    LessonRequest {
        job_id: JobId,
    },
}

impl MessagePart {
    /// A card is a native part that closes the message (one per message).
    #[must_use]
    pub const fn is_card(&self) -> bool {
        matches!(
            self,
            Self::MissionProposal { .. } | Self::LessonRequest { .. }
        )
    }
}

/// Why a generation failed. The string forms are part of the API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatErrorCode {
    UpstreamFailed,
    RateLimited,
    Internal,
    LlmKeyMissing,
    LlmKeyRejected,
    LlmCreditExhausted,
}

impl ChatErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UpstreamFailed => "upstream_failed",
            Self::RateLimited => "rate_limited",
            Self::Internal => "internal",
            Self::LlmKeyMissing => "llm_key_missing",
            Self::LlmKeyRejected => "llm_key_rejected",
            Self::LlmCreditExhausted => "llm_credit_exhausted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown chat error code `{0}`")]
pub struct UnknownChatErrorCode(pub String);

impl FromStr for ChatErrorCode {
    type Err = UnknownChatErrorCode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "upstream_failed" => Self::UpstreamFailed,
            "rate_limited" => Self::RateLimited,
            "internal" => Self::Internal,
            "llm_key_missing" => Self::LlmKeyMissing,
            "llm_key_rejected" => Self::LlmKeyRejected,
            "llm_credit_exhausted" => Self::LlmCreditExhausted,
            other => return Err(UnknownChatErrorCode(other.to_owned())),
        })
    }
}

/// Error attached to a failed message. The text is fixed per code so nothing
/// internal (provider messages, SQL) can reach the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageError {
    pub code: ChatErrorCode,
    pub message: String,
}

impl MessageError {
    #[must_use]
    /// The text is in the language of the message's topic (ADR 0019).
    pub fn from_code(code: ChatErrorCode, language: ContentLanguage) -> Self {
        let (it, en) = match code {
            ChatErrorCode::UpstreamFailed => (
                "Il servizio del modello non risponde, riprova.",
                "The model service is not responding, try again.",
            ),
            ChatErrorCode::RateLimited => (
                "Troppe richieste al modello, riprova tra poco.",
                "Too many requests to the model, try again shortly.",
            ),
            ChatErrorCode::Internal => ("Errore interno.", "Internal error."),
            ChatErrorCode::LlmKeyMissing => (
                "Manca la chiave del servizio LLM: inseriscila nelle Impostazioni.",
                "The LLM service key is missing: enter it in Settings.",
            ),
            ChatErrorCode::LlmKeyRejected => (
                "Il servizio LLM ha rifiutato la chiave: controllala nelle Impostazioni.",
                "The LLM service rejected the key: check it in Settings.",
            ),
            ChatErrorCode::LlmCreditExhausted => (
                "Il credito del servizio LLM è esaurito: ricaricalo presso il provider e riprova.",
                "The LLM service credit is exhausted: top it up with the provider and try again.",
            ),
        };
        Self {
            code,
            message: language.pick(it, en).to_owned(),
        }
    }

    /// The model declined to answer (stop reason `refusal`).
    #[must_use]
    pub fn refused(language: ContentLanguage) -> Self {
        Self {
            code: ChatErrorCode::UpstreamFailed,
            message: language
                .pick(
                    "Il modello ha rifiutato di rispondere a questo messaggio.",
                    "The model declined to answer this message.",
                )
                .to_owned(),
        }
    }
}

/// A rule of the conversation was violated by the requested transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ChatRuleError {
    #[error("message is not streaming")]
    NotStreaming,
    #[error("only the last assistant message, if failed, can be retried")]
    NotRetryable,
    #[error("the topic already has a message being generated")]
    TurnInProgress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: MessageId,
    pub topic_id: TopicId,
    pub role: MessageRole,
    pub status: MessageStatus,
    pub parts: Vec<MessagePart>,
    pub error: Option<MessageError>,
    pub created_at: OffsetDateTime,
    /// Id of the last SSE event whose content is in the stored message, so a
    /// client reloading over REST knows which deltas it already has. `None`
    /// only for rows written before the field existed.
    pub last_event_id: Option<u64>,
    pub updated_at: OffsetDateTime,
}

impl ChatMessage {
    /// A user message is complete from the start.
    #[must_use]
    pub fn user(id: MessageId, topic_id: TopicId, text: MessageText, now: OffsetDateTime) -> Self {
        Self {
            id,
            topic_id,
            role: MessageRole::User,
            status: MessageStatus::Complete,
            parts: vec![MessagePart::Text(text.0)],
            error: None,
            created_at: now,
            last_event_id: None,
            updated_at: now,
        }
    }

    /// An empty assistant message waiting for its generated text.
    #[must_use]
    pub fn assistant_streaming(id: MessageId, topic_id: TopicId, now: OffsetDateTime) -> Self {
        Self {
            id,
            topic_id,
            role: MessageRole::Assistant,
            status: MessageStatus::Streaming,
            parts: Vec::new(),
            error: None,
            created_at: now,
            last_event_id: None,
            updated_at: now,
        }
    }

    /// All text parts concatenated.
    #[must_use]
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                MessagePart::Text(t) => Some(t.as_str()),
                MessagePart::MissionProposal { .. } | MessagePart::LessonRequest { .. } => None,
            })
            .collect()
    }

    /// Appends generated text, keeping a proposal card last: the text goes to
    /// the text part before the card, or into a new one if there is none.
    /// Ignored (returns `false`) unless the message is streaming.
    pub fn append_text(&mut self, delta: &str, now: OffsetDateTime) -> bool {
        self.append_text_at(delta, now).is_some()
    }

    /// Like [`Self::append_text`], returning the index of the part the text
    /// went into (what a `message_delta` event must name).
    pub fn append_text_at(&mut self, delta: &str, now: OffsetDateTime) -> Option<usize> {
        if self.status != MessageStatus::Streaming {
            return None;
        }
        let card_last = self.parts.last().is_some_and(MessagePart::is_card);
        let end = if card_last {
            self.parts.len() - 1
        } else {
            self.parts.len()
        };
        let last_text = end
            .checked_sub(1)
            .filter(|i| matches!(self.parts[*i], MessagePart::Text(_)));
        let index = if let Some(i) = last_text {
            if let MessagePart::Text(t) = &mut self.parts[i] {
                t.push_str(delta);
            }
            i
        } else {
            self.parts.insert(end, MessagePart::Text(delta.to_owned()));
            end
        };
        self.updated_at = now;
        Some(index)
    }

    /// Adds the card of a draft the Tutor just proposed, as the last part.
    /// Ignored (returns `false`) unless the message is streaming or already has
    /// a card (one proposal per turn).
    pub fn add_mission_proposal(
        &mut self,
        mission: Mission,
        replaces_version: Option<u32>,
        now: OffsetDateTime,
    ) -> bool {
        let has_card = self.parts.iter().any(MessagePart::is_card);
        if self.status != MessageStatus::Streaming || has_card {
            return false;
        }
        self.parts.push(MessagePart::MissionProposal {
            mission,
            replaces_version,
        });
        self.updated_at = now;
        true
    }

    /// Adds the reference to the generation job the Tutor requested, as the
    /// last part. Same rules as [`Self::add_mission_proposal`].
    pub fn add_lesson_request(&mut self, job_id: JobId, now: OffsetDateTime) -> bool {
        if self.status != MessageStatus::Streaming || self.parts.iter().any(MessagePart::is_card) {
            return false;
        }
        self.parts.push(MessagePart::LessonRequest { job_id });
        self.updated_at = now;
        true
    }

    pub fn complete(&mut self, now: OffsetDateTime) -> Result<(), ChatRuleError> {
        self.require_streaming()?;
        self.status = MessageStatus::Complete;
        self.updated_at = now;
        Ok(())
    }

    /// Fails the message; the partial text stays in `parts`.
    pub fn fail(&mut self, error: MessageError, now: OffsetDateTime) -> Result<(), ChatRuleError> {
        self.require_streaming()?;
        self.status = MessageStatus::Failed;
        self.error = Some(error);
        self.updated_at = now;
        Ok(())
    }

    /// Retry: a failed assistant message that is the last of its topic goes
    /// back to `streaming`, empty, keeping its id.
    pub fn restart(&mut self, is_last: bool, now: OffsetDateTime) -> Result<(), ChatRuleError> {
        if self.role != MessageRole::Assistant || self.status != MessageStatus::Failed || !is_last {
            return Err(ChatRuleError::NotRetryable);
        }
        self.status = MessageStatus::Streaming;
        self.parts.clear();
        self.error = None;
        self.last_event_id = None;
        self.updated_at = now;
        Ok(())
    }

    fn require_streaming(&self) -> Result<(), ChatRuleError> {
        if self.status == MessageStatus::Streaming {
            Ok(())
        } else {
            Err(ChatRuleError::NotStreaming)
        }
    }
}

/// Only one turn at a time per topic: a new message is refused while the most
/// recent one is still being generated.
#[must_use]
pub fn has_active_turn(latest: Option<&ChatMessage>) -> bool {
    latest.is_some_and(|m| m.status == MessageStatus::Streaming)
}

/// Keyset position in the message ordering (`created_at`, then `id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatCursor {
    pub created_at: OffsetDateTime,
    pub id: MessageId,
}

impl ChatCursor {
    #[must_use]
    pub fn after(message: &ChatMessage) -> Self {
        Self {
            created_at: message.created_at,
            id: message.id,
        }
    }
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::IssueKind;
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-08 10:00 UTC);

    #[test]
    fn credit_exhausted_code_round_trips_and_has_the_fixed_message() {
        let code = ChatErrorCode::LlmCreditExhausted;
        assert_eq!(code.as_str(), "llm_credit_exhausted");
        assert_eq!("llm_credit_exhausted".parse::<ChatErrorCode>(), Ok(code));
        assert_eq!(
            MessageError::from_code(code, ContentLanguage::It).message,
            "Il credito del servizio LLM è esaurito: ricaricalo presso il provider e riprova."
        );
    }

    #[test]
    fn every_message_error_has_a_text_in_each_language() {
        for code in [
            ChatErrorCode::UpstreamFailed,
            ChatErrorCode::RateLimited,
            ChatErrorCode::Internal,
            ChatErrorCode::LlmKeyMissing,
            ChatErrorCode::LlmKeyRejected,
            ChatErrorCode::LlmCreditExhausted,
        ] {
            let it = MessageError::from_code(code, ContentLanguage::It);
            let en = MessageError::from_code(code, ContentLanguage::En);
            assert_eq!(it.code, en.code);
            assert_ne!(it.message, en.message, "{code:?}");
        }
        assert_eq!(
            MessageError::from_code(ChatErrorCode::Internal, ContentLanguage::En).message,
            "Internal error."
        );
        assert_eq!(
            MessageError::refused(ContentLanguage::En).message,
            "The model declined to answer this message."
        );
    }
    const T1: OffsetDateTime = datetime!(2026-11-08 10:01 UTC);

    fn assistant() -> ChatMessage {
        ChatMessage::assistant_streaming(
            MessageId::new(Uuid::from_u128(2)),
            TopicId::new(Uuid::from_u128(1)),
            T0,
        )
    }

    #[test]
    fn text_is_trimmed_and_bounded_in_code_points() {
        assert_eq!(MessageText::parse("  ciao ").unwrap().as_str(), "ciao");
        assert_eq!(
            MessageText::parse(" \n").unwrap_err().issue,
            IssueKind::Empty
        );
        assert!(MessageText::parse(&"a".repeat(8000)).is_ok());
        assert!(MessageText::parse(&"日".repeat(8000)).is_ok());
        let err = MessageText::parse(&"a".repeat(8001)).unwrap_err();
        assert_eq!((err.field, err.issue), ("text", IssueKind::TooLong));
    }

    #[test]
    fn user_message_is_complete_and_assistant_starts_empty() {
        let user = ChatMessage::user(
            MessageId::new(Uuid::from_u128(1)),
            TopicId::new(Uuid::from_u128(1)),
            MessageText::parse("hi").unwrap(),
            T0,
        );
        assert_eq!(
            (user.role, user.status),
            (MessageRole::User, MessageStatus::Complete)
        );
        assert_eq!(user.text(), "hi");
        let a = assistant();
        assert_eq!(a.status, MessageStatus::Streaming);
        assert_eq!(a.parts.len(), 0);
    }

    #[test]
    fn deltas_accumulate_in_one_text_part() {
        let mut m = assistant();
        assert!(m.append_text("Per", T1));
        assert!(m.append_text("ché", T1));
        assert_eq!(m.parts, vec![MessagePart::Text("Perché".into())]);
        assert_eq!(m.updated_at, T1);
    }

    #[test]
    fn the_lesson_request_is_the_only_card_and_stays_last() {
        let job = JobId::new(Uuid::from_u128(9));
        let mut m = assistant();
        assert_eq!(m.append_text_at("Preparo. ", T0), Some(0));
        assert!(m.add_lesson_request(job, T1));
        assert!(!m.add_lesson_request(job, T1));
        assert_eq!(m.append_text_at("Un attimo.", T1), Some(0));
        assert_eq!(m.parts[0], MessagePart::Text("Preparo. Un attimo.".into()));
        assert_eq!(m.parts[1], MessagePart::LessonRequest { job_id: job });
    }

    #[test]
    fn the_proposal_card_stays_last_and_later_text_goes_before_it() {
        let mission = Mission::draft(
            TopicId::new(Uuid::from_u128(1)),
            1,
            crate::mission::MissionContent::parse("w", "sp", &["a".to_owned()], &[], &[]).unwrap(),
            T0,
        );
        let mut m = assistant();
        assert_eq!(m.append_text_at("Ho capito. ", T0), Some(0));
        assert!(m.add_mission_proposal(mission.clone(), Some(1), T1));
        // One card per message.
        assert!(!m.add_mission_proposal(mission.clone(), None, T1));
        assert_eq!(m.append_text_at("Ok?", T1), Some(0));
        assert_eq!(m.parts.len(), 2);
        assert_eq!(m.parts[0], MessagePart::Text("Ho capito. Ok?".into()));
        assert!(matches!(
            m.parts[1],
            MessagePart::MissionProposal {
                replaces_version: Some(1),
                ..
            }
        ));
        assert_eq!(m.text(), "Ho capito. Ok?");

        // No text before the card: a text part is inserted ahead of it.
        let mut only_card = assistant();
        assert!(only_card.add_mission_proposal(mission, None, T0));
        assert_eq!(only_card.append_text_at("Ciao", T1), Some(0));
        assert!(matches!(only_card.parts[0], MessagePart::Text(_)));
        assert!(matches!(
            only_card.parts[1],
            MessagePart::MissionProposal { .. }
        ));
    }

    #[test]
    fn terminal_states_reject_further_changes() {
        let mut m = assistant();
        m.append_text("x", T0);
        m.complete(T1).unwrap();
        assert!(!m.append_text("y", T1));
        assert_eq!(m.text(), "x");
        assert_eq!(m.complete(T1), Err(ChatRuleError::NotStreaming));
        let err = MessageError::from_code(ChatErrorCode::Internal, ContentLanguage::It);
        assert_eq!(m.fail(err, T1), Err(ChatRuleError::NotStreaming));
    }

    #[test]
    fn failure_keeps_partial_text_and_sets_error() {
        let mut m = assistant();
        m.append_text("meta", T0);
        m.fail(
            MessageError::from_code(ChatErrorCode::UpstreamFailed, ContentLanguage::It),
            T1,
        )
        .unwrap();
        assert_eq!(m.status, MessageStatus::Failed);
        assert_eq!(m.text(), "meta");
        assert_eq!(
            m.error.as_ref().unwrap().code,
            ChatErrorCode::UpstreamFailed
        );
    }

    #[test]
    fn only_a_failed_last_assistant_message_restarts() {
        let mut m = assistant();
        assert_eq!(m.restart(true, T1), Err(ChatRuleError::NotRetryable));
        m.append_text("meta", T0);
        m.fail(
            MessageError::from_code(ChatErrorCode::Internal, ContentLanguage::It),
            T0,
        )
        .unwrap();
        assert_eq!(m.restart(false, T1), Err(ChatRuleError::NotRetryable));
        m.restart(true, T1).unwrap();
        assert_eq!(m.status, MessageStatus::Streaming);
        assert!(m.parts.is_empty() && m.error.is_none());
        assert_eq!(m.id, MessageId::new(Uuid::from_u128(2)));

        let mut user = ChatMessage::user(
            MessageId::new(Uuid::from_u128(3)),
            m.topic_id,
            MessageText::parse("x").unwrap(),
            T0,
        );
        assert_eq!(user.restart(true, T1), Err(ChatRuleError::NotRetryable));
    }

    #[test]
    fn active_turn_is_detected_from_the_latest_message() {
        assert!(!has_active_turn(None));
        let mut m = assistant();
        assert!(has_active_turn(Some(&m)));
        m.complete(T1).unwrap();
        assert!(!has_active_turn(Some(&m)));
    }

    #[test]
    fn error_codes_round_trip() {
        for c in [
            ChatErrorCode::UpstreamFailed,
            ChatErrorCode::RateLimited,
            ChatErrorCode::Internal,
        ] {
            assert_eq!(c.as_str().parse::<ChatErrorCode>().unwrap(), c);
        }
        assert!("x".parse::<ChatErrorCode>().is_err());
    }
}
