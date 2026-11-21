//! The learner's handwritten notes on a lesson (`docs/contracts/lessons.md` §11).
//!
//! A stroke is anchored to a top-level block of the document; its points are
//! in ten-thousandths of that block's width, so the note follows the block.
//! The backend never interprets the points beyond the ranges below.

use std::collections::HashSet;
use std::str::FromStr;

use serde_json::Value;

use crate::validation::IssueKind;

/// Most strokes one lesson can have.
pub const MAX_STROKES: usize = 2000;
/// Most points one stroke can have.
pub const MAX_POINTS_PER_STROKE: usize = 1000;
/// Most points all the strokes of a lesson can have together.
pub const MAX_POINTS_TOTAL: usize = 200_000;
/// Inclusive range of `x`, in ten-thousandths of the block width.
pub const X_RANGE: (i64, i64) = (-40_000, 50_000);
/// Inclusive range of `y`, in ten-thousandths of the block width.
pub const Y_RANGE: (i64, i64) = (-40_000, 200_000);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeColor {
    Ink,
    Blue,
    Red,
    Green,
}

impl StrokeColor {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ink => "ink",
            Self::Blue => "blue",
            Self::Red => "red",
            Self::Green => "green",
        }
    }
}

impl FromStr for StrokeColor {
    type Err = IssueKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ink" => Ok(Self::Ink),
            "blue" => Ok(Self::Blue),
            "red" => Ok(Self::Red),
            "green" => Ok(Self::Green),
            _ => Err(IssueKind::Invalid),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeWidth {
    Thin,
    Medium,
    Thick,
}

impl StrokeWidth {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thin => "thin",
            Self::Medium => "medium",
            Self::Thick => "thick",
        }
    }
}

impl FromStr for StrokeWidth {
    type Err = IssueKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "thin" => Ok(Self::Thin),
            "medium" => Ok(Self::Medium),
            "thick" => Ok(Self::Thick),
            _ => Err(IssueKind::Invalid),
        }
    }
}

/// A validated stroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stroke {
    /// The app's UUID, kept exactly as sent.
    pub id: String,
    pub block_id: String,
    pub color: StrokeColor,
    pub width: StrokeWidth,
    /// Flat `[x0, y0, x1, y1, ...]`, an even number of values, at least two.
    pub points: Vec<i32>,
}

/// One entry of a request before any check. `None` means missing or of the
/// wrong JSON type (for `points`: not an array of integers).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StrokeDraft {
    pub id: Option<String>,
    pub block_id: Option<String>,
    pub color: Option<String>,
    pub width: Option<String>,
    pub points: Option<Vec<i64>>,
}

/// A problem with the request: `field` is `strokes` or `strokes[i].<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrokeIssue {
    pub field: String,
    pub issue: IssueKind,
}

fn issue(i: usize, name: &str, issue: IssueKind) -> StrokeIssue {
    StrokeIssue {
        field: format!("strokes[{i}].{name}"),
        issue,
    }
}

/// The blocks of a document a stroke can be anchored to.
#[derive(Debug, Clone, Default)]
pub struct StrokeTargets {
    block_ids: HashSet<String>,
}

impl StrokeTargets {
    /// Reads a stored document (already validated). Only the blocks listed
    /// directly in `blocks` count: a `missing_component` is one block whatever
    /// its `fallback` holds, and nothing nested is a target.
    #[must_use]
    pub fn from_document(document: &Value) -> Self {
        let block_ids = document
            .get("blocks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|b| b.get("id").and_then(Value::as_str))
            .map(str::to_owned)
            .collect();
        Self { block_ids }
    }

    /// Checks the whole set; one bad entry rejects everything. The order of
    /// the drafts is the drawing order and is kept.
    pub fn validate(&self, drafts: &[StrokeDraft]) -> Result<Vec<Stroke>, Vec<StrokeIssue>> {
        if drafts.len() > MAX_STROKES {
            return Err(vec![StrokeIssue {
                field: "strokes".to_owned(),
                issue: IssueKind::TooMany,
            }]);
        }
        let mut issues = Vec::new();
        let mut strokes = Vec::with_capacity(drafts.len());
        let mut seen: HashSet<String> = HashSet::new();
        let mut total_points = 0usize;
        for (i, d) in drafts.iter().enumerate() {
            let before = issues.len();

            let id = d.id.as_deref().filter(|id| is_uuid(id));
            match id {
                None => issues.push(issue(i, "id", IssueKind::Invalid)),
                // The same UUID in another case is the same id.
                Some(id) if !seen.insert(id.to_ascii_lowercase()) => {
                    issues.push(issue(i, "id", IssueKind::Duplicate));
                }
                Some(_) => {}
            }

            let block = d
                .block_id
                .as_deref()
                .filter(|b| self.block_ids.contains(*b));
            if block.is_none() {
                issues.push(issue(i, "block_id", IssueKind::Invalid));
            }
            let color = d.color.as_deref().map(str::parse::<StrokeColor>);
            if !matches!(color, Some(Ok(_))) {
                issues.push(issue(i, "color", IssueKind::Invalid));
            }
            let width = d.width.as_deref().map(str::parse::<StrokeWidth>);
            if !matches!(width, Some(Ok(_))) {
                issues.push(issue(i, "width", IssueKind::Invalid));
            }

            let points = match d.points.as_deref() {
                Some(p) if !p.is_empty() && p.len() % 2 == 0 => {
                    total_points = total_points.saturating_add(p.len() / 2);
                    if p.len() / 2 > MAX_POINTS_PER_STROKE {
                        issues.push(issue(i, "points", IssueKind::TooMany));
                        None
                    } else {
                        let ok = in_ranges(p);
                        if ok.is_none() {
                            issues.push(issue(i, "points", IssueKind::OutOfRange));
                        }
                        ok
                    }
                }
                _ => {
                    issues.push(issue(i, "points", IssueKind::Invalid));
                    None
                }
            };

            if issues.len() == before
                && let (Some(id), Some(block_id), Some(Ok(color)), Some(Ok(width)), Some(points)) =
                    (id, block, color, width, points)
            {
                strokes.push(Stroke {
                    id: id.to_owned(),
                    block_id: block_id.to_owned(),
                    color,
                    width,
                    points,
                });
            }
        }
        if total_points > MAX_POINTS_TOTAL {
            issues.push(StrokeIssue {
                field: "strokes".to_owned(),
                issue: IssueKind::TooManyPoints,
            });
        }
        if issues.is_empty() {
            Ok(strokes)
        } else {
            Err(issues)
        }
    }
}

/// Canonical form only: 36 characters with hyphens (the `uuid` crate would
/// also take the simple, braced and URN spellings).
fn is_uuid(s: &str) -> bool {
    s.len() == 36 && uuid::Uuid::parse_str(s).is_ok()
}

/// The points as `i32` when every coordinate is inside its range.
fn in_ranges(points: &[i64]) -> Option<Vec<i32>> {
    points
        .as_chunks::<2>()
        .0
        .iter()
        .flat_map(|[x, y]| [(*x, X_RANGE), (*y, Y_RANGE)])
        .map(|(v, (lo, hi))| {
            (lo..=hi)
                .contains(&v)
                .then(|| i32::try_from(v).ok())
                .flatten()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const ID_A: &str = "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f10";
    const ID_B: &str = "5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f11";

    fn targets() -> StrokeTargets {
        StrokeTargets::from_document(&json!({"blocks": [
            {"type": "heading", "id": "h1", "text": "T"},
            {"type": "paragraph", "id": "p1", "text": "abc"},
            {"type": "missing_component", "id": "mc", "fallback": [
                {"type": "paragraph", "id": "nested", "text": "x"}
            ]},
        ]}))
    }

    fn d(id: &str, block: &str) -> StrokeDraft {
        StrokeDraft {
            id: Some(id.into()),
            block_id: Some(block.into()),
            color: Some("ink".into()),
            width: Some("medium".into()),
            points: Some(vec![10, 20, 30, 40]),
        }
    }

    fn fields(drafts: &[StrokeDraft]) -> Vec<(String, &'static str)> {
        targets()
            .validate(drafts)
            .unwrap_err()
            .into_iter()
            .map(|i| (i.field, i.issue.as_str()))
            .collect()
    }

    fn one(f: impl FnOnce(&mut StrokeDraft)) -> Vec<(String, &'static str)> {
        let mut s = d(ID_A, "p1");
        f(&mut s);
        fields(&[s])
    }

    fn f(name: &str, issue: &'static str) -> Vec<(String, &'static str)> {
        vec![(name.to_owned(), issue)]
    }

    #[test]
    fn a_good_set_is_kept_in_the_order_given_on_any_top_level_block() {
        let set = [d(ID_B, "mc"), d(ID_A, "h1"), d(&ID_A.to_uppercase(), "p1")];
        // The third repeats the second's id in another case.
        assert_eq!(fields(&set), f("strokes[2].id", "duplicate"));
        let ok = targets()
            .validate(&[
                d(ID_B, "mc"),
                d(ID_A, "h1"),
                d("5D7E2B1C-9A40-4F0E-8B63-1C2A7D9E4F12", "p1"),
            ])
            .unwrap();
        let ids: Vec<_> = ok.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids[..2], [ID_B, ID_A]);
        assert_eq!(
            ids[2], "5D7E2B1C-9A40-4F0E-8B63-1C2A7D9E4F12",
            "kept as sent"
        );
        assert_eq!(ok[0].points, [10, 20, 30, 40]);
        assert_eq!(targets().validate(&[]).unwrap(), Vec::new());
    }

    #[test]
    fn id_rules() {
        assert_eq!(one(|s| s.id = None), f("strokes[0].id", "invalid"));
        for bad in [
            "",
            "nope",
            "5d7e2b1c9a404f0e8b631c2a7d9e4f10",
            "{5d7e2b1c-9a40-4f0e-8b63-1c2a7d9e4f10}",
        ] {
            assert_eq!(
                one(|s| s.id = Some(bad.into())),
                f("strokes[0].id", "invalid"),
                "{bad}"
            );
        }
    }

    #[test]
    fn block_must_be_a_top_level_block() {
        for bad in [None, Some("zz"), Some("nested")] {
            assert_eq!(
                one(|s| s.block_id = bad.map(str::to_owned)),
                f("strokes[0].block_id", "invalid"),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn color_and_width_must_be_known() {
        assert_eq!(one(|s| s.color = None), f("strokes[0].color", "invalid"));
        assert_eq!(
            one(|s| s.color = Some("pink".into())),
            f("strokes[0].color", "invalid")
        );
        assert_eq!(one(|s| s.width = None), f("strokes[0].width", "invalid"));
        assert_eq!(
            one(|s| s.width = Some("huge".into())),
            f("strokes[0].width", "invalid")
        );
    }

    #[test]
    fn points_rules() {
        let invalid = f("strokes[0].points", "invalid");
        assert_eq!(one(|s| s.points = None), invalid);
        assert_eq!(one(|s| s.points = Some(vec![])), invalid);
        assert_eq!(one(|s| s.points = Some(vec![1, 2, 3])), invalid);
        let range = f("strokes[0].points", "out_of_range");
        for bad in [[-40_001, 0], [50_001, 0], [0, -40_001], [0, 200_001]] {
            assert_eq!(one(|s| s.points = Some(bad.to_vec())), range, "{bad:?}");
        }
        // The bounds themselves are fine, and so is a single point.
        for ok in [[-40_000, -40_000], [50_000, 200_000]] {
            let mut s = d(ID_A, "p1");
            s.points = Some(ok.to_vec());
            assert!(targets().validate(&[s]).is_ok());
        }
        assert_eq!(one(|s| s.points = Some(vec![i64::MAX, 0])), range);
    }

    #[test]
    fn per_stroke_and_total_limits() {
        let mut s = d(ID_A, "p1");
        s.points = Some(vec![0; 2 * MAX_POINTS_PER_STROKE]);
        assert!(targets().validate(std::slice::from_ref(&s)).is_ok());
        s.points = Some(vec![0; 2 * MAX_POINTS_PER_STROKE + 2]);
        assert_eq!(fields(&[s]), f("strokes[0].points", "too_many"));

        let many: Vec<_> = (0..=MAX_STROKES).map(|_| d(ID_A, "p1")).collect();
        assert_eq!(fields(&many), f("strokes", "too_many"));

        // 201 strokes of 1000 points: over 200 000 in all.
        let big = |n: usize| -> Vec<StrokeDraft> {
            (0..n)
                .map(|i| {
                    let mut s = d(&format!("00000000-0000-4000-8000-{i:012}"), "p1");
                    s.points = Some(vec![0; 2 * MAX_POINTS_PER_STROKE]);
                    s
                })
                .collect()
        };
        assert!(targets().validate(&big(200)).is_ok());
        assert_eq!(fields(&big(201)), f("strokes", "too_many_points"));
    }

    #[test]
    fn one_bad_entry_rejects_all_and_each_is_reported() {
        let mut bad = d(ID_B, "zz");
        bad.width = None;
        assert_eq!(
            fields(&[d(ID_A, "p1"), bad]),
            vec![
                ("strokes[1].block_id".to_owned(), "invalid"),
                ("strokes[1].width".to_owned(), "invalid"),
            ]
        );
    }
}
