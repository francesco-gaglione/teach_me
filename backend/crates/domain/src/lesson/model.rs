use serde_json::{Value, json};
use uuid::Uuid;

use super::inline::InlineScan;

/// Stable error codes of the Validator (contract §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LessonErrorCode {
    Schema,
    UnknownBlockType,
    DuplicateId,
    QuizCorrectMissing,
    QuizOptionLength,
    QuizOptionLengthCue,
    QuizAnswerPosition,
    FillBlankMismatch,
    UnknownCitation,
    UnknownResource,
    UnknownGlossaryKey,
    UnusedEntry,
    PrimarySourceCount,
    AskAgentMissing,
    NoRequiredExercise,
    TooLong,
    FirstBlockNotTitle,
    SectionWithoutContent,
    InvalidLessonLink,
    NestedMissingComponent,
    Markdown,
    DiagramStructure,
    InlineCodeTooLong,
    FillBlankAnswerInText,
    QuizOptionMarkupUneven,
}

impl LessonErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::UnknownBlockType => "unknown_block_type",
            Self::DuplicateId => "duplicate_id",
            Self::QuizCorrectMissing => "quiz_correct_missing",
            Self::QuizOptionLength => "quiz_option_length",
            Self::QuizOptionLengthCue => "quiz_option_length_cue",
            Self::QuizAnswerPosition => "quiz_answer_position",
            Self::FillBlankMismatch => "fill_blank_mismatch",
            Self::UnknownCitation => "unknown_citation",
            Self::UnknownResource => "unknown_resource",
            Self::UnknownGlossaryKey => "unknown_glossary_key",
            Self::UnusedEntry => "unused_entry",
            Self::PrimarySourceCount => "primary_source_count",
            Self::AskAgentMissing => "ask_agent_missing",
            Self::NoRequiredExercise => "no_required_exercise",
            Self::TooLong => "too_long",
            Self::FirstBlockNotTitle => "first_block_not_title",
            Self::SectionWithoutContent => "section_without_content",
            Self::InvalidLessonLink => "invalid_lesson_link",
            Self::NestedMissingComponent => "nested_missing_component",
            Self::Markdown => "markdown",
            Self::DiagramStructure => "diagram_structure",
            Self::InlineCodeTooLong => "inline_code_too_long",
            Self::FillBlankAnswerInText => "fill_blank_answer_in_text",
            Self::QuizOptionMarkupUneven => "quiz_option_markup_uneven",
        }
    }
}

/// One problem found by the Validator. `path` is a JSON Pointer (RFC 6901)
/// into the validated document; `message` is Italian and written to be fed
/// back to the Generator for repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonIssue {
    pub path: String,
    pub code: LessonErrorCode,
    pub message: String,
}

impl LessonIssue {
    #[must_use]
    pub fn new(path: impl Into<String>, code: LessonErrorCode, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            code,
            message: message.into(),
        }
    }

    /// The `{path, code, message}` shape of the contract.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({"path": self.path, "code": self.code.as_str(), "message": self.message})
    }
}

/// A parsed lesson, reduced to what the validation rules need.
#[derive(Debug, Clone, Default)]
pub struct LessonDocument {
    pub glossary: Vec<GlossaryEntry>,
    pub sources: Vec<SourceEntry>,
    pub blocks: Vec<Block>,
    /// Every inline-markdown field found, wherever it lives.
    pub md_fields: Vec<MdField>,
}

impl LessonDocument {
    /// Words of prose (paragraphs, callouts, quiz questions and options).
    #[must_use]
    pub fn prose_words(&self) -> usize {
        self.md_fields
            .iter()
            .filter(|f| f.prose)
            .map(|f| f.words)
            .sum()
    }
}

#[derive(Debug, Clone)]
pub struct GlossaryEntry {
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct SourceEntry {
    pub key: String,
    pub resource_id: Option<Uuid>,
    /// `url` is `null` (a source from an uploaded PDF).
    pub url_is_null: bool,
}

/// An inline-markdown field with the result of scanning it.
#[derive(Debug, Clone)]
pub struct MdField {
    /// JSON Pointer of the field.
    pub path: String,
    pub prose: bool,
    pub words: usize,
    pub scan: InlineScan,
}

#[derive(Debug, Clone)]
pub struct QuizOption {
    pub id: String,
    pub words: usize,
    /// Characters of the visible text (markup removed).
    pub chars: usize,
}

#[derive(Debug, Clone)]
pub struct Block {
    /// JSON Pointer of the block.
    pub path: String,
    pub id: String,
    pub kind: BlockKind,
}

#[derive(Debug, Clone)]
pub enum BlockKind {
    Heading {
        level: u8,
    },
    Paragraph,
    Callout,
    Code,
    GlossaryTerm {
        key: String,
    },
    QuizSingle {
        options: Vec<QuizOption>,
        correct: String,
        required: bool,
    },
    QuizMulti {
        options: Vec<QuizOption>,
        correct: Vec<String>,
        required: bool,
    },
    FillBlank {
        text: String,
        blank_ids: Vec<String>,
        required: bool,
    },
    OrderSteps {
        required: bool,
    },
    Flashcard {
        required: bool,
    },
    OpenAnswer {
        required: bool,
    },
    Checklist,
    PrimarySource {
        source: String,
    },
    LessonLink {
        lesson_id: Uuid,
    },
    AskAgent,
    MissingComponent {
        fallback: Vec<Block>,
    },
    /// Only well-formed diagrams get here (shape errors make a block
    /// `Malformed`), so every edge index maps to `/edges/{i}` in the input.
    Diagram {
        layout: DiagramLayout,
        nodes: Vec<String>,
        edges: Vec<DiagramEdge>,
    },
    /// `type` is not in the catalog: reported as `unknown_block_type`.
    Unknown,
    /// Known type but invalid shape: already reported as `schema`, ignored
    /// by the semantic rules.
    Malformed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramLayout {
    Flow,
    Tree,
    Cycle,
}

#[derive(Debug, Clone)]
pub struct DiagramEdge {
    pub from: String,
    pub to: String,
}

impl Block {
    /// Whether the block is an exercise the learner must complete.
    #[must_use]
    pub const fn is_required_exercise(&self) -> bool {
        matches!(
            self.kind,
            BlockKind::QuizSingle { required: true, .. }
                | BlockKind::QuizMulti { required: true, .. }
                | BlockKind::FillBlank { required: true, .. }
                | BlockKind::OrderSteps { required: true }
                | BlockKind::Flashcard { required: true }
                | BlockKind::OpenAnswer { required: true }
        )
    }

    /// This block followed by its nested fallback blocks, depth first.
    pub fn flatten<'a>(&'a self, out: &mut Vec<&'a Block>) {
        out.push(self);
        if let BlockKind::MissingComponent { fallback } = &self.kind {
            for b in fallback {
                b.flatten(out);
            }
        }
    }
}

/// Longest input value quoted in a message (contract §9.25).
const SHOWN_CHARS: usize = 60;

/// Renders an input value for a message: control characters made visible,
/// truncated to [`SHOWN_CHARS`] with an ellipsis. The Generator reads these
/// messages, so hostile content must not break their layout or inflate them.
#[must_use]
pub(crate) fn show(value: &str) -> String {
    let mut out = String::new();
    for (n, c) in value.chars().enumerate() {
        if n == SHOWN_CHARS {
            out.push('…');
            break;
        }
        if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

/// Escapes a key for use as a JSON Pointer segment (RFC 6901).
#[must_use]
pub(crate) fn pointer_escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
