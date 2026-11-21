//! The mission of a topic: why the user learns it and how they will know they
//! succeeded (`docs/contracts/mission.md`). Versioned; at most one open draft.

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use crate::ids::TopicId;
use crate::validation::{IssueKind, ValidationIssue, normalize_text};

pub const WHY_MAX_CHARS: usize = 500;
pub const STARTING_POINT_MAX_CHARS: usize = 600;
/// Applies to every entry of the three lists.
pub const ITEM_MAX_CHARS: usize = 200;
pub const SUCCESS_CRITERIA_MAX_ITEMS: usize = 5;
pub const CONSTRAINTS_MAX_ITEMS: usize = 5;
pub const OUT_OF_SCOPE_MAX_ITEMS: usize = 5;

/// The validated text of a mission, without its bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionContent {
    why: String,
    /// `None` only for versions stored before contract v1.2.
    starting_point: Option<String>,
    success_criteria: Vec<String>,
    constraints: Vec<String>,
    out_of_scope: Vec<String>,
}

impl MissionContent {
    /// Trims every text and checks the contract limits. Every offending field
    /// is reported once (the first problem found in it), so a caller can fix
    /// them all in one go. New missions always declare a starting point.
    pub fn parse(
        why: &str,
        starting_point: &str,
        success_criteria: &[String],
        constraints: &[String],
        out_of_scope: &[String],
    ) -> Result<Self, Vec<ValidationIssue>> {
        Self::parse_stored(
            why,
            Some(starting_point),
            success_criteria,
            constraints,
            out_of_scope,
        )
    }

    /// Like [`Self::parse`] for versions read back from storage: those created
    /// before contract v1.2 have no starting point and stay valid without one.
    /// A stored value, when there is one, is checked like a new one.
    pub fn parse_stored(
        why: &str,
        starting_point: Option<&str>,
        success_criteria: &[String],
        constraints: &[String],
        out_of_scope: &[String],
    ) -> Result<Self, Vec<ValidationIssue>> {
        let mut issues = Vec::new();
        let why = normalize_text(why, WHY_MAX_CHARS, "why")
            .map_err(|i| issues.push(i))
            .unwrap_or_default();
        let starting_point = starting_point.and_then(|sp| {
            normalize_text(sp, STARTING_POINT_MAX_CHARS, "starting_point")
                .map_err(|i| issues.push(i))
                .ok()
        });
        let success_criteria = parse_list(
            success_criteria,
            1,
            SUCCESS_CRITERIA_MAX_ITEMS,
            "success_criteria",
            &mut issues,
        );
        let constraints = parse_list(
            constraints,
            0,
            CONSTRAINTS_MAX_ITEMS,
            "constraints",
            &mut issues,
        );
        let out_of_scope = parse_list(
            out_of_scope,
            0,
            OUT_OF_SCOPE_MAX_ITEMS,
            "out_of_scope",
            &mut issues,
        );
        if issues.is_empty() {
            Ok(Self {
                why,
                starting_point,
                success_criteria,
                constraints,
                out_of_scope,
            })
        } else {
            Err(issues)
        }
    }

    #[must_use]
    pub fn why(&self) -> &str {
        &self.why
    }

    #[must_use]
    pub fn starting_point(&self) -> Option<&str> {
        self.starting_point.as_deref()
    }

    #[must_use]
    pub fn success_criteria(&self) -> &[String] {
        &self.success_criteria
    }

    #[must_use]
    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }

    #[must_use]
    pub fn out_of_scope(&self) -> &[String] {
        &self.out_of_scope
    }
}

fn parse_list(
    items: &[String],
    min: usize,
    max: usize,
    field: &'static str,
    issues: &mut Vec<ValidationIssue>,
) -> Vec<String> {
    if items.len() < min {
        issues.push(ValidationIssue::new(field, IssueKind::Empty));
        return Vec::new();
    }
    if items.len() > max {
        issues.push(ValidationIssue::new(field, IssueKind::TooLong));
        return Vec::new();
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match normalize_text(item, ITEM_MAX_CHARS, field) {
            Ok(text) => out.push(text),
            Err(issue) => {
                issues.push(issue);
                return Vec::new();
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionStatus {
    /// Proposed, waiting for the user.
    Draft,
    /// The current mission of the topic.
    Confirmed,
    /// Replaced by a later version (confirmed, or a draft that was discarded).
    Superseded,
}

impl MissionStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Confirmed => "confirmed",
            Self::Superseded => "superseded",
        }
    }
}

impl fmt::Display for MissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown mission status `{0}`")]
pub struct UnknownMissionStatus(pub String);

impl FromStr for MissionStatus {
    type Err = UnknownMissionStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "draft" => Self::Draft,
            "confirmed" => Self::Confirmed,
            "superseded" => Self::Superseded,
            other => return Err(UnknownMissionStatus(other.to_owned())),
        })
    }
}

/// A mission transition that the current status does not allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MissionRuleError {
    #[error("only a draft can be confirmed")]
    NotDraft,
    #[error("the mission is already superseded")]
    AlreadySuperseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mission {
    pub topic_id: TopicId,
    /// Increasing per topic, starting at 1.
    pub version: u32,
    pub status: MissionStatus,
    pub content: MissionContent,
    pub created_at: OffsetDateTime,
    /// Set when the version was confirmed (and kept once superseded).
    pub confirmed_at: Option<OffsetDateTime>,
    /// Set when the version became `superseded`. A superseded version with no
    /// `confirmed_at` is a draft that was never confirmed.
    pub superseded_at: Option<OffsetDateTime>,
}

impl Mission {
    #[must_use]
    pub fn draft(
        topic_id: TopicId,
        version: u32,
        content: MissionContent,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            topic_id,
            version,
            status: MissionStatus::Draft,
            content,
            created_at: now,
            confirmed_at: None,
            superseded_at: None,
        }
    }

    /// `draft` becomes the current mission.
    pub fn confirm(&mut self, now: OffsetDateTime) -> Result<(), MissionRuleError> {
        if self.status != MissionStatus::Draft {
            return Err(MissionRuleError::NotDraft);
        }
        self.status = MissionStatus::Confirmed;
        self.confirmed_at = Some(now);
        Ok(())
    }

    /// A draft that is replaced, or a confirmed mission that a newer
    /// confirmation replaces.
    pub fn supersede(&mut self, now: OffsetDateTime) -> Result<(), MissionRuleError> {
        if self.status == MissionStatus::Superseded {
            return Err(MissionRuleError::AlreadySuperseded);
        }
        self.status = MissionStatus::Superseded;
        self.superseded_at = Some(now);
        Ok(())
    }
}

/// What `GET /topics/{id}/mission` returns.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopicMission {
    pub current: Option<Mission>,
    pub draft: Option<Mission>,
}

/// Keyset position in the version listing (newest version first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissionCursor {
    pub version: u32,
}

impl MissionCursor {
    #[must_use]
    pub fn after(mission: &Mission) -> Self {
        Self {
            version: mission.version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: OffsetDateTime = datetime!(2026-11-08 10:00 UTC);
    const T1: OffsetDateTime = datetime!(2026-11-08 10:05 UTC);
    const T2: OffsetDateTime = datetime!(2026-11-08 10:10 UTC);

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|i| (*i).to_owned()).collect()
    }

    fn content() -> MissionContent {
        MissionContent::parse("Per lavoro", "Parto da zero", &s(&["So fare X"]), &[], &[]).unwrap()
    }

    fn issues(
        why: &str,
        success: &[String],
        constraints: &[String],
        out: &[String],
    ) -> Vec<(&'static str, IssueKind)> {
        MissionContent::parse(why, "sp", success, constraints, out)
            .unwrap_err()
            .into_iter()
            .map(|i| (i.field, i.issue))
            .collect()
    }

    #[test]
    fn valid_content_is_trimmed() {
        let c = MissionContent::parse(
            "  Per lavoro ",
            "  Kotlin sì, Rust no ",
            &s(&[" So fare X "]),
            &s(&["30 minuti"]),
            &s(&["embedded"]),
        )
        .unwrap();
        assert_eq!(c.why(), "Per lavoro");
        assert_eq!(c.starting_point(), Some("Kotlin sì, Rust no"));
        assert_eq!(c.success_criteria(), ["So fare X"]);
        assert_eq!(c.constraints(), ["30 minuti"]);
        assert_eq!(c.out_of_scope(), ["embedded"]);
    }

    #[test]
    fn why_is_required_and_bounded_in_code_points() {
        let ok = s(&["x"]);
        assert_eq!(issues(" ", &ok, &[], &[]), [("why", IssueKind::Empty)]);
        assert!(MissionContent::parse(&"è".repeat(500), "sp", &ok, &[], &[]).is_ok());
        assert_eq!(
            issues(&"è".repeat(501), &ok, &[], &[]),
            [("why", IssueKind::TooLong)]
        );
    }

    fn starting_point_issues(raw: &str) -> Option<IssueKind> {
        MissionContent::parse("w", raw, &s(&["a"]), &[], &[])
            .err()
            .map(|v| {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].field, "starting_point");
                v[0].issue
            })
    }

    #[test]
    fn starting_point_is_required_and_bounded_in_code_points() {
        assert_eq!(starting_point_issues(""), Some(IssueKind::Empty));
        assert_eq!(starting_point_issues(" \n "), Some(IssueKind::Empty));
        assert_eq!(starting_point_issues(&"è".repeat(600)), None);
        assert_eq!(
            starting_point_issues(&"è".repeat(601)),
            Some(IssueKind::TooLong)
        );
        // Padding does not count towards the limit.
        assert_eq!(
            starting_point_issues(&format!(" {} ", "x".repeat(600))),
            None
        );
    }

    #[test]
    fn a_stored_version_may_have_no_starting_point() {
        let c = MissionContent::parse_stored("w", None, &s(&["a"]), &[], &[]).unwrap();
        assert_eq!(c.starting_point(), None);
        // A stored value is still checked.
        let err = MissionContent::parse_stored("w", Some("  "), &s(&["a"]), &[], &[]).unwrap_err();
        assert_eq!(err[0].field, "starting_point");
    }

    #[test]
    fn success_criteria_need_one_to_five_entries() {
        assert_eq!(
            issues("w", &[], &[], &[]),
            [("success_criteria", IssueKind::Empty)]
        );
        assert!(MissionContent::parse("w", "sp", &s(&["a"; 5]), &[], &[]).is_ok());
        assert_eq!(
            issues("w", &s(&["a"; 6]), &[], &[]),
            [("success_criteria", IssueKind::TooLong)]
        );
    }

    #[test]
    fn optional_lists_allow_none_but_at_most_five() {
        let ok = s(&["a"]);
        assert!(MissionContent::parse("w", "sp", &ok, &[], &[]).is_ok());
        assert!(MissionContent::parse("w", "sp", &ok, &s(&["c"; 5]), &s(&["o"; 5])).is_ok());
        assert_eq!(
            issues("w", &ok, &s(&["c"; 6]), &[]),
            [("constraints", IssueKind::TooLong)]
        );
        assert_eq!(
            issues("w", &ok, &[], &s(&["o"; 6])),
            [("out_of_scope", IssueKind::TooLong)]
        );
    }

    #[test]
    fn entries_are_non_blank_and_at_most_200_chars() {
        assert_eq!(
            issues("w", &s(&["ok", "  "]), &[], &[]),
            [("success_criteria", IssueKind::Empty)]
        );
        let long = "x".repeat(201);
        assert_eq!(
            issues("w", &s(&["a"]), &[long], &[]),
            [("constraints", IssueKind::TooLong)]
        );
        assert!(MissionContent::parse("w", "sp", &["x".repeat(200)], &[], &[]).is_ok());
    }

    #[test]
    fn every_offending_field_is_reported_once() {
        let got = issues("", &[], &s(&["c"; 9]), &[String::new()]);
        assert_eq!(
            got,
            [
                ("why", IssueKind::Empty),
                ("success_criteria", IssueKind::Empty),
                ("constraints", IssueKind::TooLong),
                ("out_of_scope", IssueKind::Empty),
            ]
        );
    }

    #[test]
    fn a_draft_is_confirmed_once_and_then_superseded() {
        let mut m = Mission::draft(TopicId::new(Uuid::from_u128(1)), 1, content(), T0);
        assert_eq!((m.status, m.confirmed_at), (MissionStatus::Draft, None));
        m.confirm(T1).unwrap();
        assert_eq!(
            (m.status, m.confirmed_at),
            (MissionStatus::Confirmed, Some(T1))
        );
        assert_eq!(m.confirm(T1), Err(MissionRuleError::NotDraft));
        m.supersede(T2).unwrap();
        assert_eq!(m.status, MissionStatus::Superseded);
        // The confirmation date survives, it is history.
        assert_eq!((m.confirmed_at, m.superseded_at), (Some(T1), Some(T2)));
        assert_eq!(m.supersede(T2), Err(MissionRuleError::AlreadySuperseded));
        assert_eq!(m.confirm(T1), Err(MissionRuleError::NotDraft));
    }

    #[test]
    fn a_discarded_draft_can_be_superseded() {
        let mut m = Mission::draft(TopicId::new(Uuid::from_u128(1)), 2, content(), T0);
        m.supersede(T1).unwrap();
        assert_eq!(
            (m.status, m.confirmed_at, m.superseded_at),
            (MissionStatus::Superseded, None, Some(T1))
        );
    }

    #[test]
    fn status_round_trips_through_strings() {
        for st in [
            MissionStatus::Draft,
            MissionStatus::Confirmed,
            MissionStatus::Superseded,
        ] {
            assert_eq!(st.as_str().parse::<MissionStatus>().unwrap(), st);
        }
        assert!("nope".parse::<MissionStatus>().is_err());
    }
}
