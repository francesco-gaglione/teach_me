//! Text sections of the lesson prompts (Planner, Generator, Grader, Recorder).
//! Pure functions from stored data to the strings that fill the placeholders
//! of `backend/prompts/<role>/context.md`; formats and limits are fixed by
//! `backend/prompts/README.md` ("Fasi 6-7"). Everything the person or the web
//! wrote is flattened to one line: it is data, never layout.

use std::collections::HashMap;
use std::fmt::Write as _;

use domain::lesson::scan_inline;
use domain::{
    Answer, Attempt, ExerciseKind, GlossaryTerm, LearningRecord, Lesson, LessonExercises, LessonId,
    Resource, ResourceKind,
};
use serde_json::Value;

use crate::context::one_line;

pub const MAX_RECORDS_SHOWN: usize = 60;
pub const MAX_TERMS_SHOWN: usize = 80;
pub const MAX_LESSONS_SHOWN: usize = 20;
pub const MAX_PLANNER_SOURCES: usize = 12;
const RECORD_BODY_SHOWN: usize = 400;
/// The Generator sees fewer and shorter records than the Planner: it needs
/// the ones the plan builds on, not the whole history.
pub const MAX_GENERATOR_RECORDS: usize = 15;
const GENERATOR_RECORD_BODY_SHOWN: usize = 300;
const VALUE_SHOWN: usize = 200;
const ANSWER_SHOWN: usize = 600;
const RUBRIC_SHOWN: usize = 1500;
const CODE_SHOWN: usize = 1000;

fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((at, _)) => format!("{}…", text[..at].trim_end()),
        None => text.to_owned(),
    }
}

fn plain(md: &str) -> String {
    one_line(&scan_inline(md).plain)
}

fn lines_or(none: &str, header: Option<String>, lines: Vec<String>) -> String {
    if lines.is_empty() {
        return none.to_owned();
    }
    let mut out = Vec::new();
    out.extend(header);
    out.extend(lines);
    out.join("\n")
}

fn record_line(r: &LearningRecord, lessons: &[Lesson], body_max: usize) -> String {
    let lesson = lessons
        .iter()
        .find(|l| l.id == r.lesson_id)
        .map_or_else(|| "?".to_owned(), |l| l.number.to_string());
    format!(
        "- [{}] ({}, lezione {lesson}) {}: {}",
        r.number,
        r.kind,
        one_line(&r.title),
        cut(&one_line(&r.body), body_max)
    )
}

/// `- [{number}] ({kind}, lezione {n}) {title}: {body}` for the newest 60
/// active records, `number` ascending. `records` is in any order.
#[must_use]
pub fn records_section(records: &[LearningRecord], lessons: &[Lesson]) -> String {
    let mut sorted: Vec<&LearningRecord> = records.iter().collect();
    sorted.sort_by_key(|r| r.number);
    let total = sorted.len();
    let shown = &sorted[total.saturating_sub(MAX_RECORDS_SHOWN)..];
    let header = (total > MAX_RECORDS_SHOWN)
        .then(|| format!("(mostrati i {MAX_RECORDS_SHOWN} più recenti di {total})"));
    let lines = shown
        .iter()
        .map(|r| record_line(r, lessons, RECORD_BODY_SHOWN))
        .collect();
    lines_or("Nessuno.", header, lines)
}

/// The numbers of the records the plan's `record` prerequisites cite.
fn cited_record_numbers(plan: &Value) -> Vec<u32> {
    list_of(plan, "prerequisites")
        .iter()
        .filter(|p| str_of(p, "basis") == "record")
        .flat_map(|p| list_of(p, "record_numbers"))
        .filter_map(Value::as_u64)
        .filter_map(|n| u32::try_from(n).ok())
        .collect()
}

/// The records the Generator sees (`records` are the active ones): at most
/// 15 lines, `number` ascending, body cut at 300 characters. The ones the
/// plan cites come first in the choice, the newest fill what is left; when
/// some are left out the first line says so.
#[must_use]
pub fn generator_records_section(
    records: &[LearningRecord],
    lessons: &[Lesson],
    plan: &Value,
) -> String {
    let cited = cited_record_numbers(plan);
    let mut newest_first: Vec<&LearningRecord> = records.iter().collect();
    newest_first.sort_by_key(|r| std::cmp::Reverse(r.number));
    let (mut chosen, others): (Vec<&LearningRecord>, Vec<&LearningRecord>) = newest_first
        .into_iter()
        .partition(|r| cited.contains(&r.number));
    chosen.truncate(MAX_GENERATOR_RECORDS);
    let room = MAX_GENERATOR_RECORDS - chosen.len();
    chosen.extend(others.into_iter().take(room));
    chosen.sort_by_key(|r| r.number);
    let header = (chosen.len() < records.len()).then(|| {
        format!(
            "(mostrati {} di {}: quelli citati dal piano e i più recenti)",
            chosen.len(),
            records.len()
        )
    });
    lines_or(
        "Nessuno.",
        header,
        chosen
            .iter()
            .map(|r| record_line(r, lessons, GENERATOR_RECORD_BODY_SHOWN))
            .collect(),
    )
}

/// `- [{key}] {term}: {definition} (evita: a; b)`: the 80 most recently
/// updated terms, alphabetical.
#[must_use]
pub fn glossary_section(terms: &[GlossaryTerm]) -> String {
    let mut recent: Vec<&GlossaryTerm> = terms.iter().collect();
    recent.sort_by_key(|t| std::cmp::Reverse((t.updated_at, t.key.clone())));
    let total = recent.len();
    recent.truncate(MAX_TERMS_SHOWN);
    recent.sort_by_key(|t| (domain::term_sort_key(&t.term), t.key.clone()));
    let header = (total > MAX_TERMS_SHOWN)
        .then(|| format!("(mostrati {MAX_TERMS_SHOWN} di {total}, i più recenti)"));
    lines_or(
        "Nessuno.",
        header,
        recent.iter().map(|t| term_line(t)).collect(),
    )
}

fn term_line(t: &GlossaryTerm) -> String {
    let mut line = format!(
        "- [{}] {}: {}",
        t.key,
        one_line(&t.term),
        one_line(&t.definition)
    );
    if !t.avoid.is_empty() {
        let avoid: Vec<String> = t.avoid.iter().map(|a| one_line(a)).collect();
        let _ = write!(line, " (evita: {})", avoid.join("; "));
    }
    line
}

/// `- {id} · lezione {n} · {status} · "{title}": {objective}`: the newest 20.
/// A lesson that followed uploaded-document sections lists them after the
/// objective (` · sezioni del PDF usate: …`), so the Planner does not repeat
/// them by guessing from titles. `covered` is by lesson id.
#[must_use]
#[allow(clippy::implicit_hasher)] // an internal helper called with the default hasher
pub fn lessons_section(lessons: &[Lesson], covered: &HashMap<LessonId, String>) -> String {
    let mut sorted: Vec<&Lesson> = lessons.iter().collect();
    sorted.sort_by_key(|l| l.number);
    let total = sorted.len();
    let shown = &sorted[total.saturating_sub(MAX_LESSONS_SHOWN)..];
    let header = (total > MAX_LESSONS_SHOWN)
        .then(|| format!("(mostrate le {MAX_LESSONS_SHOWN} più recenti di {total})"));
    let lines = shown
        .iter()
        .map(|l| {
            let mut line = format!(
                "- {} · lezione {} · {} · \"{}\": {}",
                l.id,
                l.number,
                l.status,
                one_line(&l.title),
                one_line(&l.objective)
            );
            if let Some(sections) = covered.get(&l.id) {
                let _ = write!(line, " · sezioni del PDF usate: {sections}");
            }
            line
        })
        .collect();
    lines_or("Nessuna.", header, lines)
}

/// The sources a lesson can be built on: `knowledge` with an https URL and the
/// uploaded documents, in saved order (documents first), at most 12.
/// Communities and sites without a URL are not shown to the Planner
/// (`README.md`).
#[must_use]
pub fn usable_sources(resources: &[Resource]) -> Vec<&Resource> {
    resources
        .iter()
        .filter(|r| {
            r.kind == ResourceKind::Document
                || (r.kind == ResourceKind::Knowledge
                    && r.url.as_deref().is_some_and(|u| u.starts_with("https://")))
        })
        .take(MAX_PLANNER_SOURCES)
        .collect()
}

fn source_line(prefix: &str, r: &Resource) -> String {
    // A document has no address; its id says which of two PDFs with the same
    // title is meant.
    let place = match (r.url.as_deref(), r.document_id) {
        (Some(url), _) => url.to_owned(),
        (None, Some(doc)) => format!("nessun sito: PDF caricato dalla persona · documento {doc}"),
        (None, None) => "nessun sito".to_owned(),
    };
    format!(
        "- {prefix}{} · {} — {} — {} Usala per: {}",
        r.id,
        one_line(&r.title),
        place,
        one_line(&r.annotation),
        one_line(&r.use_for)
    )
}

#[must_use]
pub fn planner_resources_section(sources: &[&Resource]) -> String {
    lines_or(
        "Nessuna.",
        None,
        sources.iter().map(|r| source_line("", r)).collect(),
    )
}

/// The plan's sources, the primary first.
#[must_use]
pub fn generator_sources_section(primary: &Resource, others: &[&Resource]) -> String {
    std::iter::once(source_line("[PRIMARIA] ", primary))
        .chain(others.iter().map(|r| source_line("[utile] ", r)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn list_of<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

#[allow(clippy::needless_pass_by_value)] // callers build the list on the spot
fn bullets(items: Vec<String>) -> String {
    if items.is_empty() {
        "- nessuno".to_owned()
    } else {
        items.join("\n")
    }
}

/// Prerequisites of one `basis`, one line each; `- nessuno` when there are none.
fn prerequisites_of(plan: &Value, basis: &str) -> String {
    bullets(
        list_of(plan, "prerequisites")
            .iter()
            .filter(|p| str_of(p, "basis") == basis)
            .map(|p| {
                let mut line = format!("- {}", one_line(str_of(p, "statement")));
                let numbers: Vec<String> = list_of(p, "record_numbers")
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|n| n.to_string())
                    .collect();
                if basis == "record" && !numbers.is_empty() {
                    let _ = write!(line, " [record {}]", numbers.join(", "));
                }
                line
            })
            .collect(),
    )
}

/// One concept with its worked example, steps and typical mistake, the
/// continuation lines indented by 3 spaces.
fn concept_block(index: usize, c: &Value) -> String {
    let mut block = format!(
        "{index}. {}: {}\n   Esempio: {}\n   Passaggi:",
        one_line(str_of(c, "name")),
        one_line(str_of(c, "key_point")),
        one_line(str_of(c, "example"))
    );
    for (i, step) in list_of(c, "steps").iter().enumerate() {
        let _ = write!(
            block,
            "\n   {}) {}",
            i + 1,
            one_line(step.as_str().unwrap_or_default())
        );
    }
    let pitfall = one_line(str_of(c, "pitfall"));
    let _ = write!(
        block,
        "\n   Errore tipico: {}",
        if pitfall.is_empty() {
            "nessuno"
        } else {
            &pitfall
        }
    );
    let visual = str_of(c, "visual");
    if !matches!(visual, "" | "none") {
        let _ = write!(block, "\n   Figura: {}", one_line(visual));
    }
    block
}

/// The plan as the Generator reads it (`README.md`, "Fase 12").
#[must_use]
pub fn generator_plan_section(plan: &Value) -> String {
    let concepts = list_of(plan, "concepts")
        .iter()
        .enumerate()
        .map(|(i, c)| concept_block(i + 1, c))
        .collect::<Vec<_>>()
        .join("\n");
    let terms = plan.get("terms").cloned().unwrap_or(Value::Null);
    let introduce = bullets(
        list_of(&terms, "introduce")
            .iter()
            .map(|t| {
                format!(
                    "- [{}] {} — {}",
                    str_of(t, "key"),
                    one_line(str_of(t, "term")),
                    one_line(str_of(t, "hint"))
                )
            })
            .collect(),
    );
    let reuse = bullets(
        list_of(&terms, "reuse")
            .iter()
            .filter_map(Value::as_str)
            .map(|k| format!("- {k}"))
            .collect(),
    );
    let exercises = list_of(plan, "exercises")
        .iter()
        .map(|e| {
            let verifies = if e.get("verifies_objective") == Some(&Value::Bool(true)) {
                " (verifica l'obiettivo)"
            } else {
                ""
            };
            format!(
                "- {} — {}{verifies}",
                str_of(e, "type"),
                one_line(str_of(e, "purpose"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Titolo di lavoro: {}\nObiettivo: {}\nPerché adesso: {}\nDurata stimata: {} minuti\n\
         Prerequisiti da richiamare (nessuna prova: serve la sezione di richiamo):\n{}\n\
         Prerequisiti dichiarati dalla persona (nessuna sezione, richiamo di una clausola):\n{}\n\
         Prerequisiti dimostrati (non si ripassano):\n{}\n\
         Concetti (uno per sezione, in quest'ordine):\n{concepts}\n\
         Termini da introdurre:\n{introduce}\nTermini già nel glossario da riusare (key):\n{reuse}\n\
         Esercizi previsti, in ordine:\n{exercises}",
        one_line(str_of(plan, "title")),
        one_line(str_of(plan, "objective")),
        one_line(str_of(plan, "why_now")),
        plan.get("estimated_minutes")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        prerequisites_of(plan, "general"),
        prerequisites_of(plan, "mission"),
        prerequisites_of(plan, "record"),
    )
}

/// The reduced plan the Recorder reads.
#[must_use]
pub fn recorder_plan_section(plan: &Value) -> String {
    let concepts = list_of(plan, "concepts")
        .iter()
        .map(|c| {
            format!(
                "- {}: {}",
                one_line(str_of(c, "name")),
                one_line(str_of(c, "key_point"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Perché adesso: {}\nConcetti:\n{concepts}",
        one_line(str_of(plan, "why_now"))
    )
}

/// `- {lesson_id} · lezione {n}: "{title}" — {reason}` for the plan's links.
#[must_use]
pub fn lesson_links_section(links: &[(&Lesson, String)]) -> String {
    lines_or(
        "Nessuna.",
        None,
        links
            .iter()
            .map(|(l, reason)| {
                format!(
                    "- {} · lezione {}: \"{}\" — {}",
                    l.id,
                    l.number,
                    one_line(&l.title),
                    one_line(reason)
                )
            })
            .collect(),
    )
}

/// The document's own glossary, in the format of the Topic glossary.
#[must_use]
pub fn document_glossary_section(document: &Value) -> String {
    lines_or(
        "Nessuno.",
        None,
        list_of(document, "glossary")
            .iter()
            .filter(|e| e.get("key").is_some())
            .map(|e| {
                let mut line = format!(
                    "- [{}] {}: {}",
                    str_of(e, "key"),
                    one_line(str_of(e, "term")),
                    plain(str_of(e, "definition"))
                );
                let avoid: Vec<String> = list_of(e, "avoid")
                    .iter()
                    .filter_map(Value::as_str)
                    .map(one_line)
                    .collect();
                if !avoid.is_empty() {
                    let _ = write!(line, " (evita: {})", avoid.join("; "));
                }
                line
            })
            .collect(),
    )
}

/// The question of an exercise as plain text, for the Grader, followed by
/// its `code` (if any) in a fenced block: the question may only make sense
/// with the code in front of it.
#[must_use]
pub fn plain_question(exercise: &domain::Exercise) -> String {
    let question = plain(&exercise.prompt);
    match &exercise.code {
        Some((language, code)) => {
            // A fence longer than any backtick run in the code: the code
            // cannot close it early.
            let longest = code
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or_default();
            let fence = "`".repeat(longest.max(2) + 1);
            format!("{question}\n{fence}{language}\n{code}\n{fence}")
        }
        None => question,
    }
}

/// A rubric as written, within the limit.
#[must_use]
pub fn rubric_text(rubric: &str) -> String {
    cut(rubric.trim(), RUBRIC_SHOWN)
}

fn answer_text(exercise: &domain::Exercise, answer: &Answer) -> String {
    let option = |id: &String| {
        exercise
            .option_texts
            .iter()
            .find(|(i, _)| i == id)
            .map_or_else(|| id.clone(), |(_, t)| t.clone())
    };
    match answer {
        Answer::Option(o) => cut(&format!("\"{}\"", option(o)), VALUE_SHOWN),
        Answer::Options(o) => cut(
            &o.iter()
                .map(|id| format!("\"{}\"", option(id)))
                .collect::<Vec<_>>()
                .join("; "),
            VALUE_SHOWN,
        ),
        Answer::Blanks(b) => cut(
            &b.iter()
                .map(|(k, v)| format!("{k}=\"{}\"", one_line(v)))
                .collect::<Vec<_>>()
                .join("; "),
            VALUE_SHOWN,
        ),
        Answer::Steps(s) => {
            let text = |id: &String| {
                exercise
                    .step_texts
                    .iter()
                    .find(|(i, _)| i == id)
                    .map_or_else(|| id.clone(), |(_, t)| t.clone())
            };
            cut(
                &s.iter()
                    .enumerate()
                    .map(|(i, id)| format!("{}) {}", i + 1, text(id)))
                    .collect::<Vec<_>>()
                    .join(" "),
                VALUE_SHOWN,
            )
        }
        Answer::Rating(r) => format!("autovalutazione {}", r.as_str()),
        Answer::Text(t) => format!("\"{}\"", cut(&one_line(t), ANSWER_SHOWN)),
    }
}

/// One block per exercise of the document, in order, attempts included.
#[must_use]
pub fn exercises_section(exercises: &LessonExercises, attempts: &[Attempt]) -> String {
    let mut out = Vec::new();
    for e in exercises.iter() {
        let kind_label = if e.required {
            "obbligatorio"
        } else {
            "facoltativo"
        };
        let question = if e.kind == ExerciseKind::FillBlank {
            let mut text = plain(&e.prompt);
            for id in &e.blank_ids {
                text = text.replace(&format!("{{{{{id}}}}}"), "____");
            }
            text
        } else {
            plain(&e.prompt)
        };
        let mut block = format!(
            "[{}] {} · {kind_label}\nDomanda: {}",
            e.id,
            e.kind,
            cut(&question, VALUE_SHOWN)
        );
        if let Some((language, code)) = &e.code {
            let _ = write!(block, "\nCodice ({language}):\n{}", cut(code, CODE_SHOWN));
        }
        if e.kind == ExerciseKind::OpenAnswer {
            if let Some(r) = &e.rubric {
                let _ = write!(block, "\nRubrica: {}", rubric_text(r));
            }
        } else if !e.correct.is_empty() {
            let _ = write!(block, "\nCorretta: {}", cut(&e.correct, VALUE_SHOWN));
        }
        let mut mine: Vec<&Attempt> = attempts.iter().filter(|a| a.block_id == e.id).collect();
        mine.sort_by_key(|a| a.attempt_number);
        if mine.is_empty() {
            block.push_str("\nNessun tentativo.");
        }
        for a in mine {
            let _ = write!(
                block,
                "\nTentativo {}: {} → {}",
                a.attempt_number,
                answer_text(e, &a.answer),
                a.outcome
            );
            if let Some(g) = &a.grade {
                let _ = write!(
                    block,
                    "\nValutazione: {}; misconcezione: {}",
                    g.verdict.as_str(),
                    g.misconception
                        .as_deref()
                        .map_or("nessuna".to_owned(), one_line)
                );
            }
        }
        out.push(block);
    }
    if out.is_empty() {
        "Nessuno.".to_owned()
    } else {
        out.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{GlossaryTermId, LearningRecordId, LessonId, RecordKind, RecordStatus, TopicId};
    use time::macros::datetime;
    use uuid::Uuid;

    fn exercise_with_code(code: &str) -> domain::Exercise {
        let doc = serde_json::json!({"blocks": [{"id": "o", "type": "open_answer", "question": "Cosa?",
            "rubric": "R", "code": {"language": "md", "code": code}}]});
        domain::LessonExercises::from_document(&doc)
            .get("o")
            .unwrap()
            .clone()
    }

    #[test]
    fn the_code_fence_is_longer_than_any_backtick_run_in_the_code() {
        let plain_code = plain_question(&exercise_with_code("let a = 1;"));
        assert_eq!(plain_code, "Cosa?\n```md\nlet a = 1;\n```");
        let tricky = plain_question(&exercise_with_code("a\n```\nb\n`"));
        assert_eq!(tricky, "Cosa?\n````md\na\n```\nb\n`\n````");
    }

    fn record(n: u32, body: &str) -> LearningRecord {
        LearningRecord {
            id: LearningRecordId::new(Uuid::from_u128(u128::from(n))),
            topic_id: TopicId::new(Uuid::from_u128(1)),
            number: n,
            kind: RecordKind::Demonstrated,
            title: "Titolo".into(),
            body: body.into(),
            evidence: "e".into(),
            status: RecordStatus::Active,
            superseded_by: None,
            lesson_id: LessonId::new(Uuid::from_u128(9)),
            created_at: datetime!(2026-11-09 10:00 UTC),
        }
    }

    #[test]
    fn empty_sections_say_so_and_records_are_truncated_and_capped() {
        assert_eq!(records_section(&[], &[]), "Nessuno.");
        assert_eq!(glossary_section(&[]), "Nessuno.");
        assert_eq!(lessons_section(&[], &HashMap::new()), "Nessuna.");
        let records: Vec<_> = (1..=62).map(|n| record(n, &"x".repeat(450))).collect();
        let text = records_section(&records, &[]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "(mostrati i 60 più recenti di 62)");
        assert!(lines[1].starts_with("- [3] (demonstrated, lezione ?) Titolo: "));
        assert!(lines[1].ends_with('…'));
        assert_eq!(lines.len(), 61);
    }

    fn plan_with(prerequisites: &Value, concepts: &Value) -> Value {
        serde_json::json!({
            "title": "Gli stop", "objective": "Saper compensare.", "why_now": "Serve.",
            "estimated_minutes": 12,
            "prerequisites": prerequisites,
            "concepts": concepts,
            "terms": {"introduce": [], "reuse": []},
            "exercises": []
        })
    }

    #[test]
    fn the_plan_splits_prerequisites_by_basis_and_spells_out_each_concept() {
        let plan = plan_with(
            &serde_json::json!([
                {"statement": "Sa cos'è\nuna cella", "basis": "general", "record_numbers": []},
                {"statement": "Scrive formule", "basis": "mission", "record_numbers": []},
                {"statement": "Prevede il move", "basis": "record", "record_numbers": [1, 3]},
                {"statement": "Conosce il borrow", "basis": "record", "record_numbers": [2]}
            ]),
            &serde_json::json!([
                {"name": "Relativo", "key_point": "Si sposta.", "example": "In C2 c'è =A2.\nCopiata.",
                 "steps": ["primo", "secondo\npasso"], "pitfall": "Pensare che sia identica."},
                {"name": "Assoluto", "key_point": "Resta fermo.", "example": "E1", "steps": ["a", "b"], "pitfall": "  "}
            ]),
        );
        let text = generator_plan_section(&plan);
        let expected = "Titolo di lavoro: Gli stop
Obiettivo: Saper compensare.
Perché adesso: Serve.
Durata stimata: 12 minuti
Prerequisiti da richiamare (nessuna prova: serve la sezione di richiamo):
- Sa cos'è una cella
Prerequisiti dichiarati dalla persona (nessuna sezione, richiamo di una clausola):
- Scrive formule
Prerequisiti dimostrati (non si ripassano):
- Prevede il move [record 1, 3]
- Conosce il borrow [record 2]
Concetti (uno per sezione, in quest'ordine):
1. Relativo: Si sposta.
   Esempio: In C2 c'è =A2. Copiata.
   Passaggi:
   1) primo
   2) secondo passo
   Errore tipico: Pensare che sia identica.
2. Assoluto: Resta fermo.
   Esempio: E1
   Passaggi:
   1) a
   2) b
   Errore tipico: nessuno
Termini da introdurre:
- nessuno
Termini già nel glossario da riusare (key):
- nessuno
Esercizi previsti, in ordine:
";
        assert_eq!(text, expected);
    }

    #[test]
    fn empty_prerequisite_groups_say_nessuno() {
        let plan = plan_with(&serde_json::json!([]), &serde_json::json!([]));
        let text = generator_plan_section(&plan);
        assert_eq!(text.matches("\n- nessuno").count(), 5, "{text}");
        assert!(text.contains("Prerequisiti dimostrati (non si ripassano):\n- nessuno\n"));
    }

    fn plan_citing(numbers: &[u32]) -> Value {
        plan_with(
            &serde_json::json!([
                {"statement": "x", "basis": "record", "record_numbers": numbers},
                {"statement": "y", "basis": "mission", "record_numbers": [1]}
            ]),
            &serde_json::json!([]),
        )
    }

    #[test]
    fn the_generator_sees_all_records_when_they_fit_and_none_when_there_are_none() {
        assert_eq!(
            generator_records_section(&[], &[], &plan_citing(&[])),
            "Nessuno."
        );
        let records: Vec<_> = (1..=15).rev().map(|n| record(n, "corpo")).collect();
        let text = generator_records_section(&records, &[], &plan_citing(&[]));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 15, "no header when nothing is left out");
        assert_eq!(lines[0], "- [1] (demonstrated, lezione ?) Titolo: corpo");
        assert_eq!(lines[14], "- [15] (demonstrated, lezione ?) Titolo: corpo");
    }

    #[test]
    fn cited_records_come_first_then_the_newest_and_the_body_is_cut_at_300() {
        let records: Vec<_> = (1..=40).map(|n| record(n, &"x".repeat(350))).collect();
        // Record 2 is cited by a `record` prerequisite; record 1 only by a
        // `mission` one, so it is not.
        let text = generator_records_section(&records, &[], &plan_citing(&[2]));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0],
            "(mostrati 15 di 40: quelli citati dal piano e i più recenti)"
        );
        assert_eq!(lines.len(), 16);
        assert!(lines[1].starts_with("- [2] "), "{}", lines[1]);
        // The other 14 are the newest, 27..=40, ascending.
        assert!(lines[2].starts_with("- [27] "), "{}", lines[2]);
        assert!(lines[15].starts_with("- [40] "), "{}", lines[15]);
        assert!(!text.contains("[1] ("));
        let body = lines[1].split_once(": ").unwrap().1;
        assert_eq!(body.chars().count(), 301);
        assert!(body.ends_with('…'));
    }

    #[test]
    fn cited_records_that_do_not_exist_or_are_many_never_exceed_fifteen() {
        let records: Vec<_> = (1..=20).map(|n| record(n, "b")).collect();
        let cited: Vec<u32> = (1..=18).collect();
        let text = generator_records_section(&records, &[], &plan_citing(&cited));
        assert_eq!(text.lines().count(), 16);
        let text = generator_records_section(&records, &[], &plan_citing(&[99]));
        assert!(text.starts_with("(mostrati 15 di 20"), "{text}");
    }

    #[test]
    fn figure_line_only_for_a_real_visual() {
        let mut c = serde_json::json!({"name": "A", "key_point": "k", "example": "e", "steps": ["x", "y"], "pitfall": "p"});
        assert!(!concept_block(1, &c).contains("Figura:"));
        c["visual"] = serde_json::json!("none");
        assert!(!concept_block(1, &c).contains("Figura:"));
        c["visual"] = serde_json::json!("cycle");
        assert!(concept_block(1, &c).ends_with("Errore tipico: p\n   Figura: cycle"));
    }

    #[test]
    fn the_glossary_is_alphabetical_and_lists_synonyms_to_avoid() {
        let term = |key: &str, t: &str, avoid: Vec<String>| GlossaryTerm {
            id: GlossaryTermId::new(Uuid::from_u128(5)),
            topic_id: TopicId::new(Uuid::from_u128(1)),
            key: key.into(),
            term: t.into(),
            definition: "Def\nsu due righe".into(),
            avoid,
            lesson_id: LessonId::new(Uuid::from_u128(9)),
            first_lesson_id: Some(LessonId::new(Uuid::from_u128(9))),
            created_at: datetime!(2026-11-09 10:00 UTC),
            updated_at: datetime!(2026-11-09 10:00 UTC),
        };
        let text = glossary_section(&[
            term("move", "Move", vec![]),
            term(
                "borrow",
                "borrow",
                vec!["prestito".into(), "noleggio".into()],
            ),
        ]);
        assert_eq!(
            text,
            "- [borrow] borrow: Def su due righe (evita: prestito; noleggio)\n- [move] Move: Def su due righe"
        );
    }
}
