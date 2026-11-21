//! Context Assembler for the Tutor (phase 2): a pure function from prompts,
//! topic and stored messages to an [`LlmRequest`]. Design: `llm-port.md` §10.

use domain::llm::{LlmMessage, LlmRequest, LlmRole, MessageRole as LlmMessageRole, SystemBlock};
use domain::{
    ChatMessage, DocumentView, MessageRole, MessageStatus, Mission, Preference, PreferenceScope,
    Topic,
};

use crate::document_context::documents_index;
use crate::prompts::{DocumentsSection, TemplateError, TutorPrompts, render_with_documents};

/// How many stored messages the caller should fetch (newest first) to let the
/// assembler decide whether older ones were left out. One more than is used.
const MAX_HISTORY_MESSAGES: usize = 200;
pub const HISTORY_FETCH: u32 = 201;

const HISTORY_NOTE: &str = "I messaggi più vecchi di questa chat sono stati omessi per brevità: se ti serve un dato che non vedi, chiedilo.";

#[derive(Debug, Clone, Copy)]
pub struct HistoryLimits {
    /// Estimated tokens of history sent to the model.
    pub token_budget: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssembleError {
    #[error("prompt template: {0}")]
    Template(#[from] TemplateError),
    #[error("there is no user message to answer")]
    NothingToAnswer,
}

/// Local token estimate: deliberately pessimistic for Italian, so errors fall
/// on the safe side. The budget is far below the model's window.
fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(3) + 6
}

/// Single line, no control characters: user text is data, not layout.
pub(crate) fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// What the Tutor knows about the topic besides the chat: its mission and the
/// user's preferences, read from the repositories at the start of the turn.
#[derive(Debug, Clone, Copy, Default)]
pub struct TutorContext<'a> {
    /// The confirmed mission.
    pub confirmed: Option<&'a Mission>,
    /// The open draft (a first proposal, or a proposed change).
    pub draft: Option<&'a Mission>,
    /// Global and topic preferences, newest first. At most
    /// [`PREFERENCES_SHOWN`] are rendered; fetch one more to know whether
    /// older ones were left out.
    pub preferences_newest_first: &'a [Preference],
    /// The lesson the question comes from (`lessons.md` §7), for this turn.
    pub lesson: Option<&'a LessonNote>,
    /// The topic's uploaded documents, so the Tutor can ask which parts to
    /// study. Only the ready ones are shown.
    pub documents: &'a [DocumentView],
}

/// Which lesson a chat turn is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonNote {
    pub number: u32,
    pub title: String,
    pub objective: String,
}

/// Preferences shown to the model.
pub const PREFERENCES_SHOWN: usize = 20;

pub(crate) fn bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        return "- nessuno".to_owned();
    }
    items
        .iter()
        .map(|i| format!("- {}", one_line(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the prompts show for a mission with no starting point (saved before
/// contract v1.2).
pub(crate) const STARTING_POINT_UNDECLARED: &str = "non dichiarato";

/// The starting point on one line, or [`STARTING_POINT_UNDECLARED`].
pub(crate) fn starting_point_text(m: &Mission) -> String {
    m.content
        .starting_point()
        .map_or_else(|| STARTING_POINT_UNDECLARED.to_owned(), one_line)
}

fn render_mission_body(m: &Mission) -> String {
    format!(
        "Perché: {}\nCriteri di successo:\n{}\nPunto di partenza: {}\nVincoli:\n{}\nFuori scope:\n{}",
        one_line(m.content.why()),
        bullet_list(m.content.success_criteria()),
        starting_point_text(m),
        bullet_list(m.content.constraints()),
        bullet_list(m.content.out_of_scope()),
    )
}

/// Format fixed by `backend/prompts/README.md`.
fn render_mission_section(ctx: &TutorContext<'_>) -> String {
    let mut sections = Vec::new();
    if let Some(m) = ctx.confirmed {
        sections.push(format!(
            "Mission confermata dalla persona (versione {}):\n{}",
            m.version,
            render_mission_body(m)
        ));
    }
    if let Some(d) = ctx.draft {
        let header = match ctx.confirmed {
            Some(c) => format!(
                "Proposta di modifica in attesa di conferma (versione {}), la mission {} resta valida finché non conferma:",
                d.version, c.version
            ),
            None => format!(
                "Bozza in attesa di conferma (versione {}): la persona la vede come card con Conferma e Modifica.",
                d.version
            ),
        };
        sections.push(format!("{header}\n{}", render_mission_body(d)));
    }
    if sections.is_empty() {
        "Nessuna mission confermata e nessuna bozza.".to_owned()
    } else {
        sections.join("\n\n")
    }
}

pub(crate) fn render_preferences_section(newest_first: &[Preference]) -> String {
    if newest_first.is_empty() {
        return "Nessuna.".to_owned();
    }
    let mut lines = Vec::new();
    if newest_first.len() > PREFERENCES_SHOWN {
        lines.push(format!("(mostrate le {PREFERENCES_SHOWN} più recenti)"));
    }
    lines.extend(newest_first.iter().take(PREFERENCES_SHOWN).rev().map(|p| {
        let scope = match p.scope {
            PreferenceScope::Global => "tutti gli argomenti",
            PreferenceScope::Topic(_) => "questo argomento",
        };
        format!("- [{scope}] {}", one_line(p.text.as_str()))
    }));
    lines.join("\n")
}

/// Builds the request for answering the conversation as it stands.
///
/// `history_newest_first` is what the repository returned. Left out: messages
/// that are not `complete` (the reply being generated, failed attempts) and
/// anything past the newest 200. Consecutive user messages (after a failed
/// reply) are merged; whole messages are dropped from the oldest end to fit the
/// budget, and the history always starts with a user message.
pub fn assemble_tutor_request(
    prompts: &TutorPrompts,
    topic: &Topic,
    mission: &TutorContext<'_>,
    history_newest_first: &[ChatMessage],
    limits: HistoryLimits,
) -> Result<LlmRequest, AssembleError> {
    let mut older_omitted = history_newest_first.len() > MAX_HISTORY_MESSAGES;

    // Oldest first, complete messages only, user messages merged.
    let mut turns: Vec<(LlmMessageRole, String)> = Vec::new();
    for m in history_newest_first
        .iter()
        .take(MAX_HISTORY_MESSAGES)
        .rev()
        .filter(|m| m.status == MessageStatus::Complete)
    {
        let role = match m.role {
            MessageRole::User => LlmMessageRole::User,
            MessageRole::Assistant => LlmMessageRole::Assistant,
        };
        let text = m.text();
        if text.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some((last_role, last_text))
                if *last_role == LlmMessageRole::User && role == LlmMessageRole::User =>
            {
                last_text.push_str("\n\n");
                last_text.push_str(&text);
            }
            _ => turns.push((role, text)),
        }
    }

    // Keep the newest messages that fit; the newest is always kept.
    let mut spent = 0;
    let mut keep_from = turns.len();
    for (i, (_, text)) in turns.iter().enumerate().rev() {
        spent += estimate_tokens(text);
        if spent > limits.token_budget && keep_from != turns.len() {
            break;
        }
        keep_from = i;
    }
    if keep_from > 0 {
        older_omitted = true;
    }
    let mut kept = turns.split_off(keep_from);
    while kept
        .first()
        .is_some_and(|(role, _)| *role == LlmMessageRole::Assistant)
    {
        kept.remove(0);
        older_omitted = true;
    }
    if kept
        .last()
        .is_none_or(|(role, _)| *role != LlmMessageRole::User)
    {
        return Err(AssembleError::NothingToAnswer);
    }

    let index = documents_index(mission.documents, false);
    let context = render_with_documents(
        prompts.context_template,
        topic.language,
        &[
            ("topic_title", &one_line(topic.title.as_str())),
            ("topic_state", topic.state.as_str()),
            (
                "history_note",
                if older_omitted { HISTORY_NOTE } else { "" },
            ),
            ("mission_section", &render_mission_section(mission)),
            (
                "preferences_section",
                &render_preferences_section(mission.preferences_newest_first),
            ),
        ],
        &DocumentsSection(index.as_deref()),
    )?;

    // Appended in code: `tutor/context.md` has no placeholder for it. One
    // line per fact, titles flattened (they are data, not instructions).
    let context = match mission.lesson {
        Some(l) => format!(
            "{context}\n\n## Lezione da cui nasce la domanda\nLezione {}: \"{}\"\nObiettivo: {}",
            l.number,
            one_line(&l.title),
            one_line(&l.objective)
        ),
        None => context,
    };

    let mut request = LlmRequest::new(LlmRole::Tutor, "tutor.chat");
    request.system = vec![
        SystemBlock {
            text: prompts.system.to_owned(),
            cache_breakpoint: true,
        },
        SystemBlock {
            text: context,
            cache_breakpoint: true,
        },
    ];
    request.messages = kept
        .into_iter()
        .map(|(role, text)| LlmMessage::text(role, text))
        .collect();
    if let Some(last) = request.messages.last_mut() {
        last.cache_breakpoint = true;
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::llm::Part;
    use domain::{
        ChatErrorCode, FolderId, MessageError, MessageId, MessageText, TopicId, TopicState,
        TopicTitle,
    };
    use time::macros::datetime;
    use uuid::Uuid;

    const T0: time::OffsetDateTime = datetime!(2026-11-08 10:00 UTC);
    const BIG: HistoryLimits = HistoryLimits { token_budget: 8000 };

    fn topic(title: &str, state: TopicState) -> Topic {
        let mut t = Topic::new(
            TopicId::new(Uuid::from_u128(1)),
            domain::UserId::OWNER,
            FolderId::new(Uuid::from_u128(2)),
            TopicTitle::parse(title).unwrap(),
            T0,
        );
        t.state = state;
        t
    }

    fn user(n: u128, text: &str) -> ChatMessage {
        ChatMessage::user(
            MessageId::new(Uuid::from_u128(n)),
            TopicId::new(Uuid::from_u128(1)),
            MessageText::parse(text).unwrap(),
            T0,
        )
    }

    fn assistant(n: u128, text: &str, status: MessageStatus) -> ChatMessage {
        let mut m = ChatMessage::assistant_streaming(
            MessageId::new(Uuid::from_u128(n)),
            TopicId::new(Uuid::from_u128(1)),
            T0,
        );
        if !text.is_empty() {
            m.append_text(text, T0);
        }
        match status {
            MessageStatus::Streaming => {}
            MessageStatus::Complete => m.complete(T0).unwrap(),
            MessageStatus::Failed => m
                .fail(
                    MessageError::from_code(ChatErrorCode::Internal, domain::ContentLanguage::It),
                    T0,
                )
                .unwrap(),
        }
        m
    }

    fn newest_first(mut v: Vec<ChatMessage>) -> Vec<ChatMessage> {
        v.reverse();
        v
    }

    fn assemble(history: Vec<ChatMessage>, limits: HistoryLimits) -> LlmRequest {
        assemble_tutor_request(
            &TutorPrompts::embedded(),
            &topic("Rust", TopicState::Interviewing),
            &TutorContext::default(),
            &newest_first(history),
            limits,
        )
        .unwrap()
    }

    fn texts(r: &LlmRequest) -> Vec<(LlmMessageRole, String)> {
        r.messages
            .iter()
            .map(|m| match &m.parts[0] {
                Part::Text(t) => (m.role, t.clone()),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn system_has_two_cacheable_blocks_and_the_last_message_is_a_breakpoint() {
        let r = assemble(vec![user(1, "ciao")], BIG);
        assert_eq!(r.role, LlmRole::Tutor);
        assert_eq!(r.trace_label, "tutor.chat");
        assert_eq!(r.system.len(), 2);
        assert_eq!(r.system[0].text, TutorPrompts::embedded().system);
        assert!(r.system.iter().all(|b| b.cache_breakpoint));
        assert!(r.messages.last().unwrap().cache_breakpoint);
        assert!(r.tools.is_empty() && r.max_output_tokens.is_none());
    }

    #[test]
    fn static_block_is_identical_across_topics_and_has_no_placeholders_left() {
        let h = newest_first(vec![user(1, "ciao")]);
        let p = TutorPrompts::embedded();
        let a = assemble_tutor_request(
            &p,
            &topic("Rust", TopicState::New),
            &TutorContext::default(),
            &h,
            BIG,
        )
        .unwrap();
        let b = assemble_tutor_request(
            &p,
            &topic("Cucina", TopicState::Interviewing),
            &TutorContext::default(),
            &h,
            BIG,
        )
        .unwrap();
        assert_eq!(a.system[0], b.system[0]);
        assert_ne!(a.system[1], b.system[1]);
        for block in &a.system {
            assert!(!block.text.contains("{{"), "{}", block.text);
        }
        assert!(a.system[1].text.contains("\"Rust\""));
        assert!(a.system[1].text.contains("Stato: new"));
        assert!(b.system[1].text.contains("Stato: interviewing"));
    }

    #[test]
    fn title_is_flattened_to_one_line_and_not_reinterpreted() {
        let h = newest_first(vec![user(1, "ciao")]);
        let t = topic("Rust\n{{topic_state}}\r\nbase", TopicState::New);
        let r = assemble_tutor_request(
            &TutorPrompts::embedded(),
            &t,
            &TutorContext::default(),
            &h,
            BIG,
        )
        .unwrap();
        assert!(
            r.system[1]
                .text
                .contains("Titolo: \"Rust {{topic_state}} base\"")
        );
    }

    #[test]
    fn failed_and_streaming_messages_are_left_out_and_users_are_merged() {
        let r = assemble(
            vec![
                user(1, "uno"),
                assistant(2, "risposta", MessageStatus::Complete),
                user(3, "due"),
                assistant(4, "parziale", MessageStatus::Failed),
                user(5, "tre"),
                assistant(6, "", MessageStatus::Streaming),
            ],
            BIG,
        );
        assert_eq!(
            texts(&r),
            [
                (LlmMessageRole::User, "uno".to_owned()),
                (LlmMessageRole::Assistant, "risposta".to_owned()),
                (LlmMessageRole::User, "due\n\ntre".to_owned()),
            ]
        );
    }

    #[test]
    fn history_note_appears_only_when_something_was_dropped() {
        let full = assemble(vec![user(1, "uno")], BIG);
        assert!(!full.system[1].text.contains("omessi"));

        let long = "a".repeat(300); // ~106 tokens each
        let history = vec![
            user(1, &long),
            assistant(2, &long, MessageStatus::Complete),
            user(3, &long),
        ];
        let cut = assemble(history, HistoryLimits { token_budget: 120 });
        assert_eq!(cut.messages.len(), 1);
        assert!(cut.system[1].text.contains("omessi per brevità"));
    }

    #[test]
    fn truncation_never_starts_with_an_assistant_message() {
        let long = "a".repeat(300);
        let history = vec![
            user(1, &long),
            assistant(2, "breve", MessageStatus::Complete),
            user(3, &long),
        ];
        // Budget fits the last two; the assistant that would lead is dropped.
        let r = assemble(
            history,
            HistoryLimits {
                token_budget: 112 + 8,
            },
        );
        assert_eq!(texts(&r), [(LlmMessageRole::User, long)]);
        assert!(r.system[1].text.contains("omessi per brevità"));
    }

    #[test]
    fn a_single_oversized_message_is_still_sent_whole() {
        let huge = "a".repeat(7000);
        let r = assemble(vec![user(1, &huge)], HistoryLimits { token_budget: 10 });
        assert_eq!(texts(&r), [(LlmMessageRole::User, huge)]);
        assert!(!r.system[1].text.contains("omessi"));
    }

    #[test]
    fn only_the_newest_200_are_used() {
        let mut history = Vec::new();
        for n in 0..201u128 {
            history.push(if n % 2 == 0 {
                user(n, &format!("u{n}"))
            } else {
                assistant(n, &format!("a{n}"), MessageStatus::Complete)
            });
        }
        let r = assemble(history, BIG);
        // 201 stored (oldest = u0): u0 is out, a1 would lead and is dropped too.
        assert_eq!(r.messages.len(), 199);
        assert_eq!(texts(&r)[0].1, "u2");
        assert!(r.system[1].text.contains("omessi per brevità"));
    }

    #[test]
    fn nothing_to_answer_is_an_error() {
        let err = assemble_tutor_request(
            &TutorPrompts::embedded(),
            &topic("Rust", TopicState::New),
            &TutorContext::default(),
            &[],
            BIG,
        )
        .unwrap_err();
        assert_eq!(err, AssembleError::NothingToAnswer);
        let only_failed = newest_first(vec![assistant(1, "x", MessageStatus::Failed)]);
        assert!(
            assemble_tutor_request(
                &TutorPrompts::embedded(),
                &topic("Rust", TopicState::New),
                &TutorContext::default(),
                &only_failed,
                BIG
            )
            .is_err()
        );
    }

    fn mission(version: u32, why: &str) -> Mission {
        Mission::draft(
            TopicId::new(Uuid::from_u128(1)),
            version,
            domain::MissionContent::parse(
                why,
                "Programmo\nin Kotlin",
                &["So fare X\ne Y".to_owned()],
                &[],
                &["Embedded".to_owned()],
            )
            .unwrap(),
            T0,
        )
    }

    fn pref(n: u128, text: &str, scope: PreferenceScope) -> Preference {
        Preference {
            user_id: domain::UserId::OWNER,
            id: domain::PreferenceId::new(Uuid::from_u128(n)),
            scope,
            text: domain::PreferenceText::parse(text).unwrap(),
            created_at: T0,
        }
    }

    #[test]
    fn mission_and_preferences_default_to_the_empty_sentences() {
        let r = assemble(vec![user(1, "ciao")], BIG);
        let b = &r.system[1].text;
        assert!(
            b.contains("Nessuna mission confermata e nessuna bozza."),
            "{b}"
        );
        assert!(b.contains("Nessuna."), "{b}");
    }

    #[test]
    fn mission_sections_follow_the_documented_format() {
        let confirmed = mission(1, "Per lavoro\ncon {{topic_state}}");
        let draft = mission(2, "Nuovo");
        let h = newest_first(vec![user(1, "ciao")]);
        let render = |ctx: &TutorContext<'_>| {
            assemble_tutor_request(
                &TutorPrompts::embedded(),
                &topic("Rust", TopicState::Interviewing),
                ctx,
                &h,
                BIG,
            )
            .unwrap()
            .system[1]
                .text
                .clone()
        };
        let only_confirmed = render(&TutorContext {
            confirmed: Some(&confirmed),
            ..TutorContext::default()
        });
        assert!(only_confirmed.contains(
            "Mission confermata dalla persona (versione 1):\nPerché: Per lavoro con {{topic_state}}\n\
             Criteri di successo:\n- So fare X e Y\nPunto di partenza: Programmo in Kotlin\n\
             Vincoli:\n- nessuno\nFuori scope:\n- Embedded"
        ), "{only_confirmed}");
        assert!(!only_confirmed.contains("Bozza"));

        let first_draft = render(&TutorContext {
            draft: Some(&draft),
            ..TutorContext::default()
        });
        assert!(first_draft.contains(
            "Bozza in attesa di conferma (versione 2): la persona la vede come card con Conferma e Modifica.\nPerché: Nuovo"
        ), "{first_draft}");

        let both = render(&TutorContext {
            confirmed: Some(&confirmed),
            draft: Some(&draft),
            ..TutorContext::default()
        });
        let c = both
            .find("Mission confermata dalla persona (versione 1)")
            .unwrap();
        let d = both
            .find("Proposta di modifica in attesa di conferma (versione 2), la mission 1 resta valida finché non conferma:")
            .unwrap();
        assert!(c < d);
        // The draft and the change proposal show it too, in the same place.
        for text in [&first_draft, &both] {
            assert!(
                text.contains("Programmo in Kotlin\nVincoli:\n- nessuno"),
                "{text}"
            );
        }
        assert_eq!(
            both.matches("Punto di partenza: Programmo in Kotlin")
                .count(),
            2
        );

        // A version saved before contract v1.2 has none.
        let mut old = mission(3, "Vecchia");
        old.content = domain::MissionContent::parse_stored(
            "Vecchia",
            None,
            &["So fare X".to_owned()],
            &[],
            &[],
        )
        .unwrap();
        let undeclared = render(&TutorContext {
            confirmed: Some(&old),
            draft: Some(&old),
            ..TutorContext::default()
        });
        assert_eq!(
            undeclared
                .matches("Punto di partenza: non dichiarato\nVincoli:")
                .count(),
            2
        );
    }

    #[test]
    fn only_the_twenty_newest_preferences_are_shown_oldest_first() {
        let newest_first: Vec<Preference> = (0..21)
            .rev()
            .map(|n| {
                let scope = if n % 2 == 0 {
                    PreferenceScope::Global
                } else {
                    PreferenceScope::Topic(TopicId::new(Uuid::from_u128(1)))
                };
                pref(n, &format!("p{n}"), scope)
            })
            .collect();
        let section = render_preferences_section(&newest_first);
        let lines: Vec<_> = section.lines().collect();
        assert_eq!(lines[0], "(mostrate le 20 più recenti)");
        assert_eq!(lines.len(), 21);
        assert_eq!(lines[1], "- [questo argomento] p1");
        assert_eq!(lines[20], "- [tutti gli argomenti] p20");
    }

    #[test]
    fn estimate_counts_code_points_not_bytes() {
        assert_eq!(estimate_tokens(&"日".repeat(30)), 16);
        assert_eq!(estimate_tokens(""), 6);
    }
}
