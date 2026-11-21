//! Shape checks of a rich lesson (`backend/evals/README.md`):
//! the sections the plan implies, the chain of each concept, the length of
//! what is read between two exercises, the recall of unproven prerequisites.
//! Pure functions over the document the Generator wrote and the plan it was
//! given.
//!
//! The reading of the document is by position: an `h1`, an intro up to the
//! first `h2`, then sections that each run to the next `h2`. A section is the
//! recall when it is the first and its title starts with `Cosa ti serve:`, the
//! closing when it is the last (and not the recall), a concept otherwise.

use std::collections::HashSet;

use serde_json::Value;

use super::generator::contains_answer;
use super::sdui::{
    self, is_exercise, is_heading, kind, markdown_fields, prose, prose_words_of_block, text,
};
use crate::checks::{Check, Status, in_range, lesson_minutes, must_not_match, plain, range, words};

const RECALL_PREFIX: &str = "Cosa ti serve:";
/// The intro of a concept starts here: `heading(3)` «Esempio: …».
const EXAMPLE_PREFIX: &str = "Esempio:";
const PITFALL_PREFIX: &str = "Errore tipico:";

/// Keys of `deterministic` computed here.
pub const KEYS: &[&str] = &[
    "prose_words",
    "prose_words_target",
    "sections_from_plan",
    "explanation_run_words_max",
    "paragraphs_consecutive_max",
    "paragraph_words",
    "concept_chain",
    "h3_per_concept_section",
    "callouts",
    "intro",
    "recall_iff_general",
    "recall",
    "closing",
    "estimated_minutes_formula",
    "estimated_minutes_range",
    "exercises_per_concept_section",
    "blocks_max",
    "forbidden_phrases_warn",
    "term_first_mention_is_glossary_link",
];

/// Why the value of a key computed here cannot be read, or `None` (see
/// `planner::value_problem`).
#[must_use]
pub fn value_problem(key: &str, v: &Value) -> Option<String> {
    use crate::checks::{Want, strict_object_problem as object, value_problem as top};
    // A range, or an object that wraps it (`{"range": [10, 15], "severity": "warn"}`).
    let ranged = |v: &Value| {
        if v.is_object() {
            object(key, v, &[("range", Want::Range, true)])
        } else {
            top(key, v, Want::Range)
        }
    };
    // A check with nothing to configure: `true`, a severity word, or an
    // object that only carries meta keys.
    let bare = |v: &Value| match v {
        Value::Bool(_) => None,
        Value::String(s) if matches!(s.as_str(), "warn" | "error") => None,
        Value::Object(_) => object(key, v, &[]),
        _ => Some(format!(
            "`{key}` must be true, \"warn\", \"error\" or an object with `description` and `severity`, found {v}"
        )),
    };
    match key {
        "prose_words" | "exercises_per_concept_section" => top(key, v, Want::Range),
        "prose_words_target" | "estimated_minutes_range" => ranged(v),
        "explanation_run_words_max" => object(
            key,
            v,
            &[
                ("max", Want::UInt, true),
                ("defect_above", Want::UInt, false),
            ],
        ),
        // `blocks_max` is a number, or an object with a severity:
        // `{"max": 80, "severity": "warn"}`.
        "blocks_max" if v.is_object() => object(key, v, &[("max", Want::UInt, true)]),
        "paragraphs_consecutive_max" | "blocks_max" => top(key, v, Want::UInt),
        "paragraph_words" => object(key, v, &[("range", Want::Range, true)]),
        "closing" => object(key, v, &[("words_max", Want::UInt, true)]),
        "estimated_minutes_formula" => object(key, v, &[("tolerance", Want::Int, false)]),
        "recall" => object(
            key,
            v,
            &[
                ("expected", Want::Bool, true),
                ("prerequisites", Want::UInt, false),
            ],
        ),
        "intro" => object(
            key,
            v,
            &[
                ("paragraphs", Want::Range, false),
                ("words_max", Want::UInt, false),
                ("no_exercise", Want::Bool, false),
                ("no_h3", Want::Bool, false),
            ],
        ),
        "callouts" => object(
            key,
            v,
            &[
                ("per_section_max", Want::UInt, false),
                ("warning_max", Want::UInt, false),
                ("tip_plus_info_max", Want::UInt, false),
            ],
        ),
        "sections_from_plan"
        | "concept_chain"
        | "h3_per_concept_section"
        | "recall_iff_general"
        | "term_first_mention_is_glossary_link" => bare(v),
        _ => None,
    }
}

pub struct Section<'a> {
    pub title: &'a str,
    pub blocks: Vec<&'a Value>,
}

/// The document cut into intro and `h2` sections.
pub struct Layout<'a> {
    pub intro: Vec<&'a Value>,
    pub sections: Vec<Section<'a>>,
}

impl<'a> Layout<'a> {
    #[must_use]
    pub fn new(blocks: &[&'a Value]) -> Self {
        let mut intro = Vec::new();
        let mut sections: Vec<Section<'a>> = Vec::new();
        for &b in blocks {
            if is_heading(b, 2) {
                sections.push(Section {
                    title: text(b, "text"),
                    blocks: Vec::new(),
                });
            } else if let Some(section) = sections.last_mut() {
                section.blocks.push(b);
            } else if !is_heading(b, 1) {
                intro.push(b);
            }
        }
        Self { intro, sections }
    }

    fn recall_index(&self) -> Option<usize> {
        self.sections
            .first()
            .filter(|s| s.title.trim().starts_with(RECALL_PREFIX))
            .map(|_| 0)
    }

    fn closing_index(&self) -> Option<usize> {
        let last = self.sections.len().checked_sub(1)?;
        (Some(last) != self.recall_index()).then_some(last)
    }

    /// The concept sections, in document order.
    fn concepts(&self) -> Vec<&Section<'a>> {
        let (recall, closing) = (self.recall_index(), self.closing_index());
        self.sections
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != recall && Some(*i) != closing)
            .map(|(_, s)| s)
            .collect()
    }
}

/// What the checks read: the document, the plan it follows, its text.
pub struct Shape<'a> {
    pub doc: &'a Value,
    pub plan: &'a Value,
    /// Top-level blocks.
    pub blocks: Vec<&'a Value>,
    /// Every string the learner reads, markup removed.
    pub all_text: &'a str,
    /// Language of the case's topic.
    pub language: domain::ContentLanguage,
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn general_prerequisites(plan: &Value) -> usize {
    list(plan, "prerequisites")
        .iter()
        .filter(|p| text(p, "basis") == "general")
        .count()
}

fn prose_total(doc: &Value) -> usize {
    domain::lesson::parse_document(doc).0.prose_words()
}

fn exercises_of(doc: &Value) -> Vec<&Value> {
    sdui::all(doc)
        .into_iter()
        .filter(|b| is_exercise(b))
        .collect()
}

/// A block that declares a `lang` other than the topic's `language` (one that
/// declares none, or the topic's own, is in the topic's language).
pub(super) fn foreign_language(b: &Value, language: domain::ContentLanguage) -> bool {
    b.get("lang")
        .and_then(Value::as_str)
        .is_some_and(|l| !l.to_lowercase().starts_with(language.code()))
}

/// Runs the keys of `spec` that belong here. Each gives one check, except
/// the recall, which also reports the length of its review paragraphs.
pub fn checks(spec: &serde_json::Map<String, Value>, s: &Shape<'_>) -> Vec<Check> {
    let layout = Layout::new(&s.blocks);
    spec.iter()
        .filter(|(k, _)| KEYS.contains(&k.as_str()))
        .flat_map(|(key, v)| check(key, v, s, &layout))
        .collect()
}

#[allow(clippy::too_many_lines)] // one arm per key of the suite
fn check(key: &str, v: &Value, s: &Shape<'_>, layout: &Layout<'_>) -> Vec<Check> {
    let concepts = list(s.plan, "concepts");
    let general = general_prerequisites(s.plan);
    let one =
        |c: Option<Check>| -> Vec<Check> { c.map(|c| c.with_severity(v)).into_iter().collect() };
    match key {
        "prose_words" => {
            let n = prose_total(s.doc);
            one(range(v).map(|r| {
                Check::of(key, in_range(n, r), || {
                    format!("{n} words of prose, expected {r:?}")
                })
            }))
        }
        "prose_words_target" => {
            let n = prose_total(s.doc);
            one(range(v.get("range").unwrap_or(v)).map(|r| {
                Check::of(key, in_range(n, r), || {
                    format!("{n} words of prose, target {r:?}")
                })
            }))
        }
        "sections_from_plan" => {
            let (have, want) = (
                layout.sections.len(),
                concepts.len() + 1 + usize::from(general > 0),
            );
            one(Some(Check::of(key, have == want, || {
                format!(
                    "{have} h2 sections, the plan implies {want} ({} concepts, the closing{})",
                    concepts.len(),
                    if general > 0 { ", the recall" } else { "" }
                )
            })))
        }
        "explanation_run_words_max" => one(explanation_run(key, v, layout)),
        "paragraphs_consecutive_max" => one(v.as_u64().map(|max| {
            let (mut run, mut worst) = (0u64, 0u64);
            for b in &s.blocks {
                match kind(b) {
                    "paragraph" => run += 1,
                    // A figure sits between paragraphs without breaking the
                    // reading flow, as in `explanation_run_words_max`.
                    "diagram" => {}
                    _ => run = 0,
                }
                worst = worst.max(run);
            }
            Check::of(key, worst <= max, || {
                format!("{worst} paragraphs in a row, max {max}")
            })
        })),
        "paragraph_words" => one(v.get("range").and_then(range).map(|r| {
            let bad: Vec<String> = s
                .blocks
                .iter()
                .filter(|b| kind(b) == "paragraph" && !foreign_language(b, s.language))
                .filter_map(|b| {
                    let n = words(&plain(text(b, "text")));
                    (!in_range(n, r)).then(|| format!("{} ({n})", text(b, "id")))
                })
                .collect();
            Check::of(key, bad.is_empty(), || {
                format!("paragraphs outside {r:?} words: {bad:?}")
            })
        })),
        "concept_chain" => one(Some(concept_chain(key, concepts, layout))),
        "h3_per_concept_section" => one(Some(h3_per_concept(key, layout))),
        "callouts" => one(Some(callouts(key, v, layout))),
        "intro" => one(Some(intro(key, v, layout))),
        "recall_iff_general" => recall(key, general, layout)
            .into_iter()
            .map(|c| c.with_severity(v))
            .collect(),
        "recall" => one(recall_matches_plan(v, general)),
        "closing" => one(v.get("words_max").and_then(Value::as_u64).map(|max| {
            let Some(section) = layout.closing_index().map(|i| &layout.sections[i]) else {
                return Check::of(key, false, || "the lesson has no closing section".into());
            };
            let n: usize = section.blocks.iter().map(|b| closing_words(b)).sum();
            Check::of(key, n as u64 <= max, || {
                format!("the closing has {n} words, max {max}")
            })
        })),
        "estimated_minutes_formula" => {
            let tolerance = v.get("tolerance").and_then(Value::as_i64).unwrap_or(0);
            let exercises = exercises_of(s.doc);
            let open = exercises
                .iter()
                .filter(|b| kind(b) == "open_answer")
                .count();
            let expected = lesson_minutes(prose_total(s.doc), exercises.len(), open);
            let given = s.doc.get("estimated_minutes").and_then(Value::as_i64);
            one(Some(Check::of(
                key,
                given.is_some_and(|g| (g - expected).abs() <= tolerance),
                || {
                    format!(
                        "estimated_minutes is {given:?}, the formula gives {expected} (tolerance {tolerance})"
                    )
                },
            )))
        }
        "estimated_minutes_range" => {
            // The Planner's suites write an object with a severity, the
            // Generator's a bare range, which is a hard check.
            let given = s.doc.get("estimated_minutes").and_then(Value::as_u64);
            one(range(v.get("range").unwrap_or(v)).map(|r| {
                let ok =
                    given.is_some_and(|g| in_range(usize::try_from(g).unwrap_or(usize::MAX), r));
                Check::of(key, ok, || {
                    format!("estimated_minutes is {given:?}, expected {r:?}")
                })
            }))
        }
        "exercises_per_concept_section" => one(range(v).map(|r| {
            let bad: Vec<String> = layout
                .concepts()
                .iter()
                .filter_map(|c| {
                    let n = c.blocks.iter().filter(|b| is_exercise(b)).count();
                    (!in_range(n, r)).then(|| format!("`{}`: {n}", c.title))
                })
                .collect();
            Check::of(key, bad.is_empty(), || {
                format!("exercises per concept section outside {r:?}: {bad:?}")
            })
        })),
        "blocks_max" => one(v.get("max").unwrap_or(v).as_u64().map(|max| {
            // Top-level blocks only: what a section holds inside a
            // component does not count (the app renders one per block).
            let n = s.blocks.len();
            Check::of(key, n as u64 <= max, || {
                format!("the document has {n} top-level blocks, max {max}")
            })
        })),
        "forbidden_phrases_warn" => {
            let mut c = must_not_match(key, v, s.all_text);
            if c.status == Status::Fail {
                c.status = Status::Warn;
            }
            vec![c]
        }
        "term_first_mention_is_glossary_link" => one(Some(first_mentions(key, s))),
        _ => Vec::new(),
    }
}

/// The longest run of explanation between the start of a section (or an
/// exercise) and the next exercise: paragraphs and callouts count, code
/// neither counts nor breaks the run. The intro and the closing are not
/// explanation runs. Over `max` warns, over `defect_above` fails.
fn explanation_run(key: &str, v: &Value, layout: &Layout<'_>) -> Option<Check> {
    let max = usize::try_from(v.get("max")?.as_u64()?).ok()?;
    let defect = v
        .get("defect_above")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(max);
    let closing = layout.closing_index();
    let mut worst = (0usize, "");
    for (i, section) in layout.sections.iter().enumerate() {
        if Some(i) == closing {
            continue;
        }
        let mut run = 0usize;
        for b in &section.blocks {
            if is_exercise(b) {
                if run > worst.0 {
                    worst = (run, section.title);
                }
                run = 0;
            } else if let Some(t) = prose(b) {
                run += words(&t);
            }
        }
        if run > worst.0 {
            worst = (run, section.title);
        }
    }
    let status = if worst.0 > defect {
        Status::Fail
    } else if worst.0 > max {
        Status::Warn
    } else {
        Status::Pass
    };
    Some(Check {
        name: key.to_owned(),
        status,
        detail: format!(
            "longest explanation run: {} words (section `{}`); max {max}, defect above {defect}",
            worst.0, worst.1
        ),
    })
}

fn type_name(b: Option<&Value>) -> String {
    match b {
        None => "the end of the section".to_owned(),
        Some(b) if kind(b) == "heading" => format!(
            "heading {}",
            b.get("level").and_then(Value::as_u64).unwrap_or(0)
        ),
        Some(b) => kind(b).to_owned(),
    }
}

/// `paragraph, heading(3) «Esempio: …», code?, paragraph, paragraph,
/// callout(warning) «Errore tipico: …»?, exercise+` with `glossary_term`
/// and figures (`diagram`, `missing_component`) ignored wherever they sit; the
/// callout exactly when the plan has a pitfall.
fn chain_problems(section: &Section<'_>, wants_pitfall: bool) -> Vec<String> {
    let seq: Vec<&Value> = section
        .blocks
        .iter()
        .copied()
        .filter(|b| !matches!(kind(b), "glossary_term" | "diagram" | "missing_component"))
        .collect();
    let at = |i: usize| seq.get(i).copied();
    let expect = |i: usize, what: &str, ok: bool| {
        (!ok).then(|| format!("expected {what}, found {}", type_name(at(i))))
    };
    let mut problems = Vec::new();
    if let Some(p) = expect(
        0,
        "an intuition paragraph first",
        at(0).is_some_and(|b| kind(b) == "paragraph"),
    ) {
        return vec![p];
    }
    if let Some(p) = expect(
        1,
        "the heading 3 `Esempio: …` second",
        at(1).is_some_and(|b| {
            is_heading(b, 3) && text(b, "text").trim_start().starts_with(EXAMPLE_PREFIX)
        }),
    ) {
        return vec![p];
    }
    let mut i = 2;
    if at(i).is_some_and(|b| kind(b) == "code") {
        i += 1;
    }
    for what in ["the worked-example paragraph", "the rule paragraph"] {
        if let Some(p) = expect(i, what, at(i).is_some_and(|b| kind(b) == "paragraph")) {
            return vec![p];
        }
        i += 1;
    }
    match (wants_pitfall, at(i)) {
        (true, Some(b)) if kind(b) == "callout" => {
            let title = plain(text(b, "title"));
            if text(b, "variant") != "warning" {
                problems.push("the typical-mistake callout is not a `warning`".to_owned());
            }
            if !text(b, "title").trim_start().starts_with(PITFALL_PREFIX) {
                problems.push(format!(
                    "the callout title does not start with `{PITFALL_PREFIX}`"
                ));
            }
            if words(&title) > 8 {
                problems.push(format!(
                    "the callout title has {} words, max 8",
                    words(&title)
                ));
            }
            let chars = plain(text(b, "text")).chars().count();
            if chars > 300 {
                problems.push(format!("the callout text has {chars} characters, max 300"));
            }
            i += 1;
        }
        (true, other) => {
            problems.push(format!(
                "the plan has a pitfall: expected the callout, found {}",
                type_name(other)
            ));
            return problems;
        }
        (false, Some(b)) if kind(b) == "callout" => {
            problems.push("a callout, but the plan has no pitfall for this concept".to_owned());
            i += 1;
        }
        (false, _) => {}
    }
    let first_exercise = i;
    while at(i).is_some_and(is_exercise) {
        i += 1;
    }
    if i == first_exercise {
        problems.push(format!(
            "expected at least one exercise, found {}",
            type_name(at(i))
        ));
    } else if i < seq.len() {
        problems.push(format!("`{}` after the exercises", type_name(at(i))));
    }
    problems
}

fn concept_chain(key: &str, concepts: &[Value], layout: &Layout<'_>) -> Check {
    let sections = layout.concepts();
    let mut problems = Vec::new();
    if sections.len() != concepts.len() {
        problems.push(format!(
            "{} concept sections for {} planned concepts",
            sections.len(),
            concepts.len()
        ));
    }
    for (i, (section, concept)) in sections.iter().zip(concepts).enumerate() {
        let wants_pitfall = !text(concept, "pitfall").trim().is_empty();
        for p in chain_problems(section, wants_pitfall) {
            problems.push(format!("section {} `{}`: {p}", i + 1, section.title));
        }
    }
    Check::of(key, problems.is_empty(), || problems.join("; "))
}

fn h3_count(blocks: &[&Value]) -> usize {
    blocks.iter().filter(|b| is_heading(b, 3)).count()
}

fn h3_per_concept(key: &str, layout: &Layout<'_>) -> Check {
    let mut problems = Vec::new();
    let n = h3_count(&layout.intro);
    if n > 0 {
        problems.push(format!("{n} heading 3 in the intro"));
    }
    for (i, section) in layout.sections.iter().enumerate() {
        let n = h3_count(&section.blocks);
        let is_concept = layout.recall_index() != Some(i) && layout.closing_index() != Some(i);
        match (is_concept, n) {
            (true, 1) | (false, 0) => {}
            (true, n) => problems.push(format!(
                "concept section `{}` has {n} heading 3, expected 1",
                section.title
            )),
            (false, n) => problems.push(format!(
                "section `{}` has {n} heading 3, expected none",
                section.title
            )),
        }
    }
    Check::of(key, problems.is_empty(), || problems.join("; "))
}

fn callouts(key: &str, v: &Value, layout: &Layout<'_>) -> Check {
    let mut problems = Vec::new();
    let groups = std::iter::once(("intro", &layout.intro))
        .chain(layout.sections.iter().map(|s| (s.title, &s.blocks)));
    let (mut warning, mut other) = (0u64, 0u64);
    for (title, blocks) in groups {
        let in_section: Vec<&&Value> = blocks.iter().filter(|b| kind(b) == "callout").collect();
        for b in &in_section {
            if text(b, "variant") == "warning" {
                warning += 1;
            } else {
                other += 1;
            }
        }
        if let Some(max) = v.get("per_section_max").and_then(Value::as_u64)
            && in_section.len() as u64 > max
        {
            problems.push(format!(
                "section `{title}` has {} callouts, max {max}",
                in_section.len()
            ));
        }
    }
    if let Some(max) = v.get("warning_max").and_then(Value::as_u64)
        && warning > max
    {
        problems.push(format!("{warning} warning callouts, max {max}"));
    }
    if let Some(max) = v.get("tip_plus_info_max").and_then(Value::as_u64)
        && other > max
    {
        problems.push(format!("{other} tip and info callouts, max {max}"));
    }
    Check::of(key, problems.is_empty(), || problems.join("; "))
}

fn intro(key: &str, v: &Value, layout: &Layout<'_>) -> Check {
    let mut problems = Vec::new();
    let paragraphs = layout
        .intro
        .iter()
        .filter(|b| kind(b) == "paragraph")
        .count();
    if let Some(r) = v.get("paragraphs").and_then(range)
        && !in_range(paragraphs, r)
    {
        problems.push(format!("{paragraphs} intro paragraphs, expected {r:?}"));
    }
    if let Some(max) = v.get("words_max").and_then(Value::as_u64) {
        let n: usize = layout
            .intro
            .iter()
            .filter_map(|b| prose(b))
            .map(|t| words(&t))
            .sum();
        if n as u64 > max {
            problems.push(format!("the intro has {n} words, max {max}"));
        }
    }
    if v.get("no_exercise").and_then(Value::as_bool) == Some(true)
        && layout.intro.iter().any(|b| is_exercise(b))
    {
        problems.push("the intro has an exercise".to_owned());
    }
    if v.get("no_h3").and_then(Value::as_bool) == Some(true) && h3_count(&layout.intro) > 0 {
        problems.push("the intro has a heading 3".to_owned());
    }
    Check::of(key, problems.is_empty(), || problems.join("; "))
}

/// The recall section exists exactly when the plan has a `general`
/// prerequisite; when it does it has one optional retrieval exercise before
/// 1-3 review paragraphs, no flashcard, no heading 3, at most 300 words.
/// The review paragraphs are the paragraphs after the exercise; their length
/// (25-60 words) is a separate warning.
fn recall(key: &str, general: usize, layout: &Layout<'_>) -> Vec<Check> {
    let section = layout.recall_index().map(|i| &layout.sections[i]);
    let mut problems = Vec::new();
    let mut out = Vec::new();
    match (general > 0, section) {
        (true, None) => problems.push(format!(
            "the plan has {general} `general` prerequisites but the first section is not `{RECALL_PREFIX} …`"
        )),
        (false, Some(_)) => {
            problems.push("a recall section, but the plan has no `general` prerequisite".to_owned());
        }
        (true, Some(section)) => {
            let exercises: Vec<&&Value> = section.blocks.iter().filter(|b| is_exercise(b)).collect();
            if let [exercise] = exercises.as_slice() {
                if !matches!(kind(exercise), "quiz_single" | "fill_blank") {
                    problems.push(format!("the recall exercise is a `{}`", kind(exercise)));
                }
                if exercise.get("required") != Some(&Value::Bool(false)) {
                    problems.push("the recall exercise is not `required: false`".to_owned());
                }
                let at = section.blocks.iter().position(|b| std::ptr::eq(*b, **exercise));
                let review: Vec<&&Value> = at
                    .map_or_default(|at| section.blocks[at + 1..].iter().filter(|b| kind(b) == "paragraph").collect());
                if !(1..=3).contains(&review.len()) {
                    problems.push(format!("{} review paragraphs after the exercise, expected 1-3", review.len()));
                }
                let long: Vec<String> = review
                    .iter()
                    .filter_map(|b| {
                        let n = words(&plain(text(b, "text")));
                        (!(25..=60).contains(&n)).then(|| format!("{} ({n})", text(b, "id")))
                    })
                    .collect();
                out.push(Check::soft("recall_review_paragraph_words", long.is_empty(), || {
                    format!("review paragraphs outside 25-60 words: {long:?}")
                }));
            } else {
                problems.push(format!("the recall has {} exercises, expected exactly 1", exercises.len()));
            }
            if h3_count(&section.blocks) > 0 {
                problems.push("the recall has a heading 3".to_owned());
            }
            let n: usize = section.blocks.iter().map(|b| prose_words_of_block(b)).sum();
            if n > 300 {
                problems.push(format!("the recall has {n} words of prose, max 300"));
            }
        }
        (false, None) => {}
    }
    out.insert(
        0,
        Check::of(key, problems.is_empty(), || problems.join("; ")),
    );
    out
}

/// `deterministic.recall` is part of the case, not a check: it states what
/// the case author expects the plan to imply, and this confirms it.
fn recall_matches_plan(v: &Value, general: usize) -> Option<Check> {
    let expected = v.get("expected").and_then(Value::as_bool)?;
    let prerequisites = v.get("prerequisites").and_then(Value::as_u64);
    let ok = expected == (general > 0) && prerequisites.is_none_or(|n| n == general as u64);
    Some(Check::of("recall_expected_matches_plan", ok, || {
        format!(
            "the case expects recall {expected} with {prerequisites:?} prerequisites; the plan has {general} `general`"
        )
    }))
}

/// Words of what the closing is made of: paragraphs, the checklist, the
/// reason of the primary source.
fn closing_words(b: &Value) -> usize {
    match kind(b) {
        "paragraph" | "checklist" | "primary_source" => {
            markdown_fields(b).iter().map(|f| words(&plain(f))).sum()
        }
        _ => 0,
    }
}

/// The first event of `term` in a markdown string: a `[[key]]` link, or the
/// words in plain text (inline code and other links skipped).
#[derive(Debug, PartialEq, Eq)]
enum Mention {
    Linked,
    Plain,
}

fn first_mention(md: &str, key: &str, term: &str) -> Option<Mention> {
    let mut rest = md;
    loop {
        let link = rest.find("[[");
        let code = rest.find('`');
        let (at, is_link) = match (link, code) {
            (Some(l), Some(c)) if l < c => (l, true),
            (Some(_) | None, Some(c)) => (c, false),
            (Some(l), None) => (l, true),
            (None, None) => {
                return contains_answer(rest, term).then_some(Mention::Plain);
            }
        };
        if contains_answer(&rest[..at], term) {
            return Some(Mention::Plain);
        }
        let after = &rest[at..];
        if is_link {
            let Some(end) = after.find("]]") else {
                return contains_answer(after, term).then_some(Mention::Plain);
            };
            let inner = &after[2..end];
            if inner.split('|').next().is_some_and(|k| k.trim() == key) {
                return Some(Mention::Linked);
            }
            rest = &after[end + 2..];
        } else {
            // An unterminated span swallows the rest: nothing more to read.
            let end = after[1..].find('`')?;
            rest = &after[end + 2..];
        }
    }
}

/// Every new term of the plan is pre-taught: its first mention in the prose
/// is the `[[key]]` link, or a `glossary_term` block with that key came
/// before it. Headings and code do not count as prose.
fn first_mentions(key: &str, s: &Shape<'_>) -> Check {
    let terms: Vec<(&str, &str)> = s
        .plan
        .get("terms")
        .map_or(&[][..], |t| list(t, "introduce"))
        .iter()
        .map(|t| (text(t, "key"), text(t, "term")))
        .filter(|(k, t)| !k.is_empty() && !t.trim().is_empty())
        .collect();
    let mut bad = Vec::new();
    for (term_key, term) in terms {
        let mut pretaught = HashSet::new();
        'blocks: for b in &s.blocks {
            if kind(b) == "glossary_term" {
                pretaught.insert(text(b, "key"));
                continue;
            }
            for field in markdown_fields(b) {
                match first_mention(field, term_key, term) {
                    Some(Mention::Linked) => break 'blocks,
                    Some(Mention::Plain) => {
                        if !pretaught.contains(term_key) {
                            bad.push(format!("`{term}`"));
                        }
                        break 'blocks;
                    }
                    None => {}
                }
            }
        }
    }
    Check::of(key, bad.is_empty(), || {
        format!(
            "first mention in plain text, without a `[[key]]`: {}",
            bad.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::checks::status_of;

    fn p(id: &str, n: usize) -> Value {
        json!({"id": id, "type": "paragraph", "text": vec!["parola"; n].join(" ")})
    }

    fn h(level: u64, text: &str) -> Value {
        json!({"id": text.replace(' ', "_"), "type": "heading", "level": level, "text": text})
    }

    fn quiz(id: &str, required: bool) -> Value {
        json!({"id": id, "type": "quiz_single", "question": "Q?", "correct": "a", "required": required,
            "options": [{"id": "a", "text": "uno", "feedback": "f"}, {"id": "b", "text": "due", "feedback": "g"}]})
    }

    fn callout(variant: &str, title: &str) -> Value {
        json!({"id": "c", "type": "callout", "variant": variant, "title": title, "text": "Si crede X, ma Y."})
    }

    fn concept_section(name: &str, with_pitfall: bool) -> Vec<Value> {
        let mut v = vec![
            h(2, name),
            p(&format!("{name}_a"), 50),
            h(3, "Esempio: un caso"),
            json!({"id": format!("{name}_code"), "type": "code", "language": "text", "code": "x"}),
            p(&format!("{name}_b"), 60),
            p(&format!("{name}_c"), 40),
        ];
        if with_pitfall {
            v.push(callout("warning", "Errore tipico: copiare"));
        }
        v.push(quiz(&format!("{name}_q"), true));
        v
    }

    fn plan(general: bool, pitfalls: &[&str]) -> Value {
        json!({
            "prerequisites": if general { json!([{"statement": "a", "basis": "general", "record_numbers": []}]) } else { json!([]) },
            "concepts": pitfalls.iter().map(|p| json!({"name": "c", "pitfall": p})).collect::<Vec<_>>(),
            "terms": {"introduce": [{"key": "move", "term": "move"}], "reuse": []},
            "exercises": []
        })
    }

    /// intro + [recall] + one section per pitfall + closing.
    fn document(recall: bool, pitfalls: &[&str]) -> Value {
        let mut blocks = vec![
            h(1, "Titolo"),
            p("i1", 40),
            json!({"id": "g", "type": "glossary_term", "key": "move"}),
        ];
        if recall {
            blocks.extend([
                h(2, "Cosa ti serve: A"),
                p("r0", 30),
                quiz("rq", false),
                p("r1", 40),
                p("r2", 45),
            ]);
        }
        for (i, pit) in pitfalls.iter().enumerate() {
            blocks.extend(concept_section(&format!("s{i}"), !pit.is_empty()));
        }
        blocks.extend([
            h(2, "Per chiudere"),
            p("end", 30),
            json!({"id": "ck", "type": "checklist", "items": [{"id": "1", "text": "uno"}, {"id": "2", "text": "due"}]}),
            json!({"id": "src", "type": "primary_source", "source": "x", "why": "perché sì"}),
        ]);
        json!({"estimated_minutes": 5, "blocks": blocks})
    }

    fn run(spec: &Value, doc: &Value, plan: &Value) -> Vec<Check> {
        let blocks = sdui::top(doc);
        let all_text = sdui::visible_strings(doc).join("\n");
        checks(
            spec.as_object().unwrap(),
            &Shape {
                doc,
                plan,
                blocks,
                all_text: &all_text,
                language: domain::ContentLanguage::It,
            },
        )
    }

    #[test]
    fn a_block_is_foreign_when_its_lang_is_not_the_topics() {
        use domain::ContentLanguage::{En, It};
        let plain = json!({"type": "paragraph"});
        let it = json!({"type": "paragraph", "lang": "it-IT"});
        let en = json!({"type": "paragraph", "lang": "EN"});
        for lang in [It, En] {
            assert!(!foreign_language(&plain, lang));
        }
        assert!(!foreign_language(&it, It) && foreign_language(&it, En));
        assert!(!foreign_language(&en, En) && foreign_language(&en, It));
    }

    fn status(spec: &Value, doc: &Value, plan: &Value, name: &str) -> Status {
        status_of(&run(spec, doc, plan), name)
    }

    #[test]
    fn the_sections_follow_the_plan() {
        let plan = plan(true, &["x", ""]);
        let doc = document(true, &["x", ""]);
        let spec = json!({"sections_from_plan": {"severity": "error"}});
        assert_eq!(
            status(&spec, &doc, &plan, "sections_from_plan"),
            Status::Pass
        );
        // Without the recall: one section short.
        assert_eq!(
            status(
                &spec,
                &document(false, &["x", ""]),
                &plan,
                "sections_from_plan"
            ),
            Status::Fail
        );
    }

    fn figure() -> Value {
        json!({"id": "f", "type": "diagram", "layout": "flow", "title": "Il ciclo di vita",
            "nodes": [{"id": "n1", "label": "Parsing", "detail": "legge il testo"}, {"id": "n2", "label": "Validazione"}],
            "edges": [{"from": "n1", "to": "n2", "label": "se valido"}],
            "caption": "Le due fasi in ordine.", "alt": "Due caselle collegate da una freccia."})
    }

    #[test]
    fn a_diagram_does_not_break_the_chain() {
        let plan = plan(false, &["x", ""]);
        let mut d = document(false, &["x", ""]);
        let blocks = d["blocks"].as_array_mut().unwrap();
        // Between the intuition and the `Esempio:` heading, and among the exercises.
        let at = blocks.iter().position(|b| b["id"] == "s0_b").unwrap();
        blocks.insert(at, figure());
        assert_eq!(
            status(&json!({"concept_chain": {}}), &d, &plan, "concept_chain"),
            Status::Pass
        );
    }

    #[test]
    fn a_missing_component_does_not_break_the_chain() {
        let plan = plan(false, &["x", ""]);
        let mut d = document(false, &["x", ""]);
        let blocks = d["blocks"].as_array_mut().unwrap();
        // Where the v20 runs put it: right after the intuition, before `Esempio:`.
        let at = blocks.iter().position(|b| b["id"] == "s0_b").unwrap();
        blocks.insert(
            at,
            json!({"id": "m", "type": "missing_component", "wanted": "line_chart",
                "reason": "due curve", "fallback": [p("fb", 20)]}),
        );
        assert_eq!(
            status(&json!({"concept_chain": {}}), &d, &plan, "concept_chain"),
            Status::Pass
        );
    }

    #[test]
    fn a_diagram_is_not_prose_and_does_not_split_a_run() {
        let plan = plan(false, &["x"]);
        let doc = |with_figure: bool| {
            let mut blocks = vec![h(1, "T"), h(2, "Sezione"), p("a", 200)];
            if with_figure {
                blocks.push(figure());
            }
            blocks.extend([p("b", 120), quiz("q", true), h(2, "Fine")]);
            json!({"estimated_minutes": 5, "blocks": blocks})
        };
        let spec = json!({"explanation_run_words_max": {"max": 300, "defect_above": 350}});
        // 320 words with or without the figure: the figure neither adds words
        // (its long texts would push the run over the defect line) nor resets the run.
        let mut long = doc(true);
        long["blocks"][3]["caption"] = json!("parola ".repeat(100));
        for d in [doc(false), doc(true), long] {
            assert_eq!(
                status(&spec, &d, &plan, "explanation_run_words_max"),
                Status::Warn
            );
        }
        let words_of = |d: &Value| -> usize {
            sdui::top(d)
                .iter()
                .map(|b| sdui::prose_words_of_block(b))
                .sum()
        };
        assert_eq!(words_of(&doc(true)), words_of(&doc(false)));
    }

    #[test]
    fn a_good_chain_passes_and_every_deviation_is_named() {
        let plan = plan(false, &["x", ""]);
        let spec = json!({"concept_chain": {}, "h3_per_concept_section": {}});
        let good = document(false, &["x", ""]);
        let c = run(&spec, &good, &plan);
        assert_eq!(status_of(&c, "concept_chain"), Status::Pass, "{c:?}");
        assert_eq!(status_of(&c, "h3_per_concept_section"), Status::Pass);

        // The second concept has no pitfall in the plan: a callout is a defect.
        let extra = document(false, &["x", "y"]);
        let c = run(&spec, &extra, &plan);
        assert_eq!(status_of(&c, "concept_chain"), Status::Fail);
        // The first concept has one: a missing callout is too.
        let missing = document(false, &["", ""]);
        let c = run(&spec, &missing, &plan);
        let chain = c.iter().find(|c| c.name == "concept_chain").unwrap();
        assert_eq!(chain.status, Status::Fail);
        assert!(
            chain.detail.contains("expected the callout"),
            "{}",
            chain.detail
        );

        let mut no_h3 = document(false, &["x", ""]);
        no_h3["blocks"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["level"] != 3);
        let c = run(&spec, &no_h3, &plan);
        assert_eq!(status_of(&c, "concept_chain"), Status::Fail);
        assert_eq!(status_of(&c, "h3_per_concept_section"), Status::Fail);
    }

    #[test]
    fn the_pitfall_callout_is_a_short_warning_with_the_right_title() {
        let plan = plan(false, &["x", ""]);
        let spec = json!({"concept_chain": {}});
        let mutate = |f: &dyn Fn(&mut Value)| {
            let mut d = document(false, &["x", ""]);
            for b in d["blocks"].as_array_mut().unwrap() {
                if b["type"] == "callout" {
                    f(b);
                }
            }
            status(&spec, &d, &plan, "concept_chain")
        };
        assert_eq!(mutate(&|b| b["variant"] = json!("tip")), Status::Fail);
        assert_eq!(mutate(&|b| b["title"] = json!("Attenzione")), Status::Fail);
        assert_eq!(
            mutate(&|b| b["title"] = json!("Errore tipico: uno due tre quattro cinque sei sette")),
            Status::Fail
        );
        assert_eq!(
            mutate(&|b| b["text"] = json!("x".repeat(301))),
            Status::Fail
        );
    }

    #[test]
    fn an_exercise_before_the_third_paragraph_breaks_the_chain() {
        let plan = plan(false, &["x", ""]);
        let mut d = document(false, &["x", ""]);
        let blocks = d["blocks"].as_array_mut().unwrap();
        let at = blocks.iter().position(|b| b["id"] == "s0_b").unwrap();
        blocks.insert(at, quiz("early", true));
        assert_eq!(
            status(&json!({"concept_chain": {}}), &d, &plan, "concept_chain"),
            Status::Fail
        );
    }

    #[test]
    fn explanation_runs_skip_code_and_warn_before_failing() {
        let plan = plan(false, &["x"]);
        let long = |n: usize| json!({"estimated_minutes": 5, "blocks": [h(1, "T"), h(2, "Sezione"), p("a", n), json!({"id": "c", "type": "code", "language": "text", "code": "x"}), p("b", 100), quiz("q", true), h(2, "Fine")]});
        let spec = json!({"explanation_run_words_max": {"max": 300, "defect_above": 350}});
        let st = |n| status(&spec, &long(n), &plan, "explanation_run_words_max");
        assert_eq!(st(200), Status::Pass);
        assert_eq!(st(220), Status::Warn);
        assert_eq!(st(260), Status::Fail);
        // The closing is not an explanation run.
        let closing = json!({"estimated_minutes": 5, "blocks": [h(1, "T"), h(2, "Sezione"), p("a", 10), quiz("q", true), h(2, "Fine"), p("z", 500)]});
        assert_eq!(
            status(&spec, &closing, &plan, "explanation_run_words_max"),
            Status::Pass
        );
    }

    #[test]
    fn a_diagram_between_paragraphs_does_not_reset_the_run() {
        let fig = json!({"id": "f", "type": "diagram"});
        let doc = json!({"blocks": [p("a", 20), fig, p("b", 20), p("c", 20)]});
        let spec = json!({"paragraphs_consecutive_max": 2});
        assert_eq!(
            status(&spec, &doc, &Value::Null, "paragraphs_consecutive_max"),
            Status::Fail
        );
    }

    #[test]
    fn paragraphs_in_a_row_and_their_length() {
        let doc =
            json!({"blocks": [h(1, "T"), p("a", 20), p("b", 20), p("c", 20), quiz("q", true)]});
        let spec = json!({"paragraphs_consecutive_max": 2, "paragraph_words": {"range": [30, 110], "severity": "warn"}});
        let c = run(&spec, &doc, &Value::Null);
        assert_eq!(status_of(&c, "paragraphs_consecutive_max"), Status::Fail);
        assert_eq!(status_of(&c, "paragraph_words"), Status::Warn);
        // A quotation in another language is not measured.
        let mut quote = doc.clone();
        for b in quote["blocks"].as_array_mut().unwrap() {
            if b["type"] == "paragraph" {
                b["lang"] = json!("en");
            }
        }
        assert_eq!(
            status(&spec, &quote, &Value::Null, "paragraph_words"),
            Status::Pass
        );
    }

    #[test]
    fn callouts_are_counted_per_section_and_per_kind() {
        let doc = json!({"blocks": [h(1, "T"), h(2, "A"), callout("tip", "t"), callout("info", "i"), h(2, "B"), callout("info", "i")]});
        let spec =
            json!({"callouts": {"per_section_max": 1, "warning_max": 5, "tip_plus_info_max": 2}});
        let c = run(&spec, &doc, &Value::Null);
        let check = c.iter().find(|c| c.name == "callouts").unwrap();
        assert_eq!(check.status, Status::Fail);
        assert!(
            check.detail.contains("section `A` has 2"),
            "{}",
            check.detail
        );
        assert!(check.detail.contains("3 tip and info"), "{}", check.detail);
    }

    #[test]
    fn the_intro_is_short_and_has_no_exercise_or_subheading() {
        let spec = json!({"intro": {"paragraphs": [1, 2], "words_max": 100, "no_exercise": true, "no_h3": true}});
        let ok = json!({"blocks": [h(1, "T"), p("a", 60), h(2, "A")]});
        assert_eq!(status(&spec, &ok, &Value::Null, "intro"), Status::Pass);
        for bad in [
            json!({"blocks": [h(1, "T"), h(2, "A")]}),
            json!({"blocks": [h(1, "T"), p("a", 60), p("b", 60), h(2, "A")]}),
            json!({"blocks": [h(1, "T"), p("a", 20), quiz("q", true), h(2, "A")]}),
            json!({"blocks": [h(1, "T"), p("a", 20), h(3, "x"), h(2, "A")]}),
        ] {
            assert_eq!(
                status(&spec, &bad, &Value::Null, "intro"),
                Status::Fail,
                "{bad}"
            );
        }
    }

    #[test]
    fn the_recall_exists_iff_the_plan_has_a_general_prerequisite() {
        let spec = json!({"recall_iff_general": {"severity": "error"}});
        let with = plan(true, &["x"]);
        let without = plan(false, &["x"]);
        assert_eq!(
            status(&spec, &document(true, &["x"]), &with, "recall_iff_general"),
            Status::Pass
        );
        assert_eq!(
            status(
                &spec,
                &document(false, &["x"]),
                &without,
                "recall_iff_general"
            ),
            Status::Pass
        );
        assert_eq!(
            status(&spec, &document(false, &["x"]), &with, "recall_iff_general"),
            Status::Fail
        );
        assert_eq!(
            status(
                &spec,
                &document(true, &["x"]),
                &without,
                "recall_iff_general"
            ),
            Status::Fail
        );
    }

    #[test]
    fn the_recall_composition_is_checked() {
        let spec = json!({"recall_iff_general": {}});
        let plan = plan(true, &["x"]);
        let ok = document(true, &["x"]);
        assert_eq!(
            status(&spec, &ok, &plan, "recall_iff_general"),
            Status::Pass
        );
        let mutate = |f: &dyn Fn(&mut Vec<Value>)| {
            let mut d = ok.clone();
            f(d["blocks"].as_array_mut().unwrap());
            status(&spec, &d, &plan, "recall_iff_general")
        };
        // Required exercise.
        assert_eq!(
            mutate(&|b| b
                .iter_mut()
                .filter(|x| x["id"] == "rq")
                .for_each(|x| x["required"] = json!(true))),
            Status::Fail
        );
        // A flashcard instead of a quiz.
        assert_eq!(
            mutate(&|b| b
                .iter_mut()
                .filter(|x| x["id"] == "rq")
                .for_each(|x| x["type"] = json!("flashcard"))),
            Status::Fail
        );
        // No review paragraph after the exercise.
        assert_eq!(
            mutate(&|b| b.retain(|x| x["id"] != "r1" && x["id"] != "r2")),
            Status::Fail
        );
        // A sub-heading, or too much prose.
        assert_eq!(
            mutate(&|b| {
                let at = b.iter().position(|x| x["id"] == "r1").unwrap();
                b.insert(at, h(3, "Esempio: x"));
            }),
            Status::Fail
        );
        assert_eq!(
            mutate(&|b| b
                .iter_mut()
                .filter(|x| x["id"] == "r1")
                .for_each(|x| x["text"] = json!("parola ".repeat(300)))),
            Status::Fail
        );
        // Short review paragraphs are only a warning.
        let mut short = ok.clone();
        for b in short["blocks"].as_array_mut().unwrap() {
            if b["id"] == "r1" {
                b["text"] = json!("poche parole");
            }
        }
        let c = run(&spec, &short, &plan);
        assert_eq!(status_of(&c, "recall_iff_general"), Status::Pass);
        assert_eq!(status_of(&c, "recall_review_paragraph_words"), Status::Warn);
    }

    #[test]
    fn the_closing_counts_paragraphs_checklist_and_source() {
        let plan = plan(false, &["x"]);
        let doc = document(false, &["x"]);
        assert_eq!(
            status(
                &json!({"closing": {"words_max": 120}}),
                &doc,
                &plan,
                "closing"
            ),
            Status::Pass
        );
        assert_eq!(
            status(
                &json!({"closing": {"words_max": 5}}),
                &doc,
                &plan,
                "closing"
            ),
            Status::Fail
        );
    }

    #[test]
    fn exercises_per_concept_section_and_the_estimate() {
        let plan = plan(false, &["x", ""]);
        let doc = document(false, &["x", ""]);
        assert_eq!(
            status(
                &json!({"exercises_per_concept_section": [1, 2]}),
                &doc,
                &plan,
                "exercises_per_concept_section"
            ),
            Status::Pass
        );
        assert_eq!(
            status(
                &json!({"exercises_per_concept_section": [2, 3]}),
                &doc,
                &plan,
                "exercises_per_concept_section"
            ),
            Status::Fail
        );

        // P from the contract count, E all exercises, O open answers.
        let mut doc = doc;
        let p_words = prose_total(&doc);
        let exercises = exercises_of(&doc).len();
        let expected = lesson_minutes(p_words, exercises, 0);
        doc["estimated_minutes"] = json!(expected);
        let spec = json!({"estimated_minutes_formula": {"tolerance": 0}});
        assert_eq!(
            status(&spec, &doc, &plan, "estimated_minutes_formula"),
            Status::Pass
        );
        doc["estimated_minutes"] = json!(expected + 2);
        assert_eq!(
            status(&spec, &doc, &plan, "estimated_minutes_formula"),
            Status::Fail
        );
        let wide = json!({"estimated_minutes_formula": {"tolerance": 2}});
        assert_eq!(
            status(&wide, &doc, &plan, "estimated_minutes_formula"),
            Status::Pass
        );
        // An open answer weighs 2 + 0.5 minutes.
        assert_eq!(lesson_minutes(1500, 6, 1), 11);
    }

    #[test]
    fn blocks_max_counts_top_level_blocks_and_takes_a_number_or_an_object() {
        let plan = plan(false, &["x", ""]);
        let doc = document(false, &["x", ""]);
        let n = doc["blocks"].as_array().unwrap().len() as u64;
        let at = |spec: Value| status(&json!({ "blocks_max": spec }), &doc, &plan, "blocks_max");
        assert_eq!(at(json!(n)), Status::Pass);
        assert_eq!(at(json!(n - 1)), Status::Fail);
        assert_eq!(at(json!({"max": n})), Status::Pass);
        assert_eq!(at(json!({"max": n - 1})), Status::Fail);
        // A severity turns the failure into a warning.
        assert_eq!(at(json!({"max": n - 1, "severity": "warn"})), Status::Warn);

        // The value is checked when the suite loads.
        assert!(value_problem("blocks_max", &json!(80)).is_none());
        assert!(value_problem("blocks_max", &json!({"max": 80, "severity": "warn"})).is_none());
        for bad in [
            json!("80"),
            json!(-1),
            json!({"severity": "warn"}),
            json!([1, 2]),
        ] {
            assert!(value_problem("blocks_max", &bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn an_object_with_an_unknown_key_is_refused_so_a_typo_cannot_soften_or_harden_a_check() {
        // The typo that motivated this: the severity would be ignored and the
        // check would fail the run instead of warning.
        let typo = value_problem("blocks_max", &json!({"max": 50, "sevrity": "warn"})).unwrap();
        assert!(
            typo.contains("sevrity") && typo.contains("severity"),
            "{typo}"
        );
        for (key, bad) in [
            (
                "explanation_run_words_max",
                json!({"max": 300, "defect": 350}),
            ),
            (
                "paragraph_words",
                json!({"range": [20, 110], "sevrity": "warn"}),
            ),
            (
                "prose_words_target",
                json!({"range": [1, 2], "severty": "warn"}),
            ),
            (
                "estimated_minutes_range",
                json!({"range": [1, 2], "Severity": "warn"}),
            ),
            ("closing", json!({"words_max": 100, "max": 1})),
            ("callouts", json!({"warning_max": 5, "warnings": 1})),
            ("intro", json!({"words_max": 100, "nope": true})),
            ("recall", json!({"expected": true, "sevrity": "warn"})),
            ("estimated_minutes_formula", json!({"tolerence": 2})),
            (
                "concept_chain",
                json!({"severity": "warn", "descripton": "x"}),
            ),
            ("sections_from_plan", json!({"sevrity": "error"})),
            ("h3_per_concept_section", json!({"sevrity": "error"})),
            ("recall_iff_general", json!({"sevrity": "error"})),
            (
                "term_first_mention_is_glossary_link",
                json!({"sevrity": "error"}),
            ),
            ("blocks_max", json!({"max": 50, "severity": "warning"})),
        ] {
            assert!(value_problem(key, &bad).is_some(), "{key}: {bad}");
        }
        // The shipped shapes stay valid: meta keys and severities.
        for (key, ok) in [
            (
                "blocks_max",
                json!({"max": 50, "severity": "warn", "description": "d"}),
            ),
            (
                "explanation_run_words_max",
                json!({"max": 300, "defect_above": 350, "description": "d"}),
            ),
            (
                "prose_words_target",
                json!({"range": [1, 2], "severity": "warn"}),
            ),
            (
                "concept_chain",
                json!({"description": "d", "severity": "error"}),
            ),
            ("sections_from_plan", json!(true)),
            ("recall_iff_general", json!("warn")),
        ] {
            assert_eq!(value_problem(key, &ok), None, "{key}: {ok}");
        }
    }

    #[test]
    fn the_minutes_range_is_hard_unless_the_suite_says_warn() {
        let doc = json!({"estimated_minutes": 7, "blocks": []});
        assert_eq!(
            status(
                &json!({"estimated_minutes_range": [10, 15]}),
                &doc,
                &Value::Null,
                "estimated_minutes_range"
            ),
            Status::Fail
        );
        assert_eq!(
            status(
                &json!({"estimated_minutes_range": {"range": [10, 15], "severity": "warn"}}),
                &doc,
                &Value::Null,
                "estimated_minutes_range"
            ),
            Status::Warn
        );
    }

    #[test]
    fn the_prose_counts_like_the_contract() {
        // 3 words of paragraph + question 1 + options 1+1 and feedback 1+1.
        let doc = json!({"blocks": [{"id": "a", "type": "paragraph", "text": "uno due tre"}, quiz("q", true)]});
        assert_eq!(prose_total(&doc), 3 + 1 + 2 + 2);
        assert_eq!(
            status(
                &json!({"prose_words": [8, 8]}),
                &doc,
                &Value::Null,
                "prose_words"
            ),
            Status::Pass
        );
        assert_eq!(
            status(
                &json!({"prose_words_target": {"range": [100, 200], "severity": "warn"}}),
                &doc,
                &Value::Null,
                "prose_words_target"
            ),
            Status::Warn
        );
    }

    #[test]
    fn phrases_that_only_warn() {
        let doc = json!({"blocks": [p("a", 3), {"id": "b", "type": "paragraph", "text": "Basta installare il pacchetto."}]});
        let c = run(
            &json!({"forbidden_phrases_warn": ["(?i)\\bbasta\\s+\\w+(are|ere|ire)\\b"]}),
            &doc,
            &Value::Null,
        );
        assert_eq!(status_of(&c, "forbidden_phrases_warn"), Status::Warn);
    }

    #[test]
    fn the_recall_the_case_expects_is_confirmed_by_the_plan() {
        let spec = json!({"recall": {"expected": true, "prerequisites": 2}});
        let two =
            json!({"prerequisites": [{"basis": "general"}, {"basis": "general"}], "concepts": []});
        assert_eq!(
            status(&spec, &Value::Null, &two, "recall_expected_matches_plan"),
            Status::Pass
        );
        let one = json!({"prerequisites": [{"basis": "general"}], "concepts": []});
        assert_eq!(
            status(&spec, &Value::Null, &one, "recall_expected_matches_plan"),
            Status::Fail
        );
        let none = json!({"prerequisites": [], "concepts": []});
        assert_eq!(
            status(
                &json!({"recall": {"expected": false}}),
                &Value::Null,
                &none,
                "recall_expected_matches_plan"
            ),
            Status::Pass
        );
    }

    #[test]
    fn a_new_term_is_first_met_as_a_glossary_link() {
        let plan = |term: &str| json!({"terms": {"introduce": [{"key": "borrow", "term": term}], "reuse": []}, "concepts": [], "prerequisites": []});
        let spec = json!({"term_first_mention_is_glossary_link": {"severity": "error"}});
        let run_with = |text: &str, pre: bool| {
            let mut blocks = vec![h(1, "Il borrow")];
            if pre {
                blocks.push(json!({"id": "g", "type": "glossary_term", "key": "borrow"}));
            }
            blocks.push(json!({"id": "a", "type": "paragraph", "text": text}));
            status(
                &spec,
                &json!({"blocks": blocks}),
                &plan("borrow"),
                "term_first_mention_is_glossary_link",
            )
        };
        // The title does not count; the link does.
        assert_eq!(
            run_with("Il [[borrow]] presta un valore.", false),
            Status::Pass
        );
        assert_eq!(
            run_with("Il [[borrow|prestito]] presta un valore.", false),
            Status::Pass
        );
        // Plain first mention fails, unless the term was pre-taught by a block.
        assert_eq!(
            run_with("Il borrow presta un valore, poi [[borrow]].", false),
            Status::Fail
        );
        assert_eq!(run_with("Il borrow presta un valore.", true), Status::Pass);
        // Inline code and other keys are not mentions; a word that merely
        // contains the term is not either.
        assert_eq!(
            run_with("Scrivi `borrow` e [[altro|borrow]], poi [[borrow]].", false),
            Status::Pass
        );
        assert_eq!(
            run_with("Il borrowing non c'entra, [[borrow]].", false),
            Status::Pass
        );
        // A term that never appears is not a violation.
        assert_eq!(run_with("Nessuna menzione.", false), Status::Pass);
    }
}
