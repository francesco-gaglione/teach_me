//! Rule-by-rule tests of the Validator. Contract fixtures are exercised in
//! `crates/domain/tests/sdui_fixtures.rs`.

use std::collections::HashSet;

use serde_json::{Value, json};
use uuid::Uuid;

use super::model::show;
use super::*;

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn uuid() -> Uuid {
    Uuid::from_u128(u128::from(
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ))
}

fn opts(texts: &[&str]) -> Value {
    Value::Array(
        texts
            .iter()
            .enumerate()
            .map(|(i, t)| json!({"id": format!("o{i}"), "text": t, "feedback": "Perché sì."}))
            .collect(),
    )
}

fn quiz(id: &str, correct: &str) -> Value {
    json!({"id": id, "type": "quiz_single", "question": "Quale?",
        "options": opts(&["uno due", "tre quattro", "cinque sei"]), "correct": correct})
}

/// Minimal valid lesson; every test mutates a copy.
fn valid() -> Value {
    json!({
        "catalog_version": 1,
        "title": "Ownership",
        "objective": "Saper prevedere l'esito di una move.",
        "estimated_minutes": 8,
        "glossary": [{"key": "ownership", "term": "ownership", "definition": "Un solo proprietario."}],
        "sources": [{"key": "book", "resource_id": null, "title": "The Book", "url": "https://example.com/b"}],
        "blocks": [
            {"id": "h1", "type": "heading", "level": 1, "text": "Ownership"},
            {"id": "p1", "type": "paragraph", "text": "Il concetto di [[ownership]] [^book]."},
            quiz("q1", "o1"),
            {"id": "src", "type": "primary_source", "source": "book", "why": "Fonte."},
            {"id": "ask", "type": "ask_agent", "prompt_hint": "Spiegami meglio."}
        ]
    })
}

fn blocks(doc: &mut Value) -> &mut Vec<Value> {
    doc["blocks"].as_array_mut().unwrap()
}

fn issues(doc: &Value) -> Vec<LessonIssue> {
    LessonValidator::new(ValidatorConfig::default(), ValidationContext::fixture()).validate(doc)
}

fn codes(doc: &Value) -> Vec<&'static str> {
    issues(doc).iter().map(|i| i.code.as_str()).collect()
}

fn only(doc: &Value, code: &str, path: &str) {
    let found = issues(doc);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code.as_str(), code);
    assert_eq!(found[0].path, path);
    assert_ne!(found[0].message, "");
}

/// Valid doc whose prose has exactly `total` words.
fn with_prose(total: usize) -> Value {
    let mut doc = valid();
    let base = parse_document(&doc).0.prose_words();
    let mut left = total - base;
    let mut i = 0;
    while left > 0 {
        let n = left.min(500);
        blocks(&mut doc).insert(
            2,
            json!({"id": format!("long{i}"), "type": "paragraph", "text": "a ".repeat(n).trim()}),
        );
        left -= n;
        i += 1;
    }
    doc
}

#[test]
fn minimal_document_is_valid() {
    assert_eq!(issues(&valid()), []);
}

#[test]
fn issue_json_has_contract_shape() {
    let mut doc = valid();
    doc["title"] = json!("");
    let j = issues(&doc)[0].to_json();
    assert_eq!(j["path"], "/title");
    assert_eq!(j["code"], "schema");
    assert!(j["message"].is_string());
}

#[test]
fn schema_errors_are_all_collected_with_paths() {
    let mut doc = valid();
    doc["catalog_version"] = json!(2);
    doc["estimated_minutes"] = json!(21);
    doc["glossary"][0]["key"] = json!("Bad Key");
    blocks(&mut doc)[1]["text"] = json!(5);
    blocks(&mut doc)[0]["level"] = json!(4);
    let found = issues(&doc);
    let paths: Vec<_> = found.iter().map(|i| i.path.as_str()).collect();
    assert!(found.iter().all(|i| i.code == LessonErrorCode::Schema));
    for p in [
        "/catalog_version",
        "/estimated_minutes",
        "/glossary/0/key",
        "/blocks/1/text",
        "/blocks/0/level",
    ] {
        assert!(paths.contains(&p), "{p} missing in {paths:?}");
    }
}

#[test]
fn schema_errors_skip_cross_document_rules() {
    let mut doc = valid();
    doc["objective"] = json!("");
    blocks(&mut doc).retain(|b| b["type"] != "ask_agent");
    assert_eq!(codes(&doc), ["schema"]);
}

#[test]
fn not_an_object_and_too_few_blocks() {
    assert_eq!(issues(&json!([]))[0].path, "");
    let mut doc = valid();
    blocks(&mut doc).truncate(2);
    assert!(
        issues(&doc)
            .iter()
            .any(|i| i.path == "/blocks" && i.code == LessonErrorCode::Schema)
    );
}

#[test]
fn quiz_option_counts_by_type() {
    let mut doc = valid();
    blocks(&mut doc)[2]["options"] = opts(&["a", "b", "c", "d", "e", "f"]);
    only(&doc, "schema", "/blocks/2/options");
}

#[test]
fn unknown_block_type_is_reported_not_parse_failure() {
    let mut doc = valid();
    blocks(&mut doc).insert(2, json!({"id": "x", "type": "hologram", "foo": 1}));
    only(&doc, "unknown_block_type", "/blocks/2/type");
}

#[test]
fn duplicate_block_id_including_fallback() {
    let mut doc = valid();
    blocks(&mut doc).push(json!({"id": "m", "type": "missing_component", "wanted": "timeline",
        "purpose": "Mostrare il tempo.", "fallback": [{"id": "p1", "type": "paragraph", "text": "ciao"}]}));
    only(&doc, "duplicate_id", "/blocks/5/fallback/0/id");
}

#[test]
fn duplicate_ids_inside_blocks_and_keys() {
    let mut doc = valid();
    blocks(&mut doc)[2]["options"][1]["id"] = json!("o0");
    let found = issues(&doc);
    assert_eq!(found[0].code, LessonErrorCode::DuplicateId);
    assert_eq!(found[0].path, "/blocks/2/options/1/id");

    let mut doc = valid();
    let g = doc["glossary"][0].clone();
    doc["glossary"].as_array_mut().unwrap().push(g);
    assert_eq!(issues(&doc)[0].path, "/glossary/1/key");
}

#[test]
fn quiz_correct_must_exist() {
    let mut doc = valid();
    blocks(&mut doc)[2]["correct"] = json!("zzz");
    only(&doc, "quiz_correct_missing", "/blocks/2/correct");
}

#[test]
fn quiz_multi_all_or_none_correct() {
    let multi = |correct: Value| {
        let mut doc = valid();
        blocks(&mut doc)[2] = json!({"id": "q1", "type": "quiz_multi", "question": "Quali?",
            "options": opts(&["uno due", "tre quattro", "cinque sei"]), "correct": correct});
        doc
    };
    assert_eq!(issues(&multi(json!(["o0", "o2"]))), []);
    only(
        &multi(json!([])),
        "quiz_correct_missing",
        "/blocks/2/correct",
    );
    only(
        &multi(json!(["o0", "o1", "o2"])),
        "quiz_correct_missing",
        "/blocks/2/correct",
    );
    only(
        &multi(json!(["o0", "nope"])),
        "quiz_correct_missing",
        "/blocks/2/correct/1",
    );
    let dup = issues(&multi(json!(["o0", "o0"])));
    assert_eq!(dup[0].code, LessonErrorCode::DuplicateId);
}

#[test]
fn option_length_tolerance_is_exact() {
    let four = "a b c d";
    let mut doc = valid();
    // The shortest option is the right one: the length cue is not at play here.
    blocks(&mut doc)[2]["correct"] = json!("o0");
    // 1 vs 4 words: spread 3 = tolerance, allowed.
    blocks(&mut doc)[2]["options"] = opts(&["a", four, "a b"]);
    assert_eq!(issues(&doc), []);
    // 1 vs 5 words: spread 4, rejected and attributed to the longest option.
    blocks(&mut doc)[2]["options"] = opts(&["a", "a b c d e", "a b"]);
    only(&doc, "quiz_option_length", "/blocks/2/options/1/text");
}

#[test]
fn option_length_tolerance_is_configurable() {
    let mut doc = valid();
    blocks(&mut doc)[2]["correct"] = json!("o0");
    blocks(&mut doc)[2]["options"] = opts(&["a", "a b c d e", "a b"]);
    let cfg = ValidatorConfig {
        option_word_tolerance: 4,
        ..ValidatorConfig::default()
    };
    assert_eq!(
        LessonValidator::new(cfg, ValidationContext::fixture()).validate(&doc),
        []
    );
}

#[test]
fn answer_position_needs_three_quizzes_all_equal() {
    let mut doc = valid();
    blocks(&mut doc)[2] = quiz("q1", "o1");
    blocks(&mut doc).insert(3, quiz("q2", "o1"));
    assert_eq!(issues(&doc), [], "two quizzes are never flagged");
    blocks(&mut doc).insert(4, quiz("q3", "o1"));
    only(&doc, "quiz_answer_position", "/blocks/2/correct");
    blocks(&mut doc)[4] = quiz("q3", "o2");
    assert_eq!(issues(&doc), []);
}

fn fill(text: &str, blank_ids: &[&str]) -> Value {
    let blanks: Vec<_> = blank_ids
        .iter()
        .map(|b| json!({"id": b, "answers": ["x"]}))
        .collect();
    json!({"id": "f1", "type": "fill_blank", "text": text, "blanks": blanks,
        "feedback": {"correct": "Bene.", "incorrect": "Riprova."}})
}

#[test]
fn fill_blank_placeholders_and_blanks_must_match() {
    let mut doc = valid();
    blocks(&mut doc).insert(3, fill("Il {{a}} e il {{b}}", &["a", "b"]));
    assert_eq!(issues(&doc), []);
    blocks(&mut doc)[3] = fill("Il {{a}} e il {{b}}", &["a"]);
    only(&doc, "fill_blank_mismatch", "/blocks/3/text");
    blocks(&mut doc)[3] = fill("Il {{a}}", &["a", "b"]);
    only(&doc, "fill_blank_mismatch", "/blocks/3/blanks/1");
}

#[test]
fn citations_must_exist_and_sources_be_used() {
    let mut doc = valid();
    blocks(&mut doc)[1]["text"] = json!("Vedi [[ownership]] [^nope].");
    let c = issues(&doc);
    assert_eq!(c[0].code, LessonErrorCode::UnknownCitation);
    assert_eq!(c[0].path, "/blocks/1/text");
    assert_eq!(
        codes(&doc),
        ["unknown_citation"],
        "book is still used by primary_source"
    );
}

#[test]
fn glossary_keys_must_exist_and_entries_be_used() {
    let mut doc = valid();
    blocks(&mut doc)[1]["text"] = json!("Vedi [[nope]] [^book].");
    let found = issues(&doc);
    let c: Vec<_> = found
        .iter()
        .map(|i| (i.code.as_str(), i.path.as_str()))
        .collect();
    assert_eq!(
        c,
        [
            ("unknown_glossary_key", "/blocks/1/text"),
            ("unused_entry", "/glossary/0")
        ]
    );

    let mut doc = valid();
    blocks(&mut doc).insert(
        2,
        json!({"id": "gt", "type": "glossary_term", "key": "other"}),
    );
    only(&doc, "unknown_glossary_key", "/blocks/2/key");
}

#[test]
fn unused_source_is_reported() {
    let mut doc = valid();
    doc["sources"].as_array_mut().unwrap().push(
        json!({"key": "extra", "resource_id": null, "title": "Extra", "url": "https://example.com/e"}),
    );
    only(&doc, "unused_entry", "/sources/1");
}

#[test]
fn topic_glossary_is_accepted_in_strict_mode() {
    let mut doc = valid();
    blocks(&mut doc)[1]["text"] = json!("Vedi [[ownership]] e [[borrow]] [^book].");
    assert_eq!(codes(&doc), ["unknown_glossary_key"]);
    let ctx = ValidationContext {
        topic_glossary_keys: HashSet::from(["borrow".to_owned()]),
        ..ValidationContext::fixture()
    };
    assert_eq!(
        LessonValidator::new(ValidatorConfig::default(), ctx).validate(&doc),
        []
    );
}

#[test]
fn resource_ids_checked_only_in_strict_mode() {
    let known = uuid();
    let mut doc = valid();
    let strict = |res: &[Uuid]| {
        LessonValidator::new(
            ValidatorConfig::default(),
            ValidationContext::strict(
                res.iter().copied().collect(),
                HashSet::new(),
                HashSet::new(),
            ),
        )
    };
    let found = strict(&[known]).validate(&doc);
    assert_eq!(found.len(), 1, "null resource_id rejected in strict mode");
    assert_eq!(found[0].code, LessonErrorCode::UnknownResource);
    assert_eq!(found[0].path, "/sources/0/resource_id");

    doc["sources"][0]["resource_id"] = json!(known.to_string());
    assert_eq!(strict(&[known]).validate(&doc), []);
    assert_eq!(strict(&[uuid()]).validate(&doc).len(), 1);
    assert_eq!(
        issues(&doc),
        [],
        "fixture mode accepts any well-formed uuid"
    );
}

#[test]
fn source_url_is_null_if_and_only_if_the_resource_is_a_document() {
    let pdf = uuid();
    let web = uuid();
    let validator = LessonValidator::new(
        ValidatorConfig::default(),
        ValidationContext::production([pdf, web].into_iter().collect(), HashSet::new())
            .with_document_resources([pdf].into_iter().collect()),
    );
    let mut doc = valid();
    // A web resource with its https URL, and a PDF with a null URL: fine.
    doc["sources"][0]["resource_id"] = json!(web.to_string());
    assert_eq!(validator.validate(&doc), []);
    doc["sources"][0]["resource_id"] = json!(pdf.to_string());
    doc["sources"][0]["url"] = Value::Null;
    assert_eq!(validator.validate(&doc), []);

    // A PDF that carries a URL.
    doc["sources"][0]["url"] = json!("https://example.com/b");
    let found = validator.validate(&doc);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, "/sources/0/url");
    assert_eq!(found[0].code, LessonErrorCode::Schema);

    // A web resource without a URL.
    doc["sources"][0]["resource_id"] = json!(web.to_string());
    doc["sources"][0]["url"] = Value::Null;
    let found = validator.validate(&doc);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, "/sources/0/url");

    // Fixture mode has no resources to compare with: null is accepted.
    assert_eq!(issues(&doc), []);
}

#[test]
fn lesson_link_checked_only_in_strict_mode() {
    let id = uuid();
    let mut doc = valid();
    blocks(&mut doc).insert(
        2,
        json!({"id": "ll", "type": "lesson_link", "lesson_id": id.to_string(), "label": "Prima"}),
    );
    assert_eq!(issues(&doc), []);
    let ctx = ValidationContext {
        lesson_ids: Some(HashSet::new()),
        ..ValidationContext::fixture()
    };
    let found = LessonValidator::new(ValidatorConfig::default(), ctx).validate(&doc);
    assert_eq!(found[0].code, LessonErrorCode::InvalidLessonLink);
    assert_eq!(found[0].path, "/blocks/2/lesson_id");
    blocks(&mut doc)[2]["lesson_id"] = json!("not-a-uuid");
    only(&doc, "schema", "/blocks/2/lesson_id");
}

#[test]
fn primary_source_exactly_one() {
    let mut doc = valid();
    blocks(&mut doc).retain(|b| b["type"] != "primary_source");
    // `book` is still cited in p1, so only the count is wrong.
    only(&doc, "primary_source_count", "/blocks");
    let mut doc = valid();
    let again = json!({"id": "src2", "type": "primary_source", "source": "book", "why": "Ancora."});
    blocks(&mut doc).push(again);
    only(&doc, "primary_source_count", "/blocks");
}

#[test]
fn ask_agent_required() {
    let mut doc = valid();
    blocks(&mut doc).retain(|b| b["type"] != "ask_agent");
    only(&doc, "ask_agent_missing", "/blocks");
}

#[test]
fn required_exercise_needed() {
    let mut doc = valid();
    blocks(&mut doc)[2]["required"] = json!(false);
    only(&doc, "no_required_exercise", "/blocks");
    // `required` defaults to true; a checklist never counts.
    blocks(&mut doc)[2]
        .as_object_mut()
        .unwrap()
        .remove("required");
    assert_eq!(issues(&doc), []);
    blocks(&mut doc)[2] =
        json!({"id": "c", "type": "checklist", "items": [{"id": "i", "text": "Fatto"}]});
    only(&doc, "no_required_exercise", "/blocks");
}

#[test]
fn block_limit_is_60() {
    let mut doc = valid();
    while blocks(&mut doc).len() < 60 {
        let n = blocks(&mut doc).len();
        blocks(&mut doc).insert(
            2,
            json!({"id": format!("x{n}"), "type": "paragraph", "text": "ok"}),
        );
    }
    assert_eq!(issues(&doc), []);
    blocks(&mut doc).insert(
        2,
        json!({"id": "x_last", "type": "paragraph", "text": "ok"}),
    );
    // The 60-block cap is a shape rule in the contract (§9), not `too_long`.
    only(&doc, "schema", "/blocks");
}

#[test]
fn section_limit_is_6() {
    let mut doc = valid();
    for i in 0..6 {
        blocks(&mut doc).insert(
            2,
            json!({"id": format!("s{i}"), "type": "heading", "level": 2, "text": "Sezione"}),
        );
        blocks(&mut doc).insert(
            3,
            json!({"id": format!("c{i}"), "type": "paragraph", "text": "Contenuto"}),
        );
    }
    assert_eq!(issues(&doc), [], "6 sections are fine");
    blocks(&mut doc).insert(
        2,
        json!({"id": "s7", "type": "heading", "level": 2, "text": "Sezione"}),
    );
    blocks(&mut doc).insert(
        3,
        json!({"id": "c7", "type": "paragraph", "text": "Contenuto"}),
    );
    only(&doc, "too_long", "/blocks");
}

#[test]
fn prose_limit_is_2200_words() {
    assert_eq!(issues(&with_prose(2200)), []);
    only(&with_prose(2201), "too_long", "/blocks");
}

#[test]
fn prose_limit_is_configurable() {
    let doc = valid();
    let words = parse_document(&doc).0.prose_words();
    let at = |max| {
        LessonValidator::new(
            ValidatorConfig {
                max_prose_words: max,
                ..ValidatorConfig::default()
            },
            ValidationContext::fixture(),
        )
        .validate(&doc)
    };
    assert_eq!(at(words), []);
    assert_eq!(at(words - 1).len(), 1);
}

#[test]
fn prose_counts_quiz_question_options_and_feedback() {
    let doc = parse_document(&valid()).0;
    // p1: "Il concetto di ownership ." = 5 tokens; question 1; options 2+2+2.
    assert_eq!(doc.prose_words(), 5 + 1 + 6 + 6);
}

#[test]
fn glossary_definitions_do_not_count_as_use() {
    let mut doc = valid();
    doc["glossary"].as_array_mut().unwrap().push(
        json!({"key": "move", "term": "move", "definition": "Vedi [[ownership]]; usata in [[other]] [^book]."}),
    );
    blocks(&mut doc)[1]["text"] = json!("Il concetto di [[ownership]] [^book].");
    // `move` is only mentioned by nobody; `other` is unknown even in a definition.
    let found = issues(&doc);
    let c: Vec<_> = found
        .iter()
        .map(|i| (i.code.as_str(), i.path.as_str()))
        .collect();
    assert_eq!(
        c,
        [
            ("markdown", "/glossary/1/definition"),
            ("unused_entry", "/glossary/1")
        ]
    );
}

#[test]
fn repeated_placeholder_is_a_mismatch() {
    let mut doc = valid();
    blocks(&mut doc).insert(3, fill("Il {{a}} e ancora {{a}}", &["a"]));
    only(&doc, "fill_blank_mismatch", "/blocks/3/text");
}

#[test]
fn code_spans_are_literal() {
    let mut doc = valid();
    blocks(&mut doc)[1]["text"] = json!("Il concetto di [[ownership]] [^book] e `<b>[[x]] [^y]`.");
    assert_eq!(issues(&doc), []);
}

#[test]
fn headings_only_section_is_empty() {
    let mut doc = valid();
    blocks(&mut doc).insert(
        2,
        json!({"id": "s1", "type": "heading", "level": 2, "text": "Vuota"}),
    );
    blocks(&mut doc).insert(
        3,
        json!({"id": "s1b", "type": "heading", "level": 3, "text": "Sotto"}),
    );
    let more = [
        json!({"id": "s2", "type": "heading", "level": 2, "text": "Piena"}),
        json!({"id": "c2", "type": "paragraph", "text": "ok"}),
    ];
    for (i, b) in more.into_iter().enumerate() {
        blocks(&mut doc).insert(4 + i, b);
    }
    only(&doc, "section_without_content", "/blocks/2");
}

#[test]
fn fallback_blocks_do_not_count_for_global_rules() {
    let mut doc = valid();
    blocks(&mut doc).retain(|b| b["type"] != "ask_agent");
    blocks(&mut doc).push(json!({"id": "m", "type": "missing_component", "wanted": "timeline",
        "purpose": "Tempo.", "fallback": [{"id": "ask2", "type": "ask_agent", "prompt_hint": "Boh"}]}));
    only(&doc, "ask_agent_missing", "/blocks");
}

#[test]
fn first_block_must_be_h1() {
    let mut doc = valid();
    blocks(&mut doc)[0]["level"] = json!(2);
    let found = issues(&doc);
    assert_eq!(found[0].code, LessonErrorCode::FirstBlockNotTitle);
    assert_eq!(found[0].path, "/blocks/0");
}

#[test]
fn section_needs_content() {
    let mut doc = valid();
    blocks(&mut doc).insert(
        2,
        json!({"id": "s1", "type": "heading", "level": 2, "text": "Vuota"}),
    );
    blocks(&mut doc).insert(
        3,
        json!({"id": "s2", "type": "heading", "level": 2, "text": "Piena"}),
    );
    blocks(&mut doc).insert(4, json!({"id": "c2", "type": "paragraph", "text": "ok"}));
    only(&doc, "section_without_content", "/blocks/2");
    // A trailing empty section is empty too.
    let mut doc = valid();
    blocks(&mut doc).push(json!({"id": "s1", "type": "heading", "level": 2, "text": "Ultima"}));
    only(&doc, "section_without_content", "/blocks/5");
}

#[test]
fn missing_component_cannot_nest() {
    let mut doc = valid();
    let inner = json!({"id": "in", "type": "missing_component", "wanted": "chart",
        "purpose": "Un grafico.", "fallback": [{"id": "f", "type": "paragraph", "text": "testo"}]});
    blocks(&mut doc).push(
        json!({"id": "m", "type": "missing_component", "wanted": "timeline",
        "purpose": "Linea del tempo.", "fallback": [inner]}),
    );
    only(&doc, "nested_missing_component", "/blocks/5/fallback/0");
}

#[test]
fn markdown_restrictions() {
    let bad = |text: &str| {
        let mut doc = valid();
        blocks(&mut doc)[1]["text"] = json!(format!("{text} [[ownership]] [^book]"));
        doc
    };
    for text in [
        "riga\nriga",
        "# Titolo",
        "- voce",
        "![x](https://a.b/i.png)",
        "<b>x</b>",
        "[x](http://a.b)",
    ] {
        let found = issues(&bad(text));
        assert!(!found.is_empty(), "{text:?} accepted");
        assert!(
            found
                .iter()
                .all(|i| i.code == LessonErrorCode::Markdown && i.path == "/blocks/1/text"),
            "{text:?}: {found:?}"
        );
    }
    assert_eq!(
        issues(&bad("**forte** *corsivo* `code` [ok](https://a.b)")),
        []
    );
}

#[test]
fn markdown_is_checked_in_nested_and_option_fields() {
    let mut doc = valid();
    blocks(&mut doc)[2]["options"][0]["feedback"] = json!("a\nb");
    only(&doc, "markdown", "/blocks/2/options/0/feedback");
}

#[test]
fn multiple_independent_errors_are_all_returned() {
    let mut doc = valid();
    blocks(&mut doc).retain(|b| b["type"] != "ask_agent");
    blocks(&mut doc)[2]["correct"] = json!("zzz");
    blocks(&mut doc)[0]["level"] = json!(3);
    let mut c = codes(&doc);
    c.sort_unstable();
    assert_eq!(
        c,
        [
            "ask_agent_missing",
            "first_block_not_title",
            "quiz_correct_missing"
        ]
    );
}

#[test]
fn bad_url_and_lang_are_schema_errors() {
    let mut doc = valid();
    doc["sources"][0]["url"] = json!("http://example.com");
    blocks(&mut doc)[1]["lang"] = json!("english-long-tag-");
    let found = issues(&doc);
    let paths: Vec<_> = found.iter().map(|i| i.path.as_str()).collect();
    assert_eq!(paths, ["/sources/0/url", "/blocks/1/lang"]);
}

#[test]
fn every_block_type_parses() {
    let mut doc = valid();
    let extra = json!([
        {"id": "h2", "type": "heading", "level": 2, "text": "Sezione"},
        {"id": "cl", "type": "callout", "variant": "tip", "title": "Nota", "text": "Attenzione."},
        {"id": "co", "type": "code", "language": "rust", "code": "fn main() {}", "caption": "Esempio"},
        {"id": "gt", "type": "glossary_term", "key": "ownership"},
        {"id": "qm", "type": "quiz_multi", "question": "Quali?", "options": opts(&["uno due", "tre quattro", "cinque sei"]), "correct": ["o0"], "required": false},
        fill("Il {{a}}", &["a"]),
        {"id": "os", "type": "order_steps", "prompt": "Ordina.", "steps": [{"id": "a", "text": "Uno"}, {"id": "b", "text": "Due"}, {"id": "c", "text": "Tre"}],
         "feedback": {"correct": "Ok", "incorrect": "No"}, "required": false},
        {"id": "fc", "type": "flashcard", "front": "Fronte", "back": "Retro", "required": false},
        {"id": "oa", "type": "open_answer", "question": "Spiega.", "max_chars": 500, "rubric": "Deve citare X.", "required": false},
        {"id": "ck", "type": "checklist", "items": [{"id": "i", "text": "Fatto"}]},
        {"id": "mc", "type": "missing_component", "wanted": "timeline", "purpose": "Tempo.",
         "fallback": [{"id": "fb", "type": "paragraph", "text": "testo"}]}
    ]);
    for b in extra.as_array().unwrap() {
        blocks(&mut doc).insert(4, b.clone());
    }
    assert_eq!(issues(&doc), []);
}

#[test]
fn unknown_fields_are_rejected_as_schema() {
    type Mutation = Box<dyn Fn(&mut Value)>;
    let cases: Vec<(&str, Mutation)> = vec![
        ("/extra", Box::new(|d| d["extra"] = json!(1))),
        (
            "/glossary/0/extra",
            Box::new(|d| d["glossary"][0]["extra"] = json!(1)),
        ),
        (
            "/sources/0/extra",
            Box::new(|d| d["sources"][0]["extra"] = json!(1)),
        ),
        (
            "/blocks/1/extra",
            Box::new(|d| d["blocks"][1]["extra"] = json!(1)),
        ),
        (
            "/blocks/2/options/0/extra",
            Box::new(|d| d["blocks"][2]["options"][0]["extra"] = json!(1)),
        ),
        (
            "/blocks/5/fallback/0/extra",
            Box::new(|d| {
                d["blocks"].as_array_mut().unwrap().push(
                    json!({"id": "m", "type": "missing_component",
                    "wanted": "timeline", "purpose": "Tempo.",
                    "fallback": [{"id": "fb", "type": "paragraph", "text": "t", "extra": 1}]}),
                );
            }),
        ),
    ];
    for (path, mutate) in cases {
        let mut doc = valid();
        mutate(&mut doc);
        let found = issues(&doc);
        assert_eq!(found.len(), 1, "{path}: {found:?}");
        assert_eq!(found[0].code, LessonErrorCode::Schema);
        assert_eq!(found[0].path, path);
    }
}

#[test]
fn unknown_type_block_is_not_checked_for_fields() {
    let mut doc = valid();
    blocks(&mut doc).insert(2, json!({"id": "x", "type": "hologram", "anything": 1}));
    only(&doc, "unknown_block_type", "/blocks/2/type");
}

#[test]
fn code_language_pattern() {
    let mut doc = valid();
    let code = |l: &str| json!({"id": "c", "type": "code", "language": l, "code": "x"});
    blocks(&mut doc).insert(2, code("c++"));
    assert_eq!(issues(&doc), []);
    blocks(&mut doc)[2] = code("Rust");
    only(&doc, "schema", "/blocks/2/language");
}

#[test]
fn code_highlight_lines_rules() {
    let with = |hl: Value| {
        let mut doc = valid();
        blocks(&mut doc).insert(
            2,
            json!({"id": "c", "type": "code", "language": "rust", "code": "a\nb\nc", "highlight_lines": hl}),
        );
        doc
    };
    assert_eq!(issues(&with(json!([1, 3]))), []);
    only(&with(json!([4])), "schema", "/blocks/2/highlight_lines/0");
    only(&with(json!([0])), "schema", "/blocks/2/highlight_lines/0");
    only(
        &with(json!([1, 1])),
        "schema",
        "/blocks/2/highlight_lines/1",
    );
    only(&with(json!([])), "schema", "/blocks/2/highlight_lines");
    let seven = issues(&with(json!([1, 2, 3, 4, 5, 6, 7])));
    assert!(
        seven.iter().any(|i| i.path == "/blocks/2/highlight_lines"),
        "{seven:?}"
    );
}

#[test]
fn glossary_definition_cannot_contain_term_refs() {
    let mut doc = valid();
    doc["glossary"][0]["definition"] = json!("Vedi [[ownership]].");
    let found = issues(&doc);
    assert!(
        found
            .iter()
            .any(|i| i.code == LessonErrorCode::Markdown && i.path == "/glossary/0/definition"),
        "{found:?}"
    );
}

#[test]
fn extra_key_is_escaped_in_json_pointer() {
    let mut doc = valid();
    doc["we/ird~key"] = json!(1);
    only(&doc, "schema", "/we~1ird~0key");
}

#[test]
fn hostile_inline_input_is_linear() {
    use std::time::{Duration, Instant};
    let start = Instant::now();
    for unit in ["[ ", "[[", "[^", "`", "<a", "[a](", "![", "{{"] {
        let text = unit.repeat(100_000);
        let _ = scan_inline(&text);
        let _ = placeholders(&text);
    }
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn oversized_fields_are_not_scanned() {
    let mut doc = valid();
    blocks(&mut doc)[1]["text"] = json!("[ ".repeat(50_000));
    let start = std::time::Instant::now();
    only(&doc, "schema", "/blocks/1/text");
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}

#[test]
fn oversized_document_is_a_single_too_long() {
    let mut doc = valid();
    doc["extra_padding"] = json!("x".repeat(300 * 1024));
    only(&doc, "too_long", "");
}

#[test]
fn unknown_and_nested_blocks_skip_id_and_lang_checks() {
    let mut doc = valid();
    blocks(&mut doc).insert(2, json!({"id": "BAD ID", "lang": 5, "type": "hologram"}));
    only(&doc, "unknown_block_type", "/blocks/2/type");
    let mut doc = valid();
    let inner = json!({"id": "BAD ID", "lang": 5, "type": "missing_component", "wanted": 1});
    blocks(&mut doc).push(
        json!({"id": "m", "type": "missing_component", "wanted": "timeline",
        "purpose": "Tempo.", "fallback": [inner]}),
    );
    only(&doc, "nested_missing_component", "/blocks/5/fallback/0");
}

#[test]
fn callout_title_is_plain_text_but_counts_as_prose() {
    let mut doc = valid();
    blocks(&mut doc).insert(
        2,
        json!({"id": "c", "type": "callout", "variant": "tip", "title": "# [[nope]] due tre", "text": "ok"}),
    );
    assert_eq!(issues(&doc), []);
    let base = parse_document(&valid()).0.prose_words();
    assert_eq!(parse_document(&doc).0.prose_words(), base + 4 + 1);
}

#[test]
fn schema_alignment_details() {
    let wanted = |w: &str| {
        let mut doc = valid();
        blocks(&mut doc).push(json!({"id": "m", "type": "missing_component", "wanted": w,
            "purpose": "x", "fallback": [{"id": "f", "type": "paragraph", "text": "t"}]}));
        issues(&doc)
    };
    assert_eq!(wanted(&"a".repeat(60)), []);
    for bad in ["Abc", "1abc", "_a", "a-b", &"a".repeat(61)] {
        assert!(!wanted(bad).is_empty(), "{bad}");
    }

    let mut doc = valid();
    doc["estimated_minutes"] = json!(8.0);
    assert_eq!(issues(&doc), []);
    doc["estimated_minutes"] = json!(8.5);
    only(&doc, "schema", "/estimated_minutes");

    let id = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    let mut doc = valid();
    doc["sources"][0]["resource_id"] = json!(id.replace('-', ""));
    only(&doc, "schema", "/sources/0/resource_id");
    doc["sources"][0]["resource_id"] = json!(id.to_uppercase());
    assert_eq!(issues(&doc), []);
}

#[test]
fn messages_truncate_and_escape_input_values() {
    let mut doc = valid();
    let long = format!("{}\u{7}x", "k".repeat(100));
    blocks(&mut doc)[1]["text"] = json!(format!("Vedi [^{long}] [[ownership]] [^book]"));
    let found = issues(&doc);
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains('…'), "{}", found[0].message);
    assert!(
        found[0].message.chars().count() < 160,
        "{}",
        found[0].message
    );
    assert!(!found[0].message.contains('\u{7}'));
    assert_eq!(show("a\nb"), "a\\u{a}b");
    assert_eq!(show(&"é".repeat(61)), format!("{}…", "é".repeat(60)));
}

#[test]
fn unicode_line_separators_count_as_newlines() {
    for sep in ['\u{85}', '\u{2028}', '\u{2029}'] {
        assert_eq!(
            scan_inline(&format!("a{sep}b")).problems,
            [MarkdownProblem::Newline]
        );
    }
}

#[test]
fn main_rules_report_expected_paths() {
    let mut doc = valid();
    blocks(&mut doc)[1]["text"] = json!("Vedi [[ownership]] [^nope] [^book].");
    only(&doc, "unknown_citation", "/blocks/1/text");
    let mut doc = valid();
    blocks(&mut doc)[0]["level"] = json!(2);
    blocks(&mut doc).remove(1); // keep h2 non-empty: quiz follows
    let found = issues(&doc);
    let first = found
        .iter()
        .find(|i| i.code == LessonErrorCode::FirstBlockNotTitle)
        .expect("first_block_not_title");
    assert_eq!(first.path, "/blocks/0");
}

#[test]
fn a_correct_option_much_longer_than_the_wrong_ones_is_a_cue() {
    let mut doc = valid();
    // Correct o1: 35 characters against a median of 8.
    blocks(&mut doc)[2]["options"] =
        opts(&["uno due", "tre quattro cinque sei sette otto", "nove dieci"]);
    blocks(&mut doc)[2]["options"][1]["text"] = json!("tre quattro cinque sei sette otto");
    let found = issues(&doc);
    assert!(
        found
            .iter()
            .any(|i| i.code == LessonErrorCode::QuizOptionLengthCue
                && i.path == "/blocks/2/options/1/text"),
        "{found:?}"
    );
}

#[test]
fn the_cue_threshold_is_one_point_three_times_the_median() {
    let mut doc = valid();
    let wrong = "a".repeat(10);
    let cue_with = |doc: &mut Value, correct_len: usize| {
        let correct = "b".repeat(correct_len);
        blocks(doc)[2]["options"] = opts(&[&wrong, &correct, &wrong]);
        issues(doc)
            .iter()
            .any(|i| i.code == LessonErrorCode::QuizOptionLengthCue)
    };
    // Median 10: 13 is exactly 1.3 times, 14 is just above.
    assert!(!cue_with(&mut doc, 13));
    assert!(cue_with(&mut doc, 14));
    let message = issues(&doc)
        .into_iter()
        .find(|i| i.code == LessonErrorCode::QuizOptionLengthCue)
        .expect("cue")
        .message;
    assert!(message.contains("1,3 volte"), "{message}");
}

#[test]
fn similar_lengths_pass_the_cue_check_and_markup_is_not_counted() {
    let mut doc = valid();
    blocks(&mut doc)[2]["options"] = opts(&["uno due", "tre **quattro**", "nove dieci"]);
    assert!(
        issues(&doc)
            .iter()
            .all(|i| i.code != LessonErrorCode::QuizOptionLengthCue)
    );
    // The same words with heavy markup stay at the same visible length.
    blocks(&mut doc)[2]["options"] = opts(&["uno due", "`tre` quattro", "nove dieci"]);
    assert!(
        issues(&doc)
            .iter()
            .all(|i| i.code != LessonErrorCode::QuizOptionLengthCue)
    );
}

#[test]
fn each_correct_option_of_a_quiz_multi_is_compared_with_the_wrong_ones() {
    let mut doc = valid();
    blocks(&mut doc)[2] = json!({"id": "q1", "type": "quiz_multi", "question": "Quali?",
        "options": opts(&["uno due", "tre cui", "cinque sei sette otto nove dieci undici", "ab cd"]),
        "correct": ["o1", "o2"]});
    let found = issues(&doc);
    let cues: Vec<_> = found
        .iter()
        .filter(|i| i.code == LessonErrorCode::QuizOptionLengthCue)
        .collect();
    assert_eq!(cues.len(), 1, "{found:?}");
    assert_eq!(cues[0].path, "/blocks/2/options/2/text");
}

#[test]
fn document_minutes_come_from_prose_and_exercises_not_from_the_model() {
    let mut doc = valid();
    // Whatever the model wrote, the figure is recomputed.
    doc["estimated_minutes"] = json!(19);
    let words = parse_document(&doc).0.prose_words();
    let expected = lesson_minutes(words, 1, 0);
    assert_eq!(document_minutes(&doc), Some(expected));
    assert_eq!(expected, 1, "a short document takes the minimum");

    // An open answer adds two minutes on top of its half.
    blocks(&mut doc).insert(
        3,
        json!({"id": "oa", "type": "open_answer", "question": "Spiega.", "max_chars": 500, "rubric": "Deve citare X.", "required": false}),
    );
    assert_eq!(document_minutes(&doc), Some(lesson_minutes(words, 2, 1)));

    // Exercises inside a fallback do not count (`sdui.md` §9.6).
    let mut doc = valid();
    blocks(&mut doc).insert(
        3,
        json!({"id": "mc", "type": "missing_component", "wanted": "timeline", "purpose": "x",
            "fallback": [quiz("q2", "o0")]}),
    );
    assert_eq!(document_minutes(&doc), Some(lesson_minutes(words, 1, 0)));

    assert_eq!(document_minutes(&json!({"blocks": 3})), None);
}

#[test]
fn option_length_message_gives_the_words_per_option_and_a_valid_range() {
    let mut doc = valid();
    blocks(&mut doc)[2]["correct"] = json!("o0");
    blocks(&mut doc)[2]["options"] = opts(&["a", "a b c d e f g", "a b c d e f"]);
    let found = issues(&doc);
    assert_eq!(found.len(), 1, "{found:?}");
    // The window with most options already in it is 6..9 (two of three): only
    // the first option is out of range.
    assert!(
        found[0].message.contains(
            "opzione 0: 1 parole; opzione 1: 7 parole; opzione 2: 6 parole. Riscrivi le opzioni fuori intervallo in modo che tutte abbiano tra 6 e 9 parole."
        ),
        "{}",
        found[0].message
    );
}

// ---- diagram (contract §10) ---------------------------------------------

fn diagram(layout: &str, nodes: &[&str], edges: &[(&str, &str)]) -> Value {
    json!({
        "id": "fig", "type": "diagram", "layout": layout,
        "nodes": nodes.iter().map(|n| json!({"id": n, "label": n})).collect::<Vec<_>>(),
        "edges": edges.iter().map(|(f, t)| json!({"from": f, "to": t})).collect::<Vec<_>>(),
        "caption": "Guarda **l'ordine**.",
        "alt": "Una figura."
    })
}

fn with_diagram(d: Value) -> Value {
    let mut doc = valid();
    blocks(&mut doc).insert(2, d);
    doc
}

fn diagram_issues(d: Value) -> Vec<LessonIssue> {
    issues(&with_diagram(d))
}

#[test]
fn valid_diagrams_of_all_layouts_pass() {
    for d in [
        diagram("flow", &["a", "b", "c"], &[("a", "b"), ("b", "c")]),
        // Branch and rejoin: a -> b, a -> c, b -> d, c -> d.
        diagram(
            "flow",
            &["a", "b", "c", "d"],
            &[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")],
        ),
        diagram("tree", &["r", "x", "y"], &[("r", "x"), ("r", "y")]),
        diagram("tree", &["r", "x"], &[("r", "x")]),
        diagram("cycle", &["a", "b", "c"], &[]),
    ] {
        assert_eq!(diagram_issues(d.clone()), [], "{d}");
    }
    let mut no_edges = diagram("cycle", &["a", "b", "c"], &[]);
    no_edges.as_object_mut().unwrap().remove("edges");
    assert_eq!(diagram_issues(no_edges), []);
}

#[test]
fn diagram_inside_a_fallback_is_valid_and_checked() {
    let mut doc = valid();
    let mc = |d: Value| {
        json!({"id": "mc", "type": "missing_component", "wanted": "timeline",
        "purpose": "Tempo.", "fallback": [d]})
    };
    blocks(&mut doc).insert(2, mc(diagram("flow", &["a", "b"], &[("a", "b")])));
    assert_eq!(issues(&doc), []);
    blocks(&mut doc)[2]["fallback"][0] = diagram("flow", &["a", "b"], &[("a", "z")]);
    let found = issues(&doc);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "/blocks/2/fallback/0/edges/0/to");
}

fn only_diagram(d: Value, path: &str) -> LessonIssue {
    let found = diagram_issues(d);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code.as_str(), "diagram_structure");
    assert_eq!(found[0].path, path);
    found[0].clone()
}

#[test]
fn diagram_edge_to_a_missing_node() {
    let i = only_diagram(
        diagram("flow", &["a", "b"], &[("a", "b"), ("b", "zz")]),
        "/blocks/2/edges/1/to",
    );
    assert!(
        i.message.contains("'zz'") && i.message.contains("nodes"),
        "{}",
        i.message
    );
    only_diagram(
        diagram("tree", &["a", "b"], &[("qq", "b")]),
        "/blocks/2/edges/0/from",
    );
}

#[test]
fn diagram_self_loop_and_repeated_pair() {
    only_diagram(
        diagram("flow", &["a", "b"], &[("a", "b"), ("b", "b")]),
        "/blocks/2/edges/1",
    );
    let i = only_diagram(
        diagram("flow", &["a", "b"], &[("a", "b"), ("a", "b")]),
        "/blocks/2/edges/1",
    );
    assert!(i.message.contains("'a' -> 'b'"), "{}", i.message);
}

#[test]
fn diagram_edge_errors_hide_the_graph_rules() {
    // `c` looks isolated only because its edge points nowhere: one error.
    only_diagram(
        diagram("flow", &["a", "b", "c"], &[("a", "b"), ("c", "zz")]),
        "/blocks/2/edges/1/to",
    );
}

#[test]
fn flow_needs_edges() {
    only_diagram(diagram("flow", &["a", "b"], &[]), "/blocks/2/edges");
}

#[test]
fn flow_cycle_is_reported_on_the_closing_edge() {
    let i = only_diagram(
        diagram(
            "flow",
            &["a", "b", "c"],
            &[("a", "b"), ("b", "c"), ("c", "a")],
        ),
        "/blocks/2/edges/2",
    );
    assert!(
        i.message.contains("'c' -> 'a'") && i.message.contains("cycle"),
        "{}",
        i.message
    );
}

#[test]
fn flow_isolated_node() {
    let i = only_diagram(
        diagram("flow", &["a", "b", "c"], &[("a", "b")]),
        "/blocks/2",
    );
    assert!(i.message.contains("'c'"), "{}", i.message);
}

#[test]
fn flow_not_connected() {
    let i = only_diagram(
        diagram("flow", &["a", "b", "c", "d"], &[("a", "b"), ("c", "d")]),
        "/blocks/2",
    );
    assert!(i.message.contains("non è connesso"), "{}", i.message);
}

#[test]
fn tree_without_edges() {
    only_diagram(diagram("tree", &["a", "b"], &[]), "/blocks/2/edges");
}

#[test]
fn tree_with_two_roots() {
    // d is a child of c, a and b are both roots? a->b makes b a child: use two parents-less nodes.
    let i = only_diagram(
        diagram("tree", &["a", "b", "c", "d"], &[("a", "b"), ("c", "d")]),
        "/blocks/2",
    );
    assert!(i.message.contains("2 radici"), "{}", i.message);
}

#[test]
fn tree_node_with_two_parents() {
    let i = only_diagram(
        diagram(
            "tree",
            &["r", "x", "y"],
            &[("r", "x"), ("r", "y"), ("x", "y")],
        ),
        "/blocks/2/edges/2",
    );
    assert!(
        i.message.contains("'y'") && i.message.contains("'x'"),
        "{}",
        i.message
    );
}

#[test]
fn tree_node_not_reachable_from_the_root() {
    // x <-> y is a detached loop: both have a parent, the root reaches only s.
    let found = diagram_issues(diagram(
        "tree",
        &["r", "s", "x", "y"],
        &[("r", "s"), ("x", "y"), ("y", "x")],
    ));
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|i| i.path == "/blocks/2"));
    assert!(found[0].message.contains("'x'") && found[1].message.contains("'y'"));
}

#[test]
fn tree_without_any_root() {
    let i = only_diagram(
        diagram("tree", &["x", "y"], &[("x", "y"), ("y", "x")]),
        "/blocks/2",
    );
    assert!(i.message.contains("non ha una radice"), "{}", i.message);
}

#[test]
fn cycle_with_edges_or_too_few_nodes() {
    only_diagram(
        diagram("cycle", &["a", "b", "c"], &[("a", "b")]),
        "/blocks/2/edges",
    );
    only_diagram(diagram("cycle", &["a", "b"], &[]), "/blocks/2/nodes");
}

#[test]
fn cycle_with_edges_and_too_few_nodes_reports_only_the_edges() {
    only_diagram(
        diagram("cycle", &["a", "b"], &[("a", "b")]),
        "/blocks/2/edges",
    );
}

#[test]
fn tree_second_parent_masks_the_graph_errors() {
    // a->c, b->c: two roots as well, but only the edge is reported (§10.8).
    let i = only_diagram(
        diagram("tree", &["a", "b", "c"], &[("a", "c"), ("b", "c")]),
        "/blocks/2/edges/1",
    );
    assert!(i.message.contains("'c'"), "{}", i.message);
}

#[test]
fn diagram_shape_errors_are_schema_and_skip_the_graph_rules() {
    let mut d = diagram("flow", &["a", "b"], &[("a", "b")]);
    d["layout"] = json!("radial");
    d["nodes"][0]["detail"] = json!("");
    d["extra"] = json!(1);
    d.as_object_mut().unwrap().remove("alt");
    let found = diagram_issues(d);
    assert!(
        found.iter().all(|i| i.code == LessonErrorCode::Schema),
        "{found:?}"
    );
    let paths: Vec<_> = found.iter().map(|i| i.path.as_str()).collect();
    for p in [
        "/blocks/2/layout",
        "/blocks/2/nodes/0/detail",
        "/blocks/2/extra",
        "/blocks/2/alt",
    ] {
        assert!(paths.contains(&p), "{p} missing in {paths:?}");
    }
    // Too few / too many nodes and edges, bad edge id.
    let mut d = diagram("flow", &["a"], &[("a", "b")]);
    d["edges"][0]["from"] = json!("Bad Id");
    let found = diagram_issues(d);
    assert!(
        found.iter().all(|i| i.code == LessonErrorCode::Schema),
        "{found:?}"
    );
}

#[test]
fn diagram_node_ids_must_be_unique_in_the_block() {
    let found = diagram_issues(diagram("cycle", &["a", "b", "a"], &[]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code.as_str(), "duplicate_id");
    assert_eq!(found[0].path, "/blocks/2/nodes/2/id");
}

#[test]
fn diagram_caption_is_markdown_but_not_prose() {
    let base = parse_document(&valid()).0.prose_words();
    let doc = with_diagram(diagram("flow", &["a", "b"], &[("a", "b")]));
    let (parsed, _) = parse_document(&doc);
    assert_eq!(parsed.prose_words(), base);
    assert!(
        parsed
            .md_fields
            .iter()
            .any(|f| f.path == "/blocks/2/caption" && !f.prose)
    );
    // Markdown rules apply to the caption, not to plain-text fields.
    let mut d = diagram("flow", &["a", "b"], &[("a", "b")]);
    d["caption"] = json!("Vedi [^nope].");
    let found = diagram_issues(d);
    assert_eq!(codes_of(&found), ["unknown_citation"]);
    let mut d = diagram("flow", &["a", "b"], &[("a", "b")]);
    d["nodes"][0]["label"] = json!("**a** [^nope]");
    d["alt"] = json!("[[nope]]");
    assert_eq!(diagram_issues(d), []);
}

fn codes_of(found: &[LessonIssue]) -> Vec<&'static str> {
    found.iter().map(|i| i.code.as_str()).collect()
}

#[test]
fn diagram_does_not_change_duration_or_client_view() {
    let plain = valid();
    let with = with_diagram(diagram("flow", &["a", "b"], &[("a", "b")]));
    assert_eq!(document_minutes(&plain), document_minutes(&with));
    let view = client_view(&with);
    assert_eq!(view["blocks"][2], with["blocks"][2]);
}

// ---- exercise `code` and `inline_code_too_long` (contract §4.1, §6) --------

fn quiz_with_code(code: Value) -> Value {
    let mut doc = valid();
    blocks(&mut doc)[2]["code"] = code;
    doc
}

#[test]
fn exercise_code_is_accepted_on_the_four_exercises_with_a_question() {
    let code = json!({"language": "rust", "code": "let a = 1;\nlet b = a;"});
    assert_eq!(issues(&quiz_with_code(code.clone())), []);
    let mut doc = valid();
    blocks(&mut doc)[2]["type"] = json!("quiz_multi");
    blocks(&mut doc)[2]["correct"] = json!(["o0", "o1"]);
    blocks(&mut doc)[2]["code"] = code.clone();
    assert_eq!(issues(&doc), []);
    let mut doc = valid();
    blocks(&mut doc).insert(
        3,
        json!({"id": "oa", "type": "open_answer", "question": "Perché?", "rubric": "Dice x.",
            "code": code, "required": false}),
    );
    blocks(&mut doc).insert(
        3,
        json!({"id": "fc", "type": "flashcard", "front": "Cosa fa?", "back": "Niente.",
            "code": code, "required": false}),
    );
    assert_eq!(issues(&doc), []);
}

#[test]
fn exercise_code_shape_errors_are_schema() {
    let cases = [
        (json!("let a = 1;"), "/blocks/2/code"),
        (json!({"code": "x"}), "/blocks/2/code/language"),
        (
            json!({"language": "Rust", "code": "x"}),
            "/blocks/2/code/language",
        ),
        (json!({"language": "rust"}), "/blocks/2/code/code"),
        (
            json!({"language": "rust", "code": " \n\t "}),
            "/blocks/2/code/code",
        ),
        (
            json!({"language": "rust", "code": ""}),
            "/blocks/2/code/code",
        ),
        (
            json!({"language": "rust", "code": "a".repeat(1001)}),
            "/blocks/2/code/code",
        ),
        (
            json!({"language": "rust", "code": "x", "caption": "c"}),
            "/blocks/2/code/caption",
        ),
        (
            json!({"language": "rust", "code": "x", "highlight_lines": [1]}),
            "/blocks/2/code/highlight_lines",
        ),
    ];
    for (code, path) in cases {
        only(&quiz_with_code(code.clone()), "schema", path);
    }
    // 1000 characters is the limit, inclusive.
    let ok = json!({"language": "rust", "code": "a".repeat(1000)});
    assert_eq!(issues(&quiz_with_code(ok)), []);
}

#[test]
fn code_is_still_unknown_on_exercises_without_a_question() {
    let mut doc = valid();
    blocks(&mut doc).insert(
        3,
        json!({"id": "os", "type": "order_steps", "prompt": "Ordina", "required": false,
            "steps": [{"id":"a","text":"a"},{"id":"b","text":"b"},{"id":"c","text":"c"}],
            "code": {"language": "rust", "code": "x"}}),
    );
    let found = issues(&doc);
    assert!(
        found
            .iter()
            .any(|i| i.code == LessonErrorCode::Schema && i.path == "/blocks/3/code"),
        "{found:?}"
    );
}

fn with_question(question: &str) -> Value {
    let mut doc = valid();
    blocks(&mut doc)[2]["question"] = json!(question);
    doc
}

#[test]
fn inline_code_limit_is_sixty_characters() {
    let sixty = format!("Cosa fa `{}`?", "a".repeat(60));
    assert_eq!(issues(&with_question(&sixty)), []);
    let sixty_one = format!("Cosa fa `{}`?", "a".repeat(61));
    only(
        &with_question(&sixty_one),
        "inline_code_too_long",
        "/blocks/2/question",
    );
}

#[test]
fn inline_code_semicolons_one_is_fine_two_is_not() {
    assert_eq!(issues(&with_question("Cosa fa `let a = 1;`?")), []);
    assert_eq!(issues(&with_question("Cosa fa `a; b`?")), []);
    only(
        &with_question("Cosa fa `a; b;`?"),
        "inline_code_too_long",
        "/blocks/2/question",
    );
    // Semicolons outside a code span do not count.
    assert_eq!(issues(&with_question("Prima; poi; infine `x`?")), []);
}

#[test]
fn inline_code_message_inside_an_exercise_points_to_the_code_field() {
    let found = issues(&with_question("`a; b; c`"));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].message.contains("campo 'code' dell'esercizio"));
    assert!(found[0].message.contains("/blocks/2/question"));
}

#[test]
fn inline_code_is_checked_in_every_md_field() {
    let long = format!("`{}`", "a".repeat(61));
    // Option text and feedback of a quiz: still an exercise.
    let mut doc = valid();
    blocks(&mut doc)[2]["options"][0]["text"] = json!(format!("uno {long}"));
    blocks(&mut doc)[2]["options"][1]["feedback"] = json!(format!("Perché {long}."));
    let found = issues(&doc);
    let paths: Vec<_> = found
        .iter()
        .filter(|i| i.code == LessonErrorCode::InlineCodeTooLong)
        .map(|i| i.path.as_str())
        .collect();
    assert!(paths.contains(&"/blocks/2/options/0/text"), "{found:?}");
    assert!(paths.contains(&"/blocks/2/options/1/feedback"), "{found:?}");
    assert!(
        found
            .iter()
            .filter(|i| i.code == LessonErrorCode::InlineCodeTooLong)
            .all(|i| i.message.contains("'code' dell'esercizio"))
    );
    // A paragraph: the remedy is a `code` block.
    let mut doc = valid();
    blocks(&mut doc)[1]["text"] = json!(format!("Il concetto di [[ownership]] [^book] {long}"));
    only(&doc, "inline_code_too_long", "/blocks/1/text");
    let m = &issues(&doc)[0].message;
    assert!(m.contains("blocco 'code'"), "{m}");
    assert!(!m.contains("dell'esercizio"), "{m}");
    // Glossary definitions are md fields too.
    let mut doc = valid();
    doc["glossary"][0]["definition"] = json!(format!("Un {long}"));
    only(&doc, "inline_code_too_long", "/glossary/0/definition");
}

#[test]
fn inline_code_in_an_exercise_nested_in_a_fallback_gets_the_exercise_remedy() {
    let mut doc = valid();
    let q = json!({"id": "fq", "type": "quiz_single", "question": "`a; b; c`",
        "options": opts(&["uno due", "tre quattro"]), "correct": "o0", "required": false});
    blocks(&mut doc).insert(
        3,
        json!({"id": "mc", "type": "missing_component", "wanted": "x_y", "purpose": "Serve.",
            "fallback": [q]}),
    );
    let found = issues(&doc);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, "/blocks/3/fallback/0/question");
    assert!(found[0].message.contains("'code' dell'esercizio"));
}
