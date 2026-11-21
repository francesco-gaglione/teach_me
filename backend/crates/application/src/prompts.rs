//! Prompts embedded at compile time from `backend/prompts/` (owned by
//! `llm-engineer`). Nothing is read at run time.

use domain::ContentLanguage;

/// A role's prompts: the static system text, the per-topic template and the
/// version written to the logs of every call.
#[derive(Debug, Clone, Copy)]
pub struct TutorPrompts {
    pub system: &'static str,
    pub context_template: &'static str,
    /// JSON with the definitions of the Tutor's tools.
    pub tools: &'static str,
    /// JSON with the texts returned to the model as tool results.
    pub tool_results: &'static str,
    pub version: &'static str,
}

impl TutorPrompts {
    /// The prompts shipped in the binary.
    #[must_use]
    pub const fn embedded() -> Self {
        Self {
            system: include_str!("../../../prompts/tutor/system.md"),
            context_template: include_str!("../../../prompts/tutor/context.md"),
            tools: include_str!("../../../prompts/tutor/tools.json"),
            tool_results: include_str!("../../../prompts/tutor/tool_results.json"),
            version: include_str!("../../../prompts/tutor/VERSION"),
        }
    }
}

/// The Researcher's prompts and the schema of its output.
#[derive(Debug, Clone, Copy)]
pub struct ResearcherPrompts {
    pub system: &'static str,
    pub context_template: &'static str,
    /// JSON `{version, name, schema}` (`output_schema.json`).
    pub output_schema: &'static str,
    /// System text of the repair call (`repair_system.md`): the repair does not
    /// reuse the research blocks, it only sees the output and the allowed URLs.
    pub repair_system: &'static str,
    pub version: &'static str,
}

impl ResearcherPrompts {
    /// The prompts shipped in the binary.
    #[must_use]
    pub const fn embedded() -> Self {
        Self {
            system: include_str!("../../../prompts/researcher/system.md"),
            context_template: include_str!("../../../prompts/researcher/context.md"),
            output_schema: include_str!("../../../prompts/researcher/output_schema.json"),
            repair_system: include_str!("../../../prompts/researcher/repair_system.md"),
            version: include_str!("../../../prompts/researcher/VERSION"),
        }
    }
}

/// The prompts of a lesson role (Planner, Generator, Grader, Recorder).
#[derive(Debug, Clone, Copy)]
pub struct RolePrompts {
    pub system: &'static str,
    pub context_template: &'static str,
    /// JSON `{version, name, schema}`; `None` when the role asks for its
    /// output as plain JSON text (the Generator's document is described by
    /// its system prompt and checked by the Validator).
    pub output_schema: Option<&'static str>,
    /// Template of the repair request (`{{errors}}`, `{{previous_output}}`);
    /// empty for a role that is never repaired (the Grader).
    pub repair_template: &'static str,
    /// Second part of the Generator's user message when it reads web pages
    /// (`{{web_sources_list}}`, ADR 0009); empty for the other roles.
    pub kickoff_web_template: &'static str,
    /// Template of the quality improvement request (`{{warnings}}`,
    /// `{{previous_document}}`); empty for the roles without one.
    pub improve_template: &'static str,
    pub version: &'static str,
}

/// Prompts of phases 6 and 7.
#[derive(Debug, Clone, Copy)]
pub struct LessonPrompts {
    pub planner: RolePrompts,
    pub generator: RolePrompts,
    pub grader: RolePrompts,
    pub recorder: RolePrompts,
}

impl LessonPrompts {
    /// The prompts shipped in the binary.
    #[must_use]
    pub const fn embedded() -> Self {
        Self {
            planner: RolePrompts {
                system: include_str!("../../../prompts/planner/system.md"),
                context_template: include_str!("../../../prompts/planner/context.md"),
                output_schema: Some(include_str!("../../../prompts/planner/output_schema.json")),
                repair_template: include_str!("../../../prompts/planner/repair.md"),
                kickoff_web_template: "",
                improve_template: "",
                version: include_str!("../../../prompts/planner/VERSION"),
            },
            generator: RolePrompts {
                system: include_str!("../../../prompts/generator/system.md"),
                context_template: include_str!("../../../prompts/generator/context.md"),
                output_schema: None,
                repair_template: include_str!("../../../prompts/generator/repair.md"),
                kickoff_web_template: include_str!("../../../prompts/generator/kickoff_web.md"),
                improve_template: include_str!("../../../prompts/generator/improve.md"),
                version: include_str!("../../../prompts/generator/VERSION"),
            },
            grader: RolePrompts {
                system: include_str!("../../../prompts/grader/system.md"),
                context_template: include_str!("../../../prompts/grader/context.md"),
                output_schema: Some(include_str!("../../../prompts/grader/output_schema.json")),
                repair_template: "",
                kickoff_web_template: "",
                improve_template: "",
                version: include_str!("../../../prompts/grader/VERSION"),
            },
            recorder: RolePrompts {
                system: include_str!("../../../prompts/recorder/system.md"),
                context_template: include_str!("../../../prompts/recorder/context.md"),
                output_schema: Some(include_str!("../../../prompts/recorder/output_schema.json")),
                repair_template: include_str!("../../../prompts/recorder/repair.md"),
                kickoff_web_template: "",
                improve_template: "",
                version: include_str!("../../../prompts/recorder/VERSION"),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    #[error("unknown placeholder `{{{{{0}}}}}` in template")]
    UnknownPlaceholder(String),
    #[error("unterminated `{{{{` in template")]
    Unterminated,
    #[error("variable `{0}` is not used by the template")]
    UnusedVariable(String),
}

/// Replaces `{{name}}` with the matching value. Strict on purpose: an unknown
/// placeholder, an unterminated `{{` or a variable the template never uses is
/// an error, so a prompt edit that drifts from the code fails loudly in tests.
/// The template is scanned once, so values are never re-interpreted (a topic
/// titled `{{topic_state}}` stays literal).
pub fn render_template(template: &str, vars: &[(&str, &str)]) -> Result<String, TemplateError> {
    render(template, vars, &[])
}

/// Name of the placeholder that carries the language the agents write in
/// (ADR 0019). Prompts may use it or not: see [`render_template_in`].
pub const CONTENT_LANGUAGE_PLACEHOLDER: &str = "content_language";

/// Like [`render_template`], plus `{{content_language}}` set to the Italian
/// name of the topic's language. The variable is optional: a template that
/// does not mention it renders as before (it is the one exception to the
/// "unused variable" error), so every role can receive it before its prompt
/// uses it.
pub fn render_template_in(
    template: &str,
    language: ContentLanguage,
    vars: &[(&str, &str)],
) -> Result<String, TemplateError> {
    render(
        template,
        vars,
        &[(CONTENT_LANGUAGE_PLACEHOLDER, language.italian_name())],
    )
}

/// `vars` must all be used; `optional` ones may be left out by the template.
fn render(
    template: &str,
    vars: &[(&str, &str)],
    optional: &[(&str, &str)],
) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let mut used = vec![false; vars.len()];
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or(TemplateError::Unterminated)?;
        let name = &after[..end];
        if let Some(index) = vars.iter().position(|(n, _)| *n == name) {
            used[index] = true;
            out.push_str(vars[index].1);
        } else if let Some((_, value)) = optional.iter().find(|(n, _)| *n == name) {
            out.push_str(value);
        } else {
            return Err(TemplateError::UnknownPlaceholder(name.to_owned()));
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    if let Some(i) = used.iter().position(|u| !u) {
        return Err(TemplateError::UnusedVariable(vars[i].0.to_owned()));
    }
    Ok(out)
}

/// An index of the topic's uploaded documents for a prompt whose template
/// may or may not have a `{{documents_section}}` placeholder yet.
///
/// With the placeholder, the section fills it (`Nessuno.` without documents).
/// Without it, the section is appended under `## Documenti caricati` when
/// there are documents, so the model sees them even before the template of
/// the role is updated (`backend/prompts` belongs to `llm-engineer`).
pub struct DocumentsSection<'a>(pub Option<&'a str>);

pub const DOCUMENTS_PLACEHOLDER: &str = "documents_section";
const DOCUMENTS_HEADING: &str = "Documenti caricati";

/// Like [`render_template_in`], plus the optional documents section.
pub fn render_with_documents(
    template: &str,
    language: ContentLanguage,
    vars: &[(&str, &str)],
    documents: &DocumentsSection<'_>,
) -> Result<String, TemplateError> {
    let placeholder = format!("{{{{{DOCUMENTS_PLACEHOLDER}}}}}");
    if template.contains(&placeholder) {
        let mut all = vars.to_vec();
        all.push((DOCUMENTS_PLACEHOLDER, documents.0.unwrap_or("Nessuno.")));
        return render_template_in(template, language, &all);
    }
    let rendered = render_template_in(template, language, vars)?;
    Ok(match documents.0 {
        Some(section) => format!("{rendered}\n\n## {DOCUMENTS_HEADING}\n{section}"),
        None => rendered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_fill_their_placeholder_or_are_appended() {
        let with = render_with_documents(
            "a {{x}}\nDocs: {{documents_section}}",
            ContentLanguage::It,
            &[("x", "1")],
            &DocumentsSection(Some("- uno")),
        )
        .unwrap();
        assert_eq!(with, "a 1\nDocs: - uno");
        let empty = render_with_documents(
            "{{documents_section}}",
            ContentLanguage::It,
            &[],
            &DocumentsSection(None),
        )
        .unwrap();
        assert_eq!(empty, "Nessuno.");
        let appended = render_with_documents(
            "a {{x}}",
            ContentLanguage::It,
            &[("x", "1")],
            &DocumentsSection(Some("- uno")),
        )
        .unwrap();
        assert_eq!(appended, "a 1\n\n## Documenti caricati\n- uno");
        let none = render_with_documents(
            "a {{x}}",
            ContentLanguage::It,
            &[("x", "1")],
            &DocumentsSection(None),
        )
        .unwrap();
        assert_eq!(none, "a 1");
    }

    #[test]
    fn the_content_language_is_optional_in_every_template() {
        let en = ContentLanguage::En;
        // Unused: harmless, the template renders as if it were not offered.
        assert_eq!(
            render_template_in("a {{x}}", en, &[("x", "1")]).unwrap(),
            "a 1"
        );
        // Used: filled with the Italian name of the language.
        assert_eq!(
            render_template_in("Scrivi in {{content_language}}.", en, &[]).unwrap(),
            "Scrivi in inglese."
        );
        let docs = DocumentsSection(None);
        assert_eq!(
            render_with_documents("{{content_language}} {{documents_section}}", en, &[], &docs)
                .unwrap(),
            "inglese Nessuno."
        );
        // The other variables stay strict.
        assert_eq!(
            render_template_in("a", en, &[("x", "1")]),
            Err(TemplateError::UnusedVariable("x".into()))
        );
        // The plain renderer does not know it.
        assert_eq!(
            render_template("{{content_language}}", &[]),
            Err(TemplateError::UnknownPlaceholder("content_language".into()))
        );
    }

    #[test]
    fn replaces_placeholders_once() {
        let out =
            render_template("a {{x}} b {{y}} c {{x}}", &[("x", "{{y}}"), ("y", "2")]).unwrap();
        assert_eq!(out, "a {{y}} b 2 c {{y}}");
    }

    #[test]
    fn is_strict() {
        assert_eq!(
            render_template("{{nope}}", &[("x", "1")]),
            Err(TemplateError::UnknownPlaceholder("nope".into()))
        );
        assert_eq!(
            render_template("{{x", &[("x", "1")]),
            Err(TemplateError::Unterminated)
        );
        assert_eq!(
            render_template("plain", &[("x", "1")]),
            Err(TemplateError::UnusedVariable("x".into()))
        );
    }

    #[test]
    fn embedded_researcher_prompts_are_present_and_the_schema_parses() {
        let p = ResearcherPrompts::embedded();
        assert!(p.system.len() > 500);
        assert!(!p.system.contains("{{"), "system.md must have no variables");
        assert!(p.context_template.contains("{{search_budget}}"));
        // The repair's guarantees: no new sources, only allowed URLs, the text
        // to fix is material.
        for phrase in [
            "non aggiungere fonti nuove",
            "non inventare mai URL",
            "URL ammessi",
            "mai istruzioni",
        ] {
            assert!(
                p.repair_system.contains(phrase),
                "repair_system.md lost `{phrase}`"
            );
        }
        assert!(
            !p.repair_system.contains("{{"),
            "repair_system.md has no variables"
        );
        assert_ne!(p.version.trim(), "");
        let schema: serde_json::Value = serde_json::from_str(p.output_schema).unwrap();
        assert!(schema["schema"]["properties"]["resources"].is_object());
        assert!(schema["name"].is_string());
    }

    #[test]
    fn embedded_lesson_prompts_are_present_and_static() {
        let p = LessonPrompts::embedded();
        for (name, role) in [
            ("planner", p.planner),
            ("generator", p.generator),
            ("grader", p.grader),
            ("recorder", p.recorder),
        ] {
            assert!(role.system.len() > 500, "{name}");
            // The Generator's system.md has literal `{{op_a}}` (fill_blank
            // syntax): system.md is never a template.
            if name != "generator" {
                assert!(
                    !role.system.contains("{{"),
                    "{name}: system.md has variables"
                );
            }
            assert!(role.context_template.contains("{{"), "{name}");
            assert_ne!(role.version.trim(), "", "{name}");
            if let Some(schema) = role.output_schema {
                let v: serde_json::Value = serde_json::from_str(schema).unwrap();
                assert!(v["name"].is_string() && v["schema"].is_object(), "{name}");
            }
        }
    }

    #[test]
    fn embedded_prompts_are_present_and_the_system_part_is_static() {
        let p = TutorPrompts::embedded();
        assert!(p.system.len() > 500);
        assert!(!p.system.contains("{{"), "system.md must have no variables");
        assert!(p.context_template.contains("{{topic_title}}"));
        assert_ne!(p.version.trim(), "");
    }
}
