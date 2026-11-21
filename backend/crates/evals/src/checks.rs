//! Deterministic checks: pure functions from a text or a JSON value to a
//! pass/fail, shared by the roles. A check the suite names but this runner
//! cannot compute is reported as `skipped`, never silently passed.

use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Fail,
    /// A failed check that only warns (suite says `warn`, or a "typical" range).
    Warn,
    /// Not computed here: judged by the rubric, or not supported.
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub status: Status,
    pub detail: String,
}

impl Check {
    pub fn pass(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            status: Status::Pass,
            detail: String::new(),
        }
    }

    /// Pass, or `Fail` with `detail`.
    pub fn of(name: &str, ok: bool, detail: impl FnOnce() -> String) -> Self {
        if ok {
            Self::pass(name)
        } else {
            Self {
                name: name.to_owned(),
                status: Status::Fail,
                detail: detail(),
            }
        }
    }

    /// Pass, or `Warn` with `detail`.
    pub fn soft(name: &str, ok: bool, detail: impl FnOnce() -> String) -> Self {
        let mut c = Self::of(name, ok, detail);
        if c.status == Status::Fail {
            c.status = Status::Warn;
        }
        c
    }

    /// A passing check that still reports a measure (worst value, count).
    pub fn pass_with(name: &str, detail: String) -> Self {
        Self {
            name: name.to_owned(),
            status: Status::Pass,
            detail,
        }
    }

    pub fn skipped(name: &str, why: &str) -> Self {
        Self {
            name: name.to_owned(),
            status: Status::Skipped,
            detail: why.to_owned(),
        }
    }

    /// A failure becomes a warning when the suite value says `"severity": "warn"`
    /// (or is the string `"warn"`).
    #[must_use]
    pub fn with_severity(mut self, spec: &Value) -> Self {
        let warn = spec.as_str() == Some("warn")
            || spec.get("severity").and_then(Value::as_str) == Some("warn");
        if warn && self.status == Status::Fail {
            self.status = Status::Warn;
        }
        self
    }
}

/// Whether any check failed (warnings and skips do not count).
#[must_use]
pub fn all_ok(checks: &[Check]) -> bool {
    checks.iter().all(|c| c.status != Status::Fail)
}

// ---------------------------------------------------------------- helpers

#[must_use]
pub fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// `[min, max]` as numbers.
#[must_use]
pub fn range(v: &Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?))
}

/// The type a value of a suite key must have. A known key with a value of
/// another type would make its check return nothing, so it is refused when
/// the suite loads (see [`object_problem`]).
#[derive(Debug, Clone, Copy)]
pub enum Want {
    /// `[min, max]`
    Range,
    /// `[min, max]` or `{"range": [min, max], ...}`
    RangeOrWrapped,
    UInt,
    Int,
    Bool,
    Regex,
    StrList,
    UIntList,
    /// An object whose values are all unsigned integers.
    UIntMap,
}

impl Want {
    fn describe(self) -> &'static str {
        match self {
            Self::Range => "a range `[min, max]`",
            Self::RangeOrWrapped => "a range `[min, max]` or `{\"range\": [min, max]}`",
            Self::UInt => "an unsigned integer",
            Self::Int => "an integer",
            Self::Bool => "a boolean",
            Self::Regex => "a valid regular expression string",
            Self::StrList => "a list of strings",
            Self::UIntList => "a list of unsigned integers",
            Self::UIntMap => "an object of unsigned integers",
        }
    }

    fn accepts(self, v: &Value) -> bool {
        match self {
            Self::Range => range(v).is_some(),
            Self::RangeOrWrapped => range(v.get("range").unwrap_or(v)).is_some(),
            Self::UInt => v.as_u64().is_some(),
            Self::Int => v.as_i64().is_some(),
            Self::Bool => v.is_boolean(),
            Self::Regex => v.as_str().is_some_and(|s| Regex::new(s).is_ok()),
            Self::StrList => v.as_array().is_some_and(|a| a.iter().all(Value::is_string)),
            Self::UIntList => v
                .as_array()
                .is_some_and(|a| a.iter().all(|x| x.as_u64().is_some())),
            Self::UIntMap => v
                .as_object()
                .is_some_and(|o| o.values().all(|x| x.as_u64().is_some())),
        }
    }
}

/// `Some(reason)` when `v` is not `want`.
#[must_use]
pub fn value_problem(key: &str, v: &Value, want: Want) -> Option<String> {
    (!want.accepts(v)).then(|| format!("`{key}` must be {}, found {v}", want.describe()))
}

/// `Some(reason)` when `v` is not an object whose `fields` (`name`, type,
/// required) have the stated types.
#[must_use]
pub fn object_problem(key: &str, v: &Value, fields: &[(&str, Want, bool)]) -> Option<String> {
    let Some(o) = v.as_object() else {
        return Some(format!("`{key}` must be an object, found {v}"));
    };
    fields
        .iter()
        .find_map(|(name, want, required)| match o.get(*name) {
            None if *required => Some(format!("`{key}.{name}` is missing")),
            None => None,
            Some(x) => value_problem(&format!("{key}.{name}"), x, *want),
        })
}

/// Keys any object-valued check may carry besides its own fields: prose for
/// the reader of the suite, and the severity `with_severity` reads.
const META_KEYS: [&str; 3] = ["description", "note", "severity"];

/// Like [`object_problem`], and also refuses a key that is neither one of
/// `fields` nor a [`META_KEYS`] one: a typo such as `"sevrity": "warn"` would
/// otherwise be ignored and the check would run harder than the suite says.
/// A `severity` must be `warn` or `error`.
#[must_use]
pub fn strict_object_problem(
    key: &str,
    v: &Value,
    fields: &[(&str, Want, bool)],
) -> Option<String> {
    if let Some(problem) = object_problem(key, v, fields) {
        return Some(problem);
    }
    let o = v.as_object()?;
    if let Some(unknown) = o
        .keys()
        .find(|k| !META_KEYS.contains(&k.as_str()) && !fields.iter().any(|(n, ..)| n == k))
    {
        let mut known: Vec<&str> = fields.iter().map(|(n, ..)| *n).collect();
        known.extend(META_KEYS);
        return Some(format!(
            "`{key}` has an unknown field `{unknown}` (known: {})",
            known.join(", ")
        ));
    }
    match o.get("severity") {
        Some(sev) if !matches!(sev.as_str(), Some("warn" | "error")) => Some(format!(
            "`{key}.severity` must be \"warn\" or \"error\", found {sev}"
        )),
        _ => None,
    }
}

#[must_use]
pub fn in_range(n: usize, r: (f64, f64)) -> bool {
    #[allow(clippy::cast_precision_loss)] // counts of words and characters
    let n = n as f64;
    n >= r.0 && n <= r.1
}

/// Minutes of a lesson: `round(P/250 + 0.5*E + 2*O)` with `P` the words of
/// prose (contract §9), `E` the exercises and `O` the open answers
/// (`docs/design/components/README.md` §1.4).
#[must_use]
pub fn lesson_minutes(prose_words: usize, exercises: usize, open_answers: usize) -> i64 {
    // The backend's own function: the checks and the saved value cannot drift.
    i64::from(domain::lesson_minutes(prose_words, exercises, open_answers))
}

/// A regex from the suite; an invalid one is a failing check, not a panic.
pub fn regex(pattern: &str) -> Result<Regex, String> {
    Regex::new(pattern).map_err(|e| format!("invalid regex `{pattern}`: {e}"))
}

#[must_use]
pub fn patterns(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// `Ok(None)`: no pattern matches. `Ok(Some(m))`: the first match.
pub fn first_match(pats: &[String], text: &str) -> Result<Option<String>, String> {
    for p in pats {
        if let Some(m) = regex(p)?.find(text) {
            return Ok(Some(m.as_str().to_owned()));
        }
    }
    Ok(None)
}

pub fn must_not_match(name: &str, spec: &Value, text: &str) -> Check {
    match first_match(&patterns(spec), text) {
        Ok(None) => Check::pass(name),
        Ok(Some(m)) => Check::of(name, false, || format!("matched `{m}`")),
        Err(e) => Check::of(name, false, || e),
    }
}

/// Number of sentences, by terminal punctuation followed by a space or the end.
#[must_use]
pub fn sentences(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut n = 0;
    let mut open = false;
    for (i, c) in chars.iter().enumerate() {
        if c.is_alphanumeric() {
            open = true;
        }
        let end = matches!(c, '.' | '!' | '?' | '…')
            && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
        if end && open {
            n += 1;
            open = false;
        }
    }
    n + usize::from(open)
}

/// Text without fenced code blocks, and the lines inside them.
#[must_use]
pub fn split_code(text: &str) -> (String, Vec<String>, Vec<String>) {
    let mut prose = Vec::new();
    let mut code = Vec::new();
    let mut fences = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("```") {
            if !inside {
                fences.push(t.trim_start_matches('`').trim().to_owned());
            }
            inside = !inside;
        } else if inside {
            code.push(line.to_owned());
        } else {
            prose.push(line);
        }
    }
    (prose.join("\n"), code, fences)
}

const IT: [&str; 36] = [
    "il", "lo", "la", "gli", "le", "un", "una", "di", "che", "è", "per", "con", "non", "si",
    "come", "della", "del", "nel", "sono", "hai", "ho", "ti", "mi", "più", "cosa", "questo", "ma",
    "se", "da", "al", "anche", "dei", "delle", "puoi", "sei", "tuo",
];
const EN: [&str; 30] = [
    "the", "and", "of", "to", "is", "are", "you", "that", "for", "with", "this", "it", "as", "on",
    "be", "your", "can", "what", "how", "not", "have", "do", "in", "an", "was", "will", "from",
    "or", "but", "by",
];

/// Stopword counts `(italian, english)`, or `None` when the text is too
/// short to tell (every language check passes it).
fn stopwords(text: &str) -> Option<(usize, usize)> {
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphabetic() && c != '\'')
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.len() < 4 {
        return None;
    }
    let it = tokens.iter().filter(|t| IT.contains(t)).count();
    let en = tokens.iter().filter(|t| EN.contains(t)).count();
    Some((it, en))
}

/// Stopword heuristic: enough to catch an answer written in English.
#[must_use]
pub fn looks_italian(text: &str) -> bool {
    stopwords(text).is_none_or(|(it, en)| it >= en)
}

/// The mirror of [`looks_italian`]: enough to catch an answer written in Italian.
#[must_use]
pub fn looks_english(text: &str) -> bool {
    stopwords(text).is_none_or(|(it, en)| en >= it)
}

/// Whether `text` looks written in `language` (`it` or `en`).
#[must_use]
pub fn looks_like(language: domain::ContentLanguage, text: &str) -> bool {
    match language {
        domain::ContentLanguage::It => looks_italian(text),
        domain::ContentLanguage::En => looks_english(text),
    }
}

/// Markup removed: `[[key|text]]` -> text, `[[key]]` -> key, `[^k]` dropped,
/// emphasis and backticks dropped.
#[must_use]
pub fn plain(text: &str) -> String {
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
    let cite = Regex::new(r"\[\^[^\]]*\]").expect("static regex");
    let out = cite.replace_all(&out, "");
    out.replace(['*', '`'], "").trim().to_owned()
}

// ------------------------------------------------------------ text checks

/// `text` without its inline `` `code` `` spans: what the learner reads as
/// prose. `println!` in backticks is not an exclamation.
#[must_use]
pub fn strip_inline_code(text: &str) -> String {
    Regex::new(r"`[^`\n]*`")
        .expect("static regex")
        .replace_all(text, "")
        .into_owned()
}

/// The markdown subset and style checks shared by chat-like text. `None`
/// when `key` is not one of them.
#[must_use]
pub fn text_check(key: &str, spec: &Value, text: &str) -> Option<Check> {
    let (prose, code_lines, fences) = split_code(text);
    // Inline code is not prose: the style rules do not apply to it.
    let prose = strip_inline_code(&prose);
    let on = spec.as_bool().unwrap_or(true);
    let lines = || prose.lines().map(str::trim_start);
    let list_item = Regex::new(r"^([-*+]|\d+[.)])\s+").expect("static regex");
    Some(match key {
        "max_words" => {
            let max = spec.as_u64()?;
            let n = words(text);
            Check::of(key, n as u64 <= max, || format!("{n} words, max {max}"))
        }
        "max_question_marks" => {
            let max = spec.as_u64()?;
            let n = prose.matches('?').count();
            Check::of(key, n as u64 <= max, || {
                format!("{n} question marks, max {max}")
            })
        }
        "language" => {
            let Some(language) = spec.as_str().and_then(|s| s.parse().ok()) else {
                return Some(Check::skipped(key, "only `it` and `en` are supported"));
            };
            Check::of(key, looks_like(language, text), || {
                format!(
                    "does not look {}",
                    domain::ContentLanguage::english_name(language)
                )
            })
        }
        "must_not_match" => must_not_match(key, spec, text),
        "no_headings" if on => Check::of(key, !lines().any(|l| l.starts_with('#')), || {
            "a line starts with `#`".into()
        }),
        "no_tables" if on => {
            let table = Regex::new(r"^\|.*\|\s*$|^:?-{3,}:?\s*\|").expect("static regex");
            Check::of(key, !lines().any(|l| table.is_match(l)), || {
                "a table row".into()
            })
        }
        "no_blockquotes" if on => Check::of(key, !lines().any(|l| l.starts_with('>')), || {
            "a line starts with `>`".into()
        }),
        "no_html_or_images" if on => {
            let html = Regex::new(r"<[a-zA-Z/][^>]*>|!\[").expect("static regex");
            Check::of(key, !html.is_match(&prose), || "HTML or image".into())
        }
        "no_bare_urls" if on => {
            let link = Regex::new(r"\[[^\]]*\]\(https://[^)\s]+\)").expect("static regex");
            let stripped = link.replace_all(&prose, "");
            let bare = Regex::new(r"https?://\S+|www\.\S+").expect("static regex");
            Check::of(key, !bare.is_match(&stripped), || "a bare URL".into())
        }
        "max_bold_spans" => {
            let max = spec.as_u64()?;
            let n = prose.matches("**").count() / 2;
            Check::of(key, n as u64 <= max, || {
                format!("{n} bold spans, max {max}")
            })
        }
        "max_list_items" => {
            let max = spec.as_u64()?;
            let n = lines().filter(|l| list_item.is_match(l)).count();
            Check::of(key, n as u64 <= max, || {
                format!("{n} list items, max {max}")
            })
        }
        "code_blocks_need_language" if on => {
            Check::of(key, fences.iter().all(|f| !f.is_empty()), || {
                "a code block has no language".into()
            })
        }
        "max_code_line_chars" => {
            let max = spec.as_u64()?;
            let longest = code_lines
                .iter()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0);
            Check::of(key, longest as u64 <= max, || {
                format!("a code line has {longest} characters, max {max}")
            })
        }
        "no_exclamation_marks" if on => {
            Check::of(key, !prose.contains('!'), || "contains `!`".into())
        }
        _ => return None,
    })
}

/// Runs every key of `spec` that `text_check` knows; the rest goes to
/// `other`, which returns the check for it (or `None` for "not supported").
pub fn run_spec(
    spec: &Map<String, Value>,
    text: &str,
    mut other: impl FnMut(&str, &Value) -> Option<Check>,
) -> Vec<Check> {
    spec.iter()
        .map(|(key, value)| {
            text_check(key, value, text)
                .or_else(|| other(key, value))
                .unwrap_or_else(|| Check::skipped(key, "not computed by this runner"))
        })
        .collect()
}

/// The status of the check called `name` (tests).
#[cfg(test)]
pub fn status_of(checks: &[Check], name: &str) -> Status {
    let Some(check) = checks.iter().find(|c| c.name == name) else {
        panic!("no check `{name}` in {checks:?}")
    };
    check.status
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(key: &str, spec: &Value, text: &str) -> Status {
        text_check(key, spec, text).unwrap().status
    }

    #[test]
    fn counts_words_and_question_marks() {
        assert_eq!(run("max_words", &json!(3), "uno due tre"), Status::Pass);
        assert_eq!(run("max_words", &json!(2), "uno due tre"), Status::Fail);
        assert_eq!(run("max_question_marks", &json!(1), "a? b?"), Status::Fail);
        assert_eq!(run("max_question_marks", &json!(2), "a? b?"), Status::Pass);
    }

    #[test]
    fn language_catches_english() {
        assert_eq!(
            run(
                "language",
                &json!("it"),
                "Cosa vorresti riuscire a fare con Rust?"
            ),
            Status::Pass
        );
        assert_eq!(
            run(
                "language",
                &json!("it"),
                "What do you want to be able to do with it?"
            ),
            Status::Fail
        );
    }

    #[test]
    fn language_catches_italian_when_english_is_wanted() {
        assert_eq!(
            run(
                "language",
                &json!("en"),
                "What do you want to be able to do with it?"
            ),
            Status::Pass
        );
        assert_eq!(
            run(
                "language",
                &json!("en"),
                "Cosa vorresti riuscire a fare con Rust?"
            ),
            Status::Fail
        );
        // Too short to tell: passes in both directions.
        assert_eq!(run("language", &json!("en"), "Ciao"), Status::Pass);
        assert_eq!(run("language", &json!("it"), "Hello"), Status::Pass);
    }

    #[test]
    fn language_skips_an_unknown_code() {
        assert_eq!(run("language", &json!("fr"), "Bonjour"), Status::Skipped);
    }

    #[test]
    fn markdown_subset_rules() {
        assert_eq!(run("no_headings", &json!(true), "# Titolo"), Status::Fail);
        assert_eq!(
            run("no_headings", &json!(true), "```\n# commento\n```"),
            Status::Pass
        );
        assert_eq!(run("no_tables", &json!(true), "| a | b |"), Status::Fail);
        assert_eq!(
            run("no_blockquotes", &json!(true), "> citazione"),
            Status::Fail
        );
        assert_eq!(
            run("no_html_or_images", &json!(true), "ciao <b>x</b>"),
            Status::Fail
        );
        assert_eq!(
            run("no_bare_urls", &json!(true), "vedi https://a.it"),
            Status::Fail
        );
        assert_eq!(
            run("no_bare_urls", &json!(true), "vedi [qui](https://a.it)"),
            Status::Pass
        );
        assert_eq!(
            run("max_bold_spans", &json!(1), "**a** e **b**"),
            Status::Fail
        );
        assert_eq!(
            run("max_list_items", &json!(2), "- a\n- b\n3. c"),
            Status::Fail
        );
        assert_eq!(
            run("code_blocks_need_language", &json!(true), "```\nx\n```"),
            Status::Fail
        );
        assert_eq!(
            run("code_blocks_need_language", &json!(true), "```rust\nx\n```"),
            Status::Pass
        );
        assert_eq!(
            run("max_code_line_chars", &json!(5), "```rust\n123456\n```"),
            Status::Fail
        );
        assert_eq!(
            run("no_exclamation_marks", &json!(true), "Bene!"),
            Status::Fail
        );
    }

    #[test]
    fn forbidden_patterns_are_reported_with_the_match() {
        let c = text_check(
            "must_not_match",
            &json!(["(?i)\\bbravo\\b"]),
            "Bravo davvero",
        )
        .unwrap();
        assert_eq!(c.status, Status::Fail);
        assert!(c.detail.contains("Bravo"));
        let bad = text_check("must_not_match", &json!(["("]), "x").unwrap();
        assert_eq!(bad.status, Status::Fail);
    }

    #[test]
    fn inline_and_block_code_are_not_prose() {
        let text = "Usa `println!(\"ciao\")` per stampare.\n```rust\nprintln!(\"x\");\n```";
        assert_eq!(
            run("no_exclamation_marks", &json!(true), text),
            Status::Pass
        );
        assert_eq!(
            run("no_exclamation_marks", &json!(true), "Bene! `x`"),
            Status::Fail
        );
        assert_eq!(
            run("max_question_marks", &json!(0), "L'operatore `?` propaga."),
            Status::Pass
        );
    }

    #[test]
    fn unknown_keys_are_skipped_not_passed() {
        let spec = json!({"max_words": 5, "mystery": true});
        let checks = run_spec(spec.as_object().unwrap(), "uno due", |_, _| None);
        assert_eq!(checks[0].status, Status::Pass);
        assert_eq!(checks[1].status, Status::Skipped);
    }

    #[test]
    fn sentences_and_plain() {
        assert_eq!(sentences("Uno. Due! Tre?"), 3);
        assert_eq!(sentences("Versione 1.5 della libreria"), 1);
        assert_eq!(
            plain("Un [[move|trasferimento]] di **valore** [^b] e `x`"),
            "Un trasferimento di valore  e x"
        );
    }
}
