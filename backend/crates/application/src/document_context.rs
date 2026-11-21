//! Text the agents read about the topic's uploaded documents: the index (title,
//! pages, sections) for the Tutor, Researcher and Planner, and the excerpts
//! (the text of the sections the Planner chose) for the Generator.
//! Formats are the ones listed for `llm-engineer`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use domain::document::EXCERPT_MAX_CHARS;
use domain::{DocumentId, DocumentStatus, DocumentView};

use crate::context::one_line;

/// Sections listed per document; a book with hundreds of bookmarks would
/// otherwise take more room than the rest of the prompt.
pub const MAX_SECTIONS_SHOWN: usize = 120;

fn pages_label(start: u32, end: u32) -> String {
    if start == end {
        format!("p. {start}")
    } else {
        format!("pp. {start}-{end}")
    }
}

/// `None` when the topic has no ready document.
///
/// ```text
/// - 01933b4e-… · "Rust in Action" · 456 pagine · en
///   Sezioni:
///   - [s1] 1 Introducing Rust (pp. 1-24)
///     - [s2] 1.1 Why Rust (pp. 3-9)
/// ```
/// `with_ids` adds the document id (as the Planner must write it); the Tutor
/// and the Researcher do not need it.
#[must_use]
pub fn documents_index(views: &[DocumentView], with_ids: bool) -> Option<String> {
    let mut out = Vec::new();
    for view in views
        .iter()
        .filter(|v| v.document.status == DocumentStatus::Ready)
    {
        let d = &view.document;
        let mut head = String::from("- ");
        if with_ids {
            let _ = write!(head, "{} · ", d.id);
        }
        let _ = write!(head, "\"{}\"", one_line(&d.title));
        if let Some(n) = d.page_count {
            let _ = write!(head, " · {n} pagine");
        }
        if let Some(lang) = &d.language {
            let _ = write!(head, " · {lang}");
        }
        out.push(head);
        out.push("  Sezioni:".to_owned());
        for s in view.sections.iter().take(MAX_SECTIONS_SHOWN) {
            let indent = "  ".repeat(usize::from(s.level));
            let id = if with_ids {
                format!("[{}] ", s.id)
            } else {
                String::new()
            };
            out.push(format!(
                "{indent}- {id}{} ({})",
                one_line(&s.title),
                pages_label(s.page_start, s.page_end)
            ));
        }
        if view.sections.len() > MAX_SECTIONS_SHOWN {
            out.push(format!(
                "  (altre {} sezioni omesse)",
                view.sections.len() - MAX_SECTIONS_SHOWN
            ));
        }
    }
    (!out.is_empty()).then(|| out.join("\n"))
}

/// What a lesson covered of the uploaded documents, from its stored plan:
/// `"Libro" 1 Intro (pp. 1-24); …`. `None` when it used none (or the
/// documents are gone).
#[must_use]
pub fn sections_used(plan: &serde_json::Value, documents: &[DocumentView]) -> Option<String> {
    let mut parts = Vec::new();
    for entry in plan.get("document_sections")?.as_array()? {
        let id = entry
            .get("document_id")?
            .as_str()
            .and_then(domain::parse_uuid);
        let Some(view) = documents
            .iter()
            .find(|v| id == Some(v.document.id.as_uuid()))
        else {
            continue;
        };
        for sid in entry
            .get("section_ids")?
            .as_array()?
            .iter()
            .filter_map(|s| s.as_str())
        {
            if let Some(s) = view.sections.iter().find(|s| s.id == sid) {
                parts.push(format!(
                    "\"{}\" {} ({})",
                    one_line(&view.document.title),
                    one_line(&s.title),
                    pages_label(s.page_start, s.page_end)
                ));
            }
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// A document a lesson was built from (`Lesson.document_sources`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSource {
    pub document_id: DocumentId,
    pub title: String,
    pub page_start: u32,
    pub page_end: u32,
}

/// The documents a lesson followed, in plan order, with the page span of the
/// sections chosen. A document that is gone (or a section that no longer
/// exists) is absent.
#[must_use]
pub fn document_sources(
    plan: &serde_json::Value,
    documents: &[DocumentView],
) -> Vec<DocumentSource> {
    let mut out: Vec<DocumentSource> = Vec::new();
    let Some(entries) = plan.get("document_sections").and_then(|v| v.as_array()) else {
        return out;
    };
    for entry in entries {
        let Some(id) = entry
            .get("document_id")
            .and_then(|v| v.as_str())
            .and_then(domain::parse_uuid)
        else {
            continue;
        };
        let Some(view) = documents.iter().find(|v| v.document.id.as_uuid() == id) else {
            continue;
        };
        let sections = entry
            .get("section_ids")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str())
            .filter_map(|sid| view.sections.iter().find(|s| s.id == sid));
        for s in sections {
            match out.iter_mut().find(|o| o.document_id == view.document.id) {
                Some(o) => {
                    o.page_start = o.page_start.min(s.page_start);
                    o.page_end = o.page_end.max(s.page_end);
                }
                None => out.push(DocumentSource {
                    document_id: view.document.id,
                    title: view.document.title.clone(),
                    page_start: s.page_start,
                    page_end: s.page_end,
                }),
            }
        }
    }
    out
}

/// One section the Planner chose, with the text of its pages.
#[derive(Debug, Clone)]
pub struct ExcerptSection {
    pub title: String,
    pub page_start: u32,
    pub page_end: u32,
}

/// One document of the plan with its chosen sections (in the order of the
/// plan) and the text of the pages they cover.
#[derive(Debug, Clone)]
pub struct ExcerptDocument {
    pub title: String,
    pub sections: Vec<ExcerptSection>,
    pub pages: BTreeMap<u32, String>,
}

/// Page text as the prompt shows it: line breaks kept (code needs them), no
/// control characters, no runs of blank lines.
fn tidy(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    let mut out = String::with_capacity(cleaned.len());
    let mut blank = 0;
    for line in cleaned.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
            out.push('\n');
        } else {
            blank = 0;
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.trim().to_owned()
}

fn span(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

fn ranges(pages: &BTreeSet<u32>) -> String {
    let mut parts = Vec::new();
    let mut iter = pages.iter().copied();
    let Some(mut start) = iter.next() else {
        return String::new();
    };
    let mut end = start;
    for p in iter {
        if p == end + 1 {
            end = p;
        } else {
            parts.push(span(start, end));
            start = p;
            end = p;
        }
    }
    parts.push(span(start, end));
    parts.join(", ")
}

/// The text of the chosen sections, at most [`EXCERPT_MAX_CHARS`] characters
/// in all, headers and the note about omitted pages included: pages are taken
/// in order and the first one that does not fit ends the text (never in the
/// middle of a page), with a note of the pages left out. A page that is in two
/// sections is shown once.
#[must_use]
pub fn excerpts_section(documents: &[ExcerptDocument]) -> String {
    if documents.is_empty() {
        return "Nessuno.".to_owned();
    }
    // The note depends on what is left out, so the text is built, measured,
    // and built again with less room until the whole of it fits.
    let mut budget = EXCERPT_MAX_CHARS;
    loop {
        let text = build_excerpts(documents, budget);
        let over = text.chars().count().saturating_sub(EXCERPT_MAX_CHARS);
        if over == 0 || budget == 0 {
            return text;
        }
        budget = budget.saturating_sub(over);
    }
}

/// Titles in the omission note are cut: it must stay short.
const NOTE_TITLE_CHARS: usize = 60;

fn build_excerpts(documents: &[ExcerptDocument], budget: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    let mut included_any = false;
    let mut full = false;
    let mut notes = Vec::new();
    for doc in documents {
        let mut shown: BTreeSet<u32> = BTreeSet::new();
        let mut omitted: BTreeSet<u32> = BTreeSet::new();
        let head = format!("### Documento \"{}\"\n", one_line(&doc.title));
        used += head.chars().count();
        out.push_str(&head);
        for section in &doc.sections {
            let head = format!(
                "\n#### {} ({})\n",
                one_line(&section.title),
                pages_label(section.page_start, section.page_end)
            );
            used += head.chars().count();
            out.push_str(&head);
            for page in section.page_start..=section.page_end {
                if shown.contains(&page) || omitted.contains(&page) {
                    continue;
                }
                let Some(text) = doc.pages.get(&page) else {
                    continue;
                };
                let text = tidy(text);
                let label = format!("[p. {page}]\n");
                let cost = label.chars().count() + text.chars().count() + 2;
                if full || (used + cost > budget && included_any) {
                    full = true;
                    omitted.insert(page);
                    continue;
                }
                // A single page longer than the whole budget: cut it, rather
                // than send nothing.
                let text: String = if used + cost > budget {
                    text.chars()
                        .take(budget.saturating_sub(used + label.chars().count() + 2))
                        .collect()
                } else {
                    text
                };
                used += label.chars().count() + text.chars().count() + 2;
                included_any = true;
                shown.insert(page);
                let _ = writeln!(out, "{label}{text}\n");
            }
        }
        if !omitted.is_empty() {
            notes.push(format!(
                "\"{}\": pagine {}",
                one_line(&doc.title)
                    .chars()
                    .take(NOTE_TITLE_CHARS)
                    .collect::<String>(),
                ranges(&omitted)
            ));
        }
    }
    if !notes.is_empty() {
        let _ = write!(
            out,
            "(Pagine omesse perché il testo supera il limite: {}. Non dire cosa contengono.)",
            notes.join("; ")
        );
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{Document, DocumentId, DocumentSection, TopicId};
    use time::macros::datetime;
    use uuid::Uuid;

    fn view(status: DocumentStatus) -> DocumentView {
        let mut document = Document::importing(
            DocumentId::new(Uuid::from_u128(7)),
            TopicId::new(Uuid::from_u128(1)),
            "rust.pdf".into(),
            10,
            datetime!(2026-11-09 10:00 UTC),
        );
        document.status = status;
        document.title = "Rust\nin Action".into();
        document.page_count = Some(456);
        document.language = Some("en".into());
        let section = |id: &str, title: &str, level, a, b| DocumentSection {
            id: id.into(),
            title: title.into(),
            level,
            page_start: a,
            page_end: b,
        };
        DocumentView {
            document,
            sections: vec![
                section("s1", "1 Intro", 1, 1, 24),
                section("s2", "1.1 Why", 2, 3, 9),
            ],
        }
    }

    #[test]
    fn the_index_lists_ready_documents_only_with_or_without_ids() {
        assert_eq!(
            documents_index(&[view(DocumentStatus::Importing)], true),
            None
        );
        assert_eq!(documents_index(&[], false), None);
        let with = documents_index(&[view(DocumentStatus::Ready)], true).unwrap();
        assert_eq!(
            with,
            "- 00000000-0000-0000-0000-000000000007 · \"Rust in Action\" · 456 pagine · en\n  \
             Sezioni:\n  - [s1] 1 Intro (pp. 1-24)\n    - [s2] 1.1 Why (pp. 3-9)"
        );
        let without = documents_index(&[view(DocumentStatus::Ready)], false).unwrap();
        assert!(without.starts_with("- \"Rust in Action\" · 456 pagine · en\n"));
        assert!(without.contains("  - 1 Intro (pp. 1-24)"));
    }

    #[test]
    fn a_long_index_is_cut_with_a_note() {
        let mut v = view(DocumentStatus::Ready);
        v.sections = (0..130)
            .map(|i| DocumentSection {
                id: format!("s{i}"),
                title: "t".into(),
                level: 1,
                page_start: 1,
                page_end: 1,
            })
            .collect();
        let text = documents_index(&[v], true).unwrap();
        assert!(text.ends_with("(altre 10 sezioni omesse)"));
    }

    fn doc(pages: &[(u32, usize)], sections: &[(&str, u32, u32)]) -> ExcerptDocument {
        ExcerptDocument {
            title: "Libro".into(),
            sections: sections
                .iter()
                .map(|(t, a, b)| ExcerptSection {
                    title: (*t).into(),
                    page_start: *a,
                    page_end: *b,
                })
                .collect(),
            pages: pages
                .iter()
                .map(|(p, n)| (*p, format!("riga {p} ").repeat(*n / 8 + 1)))
                .collect(),
        }
    }

    #[test]
    fn excerpts_fit_in_the_budget_and_cut_at_the_end_of_a_page() {
        // Ten pages of about 5000 characters: four fit in 24000, the fifth does not.
        let pages: Vec<(u32, usize)> = (1..=10).map(|p| (p, 6000)).collect();
        let text = excerpts_section(&[doc(&pages, &[("Cap 1", 1, 10)])]);
        assert!(
            text.chars().count() <= EXCERPT_MAX_CHARS + 600,
            "{}",
            text.len()
        );
        assert!(text.contains("[p. 4]") && !text.contains("[p. 5]"));
        assert!(
            text.contains("pagine 5-10"),
            "{}",
            &text[text.len() - 200..]
        );
        // The page that was cut is not half there.
        assert!(text.contains("riga 4 "));
    }

    #[test]
    fn short_text_has_no_omission_note_and_shared_pages_appear_once() {
        let pages: Vec<(u32, usize)> = (1..=6).map(|p| (p, 100)).collect();
        let text = excerpts_section(&[doc(&pages, &[("Cap", 1, 6), ("Sotto", 3, 4)])]);
        assert!(!text.contains("omesse"));
        assert_eq!(text.matches("[p. 3]").count(), 1);
        assert!(text.contains("#### Sotto (pp. 3-4)"));
        assert_eq!(excerpts_section(&[]), "Nessuno.");
    }

    #[test]
    fn headers_and_the_omission_note_count_toward_the_limit() {
        for size in (500..9000).step_by(137) {
            let pages: Vec<(u32, usize)> = (1..=40).map(|p| (p, size)).collect();
            let d = doc(
                &pages,
                &[
                    ("A", 1, 10),
                    ("Sezione con un titolo piuttosto lungo", 11, 40),
                ],
            );
            let text = excerpts_section(&[d]);
            let n = text.chars().count();
            assert!(n <= EXCERPT_MAX_CHARS, "size {size}: {n}");
            assert!(n > EXCERPT_MAX_CHARS - 2 * size - 600 || !text.contains("omesse"));
        }
    }

    #[test]
    fn one_huge_page_is_cut_rather_than_dropped() {
        let pages = [(1u32, 40_000usize)];
        let text = excerpts_section(&[doc(&pages, &[("Cap", 1, 1)])]);
        assert!(text.contains("[p. 1]"));
        assert!(
            text.chars().count() <= EXCERPT_MAX_CHARS,
            "{}",
            text.chars().count()
        );
    }

    #[test]
    fn page_text_keeps_line_breaks_but_not_control_characters_or_blank_runs() {
        assert_eq!(tidy("a  \n\n\n\nb\u{0}\tc\r\n"), "a\n\nb\tc");
    }
}
