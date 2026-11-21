//! Quality rules on a structurally valid lesson (`docs/design/quality-warnings.md`).
//!
//! Two blocking errors (`fill_blank_answer_in_text`, `quiz_option_markup_uneven`)
//! run inside `LessonValidator::validate`; the non-blocking warnings are
//! produced by `LessonValidator::quality_warnings`. Pure logic over the JSON
//! value, like the rest of the Validator. Only top-level blocks are examined:
//! blocks nested in a `missing_component` fallback are never checked nor
//! counted (§0 of the design).

use serde_json::Value;

use super::model::{LessonErrorCode, LessonIssue, show};
use crate::ContentLanguage;

const MIN_ANSWER_CHARS: usize = 3;
const MAX_RUN_WORDS: usize = 350;
const MAX_CALLOUTS_PER_SECTION: usize = 1;
const MAX_TIP_INFO: usize = 2;
const MAX_WARNING_CALLOUTS: usize = 5;
const MAX_ANSWER_WORDS: usize = 3;
const COMPARE_WORDS: [&str; 9] = [
    "invece",
    "rispetto",
    "confronto",
    "mentre",
    "contro",
    "vs",
    "go",
    "prima",
    "dopo",
];
const EXERCISES: [&str; 6] = [
    "quiz_single",
    "quiz_multi",
    "fill_blank",
    "order_steps",
    "open_answer",
    "flashcard",
];

/// Stable codes of the non-blocking quality warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualityWarningCode {
    ExplanationRunTooLong,
    TooManyCallouts,
    FillBlankAnswerTooLong,
    ConsecutiveCodeBlocks,
}

impl QualityWarningCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplanationRunTooLong => "explanation_run_too_long",
            Self::TooManyCallouts => "too_many_callouts",
            Self::FillBlankAnswerTooLong => "fill_blank_answer_too_long",
            Self::ConsecutiveCodeBlocks => "consecutive_code_blocks",
        }
    }
}

/// A quality warning: `generator_message` is for the repair prompt,
/// `learner_message` is the fixed text shown in the app, in the topic's
/// language (ADR 0019).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityWarning {
    pub code: QualityWarningCode,
    pub block_id: Option<String>,
    pub generator_message: String,
    pub learner_message: String,
}

/// Weighted count used to decide whether a repair improved the lesson:
/// a long explanation run weighs 2, every other warning 1.
#[must_use]
pub fn warning_score(warnings: &[QualityWarning]) -> u32 {
    warnings
        .iter()
        .map(|w| match w.code {
            QualityWarningCode::ExplanationRunTooLong => 2,
            _ => 1,
        })
        .sum()
}

// ------------------------------------------------------------- helpers

fn blocks(doc: &Value) -> Vec<&Value> {
    doc.get("blocks")
        .and_then(Value::as_array)
        .map_or_default(|a| a.iter().collect())
}

fn kind(b: &Value) -> &str {
    b.get("type").and_then(Value::as_str).unwrap_or_default()
}

fn text<'a>(b: &'a Value, key: &str) -> &'a str {
    b.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn id(b: &Value) -> String {
    text(b, "id").to_owned()
}

fn is_exercise(b: &Value) -> bool {
    EXERCISES.contains(&kind(b))
}

fn is_h2(b: &Value) -> bool {
    kind(b) == "heading" && b.get("level").and_then(Value::as_u64) == Some(2)
}

/// Visible text: `[[key|shown]]` reduced to the shown text, citations and
/// emphasis markers removed.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("[[") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        if let Some(j) = after.find("]]") {
            let inner = &after[..j];
            out.push_str(inner.split_once('|').map_or(inner, |(_, shown)| shown));
            rest = &after[j + 2..];
        } else {
            out.push_str(&rest[i..]);
            rest = "";
        }
    }
    out.push_str(rest);
    let mut clean = String::with_capacity(out.len());
    let mut chars = out.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' && chars.peek() == Some(&'^') {
            for n in chars.by_ref() {
                if n == ']' {
                    break;
                }
            }
        } else if c != '*' && c != '`' {
            clean.push(c);
        }
    }
    clean.trim().to_owned()
}

fn strip_inline_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('`') {
        let after = &rest[i + 1..];
        match after.find(['`', '\n']) {
            Some(j) if after[j..].starts_with('`') => {
                out.push_str(&rest[..i]);
                rest = &after[j + 1..];
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Paragraph or callout text, inline code removed.
fn prose_without_code(b: &Value) -> Option<String> {
    match kind(b) {
        "paragraph" => Some(plain(&strip_inline_code(text(b, "text")))),
        "callout" => Some(plain(&strip_inline_code(&format!(
            "{} {}",
            text(b, "title"),
            text(b, "text")
        )))),
        _ => None,
    }
}

/// Paragraph or callout text with inline code kept (it is read by the learner).
fn prose(b: &Value) -> Option<String> {
    match kind(b) {
        "paragraph" => Some(plain(text(b, "text"))),
        "callout" => Some(plain(&format!("{} {}", text(b, "title"), text(b, "text")))),
        _ => None,
    }
}

fn words(t: &str) -> usize {
    t.split_whitespace().count()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whole-word, case-insensitive.
fn contains_answer(text: &str, answer: &str) -> bool {
    let a = answer.trim().to_lowercase();
    if a.is_empty() {
        return false;
    }
    let t = text.to_lowercase();
    let mut from = 0;
    while let Some(i) = t[from..].find(&a) {
        let start = from + i;
        let end = start + a.len();
        let before = t[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after = t[end..].chars().next().is_none_or(|c| !is_word_char(c));
        if before && after {
            return true;
        }
        from = start + t[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn answers(b: &Value) -> Vec<&str> {
    b.get("blanks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|bl| bl.get("answers").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

// ------------------------------------------------------- blocking errors

/// The two blocking quality errors. Call only on a shape-valid document.
#[must_use]
pub(super) fn errors(doc: &Value) -> Vec<LessonIssue> {
    let list = blocks(doc);
    let mut out = Vec::new();
    for (i, b) in list.iter().enumerate() {
        let path = format!("/blocks/{i}");
        match kind(b) {
            "fill_blank" => fill_blank_leak(&list, i, &path, &mut out),
            "quiz_single" | "quiz_multi" => uneven_markup(b, &path, &mut out),
            _ => {}
        }
    }
    out
}

fn fill_blank_leak(list: &[&Value], i: usize, path: &str, out: &mut Vec<LessonIssue>) {
    let b = list[i];
    // (where, text), in the order of the design.
    let mut context: Vec<(&str, String)> = Vec::new();
    if let Some(h) = list[..i].iter().rev().find(|x| kind(x) == "heading") {
        context.push(("titolo precedente", plain(text(h, "text"))));
    }
    if let Some(c) = list[..i].iter().rev().find(|x| kind(x) == "code") {
        context.push((
            "didascalia del codice precedente",
            plain(&strip_inline_code(text(c, "caption"))),
        ));
    }
    context.extend(
        list[..i]
            .iter()
            .rev()
            .filter(|x| !is_exercise(x))
            .filter_map(|x| prose_without_code(x))
            .take(2)
            .map(|t| ("testo precedente", t)),
    );
    if let Some(p) = list.get(i + 1).and_then(|x| prose_without_code(x)) {
        context.push(("testo successivo", p));
    }
    for a in answers(b) {
        if a.trim().chars().count() < MIN_ANSWER_CHARS {
            continue;
        }
        if let Some((dove, _)) = context.iter().find(|(_, t)| contains_answer(t, a)) {
            out.push(LessonIssue::new(
                path,
                LessonErrorCode::FillBlankAnswerInText,
                format!(
                    "La risposta «{}» del completamento compare già nel testo vicino ({dove}). \
                     Riformula il testo, cambia la parola da completare o sposta il \
                     completamento più avanti, così chi studia deve ricordarla.",
                    show(a.trim())
                ),
            ));
        }
    }
}

/// Markup kinds of one option text: code, bold, link.
fn markup_kinds(t: &str) -> [bool; 3] {
    let code = t.matches('`').count() >= 2;
    let bold = ["**", "__"].iter().any(|m| {
        t.find(m)
            .is_some_and(|s| t[s + 2..].find(m).is_some_and(|e| e > 0))
    });
    let link = t
        .find('[')
        .is_some_and(|s| t[s..].find("](").is_some_and(|m| t[s + m..].contains(')')));
    [code, bold, link]
}

fn uneven_markup(b: &Value, path: &str, out: &mut Vec<LessonIssue>) {
    let kinds: Vec<[bool; 3]> = b
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|o| markup_kinds(text(o, "text")))
        .collect();
    let with = kinds.iter().filter(|k| k.iter().any(|x| *x)).count();
    if with == 0 || with == kinds.len() {
        return;
    }
    let names = ["codice inline", "grassetto", "link"];
    let used: Vec<&str> = (0..3)
        .filter(|&n| kinds.iter().any(|k| k[n]))
        .map(|n| names[n])
        .collect();
    out.push(LessonIssue::new(
        path,
        LessonErrorCode::QuizOptionMarkupUneven,
        format!(
            "Nel quiz solo alcune opzioni hanno markup ({}). Usa lo stesso stile in tutte \
             le opzioni o in nessuna.",
            used.join(", ")
        ),
    ));
}

// ------------------------------------------------------------- warnings

/// Learner-facing text in the topic's language. The texts for the model
/// (`generator_message`) are always Italian: they are never shown.
fn localized(lang: ContentLanguage, it: &str, en: &str) -> String {
    match lang {
        ContentLanguage::It => it,
        ContentLanguage::En => en,
    }
    .to_owned()
}

#[must_use]
pub(super) fn warnings(doc: &Value, lang: ContentLanguage) -> Vec<QualityWarning> {
    let list = blocks(doc);
    let mut out = Vec::new();
    explanation_runs(&list, lang, &mut out);
    callouts(&list, lang, &mut out);
    long_answers(&list, lang, &mut out);
    consecutive_code(&list, lang, &mut out);
    // Document order (design §3): the app shows the first six. Warnings of
    // the whole lesson (no block) go last; the sort is stable within a block.
    let position = |w: &QualityWarning| {
        w.block_id
            .as_deref()
            .and_then(|id| list.iter().position(|b| text(b, "id") == id))
            .unwrap_or(usize::MAX)
    };
    out.sort_by_key(position);
    out
}

/// Blocks before the first h2, and the sections as `(title, blocks)`.
fn split_sections<'a>(list: &[&'a Value]) -> (Vec<&'a Value>, Vec<(&'a str, Vec<&'a Value>)>) {
    let mut intro = Vec::new();
    let mut sections: Vec<(&str, Vec<&Value>)> = Vec::new();
    for &b in list {
        if is_h2(b) {
            sections.push((text(b, "text"), Vec::new()));
        } else if let Some(s) = sections.last_mut() {
            s.1.push(b);
        } else {
            intro.push(b);
        }
    }
    (intro, sections)
}

fn explanation_runs(list: &[&Value], lang: ContentLanguage, out: &mut Vec<QualityWarning>) {
    let (_, sections) = split_sections(list);
    let closing = sections.len().checked_sub(1);
    for (n, (title, blocks)) in sections.iter().enumerate() {
        if Some(n) == closing {
            continue;
        }
        let mut run = 0usize;
        let mut first: Option<&Value> = None;
        let mut flush = |run: &mut usize, first: &mut Option<&Value>| {
            if *run > MAX_RUN_WORDS {
                out.push(QualityWarning {
                    code: QualityWarningCode::ExplanationRunTooLong,
                    block_id: first.map(id),
                    generator_message: format!(
                        "Ci sono {run} parole di spiegazione di seguito nella sezione \
                         «{}», senza un esercizio. Inserisci un esercizio a metà o \
                         accorcia a meno di {MAX_RUN_WORDS} parole.",
                        show(title)
                    ),
                    learner_message: localized(
                        lang,
                        "Questa parte è lunga prima del primo esercizio. Fermati a \
                         ripetere a mente i punti chiave prima di andare avanti.",
                        "This part runs long before the first exercise. Stop and recall \
                         the key points in your head before moving on.",
                    ),
                });
            }
            *run = 0;
            *first = None;
        };
        for b in blocks {
            if is_exercise(b) {
                flush(&mut run, &mut first);
            } else if let Some(t) = prose(b) {
                if first.is_none() {
                    first = Some(b);
                }
                run += words(&t);
            }
        }
        flush(&mut run, &mut first);
    }
}

fn callout_warning(block_id: String, detail: &str, lang: ContentLanguage) -> QualityWarning {
    QualityWarning {
        code: QualityWarningCode::TooManyCallouts,
        block_id: Some(block_id),
        generator_message: format!(
            "Troppi callout ({detail}). Tieni al massimo 1 per sezione, 2 tra suggerimenti e \
             info in tutta la lezione, e un solo avviso per concetto; trasforma gli altri in \
             paragrafo."
        ),
        learner_message: localized(
            lang,
            "Qui ci sono diverse note evidenziate. Non tutte hanno la stessa \
             importanza: dai la priorità a quelle di avviso.",
            "There are several highlighted notes here. Not all of them matter equally: \
             give priority to the warnings.",
        ),
    }
}

fn callouts(list: &[&Value], lang: ContentLanguage, out: &mut Vec<QualityWarning>) {
    let (_, sections) = split_sections(list);
    for (title, blocks) in &sections {
        let c: Vec<&&Value> = blocks.iter().filter(|b| kind(b) == "callout").collect();
        if c.len() > MAX_CALLOUTS_PER_SECTION {
            out.push(callout_warning(
                id(c[MAX_CALLOUTS_PER_SECTION]),
                &format!("{} nella sezione «{}»", c.len(), show(title)),
                lang,
            ));
        }
    }
    let all: Vec<&&Value> = list.iter().filter(|b| kind(b) == "callout").collect();
    let is_warning = |b: &&&Value| text(b, "variant") == "warning";
    let tip_info: Vec<&&&Value> = all.iter().filter(|b| !is_warning(b)).collect();
    let warn: Vec<&&&Value> = all.iter().filter(|b| is_warning(b)).collect();
    if tip_info.len() > MAX_TIP_INFO {
        out.push(callout_warning(
            id(tip_info[MAX_TIP_INFO]),
            &format!("{} tra suggerimenti e info", tip_info.len()),
            lang,
        ));
    }
    if warn.len() > MAX_WARNING_CALLOUTS {
        out.push(callout_warning(
            id(warn[MAX_WARNING_CALLOUTS]),
            &format!("{} avvisi", warn.len()),
            lang,
        ));
    }
}

fn long_answers(list: &[&Value], lang: ContentLanguage, out: &mut Vec<QualityWarning>) {
    for b in list.iter().filter(|b| kind(b) == "fill_blank") {
        for a in answers(b) {
            let n = words(a.trim());
            if n > MAX_ANSWER_WORDS {
                out.push(QualityWarning {
                    code: QualityWarningCode::FillBlankAnswerTooLong,
                    block_id: Some(id(b)),
                    generator_message: format!(
                        "La risposta «{}» ha {n} parole. Accorciala a un termine di 1-3 parole.",
                        show(a.trim())
                    ),
                    learner_message: localized(
                        lang,
                        "In questo completamento la risposta è lunga: se non \
                         coincide con quella attesa, controllala con il feedback.",
                        "The answer in this fill-in is long: if it does not match the \
                         expected one, check it against the feedback.",
                    ),
                });
            }
        }
    }
}

fn has_compare_word(caption: &str) -> bool {
    plain(caption)
        .to_lowercase()
        .split(|c: char| !is_word_char(c))
        .any(|w| COMPARE_WORDS.contains(&w))
}

fn consecutive_code(list: &[&Value], lang: ContentLanguage, out: &mut Vec<QualityWarning>) {
    let has_compare = list
        .iter()
        .any(|b| kind(b) == "missing_component" && text(b, "wanted") == "code_compare");
    if has_compare {
        return;
    }
    let seq: Vec<&&Value> = list.iter().filter(|b| kind(b) != "glossary_term").collect();
    for w in seq.windows(2) {
        if kind(w[0]) == "code" && kind(w[1]) == "code" && has_compare_word(text(w[1], "caption")) {
            out.push(QualityWarning {
                code: QualityWarningCode::ConsecutiveCodeBlocks,
                block_id: Some(id(w[1])),
                generator_message: "Due blocchi di codice di seguito sembrano un confronto. \
                                    Aggiungi un `missing_component` con `wanted = code_compare` \
                                    oppure separali con un paragrafo che spiega la differenza."
                    .into(),
                learner_message: localized(
                    lang,
                    "Qui due esempi di codice sono uno dopo l'altro. Confrontali \
                     riga per riga.",
                    "Two code examples follow one another here. Compare them line by line.",
                ),
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn h(level: u8, t: &str) -> Value {
        json!({"id": format!("h-{t}"), "type": "heading", "level": level, "text": t})
    }
    fn p(id: &str, t: &str) -> Value {
        json!({"id": id, "type": "paragraph", "text": t})
    }
    fn long_p(id: &str, n: usize) -> Value {
        p(id, &vec!["parola"; n].join(" "))
    }
    fn callout(id: &str, variant: &str) -> Value {
        json!({"id": id, "type": "callout", "variant": variant, "title": "T", "text": "x"})
    }
    fn code(id: &str, caption: &str) -> Value {
        json!({"id": id, "type": "code", "language": "rust", "code": "x", "caption": caption})
    }
    fn blank(id: &str, answers: &[&str]) -> Value {
        json!({"id": id, "type": "fill_blank", "text": "Il {{b}} è", "required": true,
            "blanks": [{"id": "b", "answers": answers}]})
    }
    fn quiz(id: &str, texts: &[&str]) -> Value {
        let o: Vec<Value> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| json!({"id": format!("o{i}"), "text": t}))
            .collect();
        json!({"id": id, "type": "quiz_single", "question": "Q?", "options": o, "correct": "o0"})
    }
    fn doc(blocks: &[Value]) -> Value {
        json!({"blocks": blocks})
    }
    fn codes(d: &Value) -> Vec<&'static str> {
        warnings(d, ContentLanguage::It)
            .iter()
            .map(|w| w.code.as_str())
            .collect()
    }
    fn err(d: &Value) -> Vec<LessonErrorCode> {
        errors(d).iter().map(|i| i.code).collect()
    }

    #[test]
    fn leak_in_previous_prose_next_prose_heading_and_caption() {
        let ok = doc(&[h(2, "A"), p("a", "Niente qui."), blank("f", &["borrow"])]);
        assert!(err(&ok).is_empty());
        let prev = doc(&[
            h(2, "A"),
            p("a", "Il Borrow è tutto."),
            blank("f", &["borrow"]),
        ]);
        assert_eq!(err(&prev), [LessonErrorCode::FillBlankAnswerInText]);
        let next = doc(&[h(2, "A"), blank("f", &["borrow"]), p("a", "borrow!")]);
        assert_eq!(err(&next).len(), 1);
        let head = doc(&[h(2, "borrow"), blank("f", &["borrow"])]);
        assert_eq!(err(&head).len(), 1);
        let cap = doc(&[code("c", "il borrow"), blank("f", &["borrow"])]);
        assert_eq!(err(&cap).len(), 1);
    }

    #[test]
    fn leak_exceptions() {
        // Whole word only, short answers skipped, inline code ignored.
        let part = doc(&[p("a", "borrowing"), blank("f", &["borrow"])]);
        assert!(err(&part).is_empty());
        let short = doc(&[p("a", "il tipo i32 è un i32"), blank("f", &["i3", "ok"])]);
        assert!(err(&short).is_empty());
        let inline = doc(&[p("a", "usa `borrow`"), blank("f", &["borrow"])]);
        assert!(err(&inline).is_empty());
        // Fallback contents are not examined.
        let fb = doc(&[
            json!({"id": "m", "type": "missing_component", "wanted": "x",
            "fallback": [blank("f", &["borrow"]), p("a", "borrow")]}),
        ]);
        assert!(err(&fb).is_empty());
        // Only the two previous prose blocks count, skipping exercises.
        let far = doc(&[
            p("a", "borrow"),
            p("b", "x"),
            p("c", "y"),
            blank("f", &["borrow"]),
        ]);
        assert!(err(&far).is_empty());
    }

    #[test]
    fn uneven_markup_is_an_error_only_when_mixed() {
        assert!(err(&doc(&[quiz("q", &["uno", "due", "tre"])])).is_empty());
        assert!(err(&doc(&[quiz("q", &["`a`", "`b`", "`c`"])])).is_empty());
        assert!(err(&doc(&[quiz("q", &["`a`", "**b**", "[c](https://x.y)"])])).is_empty());
        assert_eq!(
            err(&doc(&[quiz("q", &["`a`", "due", "tre"])])),
            [LessonErrorCode::QuizOptionMarkupUneven]
        );
        assert_eq!(err(&doc(&[quiz("q", &["**a**", "*b*", "tre"])])).len(), 1);
        // Italics are not markup here.
        assert!(err(&doc(&[quiz("q", &["*a*", "b", "c"])])).is_empty());
    }

    #[test]
    fn long_explanation_run() {
        let ok = doc(&[h(2, "A"), long_p("a", 350), h(2, "Fine")]);
        assert!(codes(&ok).is_empty());
        let bad = doc(&[h(2, "A"), long_p("a", 200), long_p("b", 151), h(2, "Fine")]);
        let w = warnings(&bad, ContentLanguage::It);
        assert_eq!(codes(&bad), ["explanation_run_too_long"]);
        assert_eq!(w[0].block_id.as_deref(), Some("a"));
        // An exercise resets the run; code and glossary terms do not.
        let reset = doc(&[
            h(2, "A"),
            long_p("a", 200),
            blank("f", &["xyz"]),
            long_p("b", 200),
            h(2, "Fine"),
        ]);
        assert!(codes(&reset).is_empty());
        let through = doc(&[
            h(2, "A"),
            long_p("a", 200),
            code("c", ""),
            json!({"id": "g", "type": "glossary_term", "key": "k"}),
            long_p("b", 200),
            h(2, "Fine"),
        ]);
        assert_eq!(codes(&through).len(), 1);
        // Closing and intro are exempt.
        let exempt = doc(&[
            long_p("i", 500),
            h(2, "A"),
            p("a", "x"),
            h(2, "Fine"),
            long_p("z", 500),
        ]);
        assert!(codes(&exempt).is_empty());
    }

    #[test]
    fn callout_limits() {
        let ok = doc(&[
            h(2, "A"),
            callout("c1", "tip"),
            h(2, "B"),
            callout("c2", "info"),
        ]);
        assert!(codes(&ok).is_empty());
        let per_section = doc(&[
            h(2, "A"),
            callout("c1", "tip"),
            callout("c2", "warning"),
            h(2, "F"),
        ]);
        let w = warnings(&per_section, ContentLanguage::It);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].block_id.as_deref(), Some("c2"));
        let tips = doc(&[
            h(2, "A"),
            callout("c1", "tip"),
            h(2, "B"),
            callout("c2", "info"),
            h(2, "C"),
            callout("c3", "tip"),
        ]);
        let w = warnings(&tips, ContentLanguage::It);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].block_id.as_deref(), Some("c3"));
        let mut blocks = Vec::new();
        for i in 0..6 {
            blocks.push(h(2, &format!("S{i}")));
            blocks.push(callout(&format!("w{i}"), "warning"));
        }
        let w = warnings(&doc(&blocks), ContentLanguage::It);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].block_id.as_deref(), Some("w5"));
    }

    #[test]
    fn fill_blank_answer_length() {
        assert!(codes(&doc(&[blank("f", &["uno due tre"])])).is_empty());
        let w = warnings(
            &doc(&[blank("f", &["ok", "uno due tre quattro"])]),
            ContentLanguage::It,
        );
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].code, QualityWarningCode::FillBlankAnswerTooLong);
        assert_eq!(w[0].block_id.as_deref(), Some("f"));
    }

    #[test]
    fn consecutive_code_needs_compare_caption_and_no_missing_component() {
        let pair = |cap: &str| doc(&[code("c1", ""), code("c2", cap)]);
        assert!(codes(&pair("Un esempio")).is_empty());
        assert_eq!(
            codes(&pair("Versione invece corretta")),
            ["consecutive_code_blocks"]
        );
        // The first caption and the code text do not trigger it.
        assert!(codes(&doc(&[code("c1", "prima"), code("c2", "")])).is_empty());
        let mut c2 = code("c2", "");
        c2["code"] = json!("// invece");
        assert!(codes(&doc(&[code("c1", ""), c2])).is_empty());
        // A glossary term between them is ignored, a paragraph separates them.
        let g = json!({"id": "g", "type": "glossary_term", "key": "k"});
        assert_eq!(codes(&doc(&[code("c1", ""), g, code("c2", "vs")])).len(), 1);
        assert!(codes(&doc(&[code("c1", ""), p("a", "x"), code("c2", "vs")])).is_empty());
        let mc = json!({"id": "m", "type": "missing_component", "wanted": "code_compare", "fallback": []});
        assert!(codes(&doc(&[code("c1", ""), code("c2", "vs"), mc])).is_empty());
    }

    #[test]
    fn learner_messages_follow_the_topic_language_and_repairs_stay_italian() {
        let blocks = [code("a", "Esempio"), code("b", "Versione invece corretta")];
        let d = doc(&blocks);
        let it = warnings(&d, ContentLanguage::It);
        let en = warnings(&d, ContentLanguage::En);
        assert_eq!(it.len(), 1);
        assert_eq!(en.len(), 1);
        assert_eq!(
            it[0].learner_message,
            "Qui due esempi di codice sono uno dopo l'altro. Confrontali riga per riga."
        );
        assert_eq!(
            en[0].learner_message,
            "Two code examples follow one another here. Compare them line by line."
        );
        assert_eq!(it[0].generator_message, en[0].generator_message);
        let tips: Vec<Value> = (0..4).map(|i| callout(&format!("c{i}"), "tip")).collect();
        let en = warnings(&doc(&tips), ContentLanguage::En);
        assert!(!en.is_empty());
        assert!(
            en.iter()
                .all(|w| w.learner_message.starts_with("There are several"))
        );
        let long = "parola ".repeat(400);
        let prose = json!({"id": "p", "type": "paragraph", "text": long});
        let d = doc(&[h(2, "A"), prose, h(2, "B")]);
        let en = warnings(&d, ContentLanguage::En);
        assert_eq!(en.len(), 1);
        assert!(en[0].learner_message.starts_with("This part runs long"));
        let d = doc(&[blank("f", &["uno due tre quattro"])]);
        assert!(
            warnings(&d, ContentLanguage::En)[0]
                .learner_message
                .starts_with("The answer in this fill-in")
        );
    }

    #[test]
    fn warnings_come_in_document_order() {
        // The code pair comes before the long answer, though its rule runs after.
        let d = doc(&[
            code("c1", ""),
            code("c2", "vs"),
            blank("f", &["uno due tre quattro"]),
        ]);
        assert_eq!(
            codes(&d),
            ["consecutive_code_blocks", "fill_blank_answer_too_long"]
        );
    }

    #[test]
    fn score_weights_long_runs_double() {
        let w = |code| QualityWarning {
            code,
            block_id: None,
            generator_message: String::new(),
            learner_message: String::new(),
        };
        assert_eq!(warning_score(&[]), 0);
        assert_eq!(
            warning_score(&[
                w(QualityWarningCode::ExplanationRunTooLong),
                w(QualityWarningCode::TooManyCallouts),
                w(QualityWarningCode::FillBlankAnswerTooLong),
                w(QualityWarningCode::ConsecutiveCodeBlocks),
            ]),
            5
        );
    }
}
