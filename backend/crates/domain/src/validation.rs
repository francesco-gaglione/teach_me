//! Field-level validation vocabulary shared by all entities.

/// What is wrong with a field. The string forms are part of the API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    Empty,
    TooLong,
    TooShort,
    /// The current password given to change it is not the right one.
    Wrong,
    Duplicate,
    NotFound,
    Invalid,
    /// A position outside the bounds of the text it points into.
    OutOfRange,
    /// An interval that overlaps another one of the same set.
    Overlap,
    /// More items than the set allows.
    TooMany,
    /// More points in the whole set than a lesson's strokes may have.
    TooManyPoints,
    /// A voice id that is not in the server's list.
    UnknownVoice,
}

impl IssueKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::TooShort => "too_short",
            Self::Wrong => "wrong",
            Self::Duplicate => "duplicate",
            Self::NotFound => "not_found",
            Self::Invalid => "invalid",
            Self::OutOfRange => "out_of_range",
            Self::Overlap => "overlap",
            Self::TooMany => "too_many",
            Self::TooManyPoints => "too_many_points",
            Self::UnknownVoice => "unknown_voice",
        }
    }
}

/// A problem attributed to one input field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationIssue {
    pub field: &'static str,
    pub issue: IssueKind,
}

impl ValidationIssue {
    #[must_use]
    pub const fn new(field: &'static str, issue: IssueKind) -> Self {
        Self { field, issue }
    }
}

/// Trims and checks a text value. Length is counted in Unicode code points
/// after trimming, so the same text has the same verdict on every client.
pub(crate) fn normalize_text(
    raw: &str,
    max_chars: usize,
    field: &'static str,
) -> Result<String, ValidationIssue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ValidationIssue::new(field, IssueKind::Empty));
    }
    if trimmed.chars().count() > max_chars {
        return Err(ValidationIssue::new(field, IssueKind::TooLong));
    }
    Ok(trimmed.to_owned())
}
