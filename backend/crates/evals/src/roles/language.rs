//! The `language` check of the Generator: is the lesson written in the
//! language asked for (`it` or `en`)?
//!
//! Deterministic and dependency-free. The lesson is cut into units of text
//! (a paragraph, a callout, a question, an option, a feedback, a glossary
//! definition); a unit with enough words fails when it has more English than
//! Italian function words. English technical terms inside an Italian
//! sentence do not tip the balance: the sentence is made of Italian glue.
//! The check only warns: a heuristic must not block a run.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::sdui::{self, kind, text};
use domain::ContentLanguage;

use super::shape::foreign_language;
use crate::checks::Check;

/// A unit shorter than this is too small to tell a language from.
const MIN_WORDS: usize = 8;
/// Share of the checked words in failed units above which the check warns.
const MAX_SHARE: f64 = 0.20;

// Ambiguous words (`a`, `e`, `in`, `me`, `do`, `come`) are left out of both.
const IT: &[&str] = &[
    "di", "il", "la", "lo", "le", "gli", "che", "per", "un", "una", "non", "è", "con", "dei",
    "del", "della", "delle", "nel", "nella", "sono", "si", "più", "ma", "se", "da", "al", "anche",
    "cosa", "questo", "questa", "quando", "perché", "ogni", "può", "ha", "hai", "ti", "tra",
];
const EN: &[&str] = &[
    "the", "of", "and", "to", "is", "are", "that", "this", "with", "for", "as", "you", "it", "be",
    "can", "which", "when", "what", "from", "by", "or", "an", "not", "have", "will", "your", "was",
    "its",
];

static STRIP: LazyLock<Regex> = LazyLock::new(|| {
    // inline code, `[[key]]` / `[[key|shown]]`, `[^key]`, link targets, URLs
    Regex::new(r"`[^`\n]*`|\[\[[^\]]*\]\]|\[\^[^\]]*\]|\]\([^)\s]*\)|https?://\S+|www\.\S+")
        .expect("static regex")
});

/// The units of prose of the document, markup and code removed.
fn units(doc: &Value, language: ContentLanguage) -> Vec<String> {
    let mut raw: Vec<&str> = Vec::new();
    for b in sdui::all(doc) {
        if kind(b) == "code" || foreign_language(b, language) {
            continue;
        }
        match kind(b) {
            "heading" => raw.push(text(b, "text")),
            _ => raw.extend(sdui::markdown_fields(b)),
        }
        if let Some(e) = b.get("explanation").and_then(Value::as_str) {
            raw.push(e);
        }
        for o in b
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(e) = o.get("explanation").and_then(Value::as_str) {
                raw.push(e);
            }
        }
    }
    for g in doc
        .get("glossary")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if !foreign_language(g, language) {
            raw.push(text(g, "definition"));
        }
    }
    let figures: Vec<String> = sdui::all(doc)
        .into_iter()
        .filter(|b| !foreign_language(b, language))
        .flat_map(sdui::diagram_texts)
        .collect();
    raw.into_iter()
        .chain(figures.iter().map(String::as_str))
        .map(|t| STRIP.replace_all(t, " ").replace(['*', '_'], ""))
        .collect()
}

fn tokens(unit: &str) -> Vec<String> {
    unit.to_lowercase()
        .split(|c: char| !c.is_alphabetic() && c != '\'')
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `(checked words, words in units that read as the other language)`.
fn measure(doc: &Value, language: ContentLanguage) -> (usize, usize) {
    let (mut checked, mut wrong) = (0, 0);
    for unit in units(doc, language) {
        let words = unit.split_whitespace().count();
        if words < MIN_WORDS {
            continue;
        }
        let t = tokens(&unit);
        let it = t.iter().filter(|w| IT.contains(&w.as_str())).count();
        let en = t.iter().filter(|w| EN.contains(&w.as_str())).count();
        checked += words;
        let other_wins = match language {
            ContentLanguage::It => en > it,
            ContentLanguage::En => it > en,
        };
        if other_wins {
            wrong += words;
        }
    }
    (checked, wrong)
}

/// A warning, never a failure.
pub fn check(key: &str, spec: &Value, doc: &Value) -> Check {
    let Some(language) = spec.as_str().and_then(|s| s.parse().ok()) else {
        return Check::skipped(key, "only `it` and `en` are supported");
    };
    let (checked, wrong) = measure(doc, language);
    let share = if checked == 0 {
        0.0
    } else {
        wrong as f64 / checked as f64
    };
    Check::soft(key, share <= MAX_SHARE, || {
        format!(
            "{wrong} of {checked} checked words ({:.0}%) are in units that do not read as {}, max {:.0}%",
            share * 100.0,
            language.english_name(),
            MAX_SHARE * 100.0
        )
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::checks::Status;

    fn doc(paragraphs: &[&str]) -> Value {
        let blocks: Vec<Value> = paragraphs
            .iter()
            .enumerate()
            .map(|(i, t)| json!({"id": format!("p{i}"), "type": "paragraph", "text": t}))
            .collect();
        json!({"blocks": blocks})
    }

    fn status(doc: &Value) -> Status {
        check("language", &json!("it"), doc).status
    }

    fn status_en(doc: &Value) -> Status {
        check("language", &json!("en"), doc).status
    }

    #[test]
    fn an_english_lesson_passes_for_en_and_an_italian_one_is_flagged() {
        let en = doc(&[
            "The borrow checker verifies that every reference does not outlive the value that it points to.",
            "With an explicit lifetime you can tell the compiler how long the data is valid in your program.",
        ]);
        let it = doc(&[
            "Il compilatore controlla che ogni riferimento sia valido per tutto il tempo in cui viene usato.",
            "Con un lifetime esplicito puoi dire al compilatore per quanto tempo il data resta valido nel tuo programma.",
        ]);
        assert_eq!(status_en(&en), Status::Pass);
        assert_eq!(status_en(&it), Status::Warn);
        assert_eq!(status(&it), Status::Pass);
    }

    #[test]
    fn for_en_a_block_in_italian_is_foreign_and_skipped() {
        let d = json!({"blocks": [
            {"id": "q", "type": "paragraph", "lang": "it",
             "text": "Il compilatore controlla che ogni riferimento sia valido per tutto il tempo in cui viene usato."},
            {"id": "p", "type": "paragraph",
             "text": "The compiler checks that every reference is valid for the whole time that it is used."}
        ]});
        assert_eq!(status_en(&d), Status::Pass);
        // The same blocks for Italian: the English one has no `lang`, so it counts.
        assert_eq!(status(&d), Status::Warn);
    }

    #[test]
    fn an_unknown_language_is_skipped() {
        assert_eq!(
            check("language", &json!("fr"), &doc(&[])).status,
            Status::Skipped
        );
    }

    #[test]
    fn an_italian_lesson_with_english_terms_passes() {
        let d = doc(&[
            "Il borrow checker verifica che ogni reference non sopravviva al valore a cui punta, e lo fa a compile time.",
            "Con un lifetime esplicito puoi dire al compilatore per quanto tempo il data resta valido nel tuo programma.",
        ]);
        assert_eq!(status(&d), Status::Pass);
    }

    #[test]
    fn the_text_of_a_diagram_counts_for_the_language() {
        let en = json!({"id": "f", "type": "diagram", "layout": "flow",
            "nodes": [{"id": "a", "label": "The first step of the process that you can run"},
                      {"id": "b", "label": "The second step is what you have when it is done"}],
            "caption": "This is the figure that you can see with the steps of the process",
            "alt": "The two steps are connected by an arrow, and this is what the figure shows"});
        let mut d = doc(&[]);
        d["blocks"] = json!([en]);
        // A short document: below the failing share, but flagged.
        assert_eq!(status(&d), Status::Warn);
    }

    #[test]
    fn an_english_lesson_is_flagged() {
        let d = doc(&[
            "The borrow checker verifies that every reference does not outlive the value that it points to.",
            "With an explicit lifetime you can tell the compiler how long the data is valid in your program.",
        ]);
        assert_eq!(status(&d), Status::Warn);
    }

    #[test]
    fn code_links_citations_and_foreign_blocks_are_ignored() {
        let d = json!({"blocks": [
            {"id": "c", "type": "code", "language": "rust",
             "code": "the of and to is that this with for as you it be can which"},
            {"id": "q", "type": "paragraph", "lang": "en",
             "text": "The borrow checker verifies that every reference does not outlive the value."},
            {"id": "p", "type": "paragraph",
             "text": "Il [[borrow_checker]] controlla ogni `reference` che non vive abbastanza a lungo [^s1] vedi https://the.of.and/to/is/that/this/with."}
        ]});
        assert_eq!(status(&d), Status::Pass);
    }

    #[test]
    fn short_units_are_not_judged_and_glossary_definitions_are() {
        assert_eq!(status(&doc(&["The cat is on the table."])), Status::Pass);
        let d = json!({"blocks": [], "glossary": [
            {"key": "k", "term": "Lifetime",
             "definition": "The span of the program in which a reference is valid and that the compiler tracks."}
        ]});
        assert_eq!(status(&d), Status::Warn);
    }

    #[test]
    fn a_minority_of_english_units_is_tolerated() {
        let it = "Il compilatore controlla che ogni riferimento sia valido per tutto il tempo in cui viene usato.";
        let en = "The compiler checks that every reference is valid for the whole time that it is used by you.";
        let d = doc(&[it, it, it, it, it, en]);
        assert_eq!(status(&d), Status::Pass);
    }
}
