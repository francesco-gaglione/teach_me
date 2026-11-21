//! Scanner for the inline markdown subset of the contract (§3).
//!
//! It does not render anything: it only extracts citations (`[^key]`) and
//! glossary references (`[[key]]`, `[[key|text]]`), builds a plain-text
//! version for word counting and flags markdown the contract forbids.

use super::model::show;

/// Markdown that is not allowed in an inline field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownProblem {
    Newline,
    Heading,
    ListItem,
    Image,
    Html,
    NonHttpsLink(String),
    /// `[[key]]` where cross references are not allowed (glossary definitions).
    GlossaryRef,
}

impl MarkdownProblem {
    /// Italian description used in validator messages.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Newline => "contiene a capo (usa blocchi separati per la struttura)".into(),
            Self::Heading => "contiene un'intestazione markdown (usa un blocco heading)".into(),
            Self::ListItem => "contiene un elenco (usa un blocco checklist o più paragrafi)".into(),
            Self::Image => "contiene un'immagine (non ammessa)".into(),
            Self::Html => "contiene HTML (non ammesso)".into(),
            Self::GlossaryRef => {
                "contiene un rimando [[...]] (non ammesso nelle definizioni)".into()
            }
            Self::NonHttpsLink(url) => {
                format!("contiene il link '{}' che non usa https", show(url))
            }
        }
    }
}

/// Result of scanning one inline field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineScan {
    pub citations: Vec<String>,
    pub glossary_refs: Vec<String>,
    pub problems: Vec<MarkdownProblem>,
    /// Inline code spans too long to read on a phone (contract §6,
    /// `inline_code_too_long`), as `(characters, semicolons)`.
    pub long_code_spans: Vec<(usize, usize)>,
    /// Text with markup removed; what the reader actually sees.
    pub plain: String,
}

/// Longest inline code span allowed, in characters.
pub const MAX_INLINE_CODE_CHARS: usize = 60;
/// A span with this many `;` is a program, not a name or an expression.
pub const MAX_INLINE_CODE_SEMICOLONS: usize = 1;

/// Counts words: whitespace-separated tokens (contract §9.4, exact).
#[must_use]
pub fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Ids of the `{{blank_id}}` placeholders in `text`, in order of appearance.
#[must_use]
pub fn placeholders(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            break;
        };
        out.push(after[..end].trim().to_owned());
        rest = &after[end + 2..];
    }
    out
}

/// Scans an inline markdown field.
#[must_use]
pub fn scan_inline(text: &str) -> InlineScan {
    let mut scan = InlineScan::default();
    if text.contains(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}']) {
        scan.problems.push(MarkdownProblem::Newline);
    }
    check_line_start(text, &mut scan);

    let chars: Vec<char> = text.chars().collect();
    let look = Lookups::new(&chars);
    let mut i = 0;
    while i < chars.len() {
        i = step(&chars, &look, i, &mut scan);
    }
    scan
}

fn check_line_start(text: &str, scan: &mut InlineScan) {
    let t = text.trim_start();
    let hashes = t.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
        scan.problems.push(MarkdownProblem::Heading);
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    let numbered = digits > 0 && (t[digits..].starts_with(". ") || t[digits..].starts_with(") "));
    let bulleted = ["- ", "+ ", "* ", "> "].iter().any(|p| t.starts_with(p));
    if numbered || bulleted {
        scan.problems.push(MarkdownProblem::ListItem);
    }
}

/// `table[i]` = smallest `j >= i` where `pred(j)` holds, or `chars.len()`.
/// Precomputed once so every lookup in the scan is O(1): searching on demand
/// made adversarial input like `"[ " * 50000` quadratic.
fn next_table(chars: &[char], pred: impl Fn(usize) -> bool) -> Vec<usize> {
    let n = chars.len();
    let mut table = vec![n; n + 2];
    for j in (0..n).rev() {
        table[j] = if pred(j) { j } else { table[j + 1] };
    }
    table
}

struct Lookups {
    bracket: Vec<usize>,
    double_bracket: Vec<usize>,
    paren: Vec<usize>,
    tick: Vec<usize>,
    gt: Vec<usize>,
}

impl Lookups {
    fn new(chars: &[char]) -> Self {
        let at = |j: usize| chars.get(j).copied();
        Self {
            bracket: next_table(chars, |j| at(j) == Some(']')),
            double_bracket: next_table(chars, |j| at(j) == Some(']') && at(j + 1) == Some(']')),
            paren: next_table(chars, |j| at(j) == Some(')')),
            tick: next_table(chars, |j| at(j) == Some('`')),
            gt: next_table(chars, |j| at(j) == Some('>')),
        }
    }
}

/// Position in `table` at or after `from`, if any.
fn lookup(table: &[usize], n: usize, from: usize) -> Option<usize> {
    table.get(from).copied().filter(|&j| j < n)
}

/// Handles the construct starting at `i` and returns the next index.
fn step(chars: &[char], look: &Lookups, i: usize, scan: &mut InlineScan) -> usize {
    let n = chars.len();
    let c = chars[i];
    let next = chars.get(i + 1).copied();
    match c {
        '`' => {
            if let Some(end) = lookup(&look.tick, n, i + 1) {
                let body = &chars[i + 1..end];
                let semicolons = body.iter().filter(|&&c| c == ';').count();
                if body.len() > MAX_INLINE_CODE_CHARS || semicolons > MAX_INLINE_CODE_SEMICOLONS {
                    scan.long_code_spans.push((body.len(), semicolons));
                }
                scan.plain.extend(&chars[i + 1..end]);
                return end + 1;
            }
        }
        '!' if next == Some('[') => scan.problems.push(MarkdownProblem::Image),
        '[' if next == Some('^') => {
            if let Some(end) = lookup(&look.bracket, n, i + 2) {
                let key: String = chars[i + 2..end].iter().collect();
                if !key.is_empty() {
                    scan.citations.push(key);
                    return end + 1;
                }
            }
        }
        '[' if next == Some('[') => {
            if let Some(end) = lookup(&look.double_bracket, n, i + 2) {
                let inner: String = chars[i + 2..end].iter().collect();
                let (key, visible) = inner.split_once('|').unwrap_or((&inner, &inner));
                scan.glossary_refs.push(key.trim().to_owned());
                scan.plain.push_str(visible);
                return end + 2;
            }
        }
        '[' => {
            if let Some(close) = lookup(&look.bracket, n, i + 1)
                && chars.get(close + 1) == Some(&'(')
                && let Some(end) = lookup(&look.paren, n, close + 2)
            {
                let url: String = chars[close + 2..end].iter().collect();
                if !url.starts_with("https://") {
                    scan.problems.push(MarkdownProblem::NonHttpsLink(url));
                }
                scan.plain.extend(&chars[i + 1..close]);
                return end + 1;
            }
        }
        '<' if next.is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!')
            && lookup(&look.gt, n, i + 1).is_some() =>
        {
            if !scan.problems.contains(&MarkdownProblem::Html) {
                scan.problems.push(MarkdownProblem::Html);
            }
        }
        '*' => return i + 1,
        _ => {}
    }
    scan.plain.push(c);
    i + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_citations_and_glossary_refs() {
        let s = scan_inline("Vedi [[ownership]] e [[move|lo spostamento]] [^book].");
        assert_eq!(s.citations, ["book"]);
        assert_eq!(s.glossary_refs, ["ownership", "move"]);
        assert_eq!(s.plain, "Vedi ownership e lo spostamento .");
        assert_eq!(s.problems, []);
    }

    #[test]
    fn allows_https_links_and_inline_formatting() {
        let s = scan_inline("**forte** *corsivo* `Vec<T>` [doc](https://example.com)");
        assert!(s.problems.is_empty(), "{:?}", s.problems);
        assert_eq!(s.plain, "forte corsivo Vec<T> doc");
    }

    #[test]
    fn flags_forbidden_markdown() {
        let p = |t: &str| scan_inline(t).problems;
        assert_eq!(p("a\nb"), [MarkdownProblem::Newline]);
        assert_eq!(p("# Titolo"), [MarkdownProblem::Heading]);
        assert_eq!(p("- voce"), [MarkdownProblem::ListItem]);
        assert_eq!(p("1. voce"), [MarkdownProblem::ListItem]);
        assert!(p("![alt](https://x.y/i.png)").contains(&MarkdownProblem::Image));
        assert_eq!(p("<b>x</b>"), [MarkdownProblem::Html]);
        assert_eq!(
            p("[x](http://a.b)"),
            [MarkdownProblem::NonHttpsLink("http://a.b".into())]
        );
        assert_eq!(
            p("[x](javascript:alert(1))"),
            [MarkdownProblem::NonHttpsLink("javascript:alert(1".into())]
        );
    }

    #[test]
    fn flags_long_or_multi_statement_code_spans() {
        let spans = |t: &str| scan_inline(t).long_code_spans;
        let sixty = "a".repeat(60);
        assert_eq!(spans(&format!("`{sixty}`")), []);
        assert_eq!(spans(&format!("`{}`", "a".repeat(61))), [(61, 0)]);
        assert_eq!(spans("`let a = 1;`"), []);
        assert_eq!(spans("`a; b`"), []);
        assert_eq!(spans("`a; b;`"), [(5, 2)]);
        assert_eq!(spans("`a;b;c` e `ok`"), [(5, 2)]);
        assert_eq!(spans("senza codice; ne` altro; ;"), []);
    }

    #[test]
    fn plain_less_than_is_not_html() {
        assert_eq!(scan_inline("se a < b allora c<d").problems, []);
        assert_eq!(scan_inline("*corsivo* inizio").problems, []);
    }

    #[test]
    fn words_are_whitespace_separated_tokens() {
        assert_eq!(count_words("uno - due, tre"), 4);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn placeholders_are_extracted_in_order() {
        assert_eq!(placeholders("a {{x}} b {{ y }} {{z"), ["x", "y"]);
    }
}
