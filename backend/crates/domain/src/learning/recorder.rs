//! The guarantees the backend gives about the Recorder's output
//! (`docs/contracts/lessons.md` §6.3). They live here, in code, not in the
//! prompt: "coverage is not learning" must hold whatever the model writes.

use std::collections::{HashMap, HashSet};

use super::attempt::{Attempt, Outcome};
use super::exercise::LessonExercises;
use super::record::{
    MAX_CARDS_PER_LESSON, MAX_RECORDS_PER_LESSON, MAX_TERMS_PER_LESSON, RecordKind,
};
use crate::lesson::scan_inline;

pub const RECORD_TITLE_MAX_CHARS: usize = 120;
pub const RECORD_BODY_MAX_CHARS: usize = 600;
pub const RECORD_EVIDENCE_MAX_CHARS: usize = 400;
pub const TERM_KEY_MAX_CHARS: usize = 40;
pub const TERM_MAX_CHARS: usize = 80;
pub const TERM_DEFINITION_MAX_CHARS: usize = 400;
pub const TERM_AVOID_MAX_ITEMS: usize = 3;
pub const TERM_AVOID_MAX_CHARS: usize = 120;
pub const CARD_FRONT_MAX_CHARS: usize = 300;
pub const CARD_BACK_MAX_CHARS: usize = 600;
pub const MAX_EVIDENCE_BLOCKS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordDraft {
    pub kind: RecordKind,
    pub title: String,
    pub body: String,
    pub evidence: String,
    /// Ids of the exercise blocks that prove it.
    pub evidence_blocks: Vec<String>,
    /// `number`s of the active records it replaces.
    pub supersedes: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermDraft {
    pub key: String,
    pub term: String,
    pub definition: String,
    pub avoid: Vec<String>,
    pub evidence_blocks: Vec<String>,
}

/// A card is tied to one record or term of the same output, by position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDraft {
    pub target: super::record::CardTarget,
    pub front: String,
    pub back: String,
    /// The exercise never passed that the card tests (`lessons.md` §6.1).
    pub unpassed_block: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecorderDraft {
    pub records: Vec<RecordDraft>,
    pub terms: Vec<TermDraft>,
    pub cards: Vec<CardDraft>,
}

/// What the output is checked against.
pub struct RecorderContext<'a> {
    pub attempts: &'a [Attempt],
    pub exercises: &'a LessonExercises,
    /// `number`s of the topic's active records.
    pub active_record_numbers: &'a HashSet<u32>,
}

/// What the attempts at one block show.
#[derive(Default, Clone, Copy)]
struct BlockEvidence {
    /// Some attempt was `correct` (first or second).
    passed: bool,
    /// The first attempt failed, or the Grader saw a misconception.
    stumbled: bool,
}

fn evidence(attempts: &[Attempt]) -> HashMap<&str, BlockEvidence> {
    let mut map: HashMap<&str, BlockEvidence> = HashMap::new();
    for a in attempts {
        let e = map.entry(a.block_id.as_str()).or_default();
        e.passed |= a.is_passed();
        e.stumbled |= (a.attempt_number == 1
            && matches!(a.outcome, Outcome::Incorrect | Outcome::Partial))
            || a.grade.as_ref().is_some_and(|g| g.misconception.is_some());
    }
    map
}

/// Whether the lesson has any passed attempt: without one, nothing can be
/// recorded and the Recorder is not even asked.
#[must_use]
pub fn has_passed_attempt(attempts: &[Attempt]) -> bool {
    attempts.iter().any(Attempt::is_passed)
}

fn length_error(what: &str, text: &str, max: usize) -> Option<String> {
    let n = text.trim().chars().count();
    if n == 0 {
        Some(format!("{what} è vuoto."))
    } else if n > max {
        Some(format!("{what} ha {n} caratteri, il massimo è {max}."))
    } else {
        None
    }
}

fn inline_errors(what: &str, text: &str, allow_refs: bool, out: &mut Vec<String>) {
    let scan = scan_inline(text);
    for p in &scan.problems {
        out.push(format!("{what} {}.", p.describe()));
    }
    if !allow_refs && !scan.glossary_refs.is_empty() {
        out.push(format!("{what} contiene un rimando [[...]], non ammesso."));
    }
}

fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().count() <= TERM_KEY_MAX_CHARS
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[allow(clippy::too_many_lines)] // one flat pass per kind of element reads better than helpers
/// Every problem of `draft`, in Italian and ready to be handed back to the
/// model for a repair. Empty means the draft can be stored.
#[must_use]
pub fn validate_recorder(draft: &RecorderDraft, ctx: &RecorderContext<'_>) -> Vec<String> {
    let mut errors = Vec::new();
    let seen = evidence(ctx.attempts);

    if draft.records.len() > MAX_RECORDS_PER_LESSON {
        errors.push(format!(
            "Ci sono {} record: al massimo {MAX_RECORDS_PER_LESSON}.",
            draft.records.len()
        ));
    }
    if draft.terms.len() > MAX_TERMS_PER_LESSON {
        errors.push(format!(
            "Ci sono {} termini: al massimo {MAX_TERMS_PER_LESSON}.",
            draft.terms.len()
        ));
    }
    if draft.cards.len() > MAX_CARDS_PER_LESSON {
        errors.push(format!(
            "Ci sono {} schede: al massimo {MAX_CARDS_PER_LESSON}.",
            draft.cards.len()
        ));
    }

    // A record can be replaced once only, whatever record of the draft does it.
    let mut replaced_by: HashMap<u32, usize> = HashMap::new();
    for (i, r) in draft.records.iter().enumerate() {
        let at = format!("records[{i}]");
        errors.extend(length_error(
            &format!("{at}.title"),
            &r.title,
            RECORD_TITLE_MAX_CHARS,
        ));
        errors.extend(length_error(
            &format!("{at}.body"),
            &r.body,
            RECORD_BODY_MAX_CHARS,
        ));
        errors.extend(length_error(
            &format!("{at}.evidence"),
            &r.evidence,
            RECORD_EVIDENCE_MAX_CHARS,
        ));
        inline_errors(&format!("{at}.body"), &r.body, false, &mut errors);
        for (field, text) in [("body", &r.body), ("evidence", &r.evidence)] {
            if let Some(id) = leaked_block_id(text, ctx.exercises) {
                errors.push(format!(
                    "{at}.{field} nomina l'id di esercizio '{id}': la persona non lo vede, descrivi l'esercizio a parole."
                ));
            }
        }
        let cited = cited_blocks(&at, &r.evidence_blocks, ctx, &seen, &mut errors);
        let passed = cited.iter().any(|b| b.passed);
        match r.kind {
            RecordKind::Demonstrated if !passed => errors.push(format!(
                "{at}: un record `demonstrated` richiede almeno un blocco citato con un tentativo superato (correct); nessuno dei blocchi citati lo è."
            )),
            RecordKind::MisconceptionCorrected => {
                if !cited.iter().any(|b| b.stumbled) {
                    errors.push(format!(
                        "{at}: un record `misconception_corrected` richiede un blocco citato con il primo tentativo sbagliato (incorrect/partial) o con una misconception del Grader."
                    ));
                }
                if !passed {
                    errors.push(format!(
                        "{at}: un record `misconception_corrected` richiede anche un blocco citato con un tentativo superato (correct), anche il secondo."
                    ));
                }
            }
            RecordKind::Demonstrated => {}
        }
        for n in &r.supersedes {
            if !ctx.active_record_numbers.contains(n) {
                errors.push(format!(
                    "{at}.supersedes: il record {n} non esiste o non è attivo."
                ));
            } else if let Some(first) = replaced_by.insert(*n, i) {
                errors.push(if first == i {
                    format!("{at}.supersedes: il record {n} è citato due volte.")
                } else {
                    format!(
                        "{at}.supersedes: il record {n} è già sostituito da records[{first}]: un record si sostituisce una volta sola."
                    )
                });
            }
        }
    }

    let mut keys = HashSet::new();
    for (i, t) in draft.terms.iter().enumerate() {
        let at = format!("terms[{i}]");
        if !is_key(&t.key) {
            errors.push(format!(
                "{at}.key '{}' non è valida: 1-{TERM_KEY_MAX_CHARS} caratteri tra a-z, 0-9 e _.",
                t.key.chars().take(60).collect::<String>()
            ));
        } else if !keys.insert(t.key.as_str()) {
            errors.push(format!("{at}.key '{}' è usata due volte.", t.key));
        }
        errors.extend(length_error(&format!("{at}.term"), &t.term, TERM_MAX_CHARS));
        errors.extend(length_error(
            &format!("{at}.definition"),
            &t.definition,
            TERM_DEFINITION_MAX_CHARS,
        ));
        inline_errors(
            &format!("{at}.definition"),
            &t.definition,
            false,
            &mut errors,
        );
        if t.avoid.len() > TERM_AVOID_MAX_ITEMS {
            errors.push(format!(
                "{at}.avoid ha {} voci: al massimo {TERM_AVOID_MAX_ITEMS}.",
                t.avoid.len()
            ));
        }
        for (j, a) in t.avoid.iter().enumerate() {
            errors.extend(length_error(
                &format!("{at}.avoid[{j}]"),
                a,
                TERM_AVOID_MAX_CHARS,
            ));
        }
        let cited = cited_blocks(&at, &t.evidence_blocks, ctx, &seen, &mut errors);
        if !cited.iter().any(|b| b.passed) {
            errors.push(format!(
                "{at}: un termine richiede almeno un blocco citato con un tentativo superato (correct)."
            ));
        }
    }

    for (i, c) in draft.cards.iter().enumerate() {
        let at = format!("cards[{i}]");
        errors.extend(length_error(
            &format!("{at}.front"),
            &c.front,
            CARD_FRONT_MAX_CHARS,
        ));
        errors.extend(length_error(
            &format!("{at}.back"),
            &c.back,
            CARD_BACK_MAX_CHARS,
        ));
        inline_errors(&format!("{at}.front"), &c.front, false, &mut errors);
        inline_errors(&format!("{at}.back"), &c.back, false, &mut errors);
        let exists = match c.target {
            super::record::CardTarget::Record(n) => n < draft.records.len(),
            super::record::CardTarget::Term(n) => n < draft.terms.len(),
        };
        if !exists {
            errors.push(format!(
                "{at}: la scheda non è legata a un record o termine di questo output."
            ));
        }
        if let Some(block) = &c.unpassed_block {
            let shown = block.chars().take(60).collect::<String>();
            if ctx.exercises.get(block).is_none() {
                errors.push(format!(
                    "{at}.unpassed_block: il blocco '{shown}' non è un esercizio della lezione."
                ));
            } else {
                match seen.get(block.as_str()) {
                    None => errors.push(format!(
                        "{at}.unpassed_block: il blocco '{shown}' non ha tentativi."
                    )),
                    Some(e) if e.passed => errors.push(format!(
                        "{at}.unpassed_block: il blocco '{shown}' è stato superato (ha un tentativo correct): usa null, oppure scegli un esercizio mai superato."
                    )),
                    Some(_) => {}
                }
            }
        }
    }
    errors
}

/// An exercise id that looks like an identifier (it has a digit, `_` or `-`)
/// used as a word in `text`. Plain-word ids are not checked: they could be
/// ordinary Italian.
fn leaked_block_id<'a>(text: &str, exercises: &'a LessonExercises) -> Option<&'a str> {
    let tokens: HashSet<&str> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .collect();
    exercises.iter().map(|e| e.id.as_str()).find(|id| {
        id.contains(|c: char| c.is_ascii_digit() || c == '_' || c == '-') && tokens.contains(id)
    })
}

/// Checks that each cited block is an exercise of the lesson with at least one
/// attempt; returns what the attempts show for those that are.
fn cited_blocks(
    at: &str,
    blocks: &[String],
    ctx: &RecorderContext<'_>,
    seen: &HashMap<&str, BlockEvidence>,
    errors: &mut Vec<String>,
) -> Vec<BlockEvidence> {
    if blocks.is_empty() || blocks.len() > MAX_EVIDENCE_BLOCKS {
        errors.push(format!(
            "{at}.evidence_blocks ha {} voci: servono da 1 a {MAX_EVIDENCE_BLOCKS} blocchi.",
            blocks.len()
        ));
    }
    let distinct: HashSet<&String> = blocks.iter().collect();
    if distinct.len() != blocks.len() {
        errors.push(format!("{at}.evidence_blocks ha blocchi ripetuti."));
    }
    let mut out = Vec::new();
    for b in blocks {
        if ctx.exercises.get(b).is_none() {
            errors.push(format!(
                "{at}: il blocco '{}' non è un esercizio della lezione.",
                b.chars().take(60).collect::<String>()
            ));
        } else if let Some(e) = seen.get(b.as_str()) {
            out.push(*e);
        } else {
            errors.push(format!(
                "{at}: il blocco '{b}' non ha nessun tentativo, quindi non può essere una prova."
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use time::macros::datetime;
    use uuid::Uuid;

    use super::*;
    use crate::ids::{AttemptId, LessonId};
    use crate::learning::attempt::{Answer, Grade, Verdict};
    use crate::learning::exercise::ExerciseKind;
    use crate::learning::record::CardTarget;

    fn exercises() -> LessonExercises {
        LessonExercises::from_document(&json!({"blocks": [
            {"id": "q1", "type": "quiz_multi", "question": "Q", "options": [{"id": "a"}, {"id": "b"}, {"id": "c"}]},
            {"id": "q2", "type": "quiz_single", "question": "Q", "options": [{"id": "a"}, {"id": "b"}]},
            {"id": "oa", "type": "open_answer", "question": "Q", "rubric": "R"},
            {"id": "unused", "type": "flashcard", "front": "f", "back": "b"}
        ]}))
    }

    fn attempt(block: &str, kind: ExerciseKind, n: u8, outcome: Outcome) -> Attempt {
        Attempt {
            id: AttemptId::new(Uuid::from_u128(u128::from(n) + 10)),
            lesson_id: LessonId::new(Uuid::from_u128(1)),
            block_id: block.into(),
            kind,
            attempt_number: n,
            answer: Answer::Option("a".into()),
            outcome,
            grade: None,
            answered_at: datetime!(2026-11-09 10:00 UTC),
            created_at: datetime!(2026-11-09 10:00 UTC),
        }
    }

    fn record(kind: RecordKind, blocks: &[&str]) -> RecordDraft {
        RecordDraft {
            kind,
            title: "Titolo".into(),
            body: "Corpo con `codice`.".into(),
            evidence: "Prova.".into(),
            evidence_blocks: blocks.iter().map(|b| (*b).to_owned()).collect(),
            supersedes: vec![],
        }
    }

    fn check(draft: &RecorderDraft, attempts: &[Attempt], active: &[u32]) -> Vec<String> {
        let ex = exercises();
        let active: HashSet<u32> = active.iter().copied().collect();
        validate_recorder(
            draft,
            &RecorderContext {
                attempts,
                exercises: &ex,
                active_record_numbers: &active,
            },
        )
    }

    fn draft(record: RecordDraft) -> RecorderDraft {
        RecorderDraft {
            records: vec![record],
            ..RecorderDraft::default()
        }
    }

    #[test]
    fn a_demonstrated_record_needs_a_passed_exercise() {
        let d = draft(record(RecordKind::Demonstrated, &["q1"]));
        let failed = [attempt(
            "q1",
            ExerciseKind::QuizMulti,
            1,
            Outcome::Incorrect,
        )];
        let errors = check(&d, &failed, &[]);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("demonstrated"));
        let passed = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        assert_eq!(check(&d, &passed, &[]), Vec::<String>::new());
    }

    #[test]
    fn evidence_must_be_an_exercise_with_attempts() {
        let attempts = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        for (blocks, needle) in [
            (vec!["nope"], "non è un esercizio"),
            (vec!["unused", "q1"], "nessun tentativo"),
            (vec![], "evidence_blocks ha 0 voci"),
            (vec!["q1", "q1"], "ripetuti"),
        ] {
            let d = draft(record(RecordKind::Demonstrated, &blocks));
            let errors = check(&d, &attempts, &[]);
            assert!(
                errors.iter().any(|e| e.contains(needle)),
                "{needle}: {errors:?}"
            );
        }
    }

    #[test]
    fn a_corrected_misconception_needs_a_stumble_and_a_pass() {
        let d = draft(record(RecordKind::MisconceptionCorrected, &["q1"]));
        let wrong_then_right = [
            attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Incorrect),
            attempt("q1", ExerciseKind::QuizMulti, 2, Outcome::Correct),
        ];
        assert_eq!(check(&d, &wrong_then_right, &[]), Vec::<String>::new());

        let always_right = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        let errors = check(&d, &always_right, &[]);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("primo tentativo sbagliato"))
        );

        let never_right = [attempt(
            "q1",
            ExerciseKind::QuizMulti,
            1,
            Outcome::Incorrect,
        )];
        let errors = check(&d, &never_right, &[]);
        assert!(errors.iter().any(|e| e.contains("tentativo superato")));
    }

    #[test]
    fn the_stumble_may_be_a_graders_misconception_on_another_cited_block() {
        let mut graded = attempt("oa", ExerciseKind::OpenAnswer, 1, Outcome::Correct);
        graded.grade = Some(Grade {
            verdict: Verdict::Correct,
            feedback: "Ok".into(),
            misconception: Some("Pensava che...".into()),
        });
        let d = draft(record(RecordKind::MisconceptionCorrected, &["oa"]));
        assert_eq!(check(&d, &[graded], &[]), Vec::<String>::new());
    }

    #[test]
    fn a_pending_open_answer_is_no_evidence() {
        let pending = [attempt("oa", ExerciseKind::OpenAnswer, 1, Outcome::Pending)];
        let d = draft(record(RecordKind::Demonstrated, &["oa"]));
        assert_ne!(check(&d, &pending, &[]), Vec::<String>::new());
        assert!(!has_passed_attempt(&pending));
    }

    #[test]
    fn terms_need_a_passed_exercise_and_a_valid_key() {
        let attempts = [
            attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct),
            attempt("q2", ExerciseKind::QuizSingle, 1, Outcome::Incorrect),
        ];
        let term = |key: &str, blocks: &[&str]| TermDraft {
            key: key.into(),
            term: "ownership".into(),
            definition: "Chi possiede il valore.".into(),
            avoid: vec![],
            evidence_blocks: blocks.iter().map(|b| (*b).to_owned()).collect(),
        };
        let ok = RecorderDraft {
            terms: vec![term("ownership", &["q1"])],
            ..RecorderDraft::default()
        };
        assert_eq!(check(&ok, &attempts, &[]), Vec::<String>::new());
        let bad = RecorderDraft {
            terms: vec![
                term("Own Ership", &["q2"]),
                term("ownership", &["q1"]),
                term("ownership", &["q1"]),
            ],
            ..RecorderDraft::default()
        };
        let errors = check(&bad, &attempts, &[]);
        assert!(errors.iter().any(|e| e.contains("terms[0].key")));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("terms[0]: un termine richiede"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("terms[2].key 'ownership' è usata due volte"))
        );
    }

    #[test]
    fn supersession_targets_active_records_only() {
        let attempts = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        let mut r = record(RecordKind::Demonstrated, &["q1"]);
        r.supersedes = vec![2, 9, 2];
        let errors = check(&draft(r), &attempts, &[1, 2]);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("record 9")));
        assert!(errors.iter().any(|e| e.contains("due volte")));
    }

    #[test]
    fn two_records_of_the_draft_cannot_replace_the_same_one() {
        let attempts = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        let mut a = record(RecordKind::Demonstrated, &["q1"]);
        a.supersedes = vec![1];
        let mut b = a.clone();
        b.title = "Altro".into();
        let d = RecorderDraft {
            records: vec![a, b],
            ..RecorderDraft::default()
        };
        let errors = check(&d, &attempts, &[1]);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("records[1].supersedes") && errors[0].contains("records[0]"));
    }

    #[test]
    fn cards_belong_to_something_of_the_same_output() {
        let attempts = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        let mut d = draft(record(RecordKind::Demonstrated, &["q1"]));
        d.cards = vec![
            CardDraft {
                target: CardTarget::Record(0),
                front: "Cosa succede?".into(),
                back: "Il valore si sposta.".into(),
                unpassed_block: None,
            },
            CardDraft {
                target: CardTarget::Term(0),
                front: "x".into(),
                back: "y".into(),
                unpassed_block: None,
            },
        ];
        let errors = check(&d, &attempts, &[]);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("cards[1]"));
    }

    #[test]
    fn an_unpassed_block_must_be_an_attempted_exercise_never_passed() {
        // q1 passed on the second try, q2 never passed, oa never attempted.
        let attempts = [
            attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Incorrect),
            attempt("q1", ExerciseKind::QuizMulti, 2, Outcome::Correct),
            attempt("q2", ExerciseKind::QuizMulti, 1, Outcome::Incorrect),
            attempt("q2", ExerciseKind::QuizMulti, 2, Outcome::Incorrect),
        ];
        let with = |block: Option<&str>| {
            let mut d = draft(record(RecordKind::Demonstrated, &["q1"]));
            d.cards = vec![CardDraft {
                target: CardTarget::Record(0),
                front: "Cosa succede?".into(),
                back: "Il valore si sposta.".into(),
                unpassed_block: block.map(str::to_owned),
            }];
            check(&d, &attempts, &[])
        };
        assert_eq!(with(None), Vec::<String>::new());
        assert_eq!(with(Some("q2")), Vec::<String>::new());
        let missing = with(Some("nope"));
        assert!(
            missing
                .iter()
                .any(|e| e.contains("cards[0].unpassed_block") && e.contains("non è un esercizio")),
            "{missing:?}"
        );
        let untried = with(Some("oa"));
        assert!(
            untried.iter().any(|e| e.contains("non ha tentativi")),
            "{untried:?}"
        );
        let passed = with(Some("q1"));
        assert!(
            passed.iter().any(|e| e.contains("è stato superato")),
            "{passed:?}"
        );
    }

    #[test]
    fn block_ids_do_not_leak_into_prose() {
        let attempts = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        let mut r = record(RecordKind::Demonstrated, &["q1"]);
        r.body = "Hai risposto bene alla q1.".into();
        let errors = check(&draft(r), &attempts, &[]);
        assert!(
            errors.iter().any(|e| e.contains("id di esercizio 'q1'")),
            "{errors:?}"
        );
    }

    #[test]
    fn limits_and_markdown_are_enforced() {
        let attempts = [attempt("q1", ExerciseKind::QuizMulti, 1, Outcome::Correct)];
        let mut r = record(RecordKind::Demonstrated, &["q1"]);
        r.body = "riga uno\nriga due".into();
        r.title = "t".repeat(121);
        let mut d = draft(r.clone());
        d.records = vec![r; 6];
        let errors = check(&d, &attempts, &[]);
        assert!(errors.iter().any(|e| e.contains("6 record")));
        assert!(errors.iter().any(|e| e.contains("title ha 121")));
        assert!(errors.iter().any(|e| e.contains("a capo")));
    }
}
