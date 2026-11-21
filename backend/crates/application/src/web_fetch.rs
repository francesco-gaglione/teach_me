//! Which web sources of a lesson plan the Generator reads with the
//! provider's web fetch tool, and how the tool is configured for them
//! (ADR 0009, `backend/prompts/llm-port.md` §14). Pure functions: the choice
//! of the hosts and of the limits lives here, not in the provider adapter.

use domain::Resource;
use domain::ResourceKind;
use domain::llm::ToolDef;
use url::{Host, Url};

use crate::context::one_line;

/// Pages read per lesson, at most: each one costs up to
/// [`FETCH_MAX_CONTENT_TOKENS`] input tokens, and again on every repair.
pub const MAX_FETCH_SOURCES: usize = 3;
/// Longest text taken from one page.
pub const FETCH_MAX_CONTENT_TOKENS: u32 = 8_000;

/// The tool and the list of sources the user message asks to read.
#[derive(Debug, Clone, PartialEq)]
pub struct WebFetchPlan {
    pub tool: ToolDef,
    /// One line per source, `- {resource_id} · {title} — {url}`, in the order
    /// of the plan's sources (`{{web_sources_list}}`).
    pub sources_list: String,
}

/// The bare host a provider's `allowed_domains` wants for `url`: `https`
/// only, lower case, without port, path or user info, in ASCII (punycode for
/// international names). `None` for anything that is not a domain name over
/// https, or that is not written plainly (surrounding or inner whitespace).
#[must_use]
pub fn https_host(url: &str) -> Option<String> {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    // `Url` is lenient about slashes (`https:///host` reads as `host`); the
    // provider is not, and the text of the address must say what it means.
    let after_scheme = url.get(..8).filter(|s| s.eq_ignore_ascii_case("https://"));
    if after_scheme.is_none() || url[8..].starts_with(['/', '\\']) {
        return None;
    }
    let parsed = Url::parse(url).ok()?;
    if parsed.scheme() != "https" {
        return None;
    }
    let Some(Host::Domain(domain)) = parsed.host() else {
        return None;
    };
    // `Url` lower-cases and converts to punycode; the trailing dot of an
    // absolute name is not part of what the provider matches.
    let host = domain.trim_end_matches('.').to_ascii_lowercase();
    // Only plain names the provider can match: labels of `[a-z0-9-]` and at
    // least one dot (no `localhost`, no wildcard, no stray characters).
    let plain = host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        });
    plain.then_some(host)
}

/// An address with user info (`https://user:pw@host/`): its credentials must
/// not reach the provider, so such a source is not read at all.
fn has_user_info(url: &str) -> bool {
    Url::parse(url).is_ok_and(|u| !u.username().is_empty() || u.password().is_some())
}

/// The web fetch plan for the sources the lesson plan chose (primary first),
/// or `None` when the tool is off (`max_fetches == 0`) or no source can be
/// read: uploaded PDFs already come with their text, and an address that is
/// not https or has no representable host cannot be allowed.
///
/// `max_uses` is the smallest of the web sources of the plan, `max_fetches`
/// and [`MAX_FETCH_SOURCES`].
#[must_use]
pub fn plan_web_fetch(sources: &[&Resource], max_fetches: u32) -> Option<WebFetchPlan> {
    if max_fetches == 0 {
        return None;
    }
    // Only the sources the budget can read are listed and allowed: the kickoff
    // tells the model to read every listed page, and a listed page past
    // `max_uses` comes back as `max_uses_exceeded` (issue #26, 2026-11-19).
    let limit = usize::try_from(max_fetches)
        .unwrap_or(usize::MAX)
        .min(MAX_FETCH_SOURCES);
    let readable: Vec<(&Resource, &str, String)> = sources
        .iter()
        .filter(|r| r.kind != ResourceKind::Document)
        .filter_map(|r| {
            let url = r.url.as_deref().filter(|u| !has_user_info(u))?;
            Some((*r, url, https_host(url)?))
        })
        .take(limit)
        .collect();
    if readable.is_empty() {
        return None;
    }
    let mut domains: Vec<String> = Vec::new();
    for (_, _, host) in &readable {
        if !domains.contains(host) {
            domains.push(host.clone());
        }
    }
    let uses = u32::try_from(readable.len())
        .unwrap_or(u32::MAX)
        .min(max_fetches)
        .min(u32::try_from(MAX_FETCH_SOURCES).unwrap_or(u32::MAX));
    let sources_list = readable
        .iter()
        .map(|(r, url, _)| format!("- {} · {} — {url}", r.id, one_line(&r.title)))
        .collect::<Vec<_>>()
        .join("\n");
    Some(WebFetchPlan {
        tool: ToolDef::ServerWebFetch {
            max_uses: uses,
            allowed_domains: domains,
            max_content_tokens: FETCH_MAX_CONTENT_TOKENS,
        },
        sources_list,
    })
}

#[cfg(test)]
mod tests {
    use domain::{ResourceId, SourceType, TopicId};
    use time::macros::datetime;
    use uuid::Uuid;

    use super::*;

    fn resource(n: u128, kind: ResourceKind, url: Option<&str>) -> Resource {
        Resource {
            id: ResourceId::new(Uuid::from_u128(n)),
            topic_id: TopicId::new(Uuid::from_u128(1)),
            kind,
            source_type: SourceType::Documentation,
            title: format!("Fonte\n{n}"),
            url: url.map(str::to_owned),
            language: "en".into(),
            annotation: "a".into(),
            use_for: "u".into(),
            document_id: None,
            created_at: datetime!(2026-11-08 10:00:00 UTC),
        }
    }

    fn domains(plan: &WebFetchPlan) -> Vec<String> {
        match &plan.tool {
            ToolDef::ServerWebFetch {
                allowed_domains, ..
            } => allowed_domains.clone(),
            other => panic!("not a fetch tool: {other:?}"),
        }
    }

    fn uses(plan: &WebFetchPlan) -> u32 {
        match &plan.tool {
            ToolDef::ServerWebFetch { max_uses, .. } => *max_uses,
            other => panic!("not a fetch tool: {other:?}"),
        }
    }

    #[test]
    fn hosts_are_bare_lower_case_ascii() {
        assert_eq!(
            https_host("https://Docs.Example.COM:8443/a/b?q=1#f").as_deref(),
            Some("docs.example.com")
        );
        assert_eq!(
            https_host("https://user:pw@example.com/").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            https_host("https://example.com./x").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            https_host("https://bücher.example/seite").as_deref(),
            Some("xn--bcher-kva.example")
        );
    }

    #[test]
    fn what_is_not_a_domain_over_https_has_no_host() {
        for url in [
            "http://example.com/",
            "ftp://example.com/",
            "example.com/page",
            "https://192.168.0.1/",
            "https://[::1]/",
            "https://exa mple.com/",
            " https://example.com/",
            "https:///nohost",
            "https://*.example.com/",
            "https://localhost/",
            "https://intranet/",
            "https://exa_mple.com/",
            "https://a%20b.example/",
            "",
        ] {
            assert_eq!(https_host(url), None, "{url:?}");
        }
    }

    #[test]
    fn the_plan_allows_the_hosts_of_the_web_sources_only() {
        let a = resource(1, ResourceKind::Knowledge, Some("https://A.example/x"));
        let b = resource(2, ResourceKind::Knowledge, Some("https://a.example:444/y"));
        let c = resource(3, ResourceKind::Knowledge, Some("https://docs.b.example/"));
        let pdf = resource(4, ResourceKind::Document, None);
        let insecure = resource(5, ResourceKind::Knowledge, Some("http://c.example/"));
        let plan = plan_web_fetch(&[&pdf, &a, &insecure, &b, &c], 3).unwrap();
        assert_eq!(domains(&plan), ["a.example", "docs.b.example"]);
        assert_eq!(uses(&plan), 3);
        assert_eq!(
            plan.sources_list,
            format!(
                "- {} · Fonte 1 — https://A.example/x\n- {} · Fonte 2 — https://a.example:444/y\n- {} · Fonte 3 — https://docs.b.example/",
                a.id, b.id, c.id
            )
        );
        match &plan.tool {
            ToolDef::ServerWebFetch {
                max_content_tokens, ..
            } => assert_eq!(*max_content_tokens, 8_000),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_source_with_user_info_is_neither_allowed_nor_listed() {
        let secret = resource(
            1,
            ResourceKind::Knowledge,
            Some("https://user:pw@a.example/x"),
        );
        let user_only = resource(2, ResourceKind::Knowledge, Some("https://user@b.example/"));
        let ok = resource(3, ResourceKind::Knowledge, Some("https://c.example/"));
        let plan = plan_web_fetch(&[&secret, &user_only, &ok], 3).unwrap();
        assert_eq!(domains(&plan), ["c.example"]);
        assert_eq!(plan.sources_list.lines().count(), 1);
        assert!(!plan.sources_list.contains("pw"));
        assert_eq!(plan_web_fetch(&[&secret, &user_only], 3), None);
    }

    #[test]
    fn max_uses_is_the_smallest_of_sources_setting_and_three() {
        let rs: Vec<Resource> = (1..=5)
            .map(|n| {
                resource(
                    n,
                    ResourceKind::Knowledge,
                    Some(&format!("https://s{n}.example/")),
                )
            })
            .collect();
        let refs: Vec<&Resource> = rs.iter().collect();
        assert_eq!(uses(&plan_web_fetch(&refs, 10).unwrap()), 3);
        assert_eq!(uses(&plan_web_fetch(&refs, 2).unwrap()), 2);
        assert_eq!(uses(&plan_web_fetch(&refs[..1], 3).unwrap()), 1);
        // Three at most are listed and allowed, whatever the plan has.
        let plan = plan_web_fetch(&refs, 10).unwrap();
        assert_eq!(plan.sources_list.lines().count(), 3);
        assert_eq!(domains(&plan), ["s1.example", "s2.example", "s3.example"]);
        // With a smaller budget only the first sources (primary first) are
        // listed: a listed page the budget cannot read would fail.
        let one = plan_web_fetch(&refs, 1).unwrap();
        assert_eq!(one.sources_list.lines().count(), 1);
        assert_eq!(domains(&one), ["s1.example"]);
    }

    #[test]
    fn nothing_to_read_or_a_switch_off_means_no_tool() {
        let pdf = resource(1, ResourceKind::Document, None);
        let http = resource(2, ResourceKind::Knowledge, Some("http://a.example/"));
        let ok = resource(3, ResourceKind::Knowledge, Some("https://a.example/"));
        assert_eq!(plan_web_fetch(&[&pdf, &http], 3), None);
        assert_eq!(plan_web_fetch(&[], 3), None);
        assert_eq!(plan_web_fetch(&[&ok], 0), None);
    }
}
