//! Sources found by the Researcher (`docs/contracts/resources.md`).
//!
//! A topic has one current set of resources and gaps; a successful research
//! replaces it as a whole. Nothing here knows about the model: the use case
//! turns its output into [`ResourceDraft`]s and the repository stores them.

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;

use crate::ids::{DocumentId, JobId, ResourceId, TopicId};

pub const TITLE_MAX_CHARS: usize = 300;
pub const ANNOTATION_MAX_CHARS: usize = 600;
pub const USE_FOR_MAX_CHARS: usize = 300;
pub const GAP_AREA_MAX_CHARS: usize = 200;
pub const GAP_NOTE_MAX_CHARS: usize = 400;
pub const LANGUAGE_MAX_CHARS: usize = 35;
/// A topic's sources are "a handful"; more means the model did not choose.
pub const MAX_RESOURCES: usize = 12;
pub const MAX_GAPS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown resource value `{0}`")]
pub struct UnknownResourceValue(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// High-trust material to learn the content from.
    Knowledge,
    /// Places where people learn together.
    Community,
    /// An uploaded PDF (phase 10). Created and removed by the document, never
    /// by a research.
    Document,
}

impl ResourceKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Knowledge => "knowledge",
            Self::Community => "community",
            Self::Document => "document",
        }
    }
}

impl FromStr for ResourceKind {
    type Err = UnknownResourceValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "knowledge" => Ok(Self::Knowledge),
            "community" => Ok(Self::Community),
            "document" => Ok(Self::Document),
            other => Err(UnknownResourceValue(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    Book,
    Paper,
    Article,
    Documentation,
    Video,
    Course,
    Standard,
    /// Only for [`ResourceKind::Document`].
    Pdf,
    Forum,
    SocialGroup,
    LiveClass,
    LocalGroup,
    Other,
}

impl SourceType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::Paper => "paper",
            Self::Article => "article",
            Self::Documentation => "documentation",
            Self::Video => "video",
            Self::Course => "course",
            Self::Standard => "standard",
            Self::Pdf => "pdf",
            Self::Forum => "forum",
            Self::SocialGroup => "social_group",
            Self::LiveClass => "live_class",
            Self::LocalGroup => "local_group",
            Self::Other => "other",
        }
    }

    /// Types that only make sense as a place to meet people.
    #[must_use]
    pub const fn community_only(self) -> bool {
        matches!(
            self,
            Self::Forum | Self::SocialGroup | Self::LiveClass | Self::LocalGroup
        )
    }

    /// Types that only make sense as learning material.
    #[must_use]
    pub const fn knowledge_only(self) -> bool {
        matches!(
            self,
            Self::Book
                | Self::Paper
                | Self::Article
                | Self::Documentation
                | Self::Video
                | Self::Course
                | Self::Standard
        )
    }

    /// Types that can describe a place with no web page.
    #[must_use]
    pub const fn may_lack_url(self) -> bool {
        matches!(self, Self::LiveClass | Self::LocalGroup | Self::Other)
    }
}

impl FromStr for SourceType {
    type Err = UnknownResourceValue;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "book" => Self::Book,
            "paper" => Self::Paper,
            "article" => Self::Article,
            "documentation" => Self::Documentation,
            "video" => Self::Video,
            "course" => Self::Course,
            "standard" => Self::Standard,
            "pdf" => Self::Pdf,
            "forum" => Self::Forum,
            "social_group" => Self::SocialGroup,
            "live_class" => Self::LiveClass,
            "local_group" => Self::LocalGroup,
            "other" => Self::Other,
            other => return Err(UnknownResourceValue(other.to_owned())),
        })
    }
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for SourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A resource before it is stored: no id, no timestamp. The `url`, if any, is
/// already checked (`https`) and `domain` derived from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceDraft {
    pub kind: ResourceKind,
    pub source_type: SourceType,
    pub title: String,
    pub url: Option<String>,
    pub language: String,
    pub annotation: String,
    pub use_for: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub id: ResourceId,
    pub topic_id: TopicId,
    pub kind: ResourceKind,
    pub source_type: SourceType,
    pub title: String,
    pub url: Option<String>,
    pub language: String,
    pub annotation: String,
    pub use_for: String,
    /// `Some` only for [`ResourceKind::Document`].
    pub document_id: Option<DocumentId>,
    pub created_at: OffsetDateTime,
}

impl Resource {
    /// Host of the `url` without `www.`; `None` without a URL.
    #[must_use]
    pub fn domain(&self) -> Option<String> {
        self.url.as_deref().and_then(https_host)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub area: String,
    pub note: String,
}

/// Facts about the last successful research of a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchInfo {
    pub job_id: JobId,
    /// The mission version the research was based on.
    pub mission_version: u32,
    pub finished_at: OffsetDateTime,
}

/// What `GET /topics/{id}/resources` shows (the job is added by the use case).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopicResources {
    /// In the order of importance chosen by the Researcher.
    pub resources: Vec<Resource>,
    pub gaps: Vec<Gap>,
    pub communities_skipped: bool,
    /// `None` until a research has succeeded.
    pub research: Option<ResearchInfo>,
}

/// A validated research outcome, ready to replace the topic's current set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchOutcome {
    pub topic_id: TopicId,
    pub info: ResearchInfo,
    pub communities_skipped: bool,
    /// Each with the id it will be stored under, in order.
    pub resources: Vec<(ResourceId, ResourceDraft)>,
    pub gaps: Vec<Gap>,
}

fn after_scheme(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_at_checked("https://".len())?;
    scheme.eq_ignore_ascii_case("https://").then_some(rest)
}

/// Host of an `https` URL, lowercase and without a leading `www.`. `None` for
/// anything that is not a plain `https://host[:port]/...` address: other
/// schemes, credentials, whitespace, IP literals and hosts without a dot.
#[must_use]
pub fn https_host(url: &str) -> Option<String> {
    let rest = after_scheme(url)?;
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.contains('@') {
        return None;
    }
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        Some(_) => return None,
        None => authority,
    };
    let host = host.to_lowercase();
    let valid = host.contains('.')
        && !host.starts_with(['.', '-'])
        && !host.ends_with(['.', '-'])
        && !host.contains("..")
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !valid {
        return None;
    }
    Some(host.strip_prefix("www.").unwrap_or(&host).to_owned())
}

/// Canonical form used to compare URLs: lowercase scheme and host, no
/// `#fragment`, no trailing `/` (path included). `None` when the text is not a
/// valid `https` URL (see [`https_host`]).
#[must_use]
pub fn normalize_url(url: &str) -> Option<String> {
    let url = url.trim();
    https_host(url)?;
    let without_fragment = url.split('#').next().unwrap_or(url);
    let rest = after_scheme(without_fragment)?;
    let (authority, tail) = match rest.find(['/', '?']) {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let tail = tail.trim_end_matches('/');
    Some(format!("https://{}{tail}", authority.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_is_lowercase_without_www_and_port() {
        assert_eq!(
            https_host("https://WWW.Example.org:8443/a?b#c").as_deref(),
            Some("example.org")
        );
        assert_eq!(
            https_host("https://doc.rust-lang.org/book/").as_deref(),
            Some("doc.rust-lang.org")
        );
    }

    #[test]
    fn unusable_addresses_have_no_host() {
        for bad in [
            "http://example.org",
            "ftp://example.org",
            "https://",
            "https://localhost/",
            "https://user:pw@example.org/",
            "https://exa mple.org/",
            "https://[::1]/",
            "https://example..org/",
            "https://example.org:abc/",
            "example.org",
        ] {
            assert_eq!(https_host(bad), None, "{bad}");
        }
    }

    #[test]
    fn normalization_ignores_case_fragment_and_trailing_slash() {
        let a = normalize_url("HTTPS://Doc.Rust-Lang.org/book/#intro").unwrap();
        let b = normalize_url("https://doc.rust-lang.org/book").unwrap();
        assert_eq!(a, b);
        assert_eq!(
            normalize_url("https://example.org/").unwrap(),
            "https://example.org"
        );
        // Path case and query matter.
        assert_ne!(
            normalize_url("https://example.org/A").unwrap(),
            normalize_url("https://example.org/a").unwrap()
        );
        assert_ne!(
            normalize_url("https://example.org/a?x=1").unwrap(),
            normalize_url("https://example.org/a").unwrap()
        );
        assert_eq!(normalize_url("http://example.org"), None);
    }

    #[test]
    fn kind_and_type_rules() {
        assert!(SourceType::Forum.community_only());
        assert!(SourceType::Book.knowledge_only());
        assert!(!SourceType::Other.community_only() && !SourceType::Other.knowledge_only());
        assert!(SourceType::LocalGroup.may_lack_url() && !SourceType::Forum.may_lack_url());
        for s in ["book", "social_group", "other", "live_class"] {
            assert_eq!(s.parse::<SourceType>().unwrap().as_str(), s);
        }
        assert!("blog".parse::<SourceType>().is_err());
    }
}
