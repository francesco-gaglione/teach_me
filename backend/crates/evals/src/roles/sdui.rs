//! Reading an SDUI lesson document (`docs/contracts/sdui.md`), for the
//! Generator's deterministic checks.

use serde_json::Value;

use crate::checks::plain;

pub const EXERCISES: [&str; 6] = [
    "quiz_single",
    "quiz_multi",
    "fill_blank",
    "order_steps",
    "flashcard",
    "open_answer",
];

#[must_use]
pub fn top(doc: &Value) -> Vec<&Value> {
    doc.get("blocks")
        .and_then(Value::as_array)
        .map_or_default(|a| a.iter().collect())
}

/// Every block, `missing_component` fallbacks included.
#[must_use]
pub fn all(doc: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    for b in top(doc) {
        out.push(b);
        if let Some(fb) = b.get("fallback").and_then(Value::as_array) {
            out.extend(fb.iter());
        }
    }
    out
}

#[must_use]
pub fn kind(b: &Value) -> &str {
    b.get("type").and_then(Value::as_str).unwrap_or_default()
}

#[must_use]
pub fn text<'a>(b: &'a Value, key: &str) -> &'a str {
    b.get(key).and_then(Value::as_str).unwrap_or_default()
}

#[must_use]
pub fn is_exercise(b: &Value) -> bool {
    EXERCISES.contains(&kind(b))
}

/// Paragraphs and callouts (the lesson's prose), markup removed.
#[must_use]
pub fn prose(b: &Value) -> Option<String> {
    match kind(b) {
        "paragraph" => Some(plain(text(b, "text"))),
        "callout" => Some(plain(&format!("{} {}", text(b, "title"), text(b, "text")))),
        _ => None,
    }
}

/// The options of a quiz as `(id, visible text)`.
#[must_use]
pub fn options(b: &Value) -> Vec<(String, String)> {
    b.get("options")
        .and_then(Value::as_array)
        .map_or_default(|a| {
            a.iter()
                .map(|o| (text(o, "id").to_owned(), plain(text(o, "text"))))
                .collect()
        })
}

/// Ids of the correct options of a quiz.
#[must_use]
pub fn correct_ids(b: &Value) -> Vec<String> {
    match b.get("correct") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// Every string of the document the learner reads, markup removed.
#[must_use]
pub fn visible_strings(doc: &Value) -> Vec<String> {
    fn walk(v: &Value, key: &str, out: &mut Vec<String>) {
        match v {
            Value::String(s)
                if !matches!(
                    key,
                    "id" | "type"
                        | "rubric"
                        | "url"
                        | "language"
                        | "lesson_id"
                        | "resource_id"
                        | "source"
                        | "key"
                        | "correct"
                        | "variant"
                        | "wanted"
                        // `diagram`: the layout word and the node ids an edge
                        // points at are structure, not text.
                        | "layout"
                        | "from"
                        | "to"
                ) =>
            {
                out.push(plain(s));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, key, out)),
            Value::Object(o) => o.iter().for_each(|(k, x)| walk(x, k, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(doc, "", &mut out);
    out
}

/// The texts of a `diagram` block as the learner reads them: title, node
/// label and detail, edge label, caption and alt, markup removed. They are not
/// prose for the word count (`sdui.md` §10 rule 6): [`prose_of_block`] does
/// not read them.
#[must_use]
pub fn diagram_texts(b: &Value) -> Vec<String> {
    if kind(b) != "diagram" {
        return Vec::new();
    }
    let mut out: Vec<String> = ["title", "caption", "alt"]
        .iter()
        .filter_map(|k| b.get(*k).and_then(Value::as_str))
        .map(plain)
        .collect();
    for (list, keys) in [("nodes", ["label", "detail"]), ("edges", ["label", ""])] {
        for item in b.get(list).and_then(Value::as_array).into_iter().flatten() {
            out.extend(
                keys.iter()
                    .filter_map(|k| item.get(*k).and_then(Value::as_str))
                    .map(plain),
            );
        }
    }
    out
}

/// Strings that are feedback to an answer: option feedback and the pair of a
/// `fill_blank` / `order_steps`.
#[must_use]
pub fn feedback_strings(doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for b in all(doc) {
        for o in b
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(f) = o.get("feedback").and_then(Value::as_str) {
                out.push(f.to_owned());
            }
        }
        if let Some(f) = b.get("feedback").and_then(Value::as_object) {
            out.extend(f.values().filter_map(|v| v.as_str().map(str::to_owned)));
        }
    }
    out
}

/// Like [`prose`], with inline `code` spans removed: the text a leak check
/// compares answers with.
#[must_use]
pub fn prose_without_code(b: &Value) -> Option<String> {
    let strip = crate::checks::strip_inline_code;
    match kind(b) {
        "paragraph" => Some(plain(&strip(text(b, "text")))),
        "callout" => Some(plain(&strip(&format!(
            "{} {}",
            text(b, "title"),
            text(b, "text")
        )))),
        _ => None,
    }
}

/// `heading` of the given level.
#[must_use]
pub fn is_heading(b: &Value, level: u64) -> bool {
    kind(b) == "heading" && b.get("level").and_then(Value::as_u64) == Some(level)
}

/// The prose of one block as the contract counts it (`sdui.md` §9.4):
/// paragraph, callout title and text, quiz question, options and their
/// feedback. Markup removed.
#[must_use]
pub fn prose_of_block(b: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match kind(b) {
        "paragraph" => out.push(plain(text(b, "text"))),
        "callout" => {
            out.push(plain(text(b, "title")));
            out.push(plain(text(b, "text")));
        }
        "quiz_single" | "quiz_multi" => {
            out.push(plain(text(b, "question")));
            for o in b
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                out.push(plain(text(o, "text")));
                out.push(plain(text(o, "feedback")));
            }
        }
        _ => {}
    }
    out
}

/// Words of [`prose_of_block`].
#[must_use]
pub fn prose_words_of_block(b: &Value) -> usize {
    prose_of_block(b)
        .iter()
        .map(|t| crate::checks::words(t))
        .sum()
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn items<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Every inline-markdown string of a block that the learner reads as prose
/// (raw, with `[[key]]` links), in reading order. Titles of headings and the
/// body of `code` are not prose.
#[must_use]
pub fn markdown_fields(b: &Value) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    match kind(b) {
        "paragraph" => out.extend(str_field(b, "text")),
        "callout" => {
            out.extend(str_field(b, "title"));
            out.extend(str_field(b, "text"));
        }
        "code" => out.extend(str_field(b, "caption")),
        "quiz_single" | "quiz_multi" => {
            out.extend(str_field(b, "question"));
            for o in items(b, "options") {
                out.extend(str_field(o, "text"));
                out.extend(str_field(o, "feedback"));
            }
        }
        "fill_blank" | "order_steps" => {
            out.extend(str_field(b, "text"));
            out.extend(str_field(b, "prompt"));
            for st in items(b, "steps") {
                out.extend(str_field(st, "text"));
            }
            if let Some(f) = b.get("feedback") {
                out.extend(str_field(f, "correct"));
                out.extend(str_field(f, "incorrect"));
            }
        }
        "flashcard" => {
            out.extend(str_field(b, "front"));
            out.extend(str_field(b, "back"));
        }
        "open_answer" => out.extend(str_field(b, "question")),
        "checklist" => {
            out.extend(str_field(b, "title"));
            for i in items(b, "items") {
                out.extend(str_field(i, "text"));
            }
        }
        "primary_source" => out.extend(str_field(b, "why")),
        _ => {}
    }
    out
}
