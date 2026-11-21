//! The Researcher's answer: extracting the JSON from the model's text and the
//! Validator that decides whether it can be stored (`docs/contracts/resources.md`
//! §1, `llm-port.md` §12.3). The guarantees live here, not in the prompt.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use domain::resource::{
    ANNOTATION_MAX_CHARS, GAP_AREA_MAX_CHARS, GAP_NOTE_MAX_CHARS, LANGUAGE_MAX_CHARS, MAX_GAPS,
    MAX_RESOURCES, TITLE_MAX_CHARS, USE_FOR_MAX_CHARS,
};
use domain::{Gap, ResourceDraft, ResourceKind, SourceType, normalize_url};
use serde_json::Value;

/// Most problems reported back to the model for one repair: enough to fix a
/// list, short enough not to bury it.
const MAX_REPORTED_ERRORS: usize = 20;

/// The URLs the model has legitimately seen: results of this job's web
/// searches and the topic's existing sources. Compared after normalization;
/// the stored URL is the original spelling.
#[derive(Debug, Default, Clone)]
pub struct SeenUrls {
    /// Normalized -> first spelling seen.
    by_key: HashMap<String, String>,
    /// First spellings, in order of arrival (for the repair prompt).
    ordered: Vec<String>,
}

impl SeenUrls {
    pub fn add(&mut self, url: &str) {
        let Some(key) = normalize_url(url) else {
            return;
        };
        if let Entry::Vacant(slot) = self.by_key.entry(key) {
            slot.insert(url.trim().to_owned());
            self.ordered.push(url.trim().to_owned());
        }
    }

    fn original(&self, url: &str) -> Option<&str> {
        self.by_key.get(&normalize_url(url)?).map(String::as_str)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    #[must_use]
    pub fn urls(&self) -> &[String] {
        &self.ordered
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidOutput {
    pub resources: Vec<ResourceDraft>,
    pub gaps: Vec<Gap>,
    pub communities_skipped: bool,
    /// Things worth a log line that do not block the save.
    pub warnings: Vec<String>,
}

/// Parses and validates the model's final text. `Err` lists every problem
/// found (in Italian, to be handed back to the model for a repair).
pub fn parse_and_validate(text: &str, seen: &SeenUrls) -> Result<ValidOutput, Vec<String>> {
    let Some(json) = crate::structured::extract_json(text) else {
        return Err(vec![
            "Nella risposta non c'è nessun oggetto JSON.".to_owned(),
        ]);
    };
    let value: Value =
        serde_json::from_str(json).map_err(|e| vec![format!("Il JSON non è valido ({e}).")])?;
    validate(&value, seen)
}

fn validate(root: &Value, seen: &SeenUrls) -> Result<ValidOutput, Vec<String>> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let Some(object) = root.as_object() else {
        return Err(vec![
            "La radice del JSON deve essere un oggetto.".to_owned(),
        ]);
    };

    let communities_skipped = if let Some(Value::Bool(b)) = object.get("communities_skipped") {
        *b
    } else {
        errors.push("`communities_skipped` deve essere true o false.".to_owned());
        false
    };
    let raw_resources = array(object.get("resources"), "resources", &mut errors);
    let raw_gaps = array(object.get("gaps"), "gaps", &mut errors);

    if raw_resources.len() > MAX_RESOURCES {
        errors.push(format!(
            "Ci sono {} risorse: al massimo {MAX_RESOURCES}. Tieni le migliori.",
            raw_resources.len()
        ));
    }
    if raw_gaps.len() > MAX_GAPS {
        errors.push(format!(
            "Ci sono {} lacune: al massimo {MAX_GAPS}.",
            raw_gaps.len()
        ));
    }

    let mut resources = Vec::new();
    let mut used_urls = HashSet::new();
    for (i, raw) in raw_resources.iter().enumerate() {
        let at = format!("resources[{i}]");
        let Some(draft) = resource(raw, &at, seen, &mut errors, &mut warnings) else {
            continue;
        };
        if let Some(key) = draft.url.as_deref().and_then(normalize_url)
            && !used_urls.insert(key)
        {
            errors.push(format!("{at}.url: l'URL compare più di una volta."));
            continue;
        }
        if communities_skipped && draft.kind == ResourceKind::Community {
            // The preference to skip communities is the user's, not a style
            // choice: the entry is dropped instead of sent back for a repair.
            warnings.push(format!(
                "{at}: community scartata perché communities_skipped è true"
            ));
            continue;
        }
        resources.push(draft);
    }

    let mut gaps = Vec::new();
    for (i, raw) in raw_gaps.iter().enumerate() {
        let at = format!("gaps[{i}]");
        let area = text_field(raw, "area", GAP_AREA_MAX_CHARS, &at, &mut errors);
        let note = text_field(raw, "note", GAP_NOTE_MAX_CHARS, &at, &mut errors);
        if let (Some(area), Some(note)) = (area, note) {
            gaps.push(Gap { area, note });
        }
    }

    if errors.is_empty() && resources.is_empty() && gaps.is_empty() {
        errors.push(
            "Non c'è nessuna risorsa e nessuna lacuna: se non hai trovato fonti affidabili, \
             spiega nelle lacune cosa manca."
                .to_owned(),
        );
    }
    if errors.is_empty() {
        Ok(ValidOutput {
            resources,
            gaps,
            communities_skipped,
            warnings,
        })
    } else {
        errors.truncate(MAX_REPORTED_ERRORS);
        Err(errors)
    }
}

fn array<'a>(value: Option<&'a Value>, name: &str, errors: &mut Vec<String>) -> &'a [Value] {
    if let Some(Value::Array(items)) = value {
        items
    } else {
        errors.push(format!("`{name}` deve essere un array."));
        &[]
    }
}

/// A trimmed, non-empty string field within `max` characters.
fn text_field(
    raw: &Value,
    name: &str,
    max: usize,
    at: &str,
    errors: &mut Vec<String>,
) -> Option<String> {
    let Some(text) = raw.get(name).and_then(Value::as_str) else {
        errors.push(format!("{at}.{name}: manca o non è una stringa."));
        return None;
    };
    let text = text.trim();
    let length = text.chars().count();
    if length == 0 {
        errors.push(format!("{at}.{name}: è vuoto."));
        None
    } else if length > max {
        errors.push(format!(
            "{at}.{name}: {length} caratteri, al massimo {max}. Accorcia il testo."
        ));
        None
    } else {
        Some(text.to_owned())
    }
}

fn resource(
    raw: &Value,
    at: &str,
    seen: &SeenUrls,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Option<ResourceDraft> {
    if !raw.is_object() {
        errors.push(format!("{at}: deve essere un oggetto."));
        return None;
    }
    let before = errors.len();
    let kind = raw["kind"]
        .as_str()
        .and_then(|k| k.parse::<ResourceKind>().ok())
        // Uploaded PDFs are not the model's to create.
        .filter(|k| *k != ResourceKind::Document);
    if kind.is_none() {
        errors.push(format!("{at}.kind: deve essere `knowledge` o `community`."));
    }
    let source_type = raw["source_type"]
        .as_str()
        .and_then(|k| k.parse::<SourceType>().ok())
        .filter(|t| *t != SourceType::Pdf);
    if source_type.is_none() {
        errors.push(format!("{at}.source_type: valore non ammesso."));
    }
    let title = text_field(raw, "title", TITLE_MAX_CHARS, at, errors);
    let annotation = text_field(raw, "annotation", ANNOTATION_MAX_CHARS, at, errors);
    let use_for = text_field(raw, "use_for", USE_FOR_MAX_CHARS, at, errors);
    let language = text_field(raw, "language", LANGUAGE_MAX_CHARS, at, errors);
    if let Some(language) = &language
        && !language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        errors.push(format!(
            "{at}.language: deve essere un codice di lingua come `it` o `en`."
        ));
    }

    if let (Some(kind), Some(source_type)) = (kind, source_type) {
        let incoherent = match kind {
            ResourceKind::Knowledge => source_type.community_only(),
            ResourceKind::Community => source_type.knowledge_only(),
            ResourceKind::Document => true,
        };
        if incoherent {
            errors.push(format!(
                "{at}: il tipo `{source_type}` non è coerente con il gruppo `{kind}`."
            ));
        }
    }

    let url = match raw.get("url") {
        Some(Value::String(url)) => match seen.original(url) {
            Some(original) => Some(original.to_owned()),
            None if normalize_url(url).is_none() => {
                errors.push(format!("{at}.url: non è un URL https valido."));
                None
            }
            None => {
                errors.push(format!(
                    "{at}.url: non compare tra i risultati di ricerca. Usa solo URL visti nelle ricerche, esattamente come sono."
                ));
                None
            }
        },
        Some(Value::Null) => {
            let allowed = kind == Some(ResourceKind::Community)
                && source_type.is_some_and(SourceType::may_lack_url);
            if !allowed {
                errors.push(format!(
                    "{at}.url: può mancare solo per una community fisica o senza sito."
                ));
            }
            None
        }
        _ => {
            errors.push(format!("{at}.url: deve essere una stringa https o null."));
            None
        }
    };

    if let Some(use_for) = &use_for
        && use_for.to_lowercase().starts_with("usala per")
    {
        warnings.push(format!("{at}.use_for ripete «Usala per»"));
    }

    if errors.len() > before {
        return None;
    }
    Some(ResourceDraft {
        kind: kind?,
        source_type: source_type?,
        title: title?,
        url,
        language: language?,
        annotation: annotation?,
        use_for: use_for?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn seen() -> SeenUrls {
        let mut s = SeenUrls::default();
        s.add("https://doc.rust-lang.org/book/");
        s.add("https://forum.example.org/");
        s
    }

    fn knowledge(url: &str) -> Value {
        json!({
            "kind": "knowledge", "source_type": "documentation", "title": "The Book",
            "url": url, "language": "en",
            "annotation": "Il libro ufficiale.", "use_for": "capire ownership"
        })
    }

    fn output(resources: &[Value], gaps: &[Value], skipped: bool) -> String {
        json!({ "resources": resources, "gaps": gaps, "communities_skipped": skipped }).to_string()
    }

    fn errors(text: &str) -> Vec<String> {
        parse_and_validate(text, &seen()).unwrap_err()
    }

    #[test]
    fn accepts_a_good_answer_in_a_fence_and_keeps_the_seen_spelling_of_the_url() {
        let text = format!(
            "Ecco:\n```json\n{}\n```",
            output(
                &[knowledge("https://DOC.rust-lang.org/book#intro")],
                &[json!({ "area": "Migrazioni", "note": "Niente di recente." })],
                false
            )
        );
        let ok = parse_and_validate(&text, &seen()).unwrap();
        assert_eq!(ok.resources.len(), 1);
        assert_eq!(
            ok.resources[0].url.as_deref(),
            Some("https://doc.rust-lang.org/book/")
        );
        assert_eq!(ok.gaps.len(), 1);
        assert!(!ok.communities_skipped);
    }

    #[test]
    fn a_url_that_was_never_seen_is_rejected() {
        let e = errors(&output(
            &[knowledge("https://invented.example.org/x")],
            &[],
            false,
        ));
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("resources[0].url") && e[0].contains("risultati di ricerca"));
    }

    #[test]
    fn urls_must_be_https_and_unique() {
        let e = errors(&output(
            &[knowledge("http://doc.rust-lang.org/book/")],
            &[],
            false,
        ));
        assert!(e[0].contains("URL https valido"));
        let e = errors(&output(
            &[
                knowledge("https://doc.rust-lang.org/book/"),
                knowledge("https://doc.rust-lang.org/book"),
            ],
            &[],
            false,
        ));
        assert!(e[0].contains("resources[1].url") && e[0].contains("più di una volta"));
    }

    #[test]
    fn a_null_url_is_only_for_communities_without_a_site() {
        let place = json!({
            "kind": "community", "source_type": "local_group", "title": "Rust Roma",
            "url": null, "language": "it", "annotation": "Gruppo locale.", "use_for": "confrontarsi"
        });
        assert!(
            parse_and_validate(&output(std::slice::from_ref(&place), &[], false), &seen()).is_ok()
        );
        let mut forum = place.clone();
        forum["source_type"] = json!("forum");
        assert!(errors(&output(&[forum], &[], false))[0].contains("url"));
        let mut book = knowledge("x");
        book["url"] = Value::Null;
        assert!(errors(&output(&[book], &[], false))[0].contains("url"));
    }

    #[test]
    fn kind_and_source_type_must_agree() {
        let mut r = knowledge("https://doc.rust-lang.org/book/");
        r["source_type"] = json!("forum");
        assert!(errors(&output(&[r], &[], false))[0].contains("non è coerente"));
        let mut c = knowledge("https://forum.example.org/");
        c["kind"] = json!("community");
        assert!(errors(&output(&[c], &[], false))[0].contains("non è coerente"));
    }

    #[test]
    fn lengths_are_errors_not_truncations() {
        let mut r = knowledge("https://doc.rust-lang.org/book/");
        r["annotation"] = json!("a".repeat(601));
        r["title"] = json!("");
        let e = errors(&output(&[r], &[], false));
        assert_eq!(e.len(), 2, "{e:?}");
        let gap = json!({ "area": "x".repeat(201), "note": "ok" });
        assert!(errors(&output(&[], &[gap], false))[0].contains("gaps[0].area"));
        // The limit counts characters, not bytes.
        let mut ok = knowledge("https://doc.rust-lang.org/book/");
        ok["annotation"] = json!("è".repeat(600));
        assert!(parse_and_validate(&output(&[ok], &[], false), &seen()).is_ok());
    }

    #[test]
    fn communities_are_dropped_when_skipped() {
        let community = json!({
            "kind": "community", "source_type": "forum", "title": "Forum",
            "url": "https://forum.example.org/", "language": "en",
            "annotation": "Moderato.", "use_for": "domande pratiche"
        });
        let text = output(
            &[knowledge("https://doc.rust-lang.org/book/"), community],
            &[],
            true,
        );
        let ok = parse_and_validate(&text, &seen()).unwrap();
        assert_eq!(ok.resources.len(), 1);
        assert!(ok.communities_skipped);
        assert_eq!(ok.warnings.len(), 1);
    }

    #[test]
    fn an_honest_empty_list_needs_its_gaps() {
        let gap = json!({ "area": "Tutto", "note": "Nessuna fonte affidabile." });
        let ok = parse_and_validate(&output(&[], &[gap], false), &seen()).unwrap();
        assert_eq!(ok.resources.len(), 0);
        assert!(errors(&output(&[], &[], false))[0].contains("nessuna risorsa"));
    }

    #[test]
    fn too_many_entries_and_bad_shapes_are_reported() {
        let many: Vec<Value> = (0..13)
            .map(|i| knowledge(&format!("https://doc.rust-lang.org/book/{i}")))
            .collect();
        assert!(errors(&output(&many, &[], false))[0].contains("al massimo 12"));
        assert_eq!(errors("nessun json").len(), 1);
        assert!(errors("{\"resources\": 3}").len() >= 2);
        assert_eq!(errors("[1]").len(), 1);
        assert!(errors("{ non json }")[0].starts_with("Il JSON non è valido"));
    }

    #[test]
    fn existing_urls_count_as_seen() {
        let mut s = SeenUrls::default();
        s.add("https://old.example.org/guide");
        assert_eq!(s.urls(), ["https://old.example.org/guide"]);
        let text = output(&[knowledge("https://old.example.org/guide/")], &[], false);
        assert!(parse_and_validate(&text, &s).is_ok());
    }
}
