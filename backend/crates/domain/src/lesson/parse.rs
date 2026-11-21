//! Hand-written parser: `serde_json::Value` -> [`LessonDocument`] plus every
//! shape error (`schema`), unknown block types and the per-block duplicate
//! ids. Semantic rules live in the validator.

use std::collections::HashSet;

use serde_json::{Map, Value};
use uuid::Uuid;

use super::inline::{InlineScan, MarkdownProblem, count_words, scan_inline};
use super::model::{
    Block, BlockKind, DiagramEdge, DiagramLayout, GlossaryEntry, LessonDocument, LessonErrorCode,
    LessonIssue, MdField, QuizOption, SourceEntry, pointer_escape, show,
};
use crate::ids::parse_uuid;

type Obj = Map<String, Value>;

const MAX_ID: usize = 40;

/// Parses `value`, collecting every problem. The returned document is a
/// best-effort model: parts that failed to parse are missing or `Malformed`.
#[must_use]
pub fn parse_document(value: &Value) -> (LessonDocument, Vec<LessonIssue>) {
    let mut p = Parser::default();
    let doc = p.document(value);
    (
        LessonDocument {
            md_fields: p.md_fields,
            ..doc
        },
        p.issues,
    )
}

#[derive(Default)]
struct Parser {
    issues: Vec<LessonIssue>,
    md_fields: Vec<MdField>,
}

fn is_ident(s: &str, dash: bool) -> bool {
    (1..=MAX_ID).contains(&s.chars().count())
        && s.chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || c == '_' || (dash && c == '-'))
}

/// Every block type of the catalog (the keys of `block_fields`). Exposed so
/// tools outside the validator (eval suites) do not keep their own copy.
pub const BLOCK_TYPES: &[&str] = &[
    "heading",
    "paragraph",
    "callout",
    "code",
    "glossary_term",
    "quiz_single",
    "quiz_multi",
    "fill_blank",
    "order_steps",
    "flashcard",
    "open_answer",
    "checklist",
    "primary_source",
    "lesson_link",
    "ask_agent",
    "missing_component",
    "diagram",
];

/// Type-specific fields of each catalog block; `None` for unknown types.
fn block_fields(ty: &str) -> Option<&'static [&'static str]> {
    Some(match ty {
        "heading" => &["level", "text"],
        "paragraph" => &["text"],
        "callout" => &["variant", "title", "text"],
        "code" => &["language", "code", "caption", "highlight_lines"],
        "glossary_term" => &["key"],
        "quiz_single" | "quiz_multi" => &["code", "question", "options", "correct", "required"],
        "fill_blank" => &["text", "blanks", "feedback", "required"],
        "order_steps" => &["prompt", "steps", "feedback", "required"],
        "flashcard" => &["code", "front", "back", "required"],
        "open_answer" => &["code", "question", "max_chars", "rubric", "required"],
        "checklist" => &["title", "items"],
        "primary_source" => &["source", "why"],
        "lesson_link" => &["lesson_id", "label"],
        "ask_agent" => &["label", "prompt_hint"],
        "missing_component" => &["wanted", "purpose", "fallback"],
        "diagram" => &["layout", "title", "nodes", "edges", "caption", "alt"],
        _ => return None,
    })
}

/// JSON Schema `integer`: also accepts `8.0` (no fractional part).
#[allow(clippy::cast_possible_truncation)] // guarded by the range check
fn as_int(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| {
        v.as_f64()
            .filter(|f| f.fract() == 0.0 && f.abs() < 9e15)
            .map(|f| f as i64)
    })
}

fn is_snake_case(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars().count() <= 60
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_lang_tag(s: &str) -> bool {
    let mut parts = s.split('-');
    parts
        .next()
        .is_some_and(|p| (2..=3).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphabetic()))
        && parts.all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric()))
}

impl Parser {
    /// Rejects fields outside `allowed` (contract §9.13).
    fn known(&mut self, o: &Obj, path: &str, allowed: &[&str]) {
        for key in o.keys().filter(|k| !allowed.contains(&k.as_str())) {
            self.schema(
                &format!("{path}/{}", pointer_escape(key)),
                format!("Il campo '{}' non è previsto.", show(key)),
            );
        }
    }

    fn issue(&mut self, path: &str, code: LessonErrorCode, message: String) {
        self.issues.push(LessonIssue::new(path, code, message));
    }

    fn schema(&mut self, path: &str, message: String) {
        self.issue(path, LessonErrorCode::Schema, message);
    }

    // ---- primitives -----------------------------------------------------

    fn string(
        &mut self,
        o: &Obj,
        path: &str,
        key: &str,
        len: (usize, usize),
        required: bool,
    ) -> Option<String> {
        let p = format!("{path}/{key}");
        match o.get(key) {
            None if required => {
                self.schema(&p, format!("Il campo '{key}' è obbligatorio."));
                None
            }
            None => None,
            Some(Value::String(s)) => {
                let n = s.chars().count();
                if n < len.0 || n > len.1 {
                    self.schema(
                        &p,
                        format!(
                            "Il campo '{key}' deve avere da {} a {} caratteri (trovati {n}).",
                            len.0, len.1
                        ),
                    );
                }
                Some(s.clone())
            }
            Some(_) => {
                self.schema(&p, format!("Il campo '{key}' deve essere una stringa."));
                None
            }
        }
    }

    /// Identifier unique within its container; the caller passes the set.
    fn ident(&mut self, o: &Obj, path: &str, key: &str, dash: bool) -> Option<String> {
        let s = self.string(o, path, key, (1, MAX_ID), true)?;
        if !is_ident(&s, dash) {
            let set = if dash {
                "a-z, 0-9, _ e -"
            } else {
                "a-z, 0-9 e _"
            };
            self.schema(
                &format!("{path}/{key}"),
                format!(
                    "Il campo '{key}' ('{}') deve usare solo {set}, da 1 a {MAX_ID} caratteri.",
                    show(&s)
                ),
            );
        }
        Some(s)
    }

    fn unique(&mut self, seen: &mut HashSet<String>, path: &str, id: Option<&String>) {
        if let Some(id) = id
            && !seen.insert(id.clone())
        {
            self.issue(
                &format!("{path}/id"),
                LessonErrorCode::DuplicateId,
                format!("L'id '{}' è duplicato nello stesso blocco.", show(id)),
            );
        }
    }

    fn int(
        &mut self,
        o: &Obj,
        path: &str,
        key: &str,
        range: (i64, i64),
        required: bool,
    ) -> Option<i64> {
        let p = format!("{path}/{key}");
        match o.get(key) {
            None if required => {
                self.schema(&p, format!("Il campo '{key}' è obbligatorio."));
                None
            }
            None => None,
            Some(v) => {
                let Some(n) = as_int(v) else {
                    self.schema(&p, format!("Il campo '{key}' deve essere un intero."));
                    return None;
                };
                if n < range.0 || n > range.1 {
                    self.schema(
                        &p,
                        format!(
                            "Il campo '{key}' deve essere tra {} e {} (trovato {n}).",
                            range.0, range.1
                        ),
                    );
                }
                Some(n)
            }
        }
    }

    /// Exercise flag; `true` when absent (contract §3).
    fn required_flag(&mut self, o: &Obj, path: &str) -> bool {
        match o.get("required") {
            None => true,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                self.schema(
                    &format!("{path}/required"),
                    "Il campo 'required' deve essere un booleano.".into(),
                );
                true
            }
        }
    }

    /// Inline-markdown string: length check plus scan, recorded in `md_fields`.
    fn md(
        &mut self,
        o: &Obj,
        path: &str,
        key: &str,
        len: (usize, usize),
        required: bool,
        prose: bool,
    ) -> Option<String> {
        let s = self.string(o, path, key, len, required)?;
        let n = s.chars().count();
        if n < len.0 || n > len.1 {
            return Some(s); // already a schema error: do not scan oversized input
        }
        let scan = scan_inline(&s);
        self.md_fields.push(MdField {
            path: format!("{path}/{key}"),
            prose,
            words: count_words(&scan.plain),
            scan,
        });
        Some(s)
    }

    fn object<'a>(&mut self, v: &'a Value, path: &str, what: &str) -> Option<&'a Obj> {
        let o = v.as_object();
        if o.is_none() {
            self.schema(path, format!("{what} deve essere un oggetto."));
        }
        o
    }

    /// Array of objects with a size range; returns `(item path, object)`.
    fn objects<'a>(
        &mut self,
        o: &'a Obj,
        path: &str,
        key: &str,
        size: (usize, usize),
        required: bool,
    ) -> Vec<(String, &'a Obj)> {
        let p = format!("{path}/{key}");
        let Some(v) = o.get(key) else {
            if required {
                self.schema(&p, format!("Il campo '{key}' è obbligatorio."));
            }
            return Vec::new();
        };
        let Some(list) = v.as_array() else {
            self.schema(&p, format!("Il campo '{key}' deve essere un array."));
            return Vec::new();
        };
        self.check_size(&p, key, list.len(), size);
        list.iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let ip = format!("{p}/{i}");
                self.object(item, &ip, "L'elemento").map(|obj| (ip, obj))
            })
            .collect()
    }

    fn check_size(&mut self, p: &str, key: &str, n: usize, size: (usize, usize)) {
        if n < size.0 || n > size.1 {
            self.schema(
                p,
                format!(
                    "Il campo '{key}' deve avere da {} a {} elementi (trovati {n}).",
                    size.0, size.1
                ),
            );
        }
    }

    #[allow(clippy::many_single_char_names)] // `o`, `p`: object and pointer, as everywhere here
    fn strings(
        &mut self,
        o: &Obj,
        path: &str,
        key: &str,
        size: (usize, usize),
        item_len: (usize, usize),
        required: bool,
    ) -> Vec<String> {
        let p = format!("{path}/{key}");
        let Some(v) = o.get(key) else {
            if required {
                self.schema(&p, format!("Il campo '{key}' è obbligatorio."));
            }
            return Vec::new();
        };
        let Some(list) = v.as_array() else {
            self.schema(
                &p,
                format!("Il campo '{key}' deve essere un array di stringhe."),
            );
            return Vec::new();
        };
        self.check_size(&p, key, list.len(), size);
        let mut out = Vec::new();
        for (i, item) in list.iter().enumerate() {
            match item.as_str() {
                Some(s) => {
                    let n = s.chars().count();
                    if n < item_len.0 || n > item_len.1 {
                        self.schema(
                            &format!("{p}/{i}"),
                            format!(
                                "Ogni elemento di '{key}' deve avere da {} a {} caratteri (trovati {n}).",
                                item_len.0, item_len.1
                            ),
                        );
                    }
                    out.push(s.to_owned());
                }
                None => self.schema(
                    &format!("{p}/{i}"),
                    format!("Ogni elemento di '{key}' deve essere una stringa."),
                ),
            }
        }
        out
    }

    // ---- document -------------------------------------------------------

    fn document(&mut self, value: &Value) -> LessonDocument {
        let mut doc = LessonDocument::default();
        let Some(o) = self.object(value, "", "Il documento") else {
            return doc;
        };
        match o.get("catalog_version").map(as_int) {
            Some(Some(1)) => {}
            Some(_) => self.schema(
                "/catalog_version",
                "Il campo 'catalog_version' deve essere l'intero 1.".into(),
            ),
            None => self.schema(
                "/catalog_version",
                "Il campo 'catalog_version' è obbligatorio.".into(),
            ),
        }
        self.known(
            o,
            "",
            &[
                "catalog_version",
                "title",
                "objective",
                "estimated_minutes",
                "glossary",
                "sources",
                "blocks",
            ],
        );
        self.string(o, "", "title", (1, 120), true);
        self.string(o, "", "objective", (1, 200), true);
        self.int(o, "", "estimated_minutes", (1, 20), true);
        doc.glossary = self.glossary(o);
        doc.sources = self.sources(o);
        doc.blocks = self.blocks(o);
        doc
    }

    fn glossary(&mut self, o: &Obj) -> Vec<GlossaryEntry> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (p, e) in self.objects(o, "", "glossary", (0, 12), true) {
            self.known(e, &p, &["key", "term", "definition", "avoid"]);
            let key = self.ident(e, &p, "key", false);
            self.string(e, &p, "term", (1, 80), true);
            if self
                .md(e, &p, "definition", (1, 400), true, false)
                .is_some()
                && let Some(f) = self.md_fields.last_mut()
                && !f.scan.glossary_refs.is_empty()
            {
                // No cross references between terms in v1 (§9.20).
                f.scan.glossary_refs.clear();
                f.scan.problems.push(MarkdownProblem::GlossaryRef);
            }
            self.strings(e, &p, "avoid", (0, 3), (1, 120), false);
            if let Some(key) = key {
                if !seen.insert(key.clone()) {
                    self.issue(
                        &format!("{p}/key"),
                        LessonErrorCode::DuplicateId,
                        format!("La chiave di glossario '{}' è duplicata.", show(&key)),
                    );
                }
                out.push(GlossaryEntry { key });
            }
        }
        out
    }

    fn sources(&mut self, o: &Obj) -> Vec<SourceEntry> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (p, e) in self.objects(o, "", "sources", (1, 8), true) {
            self.known(e, &p, &["key", "resource_id", "title", "url"]);
            let key = self.ident(e, &p, "key", false);
            self.string(e, &p, "title", (1, 200), true);
            // `null` is for the sources that come from an uploaded PDF; whether
            // that matches the resource is checked by the Validator.
            let url_is_null = matches!(e.get("url"), Some(Value::Null));
            if !url_is_null
                && let Some(url) = self.string(e, &p, "url", (1, 2000), true)
                && (!url.starts_with("https://")
                    || url.len() <= 8
                    || url.contains(char::is_whitespace))
            {
                self.schema(
                    &format!("{p}/url"),
                    format!(
                        "Il campo 'url' ('{}') deve essere un URL assoluto https oppure null per un PDF caricato.",
                        show(&url)
                    ),
                );
            }
            let resource_id = match e.get("resource_id") {
                Some(Value::Null) => None,
                Some(Value::String(s)) => {
                    let id = parse_uuid(s);
                    if id.is_none() {
                        self.schema(
                            &format!("{p}/resource_id"),
                            "Il campo 'resource_id' deve essere un UUID o null.".into(),
                        );
                    }
                    id
                }
                Some(_) => {
                    self.schema(
                        &format!("{p}/resource_id"),
                        "Il campo 'resource_id' deve essere un UUID o null.".into(),
                    );
                    None
                }
                None => {
                    self.schema(
                        &format!("{p}/resource_id"),
                        "Il campo 'resource_id' è obbligatorio (UUID o null).".into(),
                    );
                    None
                }
            };
            if let Some(key) = key {
                if !seen.insert(key.clone()) {
                    self.issue(
                        &format!("{p}/key"),
                        LessonErrorCode::DuplicateId,
                        format!("La chiave di fonte '{}' è duplicata.", show(&key)),
                    );
                }
                out.push(SourceEntry {
                    key,
                    resource_id,
                    url_is_null,
                });
            }
        }
        out
    }

    fn blocks(&mut self, o: &Obj) -> Vec<Block> {
        let p = "/blocks";
        let Some(v) = o.get("blocks") else {
            self.schema(p, "Il campo 'blocks' è obbligatorio.".into());
            return Vec::new();
        };
        let Some(list) = v.as_array() else {
            self.schema(p, "Il campo 'blocks' deve essere un array.".into());
            return Vec::new();
        };
        self.check_size(p, "blocks", list.len(), (3, 60));
        list.iter()
            .enumerate()
            .map(|(i, b)| self.block(b, format!("{p}/{i}"), false))
            .collect()
    }

    // ---- blocks ---------------------------------------------------------

    fn block(&mut self, v: &Value, path: String, nested: bool) -> Block {
        let malformed = |path: String, id: String| Block {
            path,
            id,
            kind: BlockKind::Malformed,
        };
        let Some(o) = self.object(v, &path, "Il blocco") else {
            return malformed(path, String::new());
        };
        let ty = o.get("type").and_then(Value::as_str);
        // Contract §9.12: an unknown type, or a nested missing_component,
        // yields only its own error: id and lang are not checked.
        if let Some(ty) = ty {
            if block_fields(ty).is_none() {
                self.issue(
                    &format!("{path}/type"),
                    LessonErrorCode::UnknownBlockType,
                    format!(
                        "Il tipo di blocco '{}' non esiste nel catalogo v1.",
                        show(ty)
                    ),
                );
                return Block {
                    path,
                    id: String::new(),
                    kind: BlockKind::Unknown,
                };
            }
            if ty == "missing_component" && nested {
                self.issue(
                    &path,
                    LessonErrorCode::NestedMissingComponent,
                    "Un 'missing_component' non può stare dentro il 'fallback' di un altro.".into(),
                );
                return Block {
                    path,
                    id: String::new(),
                    kind: BlockKind::MissingComponent {
                        fallback: Vec::new(),
                    },
                };
            }
        }
        let id = self.ident(o, &path, "id", true).unwrap_or_default();
        if let Some(lang) = o.get("lang")
            && !lang.as_str().is_some_and(is_lang_tag)
        {
            self.schema(
                &format!("{path}/lang"),
                "Il campo 'lang' deve essere un tag BCP-47 (es. 'en').".into(),
            );
        }
        let Some(ty) = ty else {
            self.schema(
                &format!("{path}/type"),
                "Il campo 'type' è obbligatorio e deve essere una stringa.".into(),
            );
            return malformed(path, id);
        };
        let before = self.issues.len();
        let kind = self.block_kind(ty, o, &path);
        let kind = match kind {
            Some(_)
                if self.issues[before..]
                    .iter()
                    .any(|i| i.code == LessonErrorCode::Schema) =>
            {
                BlockKind::Malformed
            }
            Some(k) => k,
            None => BlockKind::Unknown, // unreachable: unknown types return early
        };
        Block { path, id, kind }
    }

    /// `None` when `ty` is not in the catalog.
    #[allow(clippy::too_many_lines)] // one arm per catalog type: flat on purpose
    fn block_kind(&mut self, ty: &str, o: &Obj, p: &str) -> Option<BlockKind> {
        if let Some(extra) = block_fields(ty) {
            let mut allowed = vec!["id", "type", "lang"];
            allowed.extend_from_slice(extra);
            self.known(o, p, &allowed);
        }
        let kind = match ty {
            "heading" => {
                self.string(o, p, "text", (1, 120), true);
                let level = self.int(o, p, "level", (1, 3), true);
                BlockKind::Heading {
                    level: level.and_then(|l| u8::try_from(l).ok()).unwrap_or(0),
                }
            }
            "paragraph" => {
                self.md(o, p, "text", (1, 1200), true, true);
                BlockKind::Paragraph
            }
            "callout" => {
                match o.get("variant").and_then(Value::as_str) {
                    Some("info" | "tip" | "warning") => {}
                    _ => self.schema(
                        &format!("{p}/variant"),
                        "Il campo 'variant' deve essere 'info', 'tip' o 'warning'.".into(),
                    ),
                }
                // Plain text (§9.23): no inline scan, but it counts as prose.
                if let Some(t) = self.string(o, p, "title", (1, 80), false) {
                    self.md_fields.push(MdField {
                        path: format!("{p}/title"),
                        prose: true,
                        words: count_words(&t),
                        scan: InlineScan::default(),
                    });
                }
                self.md(o, p, "text", (1, 600), true, true);
                BlockKind::Callout
            }
            "code" => {
                self.language(o, p);
                let code = self.string(o, p, "code", (1, 3000), true);
                self.md(o, p, "caption", (1, 200), false, false);
                self.highlight_lines(o, p, code.as_deref());
                BlockKind::Code
            }
            "glossary_term" => BlockKind::GlossaryTerm {
                key: self.ident(o, p, "key", false).unwrap_or_default(),
            },
            "quiz_single" => {
                self.exercise_code(o, p);
                self.md(o, p, "question", (1, 400), true, true);
                let options = self.quiz_options(o, p, (2, 5));
                let correct = self
                    .string(o, p, "correct", (1, MAX_ID), true)
                    .unwrap_or_default();
                BlockKind::QuizSingle {
                    options,
                    correct,
                    required: self.required_flag(o, p),
                }
            }
            "quiz_multi" => {
                self.exercise_code(o, p);
                self.md(o, p, "question", (1, 400), true, true);
                let options = self.quiz_options(o, p, (3, 6));
                // Length is checked by `quiz_correct_missing`, not by schema.
                let correct = self.strings(o, p, "correct", (0, usize::MAX), (1, MAX_ID), true);
                let mut seen = HashSet::new();
                for (i, c) in correct.iter().enumerate() {
                    if !seen.insert(c) {
                        self.issue(
                            &format!("{p}/correct/{i}"),
                            LessonErrorCode::DuplicateId,
                            format!("L'id '{}' è ripetuto in 'correct'.", show(c)),
                        );
                    }
                }
                BlockKind::QuizMulti {
                    options,
                    correct,
                    required: self.required_flag(o, p),
                }
            }
            "fill_blank" => {
                let text = self
                    .md(o, p, "text", (1, 600), true, false)
                    .unwrap_or_default();
                let blank_ids = self.blanks(o, p);
                self.feedback_pair(o, p);
                BlockKind::FillBlank {
                    text,
                    blank_ids,
                    required: self.required_flag(o, p),
                }
            }
            "order_steps" => {
                self.md(o, p, "prompt", (1, 300), true, false);
                let mut seen = HashSet::new();
                for (ip, e) in self.objects(o, p, "steps", (3, 8), true) {
                    self.known(e, &ip, &["id", "text"]);
                    let id = self.ident(e, &ip, "id", true);
                    self.unique(&mut seen, &ip, id.as_ref());
                    self.md(e, &ip, "text", (1, 200), true, false);
                }
                self.feedback_pair(o, p);
                BlockKind::OrderSteps {
                    required: self.required_flag(o, p),
                }
            }
            "flashcard" => {
                self.exercise_code(o, p);
                self.md(o, p, "front", (1, 300), true, false);
                self.md(o, p, "back", (1, 600), true, false);
                BlockKind::Flashcard {
                    required: self.required_flag(o, p),
                }
            }
            "open_answer" => {
                self.exercise_code(o, p);
                self.md(o, p, "question", (1, 400), true, false);
                self.int(o, p, "max_chars", (50, 2000), false);
                self.string(o, p, "rubric", (1, 1500), true);
                BlockKind::OpenAnswer {
                    required: self.required_flag(o, p),
                }
            }
            "checklist" => {
                self.string(o, p, "title", (1, 80), false);
                let mut seen = HashSet::new();
                for (ip, e) in self.objects(o, p, "items", (1, 8), true) {
                    self.known(e, &ip, &["id", "text"]);
                    let id = self.ident(e, &ip, "id", true);
                    self.unique(&mut seen, &ip, id.as_ref());
                    self.md(e, &ip, "text", (1, 200), true, false);
                }
                BlockKind::Checklist
            }
            "primary_source" => {
                let source = self.ident(o, p, "source", false).unwrap_or_default();
                self.md(o, p, "why", (1, 300), true, false);
                BlockKind::PrimarySource { source }
            }
            "lesson_link" => {
                let lesson_id = self.string(o, p, "lesson_id", (1, 64), true).and_then(|s| {
                    let id = parse_uuid(&s);
                    if id.is_none() {
                        self.schema(
                            &format!("{p}/lesson_id"),
                            "Il campo 'lesson_id' deve essere un UUID.".into(),
                        );
                    }
                    id
                });
                self.string(o, p, "label", (1, 120), true);
                BlockKind::LessonLink {
                    lesson_id: lesson_id.unwrap_or_else(Uuid::nil),
                }
            }
            "ask_agent" => {
                self.string(o, p, "label", (1, 80), false);
                self.string(o, p, "prompt_hint", (1, 300), true);
                BlockKind::AskAgent
            }
            "diagram" => self.diagram(o, p),
            "missing_component" => {
                if let Some(w) = self.string(o, p, "wanted", (1, 60), true)
                    && !is_snake_case(&w)
                {
                    self.schema(
                        &format!("{p}/wanted"),
                        "Il campo 'wanted' deve essere in snake_case (inizia con una lettera minuscola, poi a-z, 0-9, _; massimo 60 caratteri).".into(),
                    );
                }
                self.string(o, p, "purpose", (1, 300), true);
                BlockKind::MissingComponent {
                    fallback: self.fallback(o, p),
                }
            }
            _ => return None,
        };
        Some(kind)
    }

    /// Shape of a `diagram` (§10). The graph rules live in the validator and
    /// only run on a well-formed block.
    fn diagram(&mut self, o: &Obj, p: &str) -> BlockKind {
        let layout = match o.get("layout").and_then(Value::as_str) {
            Some("flow") => Some(DiagramLayout::Flow),
            Some("tree") => Some(DiagramLayout::Tree),
            Some("cycle") => Some(DiagramLayout::Cycle),
            _ => {
                self.schema(
                    &format!("{p}/layout"),
                    "Il campo 'layout' deve essere 'flow', 'tree' o 'cycle'.".into(),
                );
                None
            }
        };
        // Plain text: no inline scan, not prose (§10.3, §10.6).
        self.string(o, p, "title", (1, 80), false);
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        for (ip, e) in self.objects(o, p, "nodes", (2, 8), true) {
            self.known(e, &ip, &["id", "label", "detail"]);
            let id = self.ident(e, &ip, "id", true);
            self.unique(&mut seen, &ip, id.as_ref());
            self.string(e, &ip, "label", (1, 40), true);
            self.string(e, &ip, "detail", (1, 60), false);
            nodes.extend(id);
        }
        let mut edges = Vec::new();
        for (ip, e) in self.objects(o, p, "edges", (0, 12), false) {
            self.known(e, &ip, &["from", "to", "label"]);
            let from = self.ident(e, &ip, "from", true);
            let to = self.ident(e, &ip, "to", true);
            self.string(e, &ip, "label", (1, 30), false);
            if let (Some(from), Some(to)) = (from, to) {
                edges.push(DiagramEdge { from, to });
            }
        }
        self.md(o, p, "caption", (1, 300), true, false);
        self.string(o, p, "alt", (1, 600), true);
        match layout {
            Some(layout) => BlockKind::Diagram {
                layout,
                nodes,
                edges,
            },
            None => BlockKind::Malformed,
        }
    }

    /// `language` of a `code` block or of an exercise `code` (contract §4/§4.1).
    fn language(&mut self, o: &Obj, p: &str) {
        if let Some(l) = self.string(o, p, "language", (1, 40), true)
            && !l
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || "_+#-".contains(c))
        {
            self.schema(
                &format!("{p}/language"),
                "Il campo 'language' deve usare solo a-z, 0-9, _, +, # e -.".into(),
            );
        }
    }

    /// Optional `code` of an exercise with a question (contract §4.1).
    fn exercise_code(&mut self, o: &Obj, p: &str) {
        let Some(v) = o.get("code") else {
            return;
        };
        let cp = format!("{p}/code");
        let Some(code) = self.object(v, &cp, "Il campo 'code'") else {
            return;
        };
        self.known(code, &cp, &["language", "code"]);
        self.language(code, &cp);
        if let Some(text) = self.string(code, &cp, "code", (1, 1000), true)
            && !text.is_empty()
            && text.trim().is_empty()
        {
            // The app discards a blank snippet, so it is a shape error.
            self.schema(
                &format!("{cp}/code"),
                "Il campo 'code' non può contenere solo spazi o a capo.".into(),
            );
        }
    }

    /// 1-6 distinct 1-based line numbers within `code` (§9.21).
    fn highlight_lines(&mut self, o: &Obj, p: &str, code: Option<&str>) {
        let Some(v) = o.get("highlight_lines") else {
            return;
        };
        let hp = format!("{p}/highlight_lines");
        let Some(list) = v.as_array() else {
            self.schema(
                &hp,
                "Il campo 'highlight_lines' deve essere un array.".into(),
            );
            return;
        };
        self.check_size(&hp, "highlight_lines", list.len(), (1, 6));
        let max = code.map_or(usize::MAX, |c| c.lines().count());
        let mut seen = HashSet::new();
        for (i, n) in list.iter().enumerate() {
            let ip = format!("{hp}/{i}");
            match as_int(n).and_then(|n| usize::try_from(n).ok()) {
                Some(n) if (1..=max).contains(&n) => {
                    if !seen.insert(n) {
                        self.schema(&ip, format!("La riga {n} è ripetuta in 'highlight_lines'."));
                    }
                }
                _ => self.schema(
                    &ip,
                    format!("Ogni elemento di 'highlight_lines' deve essere un intero tra 1 e {max} (righe di 'code')."),
                ),
            }
        }
    }

    fn fallback(&mut self, o: &Obj, p: &str) -> Vec<Block> {
        let fp = format!("{p}/fallback");
        let Some(v) = o.get("fallback") else {
            self.schema(&fp, "Il campo 'fallback' è obbligatorio.".into());
            return Vec::new();
        };
        let Some(list) = v.as_array() else {
            self.schema(&fp, "Il campo 'fallback' deve essere un array.".into());
            return Vec::new();
        };
        self.check_size(&fp, "fallback", list.len(), (1, 6));
        list.iter()
            .enumerate()
            .map(|(i, b)| self.block(b, format!("{fp}/{i}"), true))
            .collect()
    }

    fn quiz_options(&mut self, o: &Obj, p: &str, size: (usize, usize)) -> Vec<QuizOption> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (ip, e) in self.objects(o, p, "options", size, true) {
            self.known(e, &ip, &["id", "text", "feedback"]);
            let id = self.ident(e, &ip, "id", true);
            self.unique(&mut seen, &ip, id.as_ref());
            let before = self.md_fields.len();
            self.md(e, &ip, "text", (1, 200), true, true);
            let words = self.md_fields.get(before).map_or(0, |f| f.words);
            let chars = self
                .md_fields
                .get(before)
                .map_or(0, |f| f.scan.plain.trim().chars().count());
            self.md(e, &ip, "feedback", (1, 400), true, true);
            out.push(QuizOption {
                id: id.unwrap_or_default(),
                words,
                chars,
            });
        }
        out
    }

    fn blanks(&mut self, o: &Obj, p: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut ids = Vec::new();
        for (ip, e) in self.objects(o, p, "blanks", (1, 4), true) {
            self.known(e, &ip, &["id", "answers", "case_sensitive"]);
            let id = self.ident(e, &ip, "id", true);
            self.unique(&mut seen, &ip, id.as_ref());
            self.strings(e, &ip, "answers", (1, 5), (1, 60), true);
            if e.get("case_sensitive").is_some_and(|v| !v.is_boolean()) {
                self.schema(
                    &format!("{ip}/case_sensitive"),
                    "Il campo 'case_sensitive' deve essere un booleano.".into(),
                );
            }
            ids.extend(id);
        }
        ids
    }

    fn feedback_pair(&mut self, o: &Obj, p: &str) {
        let fp = format!("{p}/feedback");
        match o.get("feedback") {
            None => self.schema(&fp, "Il campo 'feedback' è obbligatorio.".into()),
            Some(v) => {
                if let Some(f) = self.object(v, &fp, "Il campo 'feedback'") {
                    self.known(f, &fp, &["correct", "incorrect"]);
                    self.md(f, &fp, "correct", (1, 400), true, false);
                    self.md(f, &fp, "incorrect", (1, 400), true, false);
                }
            }
        }
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::{BLOCK_TYPES, block_fields};

    #[test]
    fn block_types_list_matches_the_parser_catalog() {
        for t in BLOCK_TYPES {
            assert!(block_fields(t).is_some(), "{t} not parsed");
        }
        assert!(block_fields("not_a_block").is_none());
        // Catches a type added to `block_fields` but forgotten in the list.
        let src = include_str!("parse.rs");
        let arms = src
            .split("fn block_fields")
            .nth(1)
            .and_then(|b| b.split("_ => return None").next())
            .unwrap_or_default();
        let quoted = arms
            .lines()
            .filter_map(|l| l.split("=>").next())
            .flat_map(|l| l.split('"').skip(1).step_by(2))
            .count();
        assert_eq!(quoted, BLOCK_TYPES.len());
    }
}
