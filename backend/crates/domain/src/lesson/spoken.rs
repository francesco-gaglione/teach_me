//! What a lesson sounds like: the readable blocks of a document and the text
//! the voice speaks for each (`docs/contracts/audio.md` §2).
//!
//! Decided once, here, so the clip cache key and the manifest agree on it.
//! It is a dedicated cleaner and not `scan_inline().plain`: that one keeps the
//! space before a citation, shows the glossary key instead of its term and
//! does not clean the text of a link.

use serde_json::Value;

/// A block the voice can read, with the text it speaks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadableBlock {
    pub block_id: String,
    pub text: String,
}

/// The readable blocks of a stored lesson document, in document order.
///
/// Only top-level blocks count: a `missing_component` is not readable and the
/// blocks of its `fallback` have no clip in this version.
#[must_use]
pub fn readable_blocks(document: &Value) -> Vec<ReadableBlock> {
    let glossary = Glossary(document.get("glossary"));
    let Some(blocks) = document.get("blocks").and_then(Value::as_array) else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter_map(|block| {
            let id = block.get("id").and_then(Value::as_str)?;
            let text = spoken_text(block, &glossary)?;
            (!text.is_empty()).then(|| ReadableBlock {
                block_id: id.to_owned(),
                text,
            })
        })
        .collect()
}

fn spoken_text(block: &Value, glossary: &Glossary) -> Option<String> {
    let field = |name: &str| block.get(name).and_then(Value::as_str);
    let text = match block.get("type").and_then(Value::as_str)? {
        // Plain-text fields: only the whitespace is normalised.
        "heading" => squash(field("text")?),
        "diagram" => squash(field("alt")?),
        "paragraph" => clean_inline(field("text")?, glossary),
        "callout" => {
            let text = clean_inline(field("text")?, glossary);
            // The title is plain text (`sdui.md` §9.23), not markdown.
            let title = field("title").map(squash);
            match title {
                // An empty title or text must not leave a dangling separator.
                Some(title) if !title.is_empty() && !text.is_empty() => {
                    // A title that ends a sentence needs no extra full stop.
                    let sep = if title.ends_with(['.', '!', '?', ':']) {
                        " "
                    } else {
                        ". "
                    };
                    format!("{title}{sep}{text}")
                }
                Some(title) if text.is_empty() => title,
                _ => text,
            }
        }
        _ => return None,
    };
    // The engine reads `[...]` as a style command: whatever brackets are left
    // after the markdown is gone become round (`audio.md` §2).
    Some(text.replace('[', "(").replace(']', ")"))
}

struct Glossary<'a>(Option<&'a Value>);

impl Glossary<'_> {
    fn term(&self, key: &str) -> Option<&str> {
        self.0?
            .as_array()?
            .iter()
            .find(|e| e.get("key").and_then(Value::as_str) == Some(key))?
            .get("term")?
            .as_str()
    }
}

/// Collapses runs of whitespace to one space and trims the ends.
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Removes the inline markdown of `sdui.md` §3 the way a reader would hear it.
fn clean_inline(text: &str, glossary: &Glossary) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    write_clean(&chars, glossary, &mut out);
    squash(&out)
}

fn find(chars: &[char], from: usize, pred: impl Fn(usize) -> bool) -> Option<usize> {
    (from..chars.len()).find(|&j| pred(j))
}

/// End of a link URL starting at `from` (just after the opening parenthesis):
/// the `)` that closes it, skipping balanced pairs such as `Rust_(language)`.
fn link_url_end(chars: &[char], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, &c) in chars.iter().enumerate().skip(from) {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(j),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn write_clean(chars: &[char], glossary: &Glossary, out: &mut String) {
    let at = |j: usize| chars.get(j).copied();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '`' => {
                if let Some(end) = find(chars, i + 1, |j| chars[j] == '`') {
                    out.extend(&chars[i + 1..end]);
                    i = end + 1;
                    continue;
                }
            }
            // Citation: dropped together with the space before it.
            '[' if at(i + 1) == Some('^') => {
                if let Some(end) = find(chars, i + 2, |j| chars[j] == ']')
                    && end > i + 2
                {
                    out.truncate(out.trim_end().len());
                    i = end + 1;
                    continue;
                }
            }
            '[' if at(i + 1) == Some('[') => {
                if let Some(end) = find(chars, i + 2, |j| chars[j] == ']' && at(j + 1) == Some(']'))
                {
                    let inner: String = chars[i + 2..end].iter().collect();
                    if let Some((_, visible)) = inner.split_once('|') {
                        let visible: Vec<char> = visible.chars().collect();
                        write_clean(&visible, glossary, out);
                    } else {
                        let key = inner.trim();
                        out.push_str(glossary.term(key).unwrap_or(key));
                    }
                    i = end + 2;
                    continue;
                }
            }
            '[' => {
                if let Some(close) = find(chars, i + 1, |j| chars[j] == ']')
                    && at(close + 1) == Some('(')
                    && let Some(end) = link_url_end(chars, close + 2)
                {
                    // The link text may carry emphasis of its own.
                    write_clean(&chars[i + 1..close], glossary, out);
                    i = end + 1;
                    continue;
                }
            }
            '*' => {
                i += 1;
                continue;
            }
            _ => {}
        }
        out.push(c);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn doc(blocks: &Value) -> Value {
        json!({
            "glossary": [
                {"key": "ownership", "term": "ownership", "definition": "d"},
                {"key": "borrow", "term": "borrow checker", "definition": "d"}
            ],
            "blocks": blocks
        })
    }

    fn texts(blocks: &Value) -> Vec<(String, String)> {
        readable_blocks(&doc(blocks))
            .into_iter()
            .map(|b| (b.block_id, b.text))
            .collect()
    }

    fn one(block: &Value) -> Option<String> {
        texts(&json!([block])).pop().map(|(_, t)| t)
    }

    fn para(text: &str) -> Option<String> {
        one(&json!({"type": "paragraph", "id": "p", "text": text}))
    }

    #[test]
    fn heading_is_its_text() {
        assert_eq!(
            one(&json!({"type": "heading", "id": "h", "level": 1, "text": "Ownership  e move"})),
            Some("Ownership e move".into())
        );
    }

    #[test]
    fn diagram_is_its_alt() {
        let block = json!({"type": "diagram", "id": "d", "alt": " Tre riquadri in fila. ",
            "caption": "non letta", "title": "non letto"});
        assert_eq!(one(&block), Some("Tre riquadri in fila.".into()));
    }

    #[test]
    fn callout_joins_title_and_text() {
        let block = json!({"type": "callout", "id": "c", "variant": "tip",
            "title": "Attenzione", "text": "Il **valore** si sposta."});
        assert_eq!(one(&block), Some("Attenzione. Il valore si sposta.".into()));
    }

    #[test]
    fn callout_title_that_ends_a_sentence_gets_no_extra_full_stop() {
        for (title, spoken) in [
            ("Attenzione.", "Attenzione. Testo."),
            ("Davvero?", "Davvero? Testo."),
            ("Nota!", "Nota! Testo."),
            ("Nota:", "Nota: Testo."),
            ("  Nota   bene ", "Nota bene. Testo."),
        ] {
            let block = json!({"type": "callout", "id": "c", "variant": "tip",
                "title": title, "text": "Testo."});
            assert_eq!(one(&block), Some(spoken.into()), "{title}");
        }
    }

    #[test]
    fn callout_title_is_plain_text() {
        let block = json!({"type": "callout", "id": "c", "variant": "tip",
            "title": "A *b*", "text": "Testo"});
        assert_eq!(one(&block), Some("A *b*. Testo".into()));
    }

    #[test]
    fn remaining_square_brackets_become_round() {
        assert_eq!(
            para("Vedi [nota] e `v[0]` ma [link](u) e [^k] no"),
            Some("Vedi (nota) e v(0) ma link e no".into())
        );
        assert_eq!(
            one(&json!({"type": "heading", "id": "h", "level": 1, "text": "Array [T]"})),
            Some("Array (T)".into())
        );
        assert_eq!(
            one(&json!({"type": "diagram", "id": "d", "alt": "Cella [i]"})),
            Some("Cella (i)".into())
        );
    }

    #[test]
    fn callout_without_title_is_the_text_only() {
        let block = json!({"type": "callout", "id": "c", "variant": "info", "text": "Solo testo."});
        assert_eq!(one(&block), Some("Solo testo.".into()));
    }

    #[test]
    fn emphasis_and_code_lose_their_markup() {
        assert_eq!(
            para("**forte** *corsivo* `Vec<T>*2`"),
            Some("forte corsivo Vec<T>*2".into())
        );
    }

    #[test]
    fn a_link_url_with_balanced_parentheses_is_dropped_whole() {
        assert_eq!(
            para("Vedi [Rust](https://en.wikipedia.org/wiki/Rust_(language)) ora"),
            Some("Vedi Rust ora".into())
        );
    }

    #[test]
    fn links_keep_only_their_text_even_with_emphasis() {
        assert_eq!(
            para("Vedi [la **guida**](https://example.com/x) ora"),
            Some("Vedi la guida ora".into())
        );
    }

    #[test]
    fn a_citation_goes_away_with_the_space_before_it() {
        assert_eq!(
            para("Ogni valore ha un owner [^book] [^web]. Fine"),
            Some("Ogni valore ha un owner. Fine".into())
        );
        assert_eq!(para("[^a]Testo"), Some("Testo".into()));
    }

    #[test]
    fn glossary_references_read_the_term_or_the_visible_text() {
        assert_eq!(
            para("Il [[borrow]] e [[ownership|la proprietà]] e [[ownership]]."),
            Some("Il borrow checker e la proprietà e ownership.".into())
        );
    }

    #[test]
    fn the_visible_text_of_a_glossary_reference_is_cleaned_too() {
        assert_eq!(
            para("Vedi [[ownership|lo *spostamento*]] qui"),
            Some("Vedi lo spostamento qui".into())
        );
    }

    #[test]
    fn an_unknown_glossary_key_reads_as_the_key() {
        assert_eq!(para("Vedi [[ghost]]"), Some("Vedi ghost".into()));
    }

    #[test]
    fn whitespace_is_squashed_and_trimmed() {
        assert_eq!(para("  uno   due \t tre  "), Some("uno due tre".into()));
    }

    #[test]
    fn an_empty_result_is_not_readable() {
        assert_eq!(para("[^a]"), None);
        assert_eq!(para("  ** **  "), None);
        assert_eq!(
            one(&json!({"type": "heading", "id": "h", "level": 2, "text": "   "})),
            None
        );
    }

    #[test]
    fn other_types_are_not_readable() {
        let blocks = json!([
            {"type": "code", "id": "k", "language": "rust", "code": "fn main() {}"},
            {"type": "quiz_single", "id": "q", "question": "Quale?"},
            {"type": "missing_component", "id": "m", "fallback": [
                {"type": "paragraph", "id": "inner", "text": "No"}
            ]},
            {"type": "paragraph", "id": "p", "text": "Sì"}
        ]);
        assert_eq!(texts(&blocks), [("p".to_owned(), "Sì".to_owned())]);
    }

    #[test]
    fn blocks_come_in_document_order() {
        let blocks = json!([
            {"type": "heading", "id": "h1", "level": 1, "text": "Titolo"},
            {"type": "paragraph", "id": "p1", "text": "Uno"},
            {"type": "diagram", "id": "d1", "alt": "Figura"},
            {"type": "callout", "id": "c1", "variant": "info", "text": "Due"}
        ]);
        let ids: Vec<_> = texts(&blocks).into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, ["h1", "p1", "d1", "c1"]);
    }

    #[test]
    fn a_document_without_blocks_reads_nothing() {
        assert_eq!(readable_blocks(&json!({})), []);
        assert_eq!(readable_blocks(&json!({"blocks": 3})), []);
    }
}
