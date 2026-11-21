//! The Planner's plan and the checks the backend applies to it
//! (`docs/contracts/lessons.md` §4). The model proposes ids, numbers and keys;
//! only the ones that exist in the Topic are accepted.

use std::collections::HashSet;

use serde_json::Value;
use uuid::Uuid;

use crate::structured::{check_schema, parse_object};

const TITLE_MAX: usize = 120;
const OBJECTIVE_MAX: usize = 200;
const WHY_NOW_MAX: usize = 400;
/// Every lesson has at least 4 concepts: with fewer, real lessons stayed under
/// the target length (owner decision after the rich-lesson evals).
const CONCEPTS: std::ops::RangeInclusive<usize> = 4..=5;
/// A `general` prerequisite adds a recall section to the lesson, which counts
/// against the 3-6 sections the lesson may have (`sdui.md`).
const MAX_CONCEPTS_WITH_RECALL: usize = 4;
const MAX_GENERAL_PREREQUISITES: usize = 3;
const EXAMPLE_MAX: usize = 300;
const STEPS: std::ops::RangeInclusive<usize> = 2..=6;
const STEP_MAX: usize = 150;
const PITFALL_MAX: usize = 200;
const EXERCISES: std::ops::RangeInclusive<usize> = 2..=5;
const MAX_NEW_TERMS: usize = 8;
const MAX_OTHER_SOURCES: usize = 4;
const MAX_LINKS: usize = 2;
/// A lesson's own glossary holds 12 entries at most (`sdui.md` §2).
const MAX_GLOSSARY: usize = 12;
const TERM_KEY_MAX: usize = 40;
/// Sections of uploaded documents a single lesson may be built on.
pub const MAX_PLAN_SECTIONS: usize = 6;

/// What exists in the Topic, to check the plan's references against.
pub struct PlanFacts<'a> {
    pub resource_ids: &'a HashSet<Uuid>,
    pub lesson_ids: &'a HashSet<Uuid>,
    pub active_record_numbers: &'a HashSet<u32>,
    pub glossary_keys: &'a HashSet<String>,
    /// The ready documents of the topic.
    pub documents: &'a [PlanDocument],
}

/// A ready document as the plan may refer to it.
#[derive(Debug, Clone)]
pub struct PlanDocument {
    pub document_id: Uuid,
    /// Its `Resource`: the lesson must list it among its sources to cite it.
    pub resource_id: Uuid,
    pub section_ids: HashSet<String>,
}

/// A plan that passed the checks, with the parts the next steps need.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidPlan {
    pub raw: Value,
    pub primary_source: Uuid,
    pub other_sources: Vec<Uuid>,
    /// Keys of Topic glossary terms the lesson will reuse.
    pub reuse_terms: Vec<String>,
    pub links: Vec<(Uuid, String)>,
    /// Sections of uploaded documents the lesson follows, in the order of the
    /// plan: `(document_id, section ids)`. Empty for a plan from web sources
    /// only (the field is optional).
    pub document_sections: Vec<(Uuid, Vec<String>)>,
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
}

fn length(errors: &mut Vec<String>, what: &str, value: &str, max: usize) {
    let n = value.chars().count();
    if n == 0 {
        errors.push(format!("{what} è vuoto."));
    } else if n > max {
        errors.push(format!("{what} ha {n} caratteri, il massimo è {max}."));
    }
}

fn uuid_of(value: &str) -> Option<Uuid> {
    domain::parse_uuid(value)
}

fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().count() <= TERM_KEY_MAX
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Parses the model's text and checks it against the schema and the Topic.
#[allow(clippy::too_many_lines)] // a flat sequence of independent steps; helpers would only hide the order
pub fn validate_plan(
    model_text: &str,
    schema: Option<&Value>,
    facts: &PlanFacts<'_>,
) -> Result<ValidPlan, Vec<String>> {
    let plan = parse_object(model_text)?;
    if let Some(schema) = schema {
        let shape = check_schema(&plan, schema);
        if !shape.is_empty() {
            return Err(shape);
        }
    }
    let mut errors = Vec::new();
    let items = |key: &str| -> Vec<Value> {
        plan.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };

    length(&mut errors, "title", text(&plan, "title"), TITLE_MAX);
    length(
        &mut errors,
        "objective",
        text(&plan, "objective"),
        OBJECTIVE_MAX,
    );
    length(&mut errors, "why_now", text(&plan, "why_now"), WHY_NOW_MAX);
    // `estimated_minutes` is not checked: the model gets the arithmetic wrong,
    // so the backend replaces the figure once the plan is accepted.

    let prerequisites = items("prerequisites");
    for (i, p) in prerequisites.iter().enumerate() {
        length(
            &mut errors,
            &format!("prerequisites[{i}].statement"),
            text(p, "statement"),
            300,
        );
        let numbers: Vec<u64> = p
            .get("record_numbers")
            .and_then(Value::as_array)
            .map_or_default(|a| a.iter().filter_map(Value::as_u64).collect());
        if text(p, "basis") == "record" && numbers.is_empty() {
            errors.push(format!(
                "prerequisites[{i}]: con basis `record` servono i record_numbers."
            ));
        }
        for n in numbers {
            if !u32::try_from(n).is_ok_and(|n| facts.active_record_numbers.contains(&n)) {
                errors.push(format!(
                    "prerequisites[{i}].record_numbers: il record {n} non esiste o non è attivo."
                ));
            }
        }
    }

    let general = prerequisites
        .iter()
        .filter(|p| text(p, "basis") == "general")
        .count();
    if general > MAX_GENERAL_PREREQUISITES {
        errors.push(format!(
            "prerequisites ha {general} voci con basis `general`: al massimo {MAX_GENERAL_PREREQUISITES}. Un prerequisito grande diventa un concetto di supporto."
        ));
    }

    let concepts = items("concepts");
    if concepts.len() < *CONCEPTS.start() {
        errors.push(format!(
            "concepts ha {} voci: servono almeno {} concetti. Se l'argomento ne ha meno, aggiungi i concetti di supporto che servono a capirlo.",
            concepts.len(),
            CONCEPTS.start()
        ));
    } else if concepts.len() > *CONCEPTS.end() {
        errors.push(format!(
            "concepts ha {} voci: al massimo {}. Se ne servono di più, scegli una capacità più piccola.",
            concepts.len(),
            CONCEPTS.end()
        ));
    } else if general > 0 && concepts.len() > MAX_CONCEPTS_WITH_RECALL {
        errors.push(format!(
            "concepts ha {} voci: con un prerequisito `general` (sezione di richiamo) sono esattamente {MAX_CONCEPTS_WITH_RECALL}. Rendi `general` un concetto di supporto o togli un concetto.",
            concepts.len()
        ));
    }
    for (i, c) in concepts.iter().enumerate() {
        length(
            &mut errors,
            &format!("concepts[{i}].name"),
            text(c, "name"),
            120,
        );
        length(
            &mut errors,
            &format!("concepts[{i}].key_point"),
            text(c, "key_point"),
            300,
        );
        length(
            &mut errors,
            &format!("concepts[{i}].example"),
            text(c, "example"),
            EXAMPLE_MAX,
        );
        check_steps(&mut errors, i, c);
        // An empty pitfall is valid: a convention has no typical mistake.
        let pitfall = text(c, "pitfall").chars().count();
        if pitfall > PITFALL_MAX {
            errors.push(format!(
                "concepts[{i}].pitfall ha {pitfall} caratteri, il massimo è {PITFALL_MAX} (stringa vuota se non c'è un errore tipico)."
            ));
        }
        if let Some(v) = c.get("visual")
            && !v.as_str().is_some_and(crate::planned_visual::is_known)
        {
            errors.push(format!(
                "concepts[{i}].visual {v} non è valido: deve essere una stringa tra none, flow, tree, cycle, code_compare, line_chart, comparison_table, timeline e sequence (oppure omettilo)."
            ));
        }
    }
    let with_visual = concepts
        .iter()
        .filter(|c| !matches!(c.get("visual").and_then(Value::as_str), None | Some("none")))
        .count();
    if with_visual > crate::planned_visual::MAX_VISUALS {
        errors.push(format!(
            "{with_visual} concetti hanno `visual` diverso da none: al massimo {}. Tieni la figura dove aiuta di più.",
            crate::planned_visual::MAX_VISUALS
        ));
    }

    let terms = plan.get("terms").cloned().unwrap_or(Value::Null);
    let introduce: Vec<Value> = terms
        .get("introduce")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let reuse: Vec<String> = terms
        .get("reuse")
        .and_then(Value::as_array)
        .map_or_default(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        });
    if introduce.len() > MAX_NEW_TERMS {
        errors.push(format!(
            "terms.introduce ha {} voci: al massimo {MAX_NEW_TERMS}.",
            introduce.len()
        ));
    }
    let mut keys = HashSet::new();
    for (i, t) in introduce.iter().enumerate() {
        let key = text(t, "key");
        if !is_key(key) {
            errors.push(format!(
                "terms.introduce[{i}].key non è valida: 1-{TERM_KEY_MAX} caratteri tra a-z, 0-9 e _."
            ));
        } else if facts.glossary_keys.contains(key) {
            errors.push(format!(
                "terms.introduce[{i}].key '{key}' esiste già nel glossario: mettila in terms.reuse."
            ));
        } else if !keys.insert(key.to_owned()) {
            errors.push(format!("terms.introduce[{i}].key '{key}' è ripetuta."));
        }
        length(
            &mut errors,
            &format!("terms.introduce[{i}].term"),
            text(t, "term"),
            80,
        );
    }
    let mut reused = HashSet::new();
    for (i, key) in reuse.iter().enumerate() {
        if !facts.glossary_keys.contains(key) {
            errors.push(format!(
                "terms.reuse[{i}]: la chiave '{}' non è nel glossario dell'argomento.",
                key.chars().take(60).collect::<String>()
            ));
        } else if !reused.insert(key.clone()) {
            errors.push(format!("terms.reuse[{i}]: la chiave '{key}' è ripetuta."));
        }
    }
    if introduce.len() + reuse.len() > MAX_GLOSSARY {
        errors.push(format!(
            "Introduci e riusi {} termini: il glossario di una lezione ne tiene al massimo {MAX_GLOSSARY}.",
            introduce.len() + reuse.len()
        ));
    }

    let primary = text(&plan, "primary_source_id");
    let primary_id = uuid_of(primary).filter(|id| facts.resource_ids.contains(id));
    if primary_id.is_none() {
        errors.push(format!(
            "primary_source_id '{}' non è una risorsa dell'argomento.",
            primary.chars().take(60).collect::<String>()
        ));
    }
    let mut others = Vec::new();
    let raw_others: Vec<&str> = plan
        .get("other_source_ids")
        .and_then(Value::as_array)
        .map_or_default(|a| a.iter().filter_map(Value::as_str).collect());
    if raw_others.len() > MAX_OTHER_SOURCES {
        errors.push(format!(
            "other_source_ids ha {} voci: al massimo {MAX_OTHER_SOURCES}.",
            raw_others.len()
        ));
    }
    for (i, raw) in raw_others.iter().enumerate() {
        match uuid_of(raw).filter(|id| facts.resource_ids.contains(id)) {
            None => errors.push(format!(
                "other_source_ids[{i}] '{}' non è una risorsa dell'argomento.",
                raw.chars().take(60).collect::<String>()
            )),
            Some(id) if Some(id) == primary_id => errors.push(format!(
                "other_source_ids[{i}] è la fonte primaria: non ripeterla."
            )),
            Some(id) if others.contains(&id) => {
                errors.push(format!("other_source_ids[{i}] è ripetuta."));
            }
            Some(id) => others.push(id),
        }
    }

    let document_sections = check_document_sections(
        &items("document_sections"),
        facts,
        &primary_id
            .into_iter()
            .chain(others.iter().copied())
            .collect::<Vec<_>>(),
        &mut errors,
    );

    let exercises = items("exercises");
    if !EXERCISES.contains(&exercises.len()) {
        errors.push(format!(
            "exercises ha {} voci: servono da {} a {}.",
            exercises.len(),
            EXERCISES.start(),
            EXERCISES.end()
        ));
    }
    for (i, e) in exercises.iter().enumerate() {
        length(
            &mut errors,
            &format!("exercises[{i}].purpose"),
            text(e, "purpose"),
            300,
        );
    }
    if !exercises
        .iter()
        .any(|e| e.get("verifies_objective") == Some(&Value::Bool(true)))
    {
        errors.push(
            "Almeno un esercizio deve avere verifies_objective true: l'obiettivo va dimostrato."
                .to_owned(),
        );
    }

    let mut links = Vec::new();
    let raw_links = items("lesson_links");
    if raw_links.len() > MAX_LINKS {
        errors.push(format!(
            "lesson_links ha {} voci: al massimo {MAX_LINKS}.",
            raw_links.len()
        ));
    }
    for (i, l) in raw_links.iter().enumerate() {
        let raw = text(l, "lesson_id");
        match uuid_of(raw).filter(|id| facts.lesson_ids.contains(id)) {
            None => errors.push(format!(
                "lesson_links[{i}].lesson_id '{}' non è una lezione dell'argomento.",
                raw.chars().take(60).collect::<String>()
            )),
            Some(id) if links.iter().any(|(x, _)| *x == id) => {
                errors.push(format!("lesson_links[{i}].lesson_id è ripetuto."));
            }
            Some(id) => links.push((id, text(l, "reason").to_owned())),
        }
    }

    match (errors.is_empty(), primary_id) {
        (true, Some(primary_source)) => Ok(ValidPlan {
            raw: with_computed_minutes(plan),
            primary_source,
            other_sources: others,
            reuse_terms: reuse,
            links,
            document_sections,
        }),
        _ => Err(errors),
    }
}

/// The plan with `estimated_minutes` replaced by the backend's own figure
/// (the model's is wrong too often). The Planner's rule 12 is the same
/// formula: `round(P/250 + 0.5*E + 2*O)` with
/// `P = 220 + 450*concepts + 200*[some prerequisite is "general"]`,
/// `E` = exercises + 1 when there is a recall section (a `general`
/// prerequisite) and `O` = the `open_answer` exercises, clamped to 1-20 by
/// [`domain::lesson_minutes`], the one place where the formula lives.
fn with_computed_minutes(mut plan: Value) -> Value {
    let count = |key: &str| plan.get(key).and_then(Value::as_array).map_or(0, Vec::len);
    let recall = plan
        .get("prerequisites")
        .and_then(Value::as_array)
        .is_some_and(|all| {
            all.iter()
                .any(|p| p.get("basis").and_then(Value::as_str) == Some("general"))
        });
    let open_answers = plan
        .get("exercises")
        .and_then(Value::as_array)
        .map_or(0, |all| {
            all.iter()
                .filter(|e| e.get("type").and_then(Value::as_str) == Some("open_answer"))
                .count()
        });
    let words = 220 + 450 * count("concepts") + if recall { 200 } else { 0 };
    let exercises = count("exercises") + usize::from(recall);
    let minutes = domain::lesson_minutes(words, exercises, open_answers);
    if let Some(object) = plan.as_object_mut() {
        object.insert("estimated_minutes".to_owned(), Value::from(minutes));
    }
    plan
}

/// `concepts[i].steps`: 2-6 worked-example steps, each 1-150 characters.
fn check_steps(errors: &mut Vec<String>, i: usize, concept: &Value) {
    let steps = concept
        .get("steps")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    if !STEPS.contains(&steps.len()) {
        errors.push(format!(
            "concepts[{i}].steps ha {} voci: servono da {} a {}.",
            steps.len(),
            STEPS.start(),
            STEPS.end()
        ));
    }
    for (j, step) in steps.iter().enumerate() {
        let step = step.as_str().unwrap_or_default().trim();
        length(errors, &format!("concepts[{i}].steps[{j}]"), step, STEP_MAX);
    }
}

/// `document_sections`: which parts of the uploaded documents the lesson
/// follows. Optional; every reference must exist in the topic.
fn check_document_sections(
    raw: &[Value],
    facts: &PlanFacts<'_>,
    plan_sources: &[Uuid],
    errors: &mut Vec<String>,
) -> Vec<(Uuid, Vec<String>)> {
    let mut out: Vec<(Uuid, Vec<String>)> = Vec::new();
    let mut total = 0;
    for (i, entry) in raw.iter().enumerate() {
        let raw_id = text(entry, "document_id");
        let Some(doc) =
            uuid_of(raw_id).and_then(|id| facts.documents.iter().find(|d| d.document_id == id))
        else {
            errors.push(format!(
                "document_sections[{i}].document_id '{}' non è un documento pronto dell'argomento.",
                raw_id.chars().take(60).collect::<String>()
            ));
            continue;
        };
        if out.iter().any(|(id, _)| *id == doc.document_id) {
            errors.push(format!(
                "document_sections[{i}]: il documento è già elencato, unisci le sezioni."
            ));
            continue;
        }
        if !plan_sources.contains(&doc.resource_id) {
            errors.push(format!(
                "document_sections[{i}]: la risorsa del documento deve essere tra primary_source_id e other_source_ids."
            ));
        }
        let ids: Vec<&str> = entry
            .get("section_ids")
            .and_then(Value::as_array)
            .map_or_default(|a| a.iter().filter_map(Value::as_str).collect());
        if ids.is_empty() {
            errors.push(format!("document_sections[{i}].section_ids è vuoto."));
        }
        let mut chosen: Vec<String> = Vec::new();
        for (j, id) in ids.iter().enumerate() {
            if !doc.section_ids.contains(*id) {
                errors.push(format!(
                    "document_sections[{i}].section_ids[{j}] '{}' non è una sezione del documento.",
                    id.chars().take(40).collect::<String>()
                ));
            } else if chosen.iter().any(|c| c == id) {
                errors.push(format!(
                    "document_sections[{i}].section_ids[{j}] è ripetuta."
                ));
            } else {
                chosen.push((*id).to_owned());
            }
        }
        total += chosen.len();
        out.push((doc.document_id, chosen));
    }
    if total > MAX_PLAN_SECTIONS {
        errors.push(format!(
            "document_sections indica {total} sezioni: al massimo {MAX_PLAN_SECTIONS} per lezione."
        ));
    }
    out
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)]
mod tests {
    use serde_json::json;

    use super::*;

    pub(crate) const RESOURCE: u128 = 0x51;
    pub(crate) const RESOURCE_2: u128 = 0x52;
    const DOCUMENT: u128 = 0xd0;

    /// A concept with the concept fields, valid.
    pub(crate) fn concept(name: &str, key_point: &str) -> Value {
        json!({
            "name": name,
            "key_point": key_point,
            "example": "let a = String::from(\"ciao\"); let b = a;",
            "steps": ["a possiede la stringa", "b = a sposta la proprietà in b"],
            "pitfall": "Pensare che b = a copi la stringa."
        })
    }

    /// A plan that is valid against [`facts`] (record 1 active, glossary has `move`).
    pub(crate) fn plan() -> Value {
        json!({
            "title": "Ownership e move",
            "objective": "Saper prevedere se una variabile è ancora valida dopo un move.",
            "why_now": "La mission chiede di leggere gli errori del compilatore.",
            "estimated_minutes": 8,
            "prerequisites": [{"statement": "Sa cos'è una variabile", "basis": "record", "record_numbers": [1]}],
            "concepts": [
                concept("Move", "Il valore cambia proprietario."),
                concept("Borrow", "Un riferimento presta il valore senza spostarlo."),
                concept("Copy", "I tipi semplici si copiano invece di spostarsi."),
                concept("Clone", "Una copia esplicita duplica anche il contenuto.")
            ],
            "terms": {"introduce": [{"key": "ownership", "term": "ownership", "hint": "chi possiede"}], "reuse": ["move"]},
            "primary_source_id": Uuid::from_u128(RESOURCE).to_string(),
            "other_source_ids": [Uuid::from_u128(RESOURCE_2).to_string()],
            "exercises": [
                {"type": "quiz_single", "purpose": "Riconoscere un move", "verifies_objective": false},
                {"type": "open_answer", "purpose": "Spiegare perché", "verifies_objective": true}
            ],
            "lesson_links": []
        })
    }

    fn check(plan: &Value) -> Result<ValidPlan, Vec<String>> {
        let resources: HashSet<Uuid> = [RESOURCE, RESOURCE_2].map(Uuid::from_u128).into();
        let lessons: HashSet<Uuid> = [Uuid::from_u128(0x77)].into();
        let records: HashSet<u32> = [1].into();
        let keys: HashSet<String> = ["move".to_owned()].into();
        validate_plan(
            &plan.to_string(),
            None,
            &PlanFacts {
                resource_ids: &resources,
                lesson_ids: &lessons,
                active_record_numbers: &records,
                glossary_keys: &keys,
                documents: &[PlanDocument {
                    document_id: Uuid::from_u128(DOCUMENT),
                    resource_id: Uuid::from_u128(RESOURCE_2),
                    section_ids: ["s1", "s2", "s3"].map(str::to_owned).into(),
                }],
            },
        )
    }

    #[test]
    fn a_good_plan_is_accepted_and_its_references_extracted() {
        let valid = check(&plan()).unwrap();
        assert_eq!(valid.primary_source, Uuid::from_u128(RESOURCE));
        assert_eq!(valid.other_sources, vec![Uuid::from_u128(RESOURCE_2)]);
        assert_eq!(valid.reuse_terms, vec!["move".to_owned()]);
    }

    /// Minutes the backend puts in a plan, whatever the model wrote.
    fn minutes_of(edit: impl FnOnce(&mut Value)) -> u64 {
        let mut p = plan();
        edit(&mut p);
        check(&p).unwrap().raw["estimated_minutes"]
            .as_u64()
            .unwrap()
    }

    #[test]
    fn the_backend_computes_the_plan_minutes() {
        // 4 concepts: P = 2020 -> 8.08; 2 exercises -> 1; 1 open_answer -> 2: 11.08.
        assert_eq!(minutes_of(|p| p["estimated_minutes"] = json!(45)), 11);
        assert_eq!(minutes_of(|p| p["estimated_minutes"] = json!(1)), 11);
        assert_eq!(minutes_of(|p| p["estimated_minutes"] = json!("molti")), 11);
        assert_eq!(
            minutes_of(|p| {
                p.as_object_mut().unwrap().remove("estimated_minutes");
            }),
            11
        );
        // No open_answer: 8.08 + 1 = 9.08 -> 9.
        assert_eq!(
            minutes_of(|p| p["exercises"][1]["type"] = json!("fill_blank")),
            9
        );
        // A `general` prerequisite: P = 2220 -> 8.88, and the recall is one
        // more exercise (3 -> 1.5): 8.88 + 1.5 + 2 = 12.38 -> 12.
        assert_eq!(
            minutes_of(|p| p["prerequisites"][0]["basis"] = json!("general")),
            12
        );
        // Five concepts, five exercises of which five open: 2470 -> 9.88 +
        // 2.5 + 10 = 22.38, clamped to the contract's 20.
        assert_eq!(
            minutes_of(|p| {
                let concept = p["concepts"][0].clone();
                p["concepts"] = json!([concept, concept, concept, concept, concept]);
                let ex = json!({"type": "open_answer", "purpose": "p", "verifies_objective": true});
                p["exercises"] = json!([ex, ex, ex, ex, ex]);
            }),
            20
        );
    }

    #[test]
    fn invented_references_are_rejected_with_a_message_each() {
        let mut p = plan();
        p["primary_source_id"] = json!(Uuid::from_u128(1).to_string());
        p["other_source_ids"] = json!([Uuid::from_u128(RESOURCE).to_string(), "x"]);
        p["terms"]["introduce"][0]["key"] = json!("move");
        p["terms"]["reuse"] = json!(["nope"]);
        p["prerequisites"][0]["record_numbers"] = json!([9]);
        p["lesson_links"] = json!([{"lesson_id": Uuid::from_u128(5).to_string(), "reason": "r"}]);
        let errors = check(&p).unwrap_err();
        for needle in [
            "primary_source_id",
            "other_source_ids[1]",
            "esiste già nel glossario",
            "terms.reuse[0]",
            "il record 9",
            "lesson_links[0]",
        ] {
            assert!(
                errors.iter().any(|e| e.contains(needle)),
                "{needle}: {errors:?}"
            );
        }
    }

    #[test]
    fn the_primary_source_is_not_repeated_among_the_others() {
        let mut p = plan();
        p["other_source_ids"] = json!([Uuid::from_u128(RESOURCE).to_string()]);
        assert!(check(&p).unwrap_err()[0].contains("fonte primaria"));
    }

    #[test]
    fn the_objective_must_be_verified_and_the_sizes_respected() {
        let mut p = plan();
        p["exercises"][1]["verifies_objective"] = json!(false);
        p["concepts"] = json!([]);
        let errors = check(&p).unwrap_err();
        assert!(errors.iter().any(|e| e.contains("verifies_objective")));
        assert!(errors.iter().any(|e| e.contains("concepts ha 0")));
    }

    fn errors_of(edit: impl FnOnce(&mut Value)) -> Vec<String> {
        let mut p = plan();
        edit(&mut p);
        check(&p).unwrap_err()
    }

    fn has(errors: &[String], needle: &str) -> bool {
        errors.iter().any(|e| e.contains(needle))
    }

    #[test]
    fn concepts_are_four_to_five() {
        let many = |n: usize| {
            json!(
                (0..n)
                    .map(|i| concept(&format!("c{i}"), "k"))
                    .collect::<Vec<_>>()
            )
        };
        for n in [1, 3] {
            let errors = errors_of(|p| p["concepts"] = many(n));
            assert!(has(&errors, &format!("concepts ha {n}")), "{errors:?}");
            assert!(has(&errors, "almeno 4 concetti"), "{errors:?}");
        }
        for n in [4, 5] {
            assert!(
                check(&{
                    let mut p = plan();
                    p["concepts"] = many(n);
                    p
                })
                .is_ok()
            );
        }
        assert!(has(
            &errors_of(|p| p["concepts"] = many(6)),
            "concepts ha 6"
        ));
    }

    #[test]
    fn a_general_prerequisite_leaves_room_for_the_recall_section() {
        let general =
            json!({"statement": "Sa cos'è un tipo", "basis": "general", "record_numbers": []});
        let mut four = plan();
        four["prerequisites"] = json!([general]);
        four["concepts"] = json!(
            (0..4)
                .map(|i| concept(&format!("c{i}"), "k"))
                .collect::<Vec<_>>()
        );
        assert!(check(&four).is_ok());
        let errors = errors_of(|p| {
            p["prerequisites"] = json!([general]);
            p["concepts"] = json!(
                (0..5)
                    .map(|i| concept(&format!("c{i}"), "k"))
                    .collect::<Vec<_>>()
            );
        });
        assert!(has(&errors, "esattamente 4"), "{errors:?}");
        // Five concepts without a `general` are fine (the default plan has a record).
        assert!(
            check(&{
                let mut p = plan();
                p["concepts"] = json!(
                    (0..5)
                        .map(|i| concept(&format!("c{i}"), "k"))
                        .collect::<Vec<_>>()
                );
                p
            })
            .is_ok()
        );
    }

    #[test]
    fn at_most_three_general_prerequisites() {
        let general = |n: usize| {
            json!((0..n)
                .map(|i| json!({"statement": format!("p{i}"), "basis": "general", "record_numbers": []}))
                .collect::<Vec<_>>())
        };
        assert!(
            check(&{
                let mut p = plan();
                p["prerequisites"] = general(3);
                p
            })
            .is_ok()
        );
        let errors = errors_of(|p| p["prerequisites"] = general(4));
        assert!(has(&errors, "4 voci con basis `general`"), "{errors:?}");
    }

    #[test]
    fn the_worked_example_fields_of_a_concept_are_checked() {
        let errors = errors_of(|p| p["concepts"][0]["example"] = json!("x".repeat(301)));
        assert!(has(&errors, "concepts[0].example ha 301"), "{errors:?}");
        let errors = errors_of(|p| p["concepts"][0]["example"] = json!("  "));
        assert!(has(&errors, "concepts[0].example è vuoto"), "{errors:?}");

        let errors = errors_of(|p| p["concepts"][1]["steps"] = json!(["uno solo"]));
        assert!(has(&errors, "concepts[1].steps ha 1 voci"), "{errors:?}");
        let errors = errors_of(|p| p["concepts"][1]["steps"] = json!(vec!["s"; 7]));
        assert!(has(&errors, "concepts[1].steps ha 7 voci"), "{errors:?}");
        let errors = errors_of(|p| p["concepts"][0]["steps"] = json!(["ok", "x".repeat(151)]));
        assert!(has(&errors, "concepts[0].steps[1] ha 151"), "{errors:?}");
        let errors = errors_of(|p| p["concepts"][0]["steps"] = json!(["ok", ""]));
        assert!(has(&errors, "concepts[0].steps[1] è vuoto"), "{errors:?}");
        // Missing or not a list: reported, not a panic.
        let errors = errors_of(|p| p["concepts"][0]["steps"] = json!("a, b"));
        assert!(has(&errors, "concepts[0].steps ha 0 voci"), "{errors:?}");
    }

    #[test]
    fn the_pitfall_may_be_empty_but_not_long() {
        assert!(
            check(&{
                let mut p = plan();
                p["concepts"][0]["pitfall"] = json!("");
                p
            })
            .is_ok()
        );
        let errors = errors_of(|p| p["concepts"][0]["pitfall"] = json!("x".repeat(201)));
        assert!(has(&errors, "concepts[0].pitfall ha 201"), "{errors:?}");
    }

    #[test]
    fn visual_is_optional_known_and_capped() {
        assert!(check(&plan()).is_ok());
        for v in ["none", "flow", "sequence"] {
            assert!(
                check(&{
                    let mut p = plan();
                    p["concepts"][0]["visual"] = json!(v);
                    p
                })
                .is_ok(),
                "{v}"
            );
        }
        let errors = errors_of(|p| p["concepts"][0]["visual"] = json!("pie"));
        assert!(has(&errors, "concepts[0].visual \"pie\""), "{errors:?}");
        for bad in [json!(null), json!(1), json!(""), json!(" flow")] {
            let errors = errors_of(|p| p["concepts"][0]["visual"] = bad.clone());
            assert!(has(&errors, "concepts[0].visual"), "{bad}: {errors:?}");
        }
        let errors = errors_of(|p| {
            for i in 0..3 {
                p["concepts"][i]["visual"] = json!("flow");
            }
        });
        assert!(has(&errors, "3 concetti hanno `visual`"), "{errors:?}");
    }

    #[test]
    fn up_to_eight_new_terms() {
        let terms = |n: usize| json!({"introduce": (0..n).map(|i| json!({"key": format!("t{i}"), "term": format!("t{i}"), "hint": "h"})).collect::<Vec<_>>(), "reuse": []});
        assert!(
            check(&{
                let mut p = plan();
                p["terms"] = terms(8);
                p
            })
            .is_ok()
        );
        let errors = errors_of(|p| p["terms"] = terms(9));
        assert!(
            has(&errors, "terms.introduce ha 9 voci: al massimo 8"),
            "{errors:?}"
        );
    }

    #[test]
    fn not_json_is_an_error_to_repair() {
        let resources = HashSet::new();
        let facts = PlanFacts {
            resource_ids: &resources,
            lesson_ids: &resources,
            active_record_numbers: &HashSet::new(),
            glossary_keys: &HashSet::new(),
            documents: &[],
        };
        assert!(validate_plan("ecco il piano", None, &facts).is_err());
    }

    #[test]
    fn document_sections_are_optional_and_checked_against_the_topic() {
        assert!(check(&plan()).unwrap().document_sections.is_empty());

        let doc = Uuid::from_u128(DOCUMENT).to_string();
        let mut p = plan();
        p["document_sections"] = json!([{"document_id": doc, "section_ids": ["s2", "s1"]}]);
        let valid = check(&p).unwrap();
        assert_eq!(
            valid.document_sections,
            vec![(
                Uuid::from_u128(DOCUMENT),
                vec!["s2".to_owned(), "s1".to_owned()]
            )]
        );

        let cases = [
            (
                json!([{"document_id": Uuid::from_u128(1).to_string(), "section_ids": ["s1"]}]),
                "non è un documento pronto",
            ),
            (
                json!([{"document_id": doc, "section_ids": ["s9"]}]),
                "non è una sezione",
            ),
            (
                json!([{"document_id": doc, "section_ids": ["s1", "s1"]}]),
                "ripetuta",
            ),
            (json!([{"document_id": doc, "section_ids": []}]), "è vuoto"),
            (
                json!([{"document_id": doc, "section_ids": ["s1"]}, {"document_id": doc, "section_ids": ["s2"]}]),
                "già elencato",
            ),
        ];
        for (value, needle) in cases {
            let mut p = plan();
            p["document_sections"] = value;
            let errors = check(&p).unwrap_err();
            assert!(
                errors.iter().any(|e| e.contains(needle)),
                "{needle}: {errors:?}"
            );
        }

        // The document's resource must be a source of the lesson.
        let mut p = plan();
        p["other_source_ids"] = json!([]);
        p["document_sections"] = json!([{"document_id": doc, "section_ids": ["s1"]}]);
        let errors = check(&p).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("deve essere tra primary_source_id"))
        );
    }

    #[test]
    fn a_lesson_follows_at_most_six_sections() {
        let doc = Uuid::from_u128(DOCUMENT).to_string();
        let facts_sections: HashSet<String> = (1..=8).map(|i| format!("s{i}")).collect();
        let resources: HashSet<Uuid> = [RESOURCE, RESOURCE_2].map(Uuid::from_u128).into();
        let mut p = plan();
        p["document_sections"] = json!([{"document_id": doc, "section_ids": (1..=7).map(|i| format!("s{i}")).collect::<Vec<_>>()}]);
        let errors = validate_plan(
            &p.to_string(),
            None,
            &PlanFacts {
                resource_ids: &resources,
                lesson_ids: &[Uuid::from_u128(0x77)].into(),
                active_record_numbers: &[1].into(),
                glossary_keys: &["move".to_owned()].into(),
                documents: &[PlanDocument {
                    document_id: Uuid::from_u128(DOCUMENT),
                    resource_id: Uuid::from_u128(RESOURCE_2),
                    section_ids: facts_sections,
                }],
            },
        )
        .unwrap_err();
        assert!(
            errors.iter().any(|e| e.contains("al massimo 6")),
            "{errors:?}"
        );
    }
}
