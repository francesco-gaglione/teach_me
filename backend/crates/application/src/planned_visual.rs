//! The plan's per-concept `visual` request and the check that the generated
//! document honours it. Internal: its findings never reach the stored
//! `quality_warnings`.

use serde_json::Value;

/// Values that ask for a `diagram` block with that `layout`.
const DIAGRAM_VALUES: [&str; 3] = ["flow", "tree", "cycle"];
/// Values that ask for a `missing_component` block with that `wanted`.
const COMPONENT_VALUES: [&str; 5] = [
    "code_compare",
    "line_chart",
    "comparison_table",
    "timeline",
    "sequence",
];
/// At most this many concepts of a plan carry a figure.
pub const MAX_VISUALS: usize = 2;

/// `true` when `value` is a legal `visual` (`none` included).
#[must_use]
pub fn is_known(value: &str) -> bool {
    value == "none" || DIAGRAM_VALUES.contains(&value) || COMPONENT_VALUES.contains(&value)
}

/// The concepts that ask for a figure, as `(name, value)`.
fn requested(plan: &Value) -> Vec<(String, String)> {
    plan.get("concepts")
        .and_then(Value::as_array)
        .map_or_default(|cs| {
            cs.iter()
                .filter_map(|c| {
                    let v = c.get("visual").and_then(Value::as_str)?;
                    (v != "none" && is_known(v)).then(|| {
                        let name = c.get("name").and_then(Value::as_str).unwrap_or_default();
                        (name.to_owned(), v.to_owned())
                    })
                })
                .collect()
        })
}

/// Top-level blocks of the document that provide `value`.
fn provided(document: &Value, value: &str) -> usize {
    let (kind, field) = if DIAGRAM_VALUES.contains(&value) {
        ("diagram", "layout")
    } else {
        ("missing_component", "wanted")
    };
    document
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, |bs| {
            bs.iter()
                .filter(|b| {
                    b.get("type").and_then(Value::as_str) == Some(kind)
                        && b.get(field).and_then(Value::as_str) == Some(value)
                })
                .count()
        })
}

/// One entry per requested value, in plan order: `(value, names, met)` where
/// `met` is `min(requested, provided)`.
fn status(plan: &Value, document: &Value) -> Vec<(String, Vec<String>, usize)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (name, value) in requested(plan) {
        match out.iter_mut().find(|(v, _)| *v == value) {
            Some(entry) => entry.1.push(name),
            None => out.push((value, vec![name])),
        }
    }
    out.into_iter()
        .map(|(v, names)| {
            let met = provided(document, &v).min(names.len());
            (v, names, met)
        })
        .collect()
}

/// How many requested figures the document provides (per value, capped at
/// the number requested).
#[must_use]
pub fn satisfied(plan: &Value, document: &Value) -> usize {
    status(plan, document).iter().map(|s| s.2).sum()
}

/// `true` when `better` provides fewer figures than `document` for some
/// requested value: a satisfied request must never be given up.
#[must_use]
pub fn lost_any(plan: &Value, document: &Value, better: &Value) -> bool {
    status(plan, document)
        .iter()
        .zip(status(plan, better).iter())
        .any(|(before, after)| after.2 < before.2)
}

/// Improvement points (Italian, for the `improve` prompt) for the figures
/// the plan asks for and the document lacks, with the actual deficit per
/// value. Empty when all are there.
#[must_use]
pub fn planned_visual_missing(plan: &Value, document: &Value) -> Vec<String> {
    status(plan, document)
        .into_iter()
        .filter(|(_, names, met)| *met < names.len())
        .map(|(value, names, met)| {
            let lacking = names.len() - met;
            let names = names
                .iter()
                .map(|n| format!("«{n}»"))
                .collect::<Vec<_>>()
                .join(", ");
            let count = if lacking > 1 {
                format!(" ({lacking} figure mancano)")
            } else {
                String::new()
            };
            if DIAGRAM_VALUES.contains(&value.as_str()) {
                format!(
                    "Il piano chiede per il concetto {names} una figura `{value}` (blocco `diagram` con `layout` `{value}`) che manca{count}: aggiungila nella sua sezione secondo § «Figura»"
                )
            } else {
                format!(
                    "Il piano chiede per il concetto {names} una figura `{value}` (un `missing_component` con `wanted` `{value}` e il suo `fallback`) che manca{count}: aggiungila nella sua sezione secondo § «Figura»"
                )
            }
        })
        .collect()
}

/// Title prefix of the recall section (as in the `concept_chain` eval check).
const RECALL_PREFIX: &str = "Cosa ti serve:";

/// `true` when the concept carries a typical mistake (non-empty, not "nessuno").
fn has_pitfall(concept: &Value) -> bool {
    let p = concept
        .get("pitfall")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    !p.is_empty() && !p.eq_ignore_ascii_case("nessuno")
}

/// The top-level blocks of each concept section: `heading` 2 sections in
/// order, minus the recall section (first, `Cosa ti serve:`) and the closing
/// (last), as the `concept_chain` eval check cuts them.
fn concept_sections(document: &Value) -> Vec<Vec<&Value>> {
    let is_h2 = |b: &Value| {
        b.get("type").and_then(Value::as_str) == Some("heading")
            && b.get("level").and_then(Value::as_u64) == Some(2)
    };
    let mut sections: Vec<(&str, Vec<&Value>)> = Vec::new();
    for b in document
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
    {
        if is_h2(b) {
            let title = b.get("text").and_then(Value::as_str).unwrap_or_default();
            sections.push((title, Vec::new()));
        } else if let Some(last) = sections.last_mut() {
            last.1.push(b);
        }
    }
    let recall = sections
        .first()
        .is_some_and(|s| s.0.trim().starts_with(RECALL_PREFIX));
    let skip_front = usize::from(recall);
    // The closing is the last section, unless that is the recall itself.
    let end = if sections.len() > skip_front {
        sections.len() - 1
    } else {
        sections.len()
    };
    sections
        .into_iter()
        .take(end)
        .skip(skip_front)
        .map(|s| s.1)
        .collect()
}

/// Per plan concept with a typical mistake: `(index, name, present)`. Empty
/// when the concept sections do not line up with the plan's concepts.
///
/// One-sided on purpose: the `concept_chain` eval wants the `warning` callout
/// if and only if the concept has a pitfall, but here a callout in a concept
/// without one is not reported. This feeds an improvement step, which only
/// adds what is missing; an invented callout is a matter for the eval.
fn pitfall_status(plan: &Value, document: &Value) -> Vec<(usize, String, bool)> {
    let concepts = plan
        .get("concepts")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let sections = concept_sections(document);
    if sections.len() != concepts.len() {
        return Vec::new();
    }
    concepts
        .iter()
        .zip(sections)
        .enumerate()
        .filter(|(_, (c, _))| has_pitfall(c))
        .map(|(i, (c, blocks))| {
            let name = c.get("name").and_then(Value::as_str).unwrap_or_default();
            let present = blocks.iter().any(|b| {
                b.get("type").and_then(Value::as_str) == Some("callout")
                    && b.get("variant").and_then(Value::as_str) == Some("warning")
            });
            (i, name.to_owned(), present)
        })
        .collect()
}

/// How many planned typical mistakes the document has as a `warning` callout.
#[must_use]
pub fn pitfalls_satisfied(plan: &Value, document: &Value) -> usize {
    pitfall_status(plan, document)
        .iter()
        .filter(|s| s.2)
        .count()
}

/// `true` when `document` has a planned typical mistake that `better` lacks
/// (or can no longer be checked for).
#[must_use]
pub fn pitfalls_lost(plan: &Value, document: &Value, better: &Value) -> bool {
    let after = pitfall_status(plan, better);
    pitfall_status(plan, document)
        .iter()
        .filter(|s| s.2)
        .any(|(i, _, _)| !after.iter().any(|a| a.0 == *i && a.2))
}

/// Improvement points (Italian) for planned typical mistakes the document
/// lacks. Empty when all are there or the sections are not aligned.
#[must_use]
pub fn pitfalls_missing(plan: &Value, document: &Value) -> Vec<String> {
    pitfall_status(plan, document)
        .into_iter()
        .filter(|s| !s.2)
        .map(|(_, name, _)| {
            format!(
                "Il piano dà per il concetto «{name}» un errore tipico che manca: aggiungi nella sua sezione, dopo il paragrafo della regola e prima degli esercizi, il `callout` `warning` con titolo «Errore tipico: …» e testo in tre mosse (che cosa si tende a credere, perché sembra ragionevole, come stanno le cose), senza togliere altro; se serve spazio accorcia i paragrafi"
            )
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)]
mod tests {
    use serde_json::json;

    use super::*;

    fn plan() -> Value {
        json!({"concepts": [
            {"name": "A", "visual": "flow"},
            {"name": "B", "visual": "timeline"},
            {"name": "C"}
        ]})
    }

    #[test]
    fn satisfied_plan_has_no_missing() {
        let doc = json!({"blocks": [
            {"type": "diagram", "layout": "flow"},
            {"type": "missing_component", "wanted": "timeline"}
        ]});
        assert!(planned_visual_missing(&plan(), &doc).is_empty());
        assert_eq!(satisfied(&plan(), &doc), 2);
    }

    #[test]
    fn missing_figures_are_reported() {
        let doc = json!({"blocks": [{"type": "diagram", "layout": "tree"}]});
        let m = planned_visual_missing(&plan(), &doc);
        assert_eq!(m.len(), 2);
        assert!(m[0].starts_with("Il piano chiede per il concetto «A»"));
        assert!(m[1].contains("`missing_component` con `wanted` `timeline`"));
    }

    #[test]
    fn nested_blocks_do_not_count() {
        let doc = json!({"blocks": [{"type": "callout", "blocks": [{"type": "diagram", "layout": "flow"}]}]});
        assert_eq!(satisfied(&plan(), &doc), 0);
    }

    #[test]
    fn two_flows_with_one_present_report_one_missing_and_loss_is_detected() {
        let p = json!({"concepts": [
            {"name": "A", "visual": "flow"},
            {"name": "B", "visual": "flow"}
        ]});
        let one = json!({"blocks": [{"type": "diagram", "layout": "flow"}]});
        let none = json!({"blocks": []});
        assert_eq!(satisfied(&p, &one), 1);
        assert_eq!(satisfied(&p, &none), 0);
        let m = planned_visual_missing(&p, &one);
        assert_eq!(m.len(), 1);
        assert!(!m[0].contains("2 figure"));
        assert!(planned_visual_missing(&p, &none)[0].contains("(2 figure mancano)"));
        assert!(lost_any(&p, &one, &none));
        assert!(!lost_any(&p, &none, &one));
    }

    #[test]
    fn fallback_diagram_does_not_count() {
        let doc = json!({"blocks": [{"type": "missing_component", "wanted": "timeline",
            "fallback": [{"type": "diagram", "layout": "flow"}]}]});
        assert_eq!(satisfied(&plan(), &doc), 1);
    }

    #[test]
    fn old_plan_asks_nothing() {
        let p = json!({"concepts": [{"name": "A"}, {"name": "B", "visual": "none"}]});
        assert!(planned_visual_missing(&p, &json!({"blocks": []})).is_empty());
    }

    fn sec(title: &str, extra: Vec<Value>) -> Vec<Value> {
        let mut v = vec![json!({"type": "heading", "level": 2, "text": title})];
        v.extend(extra);
        v
    }

    fn warn() -> Value {
        json!({"type": "callout", "variant": "warning"})
    }

    fn pit_plan() -> Value {
        json!({"concepts": [
            {"name": "A", "pitfall": "Sbaglia così."},
            {"name": "B", "pitfall": "nessuno"},
            {"name": "C", "pitfall": ""}
        ]})
    }

    fn pit_doc(first: Vec<Value>) -> Value {
        let mut blocks = vec![json!({"type": "heading", "level": 1, "text": "T"})];
        blocks.extend(sec("A", first));
        blocks.extend(sec("B", vec![]));
        blocks.extend(sec("C", vec![]));
        blocks.extend(sec("Chiusura", vec![]));
        json!({"blocks": blocks})
    }

    #[test]
    fn pitfall_present_or_missing() {
        let ok = pit_doc(vec![warn()]);
        assert!(pitfalls_missing(&pit_plan(), &ok).is_empty());
        assert_eq!(pitfalls_satisfied(&pit_plan(), &ok), 1);
        let bad = pit_doc(vec![json!({"type": "callout", "variant": "tip"})]);
        let m = pitfalls_missing(&pit_plan(), &bad);
        assert_eq!(m.len(), 1);
        assert!(m[0].starts_with("Il piano dà per il concetto «A»"));
        assert!(pitfalls_lost(&pit_plan(), &ok, &bad));
        assert!(!pitfalls_lost(&pit_plan(), &bad, &ok));
    }

    #[test]
    fn recall_section_is_skipped_and_misaligned_sections_are_not_checked() {
        let mut doc = pit_doc(vec![]);
        let blocks = doc["blocks"].as_array_mut().unwrap();
        blocks.splice(1..1, sec("Cosa ti serve: base", vec![]));
        assert_eq!(pitfalls_missing(&pit_plan(), &doc).len(), 1);
        // One concept section too many: not applicable.
        let blocks = doc["blocks"].as_array_mut().unwrap();
        blocks.splice(1..1, sec("Extra", vec![]));
        assert!(pitfalls_missing(&pit_plan(), &doc).is_empty());
        assert_eq!(pitfalls_satisfied(&pit_plan(), &doc), 0);
    }

    #[test]
    fn a_candidate_that_loses_alignment_loses_the_pitfall() {
        let ok = pit_doc(vec![warn()]);
        let mut misaligned = ok.clone();
        let blocks = misaligned["blocks"].as_array_mut().unwrap();
        blocks.splice(1..1, sec("Extra", vec![]));
        assert!(pitfall_status(&pit_plan(), &misaligned).is_empty());
        assert!(pitfalls_lost(&pit_plan(), &ok, &misaligned));
    }

    #[test]
    fn a_warning_in_the_wrong_section_does_not_count() {
        let mut blocks = vec![json!({"type": "heading", "level": 1, "text": "T"})];
        blocks.extend(sec("A", vec![]));
        blocks.extend(sec("B", vec![warn()]));
        blocks.extend(sec("C", vec![]));
        blocks.extend(sec("Chiusura", vec![]));
        let doc = json!({"blocks": blocks});
        assert_eq!(pitfalls_missing(&pit_plan(), &doc).len(), 1);
        assert_eq!(pitfalls_satisfied(&pit_plan(), &doc), 0);
    }

    #[test]
    fn without_the_closing_the_last_concept_is_cut_and_nothing_is_checked() {
        let mut doc = pit_doc(vec![]);
        let blocks = doc["blocks"].as_array_mut().unwrap();
        blocks.truncate(blocks.len() - 1); // drop the closing heading
        assert_eq!(concept_sections(&doc).len(), 2);
        assert!(pitfalls_missing(&pit_plan(), &doc).is_empty());
    }

    #[test]
    fn recall_as_the_only_section_yields_no_concept_sections() {
        let doc = json!({"blocks": sec("Cosa ti serve: base", vec![warn()])});
        assert!(concept_sections(&doc).is_empty());
        assert!(pitfalls_missing(&pit_plan(), &doc).is_empty());
        assert_eq!(pitfalls_satisfied(&pit_plan(), &doc), 0);
    }

    #[test]
    fn nested_warning_does_not_count() {
        let nested = json!({"type": "list", "blocks": [warn()]});
        assert_eq!(
            pitfalls_missing(&pit_plan(), &pit_doc(vec![nested])).len(),
            1
        );
    }
}
