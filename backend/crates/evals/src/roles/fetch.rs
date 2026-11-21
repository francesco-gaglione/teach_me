//! What the Generator's web fetch tool did (ADR 0009, `llm-port.md` §14):
//! the checks of `fetch_checks`, the metrics of `report_metrics`, and the
//! text of the pages the judge is shown. Everything is read from the calls the
//! runner recorded (requests and the provider blocks of the answers), so it
//! measures what really went over the wire, not what the code meant to send.

use std::collections::HashMap;
use std::fmt::Write as _;

use domain::llm::Part;
use regex::Regex;
use serde_json::{Map, Value, json};

use crate::checks::{Check, Want, range, strict_object_problem, value_problem as top};
use crate::llm::{CallRecord, InjectedPage};

/// The label of the Generator's repair calls.
const REPAIR_LABEL: &str = "generator.repair";
/// What the user message of a Generator call without the tool says: the
/// fixed kickoff, and no list of sources to read.
const KICKOFF: &str = "Procedi.";
const SOURCES_LIST_MARKER: &str = "Fonti da leggere";

/// Longest text of one page shown to the judge: all the Generator can have
/// seen (`FETCH_MAX_CONTENT_TOKENS`, 8000 tokens, ~4 characters each) with a
/// margin. At 12000 the judge missed the end of the Rust book pages and called
/// claims taken from there unsupported (issue #26, 2026-11-19).
pub const JUDGE_PAGE_CHARS: usize = 40_000;
/// Longest text of all the pages shown to the judge together: three full
/// pages would cost ~30k tokens a judgement.
pub const JUDGE_PAGES_TOTAL_CHARS: usize = 80_000;

/// Keys of `fetch_checks` (or of `deterministic`, where they may be moved).
pub const KEYS: &[&str] = &[
    "web_fetch_requests",
    "fetch_errors",
    "fetch_urls_only_from_sources",
    "no_refetch_in_repair",
    "must_match_min",
    "fixture_page_served",
    "citations_in_fetched_pages",
    "user_message_has_no_sources_list",
];

/// Names `report_metrics` may list.
pub const METRICS: &[&str] = &[
    "cost_usd",
    "input_tokens",
    "output_tokens",
    "web_fetch_requests",
    "fetched_tokens",
    "model_calls",
    "documents",
    "unsupported_citations",
];

/// Why the value of a fetch check cannot be read, or `None`.
#[must_use]
pub fn value_problem(key: &str, v: &Value) -> Option<String> {
    match key {
        "web_fetch_requests" | "fetch_errors" => top(key, v, Want::Range),
        "fetch_urls_only_from_sources"
        | "no_refetch_in_repair"
        | "user_message_has_no_sources_list"
        | "fixture_page_served" => top(key, v, Want::Bool),
        "citations_in_fetched_pages" => {
            strict_object_problem(key, v, &[("max_unsupported", Want::UInt, true)])
        }
        "must_match_min" => strict_object_problem(
            key,
            v,
            &[("min", Want::UInt, true), ("patterns", Want::StrList, true)],
        )
        .or_else(|| {
            v["patterns"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find_map(|p| {
                    Regex::new(p)
                        .err()
                        .map(|e| format!("`{key}.patterns`: invalid regex `{p}`: {e}"))
                })
        }),
        _ => None,
    }
}

/// Why a `report_metrics` value cannot be read, or `None`.
#[must_use]
pub fn metrics_problem(v: &Value) -> Option<String> {
    let Some(list) = v.as_array() else {
        return Some(format!("must be a list of metric names, found {v}"));
    };
    list.iter().find_map(|m| match m.as_str() {
        Some(name) if METRICS.contains(&name) => None,
        _ => Some(format!(
            "unknown metric {m}: known are {}",
            METRICS.join(", ")
        )),
    })
}

/// One page the provider read (or failed to read) for the Generator.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub url: String,
    /// The text, when the page came back as text (a remote PDF does not).
    pub text: Option<String>,
    /// The provider's error code when the read failed.
    pub error: Option<String>,
}

/// Every read of the calls, in order, from the blocks of their answers:
/// `server_tool_use` names the address, `web_fetch_tool_result` carries the
/// text or an error code.
#[must_use]
pub fn pages(calls: &[CallRecord]) -> Vec<Page> {
    let mut urls: HashMap<String, String> = HashMap::new();
    let mut out = Vec::new();
    for block in calls.iter().flat_map(|c| &c.blocks) {
        match block["type"].as_str() {
            Some("server_tool_use") if block["name"] == "web_fetch" => {
                if let (Some(id), Some(url)) = (
                    block["id"].as_str(),
                    domain::llm::input_url(&block["input"]),
                ) {
                    urls.insert(id.to_owned(), url);
                }
            }
            Some("web_fetch_tool_result") => {
                let known = block["tool_use_id"]
                    .as_str()
                    .and_then(|id| urls.get(id))
                    .cloned();
                let content = &block["content"];
                if content["type"] == "web_fetch_tool_result_error" {
                    out.push(Page {
                        url: known.unwrap_or_else(|| NO_URL.to_owned()),
                        text: None,
                        error: Some(
                            content["error_code"]
                                .as_str()
                                .unwrap_or("unknown")
                                .to_owned(),
                        ),
                    });
                } else {
                    let source = &content["content"]["source"];
                    out.push(Page {
                        url: content["url"]
                            .as_str()
                            .map(str::to_owned)
                            .or(known)
                            .unwrap_or_else(|| NO_URL.to_owned()),
                        text: (source["type"] == "text")
                            .then(|| source["data"].as_str().map(str::to_owned))
                            .flatten(),
                        error: None,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// What the check on the sources found for one read: the addresses read
/// (from the request block or, failing that, from the result) and how many
/// reads have no derivable address.
struct Requested {
    urls: Vec<String>,
    unknown: usize,
}

fn requested_urls(calls: &[CallRecord]) -> Requested {
    let mut out = Requested {
        urls: Vec::new(),
        unknown: 0,
    };
    for read in calls
        .iter()
        .flat_map(|c| domain::llm::web_fetch_reads(&c.blocks))
    {
        match read.url {
            Some(u) => out.urls.push(u),
            None => out.unknown += 1,
        }
    }
    out
}

/// What a page's address is when no block carries one.
const NO_URL: &str = "(URL non ricavabile dal blocco)";

/// Longest `input` kept in the diagnostics.
const DIAG_INPUT_CHARS: usize = 300;

/// What the blocks of the calls with provider-run tools look like: the `type`
/// of each, and for `server_tool_use` the `name` and the `input` serialized
/// and cut. Nothing of the pages or of the lesson text. It exists to read
/// the real shape of a provider's blocks.
#[must_use]
pub fn block_diagnostics(calls: &[CallRecord]) -> Vec<Value> {
    calls
        .iter()
        .filter(|c| {
            c.blocks
                .iter()
                .any(|b| b["type"].as_str().is_some_and(|t| t != "text"))
        })
        .map(|c| {
            let blocks: Vec<Value> = c
                .blocks
                .iter()
                .map(|b| {
                    let ty = b["type"].as_str().unwrap_or("?");
                    if ty == "server_tool_use" {
                        let input: String = b["input"]
                            .to_string()
                            .chars()
                            .take(DIAG_INPUT_CHARS)
                            .collect();
                        json!({"type": ty, "name": b["name"], "input": input})
                    } else {
                        json!({"type": ty})
                    }
                })
                .collect();
            json!({"label": c.label, "blocks": blocks})
        })
        .collect()
}

/// Reads of one call: the provider's figure or the reads in the blocks,
/// whichever is larger (the recorder already applies it; here it holds for any
/// record).
#[must_use]
pub fn call_web_fetches(c: &CallRecord) -> u32 {
    let seen = u32::try_from(domain::llm::web_fetch_reads(&c.blocks).len()).unwrap_or(u32::MAX);
    c.usage.web_fetches.max(seen)
}

fn web_fetches(calls: &[&CallRecord]) -> u32 {
    calls.iter().map(|c| call_web_fetches(c)).sum()
}

/// What the checks need to know.
pub struct FetchFacts<'a> {
    /// The Generator's calls of the run, in order (writing, continuations,
    /// repairs).
    pub calls: &'a [CallRecord],
    /// Addresses of the sources the plan chose.
    pub plan_urls: &'a [String],
    /// Every visible string of the final lesson, prose and code.
    pub lesson_text: &'a str,
    /// The final lesson document, when there is one.
    pub document: Option<&'a Value>,
    /// The case's `fixture_pages`: URL -> reference text (never served).
    pub fixture_pages: &'a Map<String, Value>,
    /// Pages the runner put in the conversation as already read (not read by
    /// the provider): they count as reads for the checks.
    pub injected: &'a [InjectedPage],
}

/// The injected pages as reads, then the reads of the calls.
#[must_use]
pub fn pages_with_injected(calls: &[CallRecord], injected: &[InjectedPage]) -> Vec<Page> {
    injected
        .iter()
        .map(|p| Page {
            url: p.url.clone(),
            text: Some(p.text.clone()),
            error: None,
        })
        .chain(pages(calls))
        .collect()
}

fn text_of(parts: &[Part]) -> String {
    parts
        .iter()
        .filter_map(|p| match p {
            Part::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

/// The checks named by `spec` (only the keys of [`KEYS`]; `false` switches a
/// boolean check off).
#[must_use]
pub fn checks(spec: &Map<String, Value>, f: &FetchFacts<'_>) -> Vec<Check> {
    let mut out = Vec::new();
    for (key, v) in spec.iter().filter(|(k, _)| KEYS.contains(&k.as_str())) {
        if v == &Value::Bool(false) {
            continue;
        }
        let all: Vec<&CallRecord> = f.calls.iter().collect();
        match key.as_str() {
            "web_fetch_requests" => {
                let Some((min, max)) = range(v) else { continue };
                // An injected page is the one read the model was shown.
                let n = web_fetches(&all) + u32::try_from(f.injected.len()).unwrap_or(u32::MAX);
                let (ok_min, ok_max) = (f64::from(n) >= min, f64::from(n) <= max);
                // Too many is the ceiling broken (`max_uses`); too few is the
                // model reading less than asked, which the suite wants in
                // most runs, not all.
                out.push(if ok_max {
                    Check::soft(key, ok_min, || {
                        format!("{n} page reads, expected {min}-{max}")
                    })
                } else {
                    Check::of(key, false, || {
                        format!("{n} page reads, more than the {max} allowed")
                    })
                });
            }
            "fetch_errors" => {
                let Some((min, max)) = range(v) else { continue };
                let codes: Vec<String> =
                    pages(f.calls).into_iter().filter_map(|p| p.error).collect();
                let n = codes.len();
                out.push(Check::of(
                    key,
                    crate::checks::in_range(n, (min, max)),
                    || format!("{n} failed reads {codes:?}, expected {min}-{max}"),
                ));
            }
            "fetch_urls_only_from_sources" => {
                let mut requested = requested_urls(f.calls);
                requested
                    .urls
                    .extend(f.injected.iter().map(|p| p.url.clone()));
                let strays: Vec<&String> = requested
                    .urls
                    .iter()
                    .filter(|u| !f.plan_urls.contains(u))
                    .collect();
                // A read whose address cannot be derived is a detail, not a
                // failure: only an address really outside the sources is.
                out.push(Check::of(key, strays.is_empty(), || {
                    let mut m = format!("read outside the plan's sources: {strays:?}");
                    if requested.unknown > 0 {
                        let _ = write!(
                            m,
                            "; {} read(s) with the URL not derivable from the block",
                            requested.unknown
                        );
                    }
                    m
                }));
                if strays.is_empty() && requested.unknown > 0 {
                    out.push(Check::soft("fetch_urls_derivable", false, || {
                        format!(
                            "{} read(s): URL non ricavabile dal blocco",
                            requested.unknown
                        )
                    }));
                }
            }
            "no_refetch_in_repair" => {
                let repairs: Vec<&CallRecord> =
                    f.calls.iter().filter(|c| c.label == REPAIR_LABEL).collect();
                let requested =
                    requested_urls(&repairs.iter().map(|c| (*c).clone()).collect::<Vec<_>>());
                let blocks = requested.urls.len() + requested.unknown;
                let n = web_fetches(&repairs);
                out.push(Check::of(key, n == 0 && blocks == 0, || {
                    format!(
                        "{n} page reads ({blocks} requests) in {} repair call(s)",
                        repairs.len()
                    )
                }));
            }
            "must_match_min" => {
                let min = v["min"].as_u64().unwrap_or(0);
                let hits: Vec<&str> = v["patterns"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|p| Regex::new(p).is_ok_and(|r| r.is_match(f.lesson_text)))
                    .collect();
                // Soft: the suite wants it in most runs (80%), not all.
                out.push(Check::soft(key, hits.len() as u64 >= min, || {
                    format!(
                        "{} of the patterns match, at least {min} expected",
                        hits.len()
                    )
                }));
            }
            "fixture_page_served" => {
                let read = pages_with_injected(f.calls, f.injected);
                let mut problems = Vec::new();
                let mut found_all = Vec::new();
                for (url, reference) in f.fixture_pages {
                    let reference = reference.as_str().unwrap_or_default();
                    let text = read
                        .iter()
                        .filter(|p| &p.url == url)
                        .find_map(|p| p.text.as_deref());
                    match text {
                        None => problems.push(format!("{url}: page not read (or without text)")),
                        Some(text) => {
                            let (found, total) = paragraphs_found(reference, text);
                            found_all.push(format!("{found}/{total} paragraphs of {url}"));
                            // At least 80% of the paragraphs.
                            if found * 5 < total * 4 {
                                problems.push(format!(
                                    "only {found} of {total} paragraphs of the fixture found in the page read from {url}"
                                ));
                            }
                        }
                    }
                }
                out.push(Check::of(key, problems.is_empty(), || {
                    if problems.is_empty() {
                        found_all.join("; ")
                    } else {
                        problems.join("; ")
                    }
                }));
            }
            "citations_in_fetched_pages" => {
                let max = usize::try_from(v["max_unsupported"].as_u64().unwrap_or(0))
                    .unwrap_or(usize::MAX);
                let bad = f.document.map_or_default(|d| {
                    unsupported_citations(d, &pages_with_injected(f.calls, f.injected))
                });
                out.push(Check::soft(key, bad.len() <= max, || {
                    let shown: Vec<String> = bad
                        .iter()
                        .take(5)
                        .map(|u| format!("{}: {} ({})", u.block_id, u.term, u.key))
                        .collect();
                    format!(
                        "{} block(s) cite a read page for a term it does not contain, at most {max} allowed: {}",
                        bad.len(),
                        shown.join("; ")
                    )
                }));
            }
            "user_message_has_no_sources_list" => {
                let first = f.calls.first();
                let message = first.map_or_default(|c| {
                    c.request
                        .messages
                        .iter()
                        .filter(|m| m.role == domain::llm::MessageRole::User)
                        .map(|m| text_of(&m.parts))
                        .collect::<Vec<_>>()
                        .join("\n")
                });
                let tools = first.map_or(0, |c| c.request.tools.len());
                out.push(Check::of(
                    key,
                    first.is_some()
                        && message.trim() == KICKOFF
                        && !message.contains(SOURCES_LIST_MARKER)
                        && tools == 0,
                    || {
                        format!(
                            "the first call has {tools} tool(s) and the user message {message:?}"
                        )
                    },
                ));
            }
            _ => {}
        }
    }
    out
}

/// Lowercase, collapsed spaces; `strip_markup` also drops `#`, backticks,
/// `[`, `]` and `*` (what an HTML->text conversion may add or remove).
fn normalize(text: &str, strip_markup: bool) -> String {
    let kept: String = if strip_markup {
        text.chars().filter(|c| !"#`[]*".contains(*c)).collect()
    } else {
        text.to_owned()
    };
    kept.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// How many of the non-empty paragraphs of `reference` (blocks split by a
/// blank line) the page `text` contains, and how many there are.
#[must_use]
pub fn paragraphs_found(reference: &str, text: &str) -> (usize, usize) {
    let haystack = normalize(text, true);
    let paragraphs: Vec<String> = reference
        .split("\n\n")
        .map(|p| normalize(p, true))
        .filter(|p| !p.is_empty())
        .collect();
    let found = paragraphs.iter().filter(|p| haystack.contains(*p)).count();
    (found, paragraphs.len())
}

/// A key term of a block that the page it cites does not contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub block_id: String,
    pub term: String,
    pub key: String,
}

fn collect_strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
        Value::Object(o) => o
            .iter()
            .filter(|(k, _)| k.as_str() != "id")
            .for_each(|(_, x)| collect_strings(x, out)),
        _ => {}
    }
}

/// The terms a claim leans on that a page must contain to support it: code
/// spans that name something (`&String`, `first_word`, `E0502`) and section or
/// error tokens (`4.1`, `E0499`), the `[^key]` marks left out. Left out too,
/// because a lesson writes its own examples on purpose (the prompt asks it to):
/// spans with a path, an index, a literal, an assignment or a call with
/// arguments (`/home/ana/bin`, `&s[3..5]`, `"ciao"`, `SALUTO=ciao`, `f(x)`),
/// and bare words of three letters or fewer (`ls`, `cd`), which say nothing.
#[must_use]
pub fn key_terms(text: &str) -> Vec<String> {
    let code = Regex::new("`([^`]+)`").expect("static regex");
    let cite = Regex::new(r"\[\^[^\]]*\]").expect("static regex");
    let token = Regex::new(r"\b[A-Za-z]*\d[A-Za-z0-9]*(?:\.\d+)*\b").expect("static regex");
    let mut terms: Vec<String> = code
        .captures_iter(text)
        .map(|c| c[1].trim().to_owned())
        .filter(|t| names_something(t))
        .collect();
    let rest = code.replace_all(text, " ");
    let rest = cite.replace_all(&rest, " ");
    for m in token.find_iter(&rest) {
        let t = m.as_str();
        if (t.contains('.') || t.chars().any(char::is_alphabetic)) && !terms.iter().any(|x| x == t)
        {
            terms.push(t.to_owned());
        }
    }
    terms
}

/// Whether a code span names a thing of the source (a type, a function, an
/// error, a keyword phrase) rather than being an example the lesson made up.
fn names_something(span: &str) -> bool {
    let s = span.trim();
    let example = s.contains('/')
        || s.contains('"')
        || s.contains('\'')
        || s.contains('=')
        || s.contains('[')
        || s.contains(',')
        || s.contains("..")
        || s.contains(':') && !s.contains("::")
        || Regex::new(r"\([^)]").expect("static regex").is_match(s);
    let short_word = s.len() <= 3 && s.chars().all(|c| c.is_ascii_alphanumeric());
    // A plain lower-case word (`frase`, `&frase`, `&mut testo`) is a variable
    // of the lesson's example; a source's names have a capital, `_`, `::` or
    // `()` (`String`, `first_word`, `String::from`, `len()`). `&str` is the
    // exception that matters: a short word after a sigil stays a type.
    let bare = s.trim_start_matches("&mut ").trim_start_matches('&');
    let plain_variable = bare.len() > 3 && bare.chars().all(|c| c.is_ascii_lowercase());
    !s.is_empty() && !example && !short_word && !plain_variable
}

/// The sentences of `text` that carry a `[^key]` mark, each with its keys: a
/// mark stands for the sentence before it, not for the whole block.
fn cited_sentences(text: &str) -> Vec<(String, Vec<String>)> {
    let cite = Regex::new(r"\[\^([^\]]+)\]").expect("static regex");
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut start = 0;
    for m in cite.captures_iter(text) {
        let whole = m.get(0).expect("match");
        let key = m[1].to_owned();
        let before = &text[start..whole.start()];
        // The sentence the mark closes: from the last sentence end before it.
        let from = before
            .char_indices()
            .filter(|(i, c)| {
                matches!(c, '.' | '!' | '?' | '\n')
                    && before[i + c.len_utf8()..].starts_with(char::is_whitespace)
            })
            .map(|(i, c)| i + c.len_utf8())
            .next_back()
            .unwrap_or(0);
        let sentence = before[from..].trim();
        if sentence.is_empty() {
            // `…[^a][^b]`: one more key for the same sentence.
            if let Some(last) = out.last_mut() {
                last.1.push(key);
            }
        } else {
            out.push((sentence.to_owned(), vec![key]));
        }
        start = whole.end();
    }
    out
}

/// The blocks of `doc` with a sentence that cites (`[^key]`, a key of the
/// document's own `sources[]`) a source whose page was read and leans on a
/// key term the page does not contain. Only the sentence a mark closes is
/// checked, not the whole block: an example sentence without a mark is the
/// lesson's own. One entry per block and key. Sentences with no key term, or
/// whose source was not read as text, are not judged. Pure: it knows nothing
/// of the runner, so the Validator may use it.
#[must_use]
pub fn unsupported_citations(doc: &Value, read: &[Page]) -> Vec<Unsupported> {
    let mut out: Vec<Unsupported> = Vec::new();
    for block in doc["blocks"].as_array().into_iter().flatten() {
        let mut strings = Vec::new();
        collect_strings(block, &mut strings);
        let id = block["id"].as_str().unwrap_or("?");
        for text in strings {
            for (sentence, keys) in cited_sentences(text) {
                let terms = key_terms(&sentence);
                if terms.is_empty() {
                    continue;
                }
                for key in keys {
                    if out.iter().any(|u| u.block_id == id && u.key == key) {
                        continue;
                    }
                    let Some(url) = doc["sources"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|s| s["key"].as_str() == Some(key.as_str()))
                        .and_then(|s| s["url"].as_str())
                    else {
                        continue;
                    };
                    let Some(page) = read
                        .iter()
                        .filter(|p| p.url == url)
                        .find_map(|p| p.text.as_deref())
                    else {
                        continue;
                    };
                    let page = normalize(page, false);
                    if let Some(missing) =
                        terms.iter().find(|t| !page.contains(&normalize(t, false)))
                    {
                        out.push(Unsupported {
                            block_id: id.to_owned(),
                            term: missing.clone(),
                            key,
                        });
                    }
                }
            }
        }
    }
    out
}

/// A rough count of the tokens of the pages read: a token is ~4 characters.
/// An estimate, not the provider's figure (it does not give one).
#[must_use]
pub fn fetched_tokens_estimate(pages: &[Page]) -> f64 {
    let chars: usize = pages
        .iter()
        .filter_map(|p| p.text.as_deref())
        .map(|t| t.chars().count())
        .sum();
    #[allow(clippy::cast_precision_loss)] // a few tens of thousands
    let tokens = (chars as f64 / 4.0).ceil();
    tokens
}

/// The metrics named by `report_metrics`, from the calls of the run.
#[must_use]
pub fn metrics(
    names: &[&str],
    calls: &[CallRecord],
    documents: usize,
    unsupported_citations: usize,
) -> Vec<(String, f64)> {
    let all: Vec<&CallRecord> = calls.iter().collect();
    let read = pages(calls);
    #[allow(clippy::cast_precision_loss)] // small counts
    let count = |n: usize| n as f64;
    names
        .iter()
        .filter_map(|name| {
            let value = match *name {
                "cost_usd" => calls.iter().map(|c| c.cost_usd).sum(),
                "input_tokens" => calls
                    .iter()
                    .map(|c| {
                        f64::from(c.usage.input_tokens)
                            + f64::from(c.usage.cache_read_tokens)
                            + f64::from(c.usage.cache_write_tokens)
                    })
                    .sum(),
                "output_tokens" => calls.iter().map(|c| f64::from(c.usage.output_tokens)).sum(),
                "web_fetch_requests" => f64::from(web_fetches(&all)),
                "fetched_tokens" => fetched_tokens_estimate(&read),
                "model_calls" => count(calls.len()),
                "documents" => count(documents),
                "unsupported_citations" => count(unsupported_citations),
                _ => return None,
            };
            Some(((*name).to_owned(), value))
        })
        .collect()
}

/// The pages the Generator read, as text for the judge: each one cut to
/// [`JUDGE_PAGE_CHARS`], a failed read said as such. Without it a criterion
/// such as "what the lesson attributes to the source is in the page" could
/// not be judged. Empty when nothing was read.
#[must_use]
pub fn judge_material(pages: &[Page]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = String::new();
    let mut left = JUDGE_PAGES_TOTAL_CHARS;
    for page in pages {
        if seen.contains(&page.url.as_str()) {
            continue;
        }
        seen.push(&page.url);
        let _ = writeln!(
            out,
            "--- Page read with the web fetch tool: {} ---",
            page.url
        );
        match (&page.error, &page.text) {
            (Some(code), _) => {
                let _ = write!(
                    out,
                    "(the read failed: {code}; the Generator saw no text of this page)\n\n"
                );
            }
            (None, Some(text)) => {
                let room = JUDGE_PAGE_CHARS.min(left);
                let cut: String = text.chars().take(room).collect();
                left -= cut.chars().count();
                out.push_str(&cut);
                if text.chars().count() > room {
                    out.push_str("\n[... cut here for the judge ...]");
                }
                out.push_str("\n\n");
            }
            (None, None) => out.push_str("(not a text page: its content is not shown here)\n\n"),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use domain::llm::{LlmMessage, LlmRequest, LlmRole, MessageRole, ToolDef, Usage};
    use serde_json::json;

    use super::*;
    use crate::checks::Status;
    use crate::llm::Source;

    fn call(label: &str, fetches: u32, blocks: Vec<Value>) -> CallRecord {
        let mut request = LlmRequest::new(LlmRole::Generator, "x");
        request.messages = vec![LlmMessage::text(MessageRole::User, KICKOFF)];
        CallRecord {
            source: Source::Subject,
            role: LlmRole::Generator,
            label: label.to_owned(),
            model: "m".into(),
            request,
            text: String::new(),
            tool_calls: vec![],
            stop_reason: "EndTurn".into(),
            usage: Usage {
                web_fetches: fetches,
                input_tokens: 100,
                output_tokens: 10,
                ..Usage::default()
            },
            latency: Duration::ZERO,
            search_urls: vec![],
            blocks,
            error: None,
            cost_usd: 0.5,
            price_assumed: false,
        }
    }

    fn read(id: &str, url: &str) -> Vec<Value> {
        vec![
            json!({"type": "server_tool_use", "id": id, "name": "web_fetch", "input": {"url": url}}),
            json!({"type": "web_fetch_tool_result", "tool_use_id": id, "content": {
                "type": "web_fetch_result", "url": url,
                "content": {"type": "document", "source": {"type": "text", "data": "Testo della pagina"}}}}),
        ]
    }

    fn failed(id: &str, url: &str, code: &str) -> Vec<Value> {
        vec![
            json!({"type": "server_tool_use", "id": id, "name": "web_fetch", "input": {"url": url}}),
            json!({"type": "web_fetch_tool_result", "tool_use_id": id,
                   "content": {"type": "web_fetch_tool_result_error", "error_code": code}}),
        ]
    }

    fn run(spec: Value, calls: &[CallRecord], plan: &[&str], text: &str) -> Vec<Check> {
        let plan: Vec<String> = plan.iter().map(|s| (*s).to_owned()).collect();
        checks(
            spec.as_object().unwrap(),
            &FetchFacts {
                calls,
                plan_urls: &plan,
                lesson_text: text,
                document: None,
                fixture_pages: &Map::new(),
                injected: &[],
            },
        )
    }

    fn status(checks: &[Check], name: &str) -> Status {
        checks.iter().find(|c| c.name == name).unwrap().status
    }

    #[test]
    fn requests_are_summed_over_every_call_and_the_ceiling_is_hard() {
        let calls = [
            call("generator.write", 2, read("a", "https://a.example/")),
            call("generator.write", 1, vec![]),
        ];
        let at = |range: Value| {
            status(
                &run(json!({"web_fetch_requests": range}), &calls, &[], ""),
                "web_fetch_requests",
            )
        };
        assert_eq!(at(json!([3, 3])), Status::Pass);
        assert_eq!(
            at(json!([4, 5])),
            Status::Warn,
            "fewer than asked: information"
        );
        assert_eq!(at(json!([1, 2])), Status::Fail, "more than the ceiling");
    }

    #[test]
    fn failed_reads_are_counted_from_the_error_blocks_of_the_answers() {
        let calls = [call(
            "generator.write",
            1,
            failed("a", "https://x.invalid/", "url_not_accessible"),
        )];
        let c = run(json!({"fetch_errors": [1, 1]}), &calls, &[], "");
        assert_eq!(status(&c, "fetch_errors"), Status::Pass);
        let c = run(json!({"fetch_errors": [0, 0]}), &calls, &[], "");
        assert_eq!(status(&c, "fetch_errors"), Status::Fail);
        assert!(
            c[0].detail.contains("url_not_accessible"),
            "{}",
            c[0].detail
        );
    }

    #[test]
    fn a_read_without_a_url_in_its_request_uses_the_result_and_is_no_stray() {
        let blocks = vec![
            json!({"type": "server_tool_use", "id": "a", "name": "web_fetch", "input": {}}),
            json!({"type": "web_fetch_tool_result", "tool_use_id": "a", "content": {
                "type": "web_fetch_result", "url": "https://a.example/x"}}),
            json!({"type": "server_tool_use", "id": "b", "name": "web_fetch", "input": ""}),
        ];
        let calls = [call("generator.write", 0, blocks)];
        let c = run(
            json!({"fetch_urls_only_from_sources": true}),
            &calls,
            &["https://a.example/x"],
            "",
        );
        assert_eq!(status(&c, "fetch_urls_only_from_sources"), Status::Pass);
        let note = c.iter().find(|c| c.name == "fetch_urls_derivable").unwrap();
        assert_eq!(note.status, Status::Warn);
        assert!(note.detail.contains("URL non ricavabile dal blocco"));
        assert!(!c.iter().any(|c| c.detail.contains('?')));
    }

    #[test]
    fn the_diagnostics_show_block_types_and_the_cut_input_and_nothing_else() {
        let long = "x".repeat(900);
        let blocks = vec![
            json!({"type": "text", "text": "il testo della lezione"}),
            json!({"type": "server_tool_use", "id": "a", "name": "web_fetch", "input": {"u": long}}),
            read("b", "https://a.example/x").remove(1),
        ];
        let d = block_diagnostics(&[call("generator.write", 1, blocks)]);
        let text = serde_json::to_string(&d).unwrap();
        assert!(!text.contains("lezione") && !text.contains("Testo della pagina"));
        let b = &d[0]["blocks"];
        assert_eq!(b[0], json!({"type": "text"}));
        assert_eq!(b[1]["name"], "web_fetch");
        assert!(b[1]["input"].as_str().unwrap().chars().count() <= DIAG_INPUT_CHARS);
        assert_eq!(b[2], json!({"type": "web_fetch_tool_result"}));
    }

    #[test]
    fn reads_seen_in_the_blocks_count_when_the_usage_says_zero_in_the_checks() {
        let calls = [call("generator.write", 0, read("a", "https://a.example/x"))];
        let m = metrics(&["web_fetch_requests"], &calls, 0, 0);
        assert_eq!(m[0].1, 1.0);
    }

    #[test]
    fn only_the_plans_addresses_may_be_read() {
        let mut blocks = read("a", "https://a.example/x");
        blocks.extend(read("b", "https://elsewhere.example/"));
        let calls = [call("generator.write", 2, blocks)];
        let c = run(
            json!({"fetch_urls_only_from_sources": true}),
            &calls,
            &["https://a.example/x"],
            "",
        );
        assert_eq!(status(&c, "fetch_urls_only_from_sources"), Status::Fail);
        assert!(c[0].detail.contains("elsewhere"));
        let c = run(
            json!({"fetch_urls_only_from_sources": true}),
            &calls[..0],
            &[],
            "",
        );
        assert_eq!(status(&c, "fetch_urls_only_from_sources"), Status::Pass);
        let c = run(
            json!({"fetch_urls_only_from_sources": false}),
            &calls,
            &[],
            "",
        );
        assert!(c.is_empty(), "false switches the check off");
    }

    #[test]
    fn a_repair_must_not_read_again() {
        let write = call("generator.write", 1, read("a", "https://a.example/"));
        let quiet = call("generator.repair", 0, vec![]);
        let again = call("generator.repair", 1, read("b", "https://a.example/"));
        let ok = run(
            json!({"no_refetch_in_repair": true}),
            &[write.clone(), quiet],
            &[],
            "",
        );
        assert_eq!(status(&ok, "no_refetch_in_repair"), Status::Pass);
        let bad = run(
            json!({"no_refetch_in_repair": true}),
            &[write, again],
            &[],
            "",
        );
        assert_eq!(status(&bad, "no_refetch_in_repair"), Status::Fail);
    }

    #[test]
    fn the_lesson_must_match_enough_patterns() {
        let spec = json!({"must_match_min": {"min": 2, "patterns": ["E0499", "(?i)data race", "dangling"]}});
        let none: [CallRecord; 0] = [];
        let c = run(
            spec.clone(),
            &none,
            &[],
            "Il codice E0499 causa un data race.",
        );
        assert_eq!(status(&c, "must_match_min"), Status::Pass);
        let c = run(spec, &none, &[], "Solo E0499.");
        assert_eq!(status(&c, "must_match_min"), Status::Warn);
    }

    #[test]
    fn the_twin_gets_neither_the_tool_nor_the_list() {
        let twin = call("generator.write", 0, vec![]);
        let spec = json!({"user_message_has_no_sources_list": true});
        assert_eq!(
            status(
                &run(spec.clone(), std::slice::from_ref(&twin), &[], ""),
                "user_message_has_no_sources_list"
            ),
            Status::Pass
        );
        let mut with_list = twin.clone();
        with_list.request.messages = vec![LlmMessage::text(
            MessageRole::User,
            "Procedi.\n\nFonti da leggere (id della risorsa · titolo — indirizzo):\n- x",
        )];
        assert_eq!(
            status(
                &run(spec.clone(), &[with_list], &[], ""),
                "user_message_has_no_sources_list"
            ),
            Status::Fail
        );
        let mut with_tool = twin;
        with_tool.request.tools = vec![ToolDef::ServerWebFetch {
            max_uses: 1,
            allowed_domains: vec![],
            max_content_tokens: 1,
        }];
        assert_eq!(
            status(
                &run(spec, &[with_tool], &[], ""),
                "user_message_has_no_sources_list"
            ),
            Status::Fail
        );
    }

    #[test]
    fn pages_come_from_the_blocks_and_the_judge_sees_them_cut() {
        let long = "x".repeat(JUDGE_PAGE_CHARS + 50);
        let mut blocks = read("a", "https://a.example/");
        blocks.extend(failed("b", "https://b.example/", "url_not_allowed"));
        blocks.extend([
            json!({"type": "server_tool_use", "id": "c", "name": "web_fetch", "input": {"url": "https://c.example/"}}),
            json!({"type": "web_fetch_tool_result", "tool_use_id": "c", "content": {
                "type": "web_fetch_result", "url": "https://c.example/",
                "content": {"type": "document", "source": {"type": "text", "data": long}}}}),
            // A search block is not a page.
            json!({"type": "web_search_tool_result", "tool_use_id": "z", "content": []}),
        ]);
        let found = pages(&[call("generator.write", 3, blocks)]);
        assert_eq!(found.len(), 3);
        assert_eq!(found[1].error.as_deref(), Some("url_not_allowed"));
        assert_eq!(
            found[1].url, "https://b.example/",
            "the address comes from the request"
        );
        let material = judge_material(&found);
        assert!(material.contains("Testo della pagina"));
        assert!(material.contains("the read failed: url_not_allowed"));
        assert!(material.contains("cut here for the judge"));
        assert!(!material.contains(&"x".repeat(JUDGE_PAGE_CHARS + 1)));
        assert_eq!(judge_material(&[]), "");
    }

    #[test]
    fn metrics_are_the_ones_asked() {
        let calls = [
            call("generator.write", 2, read("a", "https://a.example/")),
            call("generator.repair", 0, vec![]),
        ];
        let m: HashMap<String, f64> = metrics(
            &[
                "cost_usd",
                "model_calls",
                "input_tokens",
                "web_fetch_requests",
                "documents",
                "fetched_tokens",
                "unsupported_citations",
            ],
            &calls,
            2,
            3,
        )
        .into_iter()
        .collect();
        assert!((m["cost_usd"] - 1.0).abs() < 1e-9);
        assert!((m["model_calls"] - 2.0).abs() < 1e-9);
        assert!((m["input_tokens"] - 200.0).abs() < 1e-9);
        assert!((m["web_fetch_requests"] - 2.0).abs() < 1e-9);
        assert!((m["documents"] - 2.0).abs() < 1e-9);
        assert!(m["fetched_tokens"] > 0.0);
        assert!((m["unsupported_citations"] - 3.0).abs() < 1e-9);
        assert_eq!(metrics(&["nope"], &calls, 1, 0).len(), 0);
    }

    #[test]
    fn values_are_validated_when_a_suite_loads() {
        assert_eq!(value_problem("web_fetch_requests", &json!([1, 3])), None);
        assert!(value_problem("web_fetch_requests", &json!(3)).is_some());
        assert!(value_problem("fetch_errors", &json!("0")).is_some());
        assert_eq!(value_problem("no_refetch_in_repair", &json!(true)), None);
        assert!(value_problem("no_refetch_in_repair", &json!("yes")).is_some());
        let ok = json!({"min": 2, "description": "d", "patterns": ["a", "(?i)b"]});
        assert_eq!(value_problem("must_match_min", &ok), None);
        for bad in [
            json!({"patterns": ["a"]}),
            json!({"min": 2}),
            json!({"min": 2, "patterns": ["("]}),
            json!({"min": 2, "patterns": ["a"], "mn": 1}),
            json!({"min": "2", "patterns": ["a"]}),
        ] {
            assert!(value_problem("must_match_min", &bad).is_some(), "{bad}");
        }
        assert_eq!(metrics_problem(&json!(["cost_usd", "documents"])), None);
        assert!(metrics_problem(&json!(["cost"])).is_some());
        assert!(metrics_problem(&json!("cost_usd")).is_some());
    }

    fn page_call(url: &str, text: &str) -> CallRecord {
        call(
            "generator.write",
            1,
            vec![
                json!({"type": "server_tool_use", "id": "p", "name": "web_fetch", "input": {"url": url}}),
                json!({"type": "web_fetch_tool_result", "tool_use_id": "p", "content": {
                    "type": "web_fetch_result", "url": url,
                    "content": {"type": "document", "source": {"type": "text", "data": text}}}}),
            ],
        )
    }

    fn served(calls: &[CallRecord], fixture: &str) -> Status {
        let mut pages = Map::new();
        pages.insert("https://p.example/x".into(), json!(fixture));
        let spec = json!({"fixture_page_served": true});
        let out = checks(
            spec.as_object().unwrap(),
            &FetchFacts {
                calls,
                plan_urls: &[],
                lesson_text: "",
                document: None,
                fixture_pages: &pages,
                injected: &[],
            },
        );
        status(&out, "fixture_page_served")
    }

    #[test]
    fn fixture_page_served_needs_the_published_page_to_match_the_fixture() {
        let fixture = "# Titolo\n\nPrimo `paragrafo` qui.\n\nSecondo **paragrafo**.\n\n\n\nTerzo.\n\nQuarto.\n\nQuinto.";
        // Converted text: markup gone, spaces and case changed. 5 of 6 found.
        let ok = "Titolo\n\nprimo paragrafo   qui.\nSecondo paragrafo.\nTerzo. Quarto. Quinto.";
        let good = [page_call("https://p.example/x", ok)];
        assert_eq!(served(&good, fixture), Status::Pass);
        let wrong = [page_call("https://p.example/x", "Titolo Terzo.")];
        assert_eq!(served(&wrong, fixture), Status::Fail);
        let other_url = [page_call("https://q.example/x", ok)];
        assert_eq!(served(&other_url, fixture), Status::Fail);
        assert_eq!(served(&[], fixture), Status::Fail);
        let (found, total) = paragraphs_found(fixture, ok);
        assert_eq!((found, total), (6, 6));
    }

    #[test]
    fn key_terms_are_code_spans_and_numeric_tokens() {
        let t = key_terms("Vedi §4.1 e `&String` con E0499 [^b] e versione 3.14, anno 2026.");
        assert!(t.contains(&"&String".to_owned()), "{t:?}");
        assert!(t.contains(&"4.1".to_owned()), "{t:?}");
        assert!(t.contains(&"E0499".to_owned()), "{t:?}");
        assert!(t.contains(&"3.14".to_owned()), "{t:?}");
        assert!(!t.iter().any(|x| x == "2026"), "{t:?}");
    }

    #[test]
    fn examples_written_by_the_lesson_are_not_key_terms() {
        // Seen in the runs of 2026-11-19: all of these are the lesson's own
        // examples, not claims about the page.
        for span in [
            "/home/ana/bin",
            "/usr/local/bin:/usr/bin",
            "&s[3..5]",
            "&s[..b]",
            "\"ciao mondo\"",
            "SALUTO=ciao",
            "prima_parola(testo: &str) -> &str",
            "ls",
            "cd",
            "frase",
            "&frase",
            "ciao, mondo",
        ] {
            let t = key_terms(&format!("Usa `{span}` qui [^b]."));
            assert!(t.is_empty(), "{span}: {t:?}");
        }
        for span in [
            "&String",
            "&str",
            "first_word",
            "E0502",
            "String::from",
            "push_str()",
        ] {
            let t = key_terms(&format!("Usa `{span}` qui [^b]."));
            assert_eq!(t, [span], "{span}");
        }
    }

    #[test]
    fn only_the_sentence_a_mark_closes_is_checked() {
        let doc = json!({
            "sources": [{"key": "b", "url": "https://b.example/"}],
            "blocks": [
                // The example sentence has no mark: the lesson's own.
                {"id": "ex", "type": "paragraph",
                 "text": "Prima scrivi `inventata_qui`. Poi la regola vale per `export_var`[^b]."},
                // The marked sentence names something the page lacks.
                {"id": "bad", "type": "paragraph",
                 "text": "Vale `export_var`. Lo dice anche `deref_coercion`[^b]."}
            ]
        });
        let read = pages(&[page_call(
            "https://b.example/",
            "usa export_var per esportare",
        )]);
        let bad = unsupported_citations(&doc, &read);
        assert_eq!(
            bad,
            vec![Unsupported {
                block_id: "bad".into(),
                term: "deref_coercion".into(),
                key: "b".into()
            }]
        );
        // Accented text around the marks: the sentence split works on chars.
        let it = json!({"sources": [{"key": "b", "url": "https://b.example/"}],
            "blocks": [{"id": "it", "type": "paragraph",
                "text": "È così. Perché `export_var` è già lì[^b]. Più avanti `altro_nome` c'è[^b]."}]});
        let it_bad = unsupported_citations(&it, &read);
        assert_eq!(it_bad.len(), 1, "{it_bad:?}");
        assert_eq!(it_bad[0].term, "altro_nome");
    }

    #[test]
    fn citations_are_checked_against_the_pages_read() {
        let doc = json!({
            "sources": [{"key": "b", "url": "https://b.example/"}, {"key": "c", "url": "https://c.example/"}],
            "blocks": [
                {"id": "ok", "type": "paragraph", "text": "La regola 4.1 usa `&str` [^b]."},
                {"id": "bad", "type": "paragraph", "text": "La regola 4.3 vale [^b]. Come dice [^c] e ancora la 4.3 [^b]."},
                {"id": "nocode", "type": "paragraph", "text": "Solo prosa [^b]."},
                {"id": "unread", "type": "paragraph", "text": "Vedi `zzz` [^c]."}
            ]
        });
        let read = pages(&[page_call(
            "https://b.example/",
            "Sezione 4.1: il tipo  &STR è un riferimento",
        )]);
        let bad = unsupported_citations(&doc, &read);
        assert_eq!(
            bad,
            vec![Unsupported {
                block_id: "bad".into(),
                term: "4.3".into(),
                key: "b".into()
            }]
        );
        // Soft check: one unsupported block is a warning only above the limit.
        let calls = [page_call("https://b.example/", "Sezione 4.1: &str")];
        let at = |max: u64| {
            let spec = json!({"citations_in_fetched_pages": {"max_unsupported": max}});
            let empty = Map::new();
            status(
                &checks(
                    spec.as_object().unwrap(),
                    &FetchFacts {
                        calls: &calls,
                        plan_urls: &[],
                        lesson_text: "",
                        document: Some(&doc),
                        fixture_pages: &empty,
                        injected: &[],
                    },
                ),
                "citations_in_fetched_pages",
            )
        };
        assert_eq!(at(0), Status::Warn);
        assert_eq!(at(1), Status::Pass);
        assert!(value_problem("citations_in_fetched_pages", &json!(1)).is_some());
        assert!(value_problem("fixture_page_served", &json!("x")).is_some());
    }
}

#[cfg(test)]
mod injected_tests {
    use super::*;

    #[test]
    fn an_injected_page_counts_as_read() {
        let injected = [InjectedPage {
            url: "https://g.invalid/p".into(),
            title: "T".into(),
            text: "Primo paragrafo.\n\nSecondo paragrafo.".into(),
        }];
        let read = pages_with_injected(&[], &injected);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].text.as_deref(), Some(injected[0].text.as_str()));
        // The blocks the injector writes are read back by `pages` too.
        let blocks = crate::llm::injected_blocks(&injected);
        assert_eq!(blocks.len(), 2);
        let reads = domain::llm::web_fetch_reads(&blocks);
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].url.as_deref(), Some("https://g.invalid/p"));

        let fixtures: Map<String, Value> =
            serde_json::from_value(json!({"https://g.invalid/p": injected[0].text})).unwrap();
        let spec: Map<String, Value> = serde_json::from_value(json!({
            "fixture_page_served": true,
            "web_fetch_requests": [1, 1],
            "fetch_urls_only_from_sources": true
        }))
        .unwrap();
        let out = checks(
            &spec,
            &FetchFacts {
                calls: &[],
                plan_urls: &["https://g.invalid/p".to_owned()],
                lesson_text: "",
                document: None,
                fixture_pages: &fixtures,
                injected: &injected,
            },
        );
        assert_eq!(out.len(), 3);
        assert!(
            out.iter().all(|c| c.status == crate::checks::Status::Pass),
            "{out:?}"
        );
    }
}
