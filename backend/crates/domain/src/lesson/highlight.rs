//! The learner's highlights on a lesson text (`docs/contracts/lessons.md` §10).
//!
//! A highlight is anchored to the *source* of the `text` field of a block (the
//! inline markdown as stored in the document), in UTF-16 code units: that is
//! the only text backend and app share, and what Android's `String` counts.

use std::collections::HashMap;
use std::str::FromStr;

use serde_json::Value;

use crate::validation::IssueKind;

/// Most highlights one lesson can have.
pub const MAX_HIGHLIGHTS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
}

impl HighlightColor {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
        }
    }
}

impl FromStr for HighlightColor {
    type Err = IssueKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "yellow" => Ok(Self::Yellow),
            "green" => Ok(Self::Green),
            "blue" => Ok(Self::Blue),
            "pink" => Ok(Self::Pink),
            _ => Err(IssueKind::Invalid),
        }
    }
}

/// A validated highlight: `[start, end)` in UTF-16 code units of the block's
/// `text`, with `start < end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highlight {
    pub block_id: String,
    pub start: u32,
    pub end: u32,
    pub color: HighlightColor,
}

/// One entry of a request before any check. `None` means missing or of the
/// wrong JSON type.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HighlightDraft {
    pub block_id: Option<String>,
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub color: Option<String>,
}

/// A problem with the request: `field` is `highlights` or
/// `highlights[i].<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightIssue {
    pub field: String,
    pub issue: IssueKind,
}

fn issue(i: usize, name: &str, issue: IssueKind) -> HighlightIssue {
    HighlightIssue {
        field: format!("highlights[{i}].{name}"),
        issue,
    }
}

/// The blocks of a document that can be highlighted, in document order.
#[derive(Debug, Clone, Default)]
pub struct HighlightTargets {
    /// (block id, length of `text` in UTF-16 code units).
    blocks: Vec<(String, usize)>,
}

impl HighlightTargets {
    /// Reads a stored document (already validated). Looks into the `fallback`
    /// of a `missing_component` too.
    #[must_use]
    pub fn from_document(document: &Value) -> Self {
        let mut blocks = Vec::new();
        if let Some(list) = document.get("blocks") {
            collect(list, &mut blocks);
        }
        Self { blocks }
    }

    fn position(&self, block_id: &str) -> Option<usize> {
        self.blocks.iter().position(|(id, _)| id == block_id)
    }

    /// Checks the whole set; one bad entry rejects everything. On success the
    /// highlights are sorted by block position in the document, then `start`.
    pub fn validate(
        &self,
        drafts: &[HighlightDraft],
    ) -> Result<Vec<Highlight>, Vec<HighlightIssue>> {
        if drafts.len() > MAX_HIGHLIGHTS {
            return Err(vec![HighlightIssue {
                field: "highlights".to_owned(),
                issue: IssueKind::TooMany,
            }]);
        }
        let mut issues = Vec::new();
        // (index in the request, block position, highlight)
        let mut valid: Vec<(usize, usize, Highlight)> = Vec::new();
        for (i, d) in drafts.iter().enumerate() {
            let before = issues.len();
            let block = d
                .block_id
                .as_deref()
                .and_then(|id| self.position(id).map(|p| (p, id)));
            if block.is_none() {
                issues.push(issue(i, "block_id", IssueKind::Invalid));
            }
            let color = d.color.as_deref().map(str::parse::<HighlightColor>);
            if !matches!(color, Some(Ok(_))) {
                issues.push(issue(i, "color", IssueKind::Invalid));
            }
            // Numbers: absent or not an integer is malformed; an integer out
            // of the text's bounds is `out_of_range`.
            let len = block.map(|(p, _)| self.blocks[p].1);
            let start = bound(d.start, len);
            let end = bound(d.end, len);
            for (name, r) in [("start", &start), ("end", &end)] {
                if let Err(kind) = r {
                    issues.push(issue(i, name, *kind));
                }
            }
            if let (Ok(Some(s)), Ok(Some(e))) = (&start, &end)
                && s >= e
            {
                issues.push(issue(i, "start", IssueKind::OutOfRange));
            }
            if issues.len() == before
                && let (Some((p, id)), Some(Ok(color)), Ok(Some(s)), Ok(Some(e))) =
                    (block, color, start, end)
            {
                valid.push((
                    i,
                    p,
                    Highlight {
                        block_id: id.to_owned(),
                        start: s,
                        end: e,
                        color,
                    },
                ));
            }
        }
        if !issues.is_empty() {
            return Err(issues);
        }

        valid.sort_by_key(|(_, p, h)| (*p, h.start, h.end));
        // Touching is fine; overlapping is reported on the second of the two.
        let mut furthest: HashMap<usize, u32> = HashMap::new();
        let mut overlaps: Vec<usize> = Vec::new();
        for (i, p, h) in &valid {
            let reach = furthest.entry(*p).or_insert(0);
            if h.start < *reach {
                overlaps.push(*i);
            }
            *reach = (*reach).max(h.end);
        }
        if !overlaps.is_empty() {
            overlaps.sort_unstable();
            return Err(overlaps
                .into_iter()
                .map(|i| issue(i, "start", IssueKind::Overlap))
                .collect());
        }
        Ok(valid.into_iter().map(|(_, _, h)| h).collect())
    }
}

/// `Ok(None)` when the value is fine but the block is unknown (no bound to
/// check against).
fn bound(value: Option<i64>, len: Option<usize>) -> Result<Option<u32>, IssueKind> {
    let v = value.ok_or(IssueKind::Invalid)?;
    let Some(len) = len else {
        return Ok(None);
    };
    u32::try_from(v)
        .ok()
        .filter(|v| (*v as usize) <= len)
        .map(Some)
        .ok_or(IssueKind::OutOfRange)
}

fn collect(blocks: &Value, out: &mut Vec<(String, usize)>) {
    let Some(list) = blocks.as_array() else {
        return;
    };
    for block in list {
        let ty = block.get("type").and_then(Value::as_str);
        if matches!(ty, Some("paragraph" | "callout"))
            && let (Some(id), Some(text)) = (
                block.get("id").and_then(Value::as_str),
                block.get("text").and_then(Value::as_str),
            )
        {
            out.push((id.to_owned(), text.encode_utf16().count()));
        }
        if let Some(fallback) = block.get("fallback") {
            collect(fallback, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn targets() -> HighlightTargets {
        HighlightTargets::from_document(&json!({"blocks": [
            {"type": "heading", "id": "h1", "text": "Titolo"},
            {"type": "paragraph", "id": "p1", "text": "0123456789"},
            {"type": "missing_component", "id": "mc", "fallback": [
                {"type": "callout", "id": "c1", "title": "T", "text": "ab😀cd"}
            ]},
            {"type": "paragraph", "id": "p2", "text": "xyz"},
        ]}))
    }

    fn d(block: &str, start: i64, end: i64) -> HighlightDraft {
        HighlightDraft {
            block_id: Some(block.into()),
            start: Some(start),
            end: Some(end),
            color: Some("yellow".into()),
        }
    }

    fn fields(r: Result<Vec<Highlight>, Vec<HighlightIssue>>) -> Vec<(String, &'static str)> {
        r.unwrap_err()
            .into_iter()
            .map(|i| (i.field, i.issue.as_str()))
            .collect()
    }

    #[test]
    fn orders_by_document_position_then_start() {
        let out = targets()
            .validate(&[d("p2", 0, 1), d("c1", 0, 2), d("p1", 5, 7), d("p1", 0, 5)])
            .unwrap();
        let got: Vec<_> = out.iter().map(|h| (h.block_id.as_str(), h.start)).collect();
        assert_eq!(got, [("p1", 0), ("p1", 5), ("c1", 0), ("p2", 0)]);
    }

    #[test]
    fn touching_intervals_are_accepted_and_empty_set_too() {
        assert_eq!(
            targets()
                .validate(&[d("p1", 0, 3), d("p1", 3, 10)])
                .unwrap()
                .len(),
            2
        );
        assert_eq!(targets().validate(&[]).unwrap(), Vec::new());
    }

    #[test]
    fn limits_are_in_utf16_units() {
        // "ab😀cd" is 5 code points but 6 UTF-16 units.
        assert!(targets().validate(&[d("c1", 0, 6)]).is_ok());
        assert_eq!(
            fields(targets().validate(&[d("c1", 0, 7)])),
            [("highlights[0].end".to_owned(), "out_of_range")]
        );
        assert!(targets().validate(&[d("p1", 0, 10)]).is_ok());
    }

    #[test]
    fn reports_each_error_of_the_contract() {
        let t = targets();
        let cases: Vec<(HighlightDraft, &str, &str)> = vec![
            (d("nope", 0, 1), "block_id", "invalid"),
            (d("h1", 0, 1), "block_id", "invalid"),
            (d("mc", 0, 1), "block_id", "invalid"),
            (d("p1", -1, 3), "start", "out_of_range"),
            (d("p1", 0, 11), "end", "out_of_range"),
            (d("p1", 4, 4), "start", "out_of_range"),
            (d("p1", 5, 2), "start", "out_of_range"),
            (
                HighlightDraft {
                    color: Some("red".into()),
                    ..d("p1", 0, 1)
                },
                "color",
                "invalid",
            ),
            (
                HighlightDraft {
                    color: None,
                    ..d("p1", 0, 1)
                },
                "color",
                "invalid",
            ),
            (
                HighlightDraft {
                    start: None,
                    ..d("p1", 0, 1)
                },
                "start",
                "invalid",
            ),
        ];
        for (draft, field, kind) in cases {
            assert_eq!(
                fields(t.validate(std::slice::from_ref(&draft))),
                [(format!("highlights[0].{field}"), kind)],
                "{draft:?}"
            );
        }
    }

    #[test]
    fn overlap_is_reported_on_the_second_of_the_two() {
        let t = targets();
        // Sent out of order: the second by position is index 0.
        assert_eq!(
            fields(t.validate(&[d("p1", 4, 8), d("p1", 0, 5), d("p2", 0, 1)])),
            [("highlights[0].start".to_owned(), "overlap")]
        );
        // The same interval twice, and one inside another.
        assert_eq!(
            fields(t.validate(&[d("p1", 0, 5), d("p1", 0, 5), d("p1", 1, 2)])),
            [
                ("highlights[1].start".to_owned(), "overlap"),
                ("highlights[2].start".to_owned(), "overlap"),
            ]
        );
        // Same range in different blocks is fine.
        assert!(t.validate(&[d("p1", 0, 3), d("p2", 0, 3)]).is_ok());
    }

    #[test]
    fn more_than_the_maximum_is_too_many() {
        let many: Vec<_> = (0..=MAX_HIGHLIGHTS).map(|_| d("p1", 0, 1)).collect();
        assert_eq!(
            fields(targets().validate(&many)),
            [("highlights".to_owned(), "too_many")]
        );
        // Exactly the maximum is fine: a long text with a highlight per unit.
        let long = HighlightTargets::from_document(
            &json!({"blocks": [{"type": "paragraph", "id": "p", "text": "x".repeat(MAX_HIGHLIGHTS)}]}),
        );
        let ok: Vec<_> = (0..MAX_HIGHLIGHTS)
            .map(|i| {
                d(
                    "p",
                    i64::try_from(i).unwrap(),
                    i64::try_from(i).unwrap() + 1,
                )
            })
            .collect();
        assert_eq!(long.validate(&ok).unwrap().len(), MAX_HIGHLIGHTS);
    }
}
