//! Uploaded PDFs (`docs/contracts/documents.md`): the entity, the rules that
//! turn extracted text into sections, and the ports the import needs.
//!
//! Nothing here reads a file or knows a PDF library: the text comes from a
//! [`PdfTextExtractor`] and the rules below are pure functions of it.

use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::ids::{DocumentId, ResourceId, TopicId};
use crate::job::{JobError, JobErrorCode};
use crate::resource::ResourceDraft;

pub const MAX_DOCUMENTS_PER_TOPIC: usize = 5;
/// Largest upload accepted (`documents.md` §3).
pub const MAX_UPLOAD_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_PAGES: u32 = 1_500;
/// Below this many characters per page (on average) a PDF is treated as a scan.
pub const MIN_AVG_CHARS_PER_PAGE: usize = 200;
/// A page with fewer non-blank characters than this counts as "without text":
/// a scan with only a printed page number still has no usable text.
pub const BLANK_PAGE_CHARS: usize = 20;
pub const MAX_SECTIONS: usize = 300;
/// Sections longer than this are split into parts.
pub const MAX_SECTION_PAGES: u32 = 30;
pub const PAGE_BLOCK: u32 = 10;
/// Deepest bookmark level that becomes a section.
pub const MAX_OUTLINE_LEVEL: u8 = 3;
pub const FILENAME_MAX_CHARS: usize = 255;
pub const TITLE_MAX_CHARS: usize = 300;
pub const SECTION_TITLE_MAX_CHARS: usize = 200;
/// Text of the sections handed to the Generator, per lesson.
pub const EXCERPT_MAX_CHARS: usize = 24_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown document status `{0}`")]
pub struct UnknownDocumentStatus(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentStatus {
    Importing,
    Ready,
    Failed,
}

impl DocumentStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Importing => "importing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for DocumentStatus {
    type Err = UnknownDocumentStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "importing" => Ok(Self::Importing),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            other => Err(UnknownDocumentStatus(other.to_owned())),
        }
    }
}

impl fmt::Display for DocumentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub id: DocumentId,
    pub topic_id: TopicId,
    /// The `document` resource created when the import succeeds.
    pub resource_id: Option<ResourceId>,
    pub filename: String,
    pub title: String,
    pub status: DocumentStatus,
    pub size_bytes: u64,
    pub page_count: Option<u32>,
    pub language: Option<String>,
    /// Why the import failed; `Some` only with `status: failed`.
    pub error: Option<JobError>,
    pub created_at: OffsetDateTime,
}

impl Document {
    /// A document just uploaded: its title is the file name until the import
    /// reads the PDF's own.
    #[must_use]
    pub fn importing(
        id: DocumentId,
        topic_id: TopicId,
        filename: String,
        size_bytes: u64,
        now: OffsetDateTime,
    ) -> Self {
        let title = title_from_filename(&filename);
        Self {
            id,
            topic_id,
            resource_id: None,
            filename,
            title,
            status: DocumentStatus::Importing,
            size_bytes,
            page_count: None,
            language: None,
            error: None,
            created_at: now,
        }
    }
}

/// A part of a document the Planner can choose. Pages are numbered from 1 as
/// in the file, not by the printed labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSection {
    /// `s1`, `s2`…, stable within the document.
    pub id: String,
    pub title: String,
    pub level: u8,
    pub page_start: u32,
    pub page_end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentView {
    pub document: Document,
    /// Empty until the document is `ready`.
    pub sections: Vec<DocumentSection>,
}

/// Everything the import produced, stored in one transaction together with
/// the `document` resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedDocument {
    pub document_id: DocumentId,
    pub title: String,
    pub language: Option<String>,
    /// Text of each page, in order (page 1 first).
    pub pages: Vec<String>,
    pub sections: Vec<DocumentSection>,
    pub resource_id: ResourceId,
    pub resource: ResourceDraft,
}

/// A bookmark with its destination resolved to a page (from 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    pub title: String,
    /// 1 for top-level bookmarks.
    pub level: u8,
    pub page: u32,
}

/// What an extractor read from a PDF.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtractedPdf {
    /// The `Title` of the PDF's metadata, as found (not yet checked).
    pub title: Option<String>,
    pub pages: Vec<String>,
    /// In document order, every level.
    pub outline: Vec<OutlineEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("not a valid PDF")]
    Invalid,
    #[error("the PDF is protected by a password")]
    Encrypted,
    #[error("the PDF has {0} pages")]
    TooManyPages(u32),
    /// The extraction engine is not installed or could not start.
    #[error("PDF engine unavailable: {0}")]
    Unavailable(String),
    #[error("cannot read the PDF: {0}")]
    Internal(String),
}

impl PdfError {
    /// The job error code this failure maps to.
    #[must_use]
    pub const fn job_code(&self) -> JobErrorCode {
        match self {
            Self::Invalid => JobErrorCode::InvalidPdf,
            Self::Encrypted => JobErrorCode::EncryptedPdf,
            Self::TooManyPages(_) => JobErrorCode::TooManyPages,
            Self::Unavailable(_) | Self::Internal(_) => JobErrorCode::Internal,
        }
    }
}

/// Reads the text of a PDF page by page, plus its bookmarks. Blocking and
/// CPU-bound: callers run it off the async runtime.
pub trait PdfTextExtractor: Send + Sync {
    /// `on_page(done, total)` is called as pages are read. Fails with
    /// [`PdfError::TooManyPages`] before reading any text when the document
    /// has more than `max_pages`.
    fn extract(
        &self,
        path: &Path,
        max_pages: u32,
        on_page: &mut dyn FnMut(u32, u32),
    ) -> Result<ExtractedPdf, PdfError>;
}

/// Where uploaded files wait for their import.
#[async_trait]
pub trait DocumentFiles: Send + Sync {
    /// Final location of the upload of `id`.
    fn path(&self, id: DocumentId) -> PathBuf;
    /// Location the upload is streamed to before it is complete.
    fn partial_path(&self, id: DocumentId) -> PathBuf;
    /// Deletes the upload (and any partial one); a missing file is not an error.
    async fn remove(&self, id: DocumentId);
}

// ----- rules -----

/// The name of an uploaded file reduced to a plain file name: no directories
/// (either separator), no control characters, within the length limit.
/// `None` when nothing usable is left.
#[must_use]
pub fn clean_filename(raw: &str) -> Option<String> {
    let name = raw.rsplit(['/', '\\']).next().unwrap_or(raw);
    let name: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let name = name.trim_matches('.').trim();
    if name.is_empty() {
        return None;
    }
    Some(name.chars().take(FILENAME_MAX_CHARS).collect())
}

/// File name without the extension, as a title.
#[must_use]
pub fn title_from_filename(filename: &str) -> String {
    let stem = match filename.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && ext.len() <= 5 => stem,
        _ => filename,
    };
    let stem = stem.trim();
    let stem = if stem.is_empty() { filename } else { stem };
    stem.chars().take(TITLE_MAX_CHARS).collect()
}

/// The PDF's own title when it looks like one; generators often leave the
/// name of the source file or a placeholder there.
#[must_use]
pub fn plausible_title(meta: Option<&str>) -> Option<String> {
    let title: String = meta?
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let lower = title.to_lowercase();
    let junk = lower.chars().count() < 3
        || ["untitled", "senza titolo", "microsoft word", "document1"]
            .iter()
            .any(|j| lower.starts_with(j))
        || [".pdf", ".tex", ".doc", ".docx", ".dvi", ".indd", ".odt"]
            .iter()
            .any(|e| lower.ends_with(e));
    (!junk).then(|| title.chars().take(TITLE_MAX_CHARS).collect())
}

fn text_chars(page: &str) -> usize {
    page.chars().filter(|c| !c.is_whitespace()).count()
}

/// `Err(NoText)` for a scan or nearly: fewer than 200 characters per page on
/// average, or more than half of the pages without text (`documents.md` §4).
pub fn check_has_text(pages: &[String]) -> Result<(), JobErrorCode> {
    if pages.is_empty() {
        return Err(JobErrorCode::NoText);
    }
    let counts: Vec<usize> = pages.iter().map(|p| text_chars(p)).collect();
    let total: usize = counts.iter().sum();
    let blank = counts.iter().filter(|c| **c < BLANK_PAGE_CHARS).count();
    if total / pages.len() < MIN_AVG_CHARS_PER_PAGE || blank * 2 > pages.len() {
        return Err(JobErrorCode::NoText);
    }
    Ok(())
}

fn cut_title(title: &str, max: usize) -> String {
    title
        .chars()
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn block_title(first: u32, last: u32) -> String {
    if first == last {
        format!("Pagina {first}")
    } else {
        format!("Pagine {first}-{last}")
    }
}

/// Sections from bookmarks down to level 3, or blocks of 10 pages without
/// usable bookmarks. A section spans from its page to the page before the next
/// bookmark of the same or a higher level, so a chapter contains its
/// subsections; one longer than 30 pages is split into parts. At most 300.
#[must_use]
pub fn build_sections(outline: &[OutlineEntry], page_count: u32) -> Vec<DocumentSection> {
    if page_count == 0 {
        return Vec::new();
    }
    for max_level in (1..=MAX_OUTLINE_LEVEL).rev() {
        let sections = sections_from_outline(outline, page_count, max_level);
        if !sections.is_empty() && sections.len() <= MAX_SECTIONS {
            return number(sections);
        }
        if sections.is_empty() {
            break;
        }
    }
    number(page_blocks(page_count))
}

fn number(mut sections: Vec<DocumentSection>) -> Vec<DocumentSection> {
    for (i, s) in sections.iter_mut().enumerate() {
        s.id = format!("s{}", i + 1);
    }
    sections
}

fn page_blocks(page_count: u32) -> Vec<DocumentSection> {
    (1..=page_count)
        .step_by(PAGE_BLOCK as usize)
        .map(|first| {
            let last = (first + PAGE_BLOCK - 1).min(page_count);
            DocumentSection {
                id: String::new(),
                title: block_title(first, last),
                level: 1,
                page_start: first,
                page_end: last,
            }
        })
        .collect()
}

fn sections_from_outline(
    outline: &[OutlineEntry],
    page_count: u32,
    max_level: u8,
) -> Vec<DocumentSection> {
    let mut entries: Vec<&OutlineEntry> = outline
        .iter()
        .filter(|e| {
            (1..=max_level).contains(&e.level)
                && (1..=page_count).contains(&e.page)
                && !e.title.trim().is_empty()
        })
        .collect();
    // Bookmarks can point backwards; sections are in page order.
    entries.sort_by_key(|e| e.page);
    let mut out = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let next_start = entries[i + 1..]
            .iter()
            .find(|n| n.level <= entry.level)
            .map_or(page_count + 1, |n| n.page);
        let end = next_start.saturating_sub(1).max(entry.page);
        let title = cut_title(&entry.title, SECTION_TITLE_MAX_CHARS - 12);
        let pages = end - entry.page + 1;
        if pages <= MAX_SECTION_PAGES {
            out.push(DocumentSection {
                id: String::new(),
                title,
                level: entry.level,
                page_start: entry.page,
                page_end: end,
            });
            continue;
        }
        let parts = pages.div_ceil(MAX_SECTION_PAGES);
        for part in 0..parts {
            let start = entry.page + part * MAX_SECTION_PAGES;
            out.push(DocumentSection {
                id: String::new(),
                title: format!("{title}, parte {}", part + 1),
                level: entry.level,
                page_start: start,
                page_end: (start + MAX_SECTION_PAGES - 1).min(end),
            });
        }
    }
    out
}

/// Stopwords of the languages a learner is likely to upload.
const STOPWORDS: &[(&str, &[&str])] = &[
    (
        "en",
        &[
            "the", "and", "of", "to", "is", "that", "in", "it", "for", "with", "as", "this", "are",
            "be",
        ],
    ),
    (
        "it",
        &[
            "il", "di", "che", "e", "la", "per", "un", "una", "non", "con", "sono", "del", "le",
            "della",
        ],
    ),
    (
        "es",
        &[
            "el", "de", "que", "y", "la", "en", "los", "las", "un", "una", "por", "con", "del",
            "es",
        ],
    ),
    (
        "fr",
        &[
            "le", "de", "et", "les", "des", "est", "que", "pour", "une", "dans", "qui", "pas",
            "sur", "du",
        ],
    ),
    (
        "de",
        &[
            "der", "die", "und", "das", "ist", "nicht", "ein", "eine", "mit", "den", "zu", "von",
            "auf", "für",
        ],
    ),
];

/// BCP-47 code of the language of `text`, estimated from stopword frequency;
/// `None` when it is not clear (too short, or two languages close together).
#[must_use]
pub fn guess_language(text: &str) -> Option<String> {
    let mut counts = vec![0usize; STOPWORDS.len()];
    let mut words = 0usize;
    for word in text
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .take(20_000)
    {
        words += 1;
        let lower = word.to_lowercase();
        for (i, (_, list)) in STOPWORDS.iter().enumerate() {
            if list.contains(&lower.as_str()) {
                counts[i] += 1;
            }
        }
    }
    if words < 200 {
        return None;
    }
    let (best, &top) = counts.iter().enumerate().max_by_key(|(_, c)| **c)?;
    let second = counts
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != best)
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(0);
    // At least 5% of the words are stopwords of the language, and clearly
    // more than of any other.
    (top * 20 >= words && top * 2 > second * 3).then(|| STOPWORDS[best].0.to_owned())
}

/// The fixed texts of the `document` resource (`resources.md`: annotation and
/// `use_for` are written by the backend).
#[must_use]
pub fn resource_draft(title: &str, page_count: u32, language: Option<&str>) -> ResourceDraft {
    use crate::resource::{ResourceKind, SourceType};
    ResourceDraft {
        kind: ResourceKind::Document,
        source_type: SourceType::Pdf,
        title: cut_title(title, crate::resource::TITLE_MAX_CHARS),
        url: None,
        language: language.unwrap_or("und").to_owned(),
        annotation: format!("PDF caricato da te, {page_count} pagine."),
        use_for: "seguire il tuo PDF, citandone le pagine".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, level: u8, page: u32) -> OutlineEntry {
        OutlineEntry {
            title: title.into(),
            level,
            page,
        }
    }

    #[test]
    fn filenames_lose_directories_and_control_characters() {
        assert_eq!(
            clean_filename("C:\\Users\\me\\book.pdf").as_deref(),
            Some("book.pdf")
        );
        assert_eq!(
            clean_filename("../../etc/pass\nwd.pdf").as_deref(),
            Some("pass wd.pdf")
        );
        assert_eq!(clean_filename("..").as_deref(), None);
        assert_eq!(clean_filename("  ").as_deref(), None);
        assert_eq!(clean_filename(&"a".repeat(400)).unwrap().len(), 255);
    }

    #[test]
    fn titles_come_from_the_file_name_when_the_metadata_is_junk() {
        assert_eq!(title_from_filename("rust-in-action.pdf"), "rust-in-action");
        assert_eq!(title_from_filename("noext"), "noext");
        assert_eq!(
            plausible_title(Some("Rust  in Action")).unwrap(),
            "Rust in Action"
        );
        for junk in [
            "",
            "ab",
            "Untitled",
            "Microsoft Word - cap1.doc",
            "book.tex",
        ] {
            assert_eq!(plausible_title(Some(junk)), None, "{junk}");
        }
        assert_eq!(plausible_title(None), None);
    }

    #[test]
    fn scans_are_recognised_by_average_and_by_blank_pages() {
        let rich = "x".repeat(500);
        assert!(check_has_text(&vec![rich.clone(); 4]).is_ok());
        assert_eq!(check_has_text(&[]), Err(JobErrorCode::NoText));
        // Average under 200.
        assert_eq!(
            check_has_text(&vec!["x".repeat(150); 4]),
            Err(JobErrorCode::NoText)
        );
        // Good average, but more than half the pages are blank.
        let mut pages = vec![String::new(); 3];
        pages.push("x".repeat(5000));
        assert_eq!(check_has_text(&pages), Err(JobErrorCode::NoText));
        // Exactly half blank is accepted.
        let mut pages = vec![String::from("  "); 2];
        pages.extend(vec![rich; 2]);
        assert!(check_has_text(&pages).is_ok());
    }

    #[test]
    fn without_bookmarks_pages_are_split_in_blocks_of_ten() {
        let s = build_sections(&[], 23);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].title, "Pagine 1-10");
        assert_eq!((s[2].page_start, s[2].page_end), (21, 23));
        assert_eq!(s[2].title, "Pagine 21-23");
        assert_eq!(s[1].id, "s2");
        assert_eq!(build_sections(&[], 11)[1].title, "Pagina 11");
        assert_eq!(build_sections(&[], 0), vec![]);
    }

    #[test]
    fn a_chapter_spans_its_subsections() {
        let outline = [
            entry("1 Intro", 1, 1),
            entry("1.1 Why", 2, 3),
            entry("1.2 How", 2, 6),
            entry("2 Basics", 1, 9),
        ];
        let s = build_sections(&outline, 20);
        let spans: Vec<_> = s
            .iter()
            .map(|x| (x.title.as_str(), x.level, x.page_start, x.page_end))
            .collect();
        assert_eq!(
            spans,
            vec![
                ("1 Intro", 1, 1, 8),
                ("1.1 Why", 2, 3, 5),
                ("1.2 How", 2, 6, 8),
                ("2 Basics", 1, 9, 20),
            ]
        );
    }

    #[test]
    fn levels_past_three_and_pages_outside_the_file_are_ignored() {
        let outline = [
            entry("A", 1, 1),
            entry("deep", 4, 2),
            entry("gone", 1, 99),
            entry("blank", 1, 5).clone(),
        ];
        let s = build_sections(&outline, 10);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].page_start, s[0].page_end), (1, 4));
        assert_eq!((s[1].title.as_str(), s[1].page_end), ("blank", 10));
    }

    #[test]
    fn long_sections_are_split_in_parts_of_thirty_pages() {
        let s = build_sections(&[entry("Big", 1, 1)], 70);
        let spans: Vec<_> = s
            .iter()
            .map(|x| (x.title.as_str(), x.page_start, x.page_end))
            .collect();
        assert_eq!(
            spans,
            vec![
                ("Big, parte 1", 1, 30),
                ("Big, parte 2", 31, 60),
                ("Big, parte 3", 61, 70)
            ]
        );
    }

    #[test]
    fn too_many_sections_drop_the_deepest_level_then_fall_back_to_blocks() {
        let mut outline = vec![entry("c", 1, 1), entry("d", 1, 2)];
        outline.extend((0..400).map(|i| entry("sub", 3, 3 + i % 50)));
        let s = build_sections(&outline, 100);
        assert!(s.len() <= MAX_SECTIONS && s.iter().all(|x| x.level <= 2));
        let flat: Vec<_> = (0..400).map(|i| entry("x", 1, 1 + i % 100)).collect();
        let s = build_sections(&flat, 100);
        assert_eq!(s[0].title, "Pagine 1-10");
    }

    #[test]
    fn language_is_guessed_from_stopwords_or_left_unknown() {
        let en =
            "the cat is in the house and it is for the people with a dog that is big ".repeat(20);
        assert_eq!(guess_language(&en).as_deref(), Some("en"));
        let it = "il gatto non è che la casa di un cane per una persona con il del ".repeat(20);
        assert_eq!(guess_language(&it).as_deref(), Some("it"));
        assert_eq!(guess_language("troppo corto"), None);
        assert_eq!(guess_language(&"zzz qqq ".repeat(300)), None);
    }

    #[test]
    fn the_document_resource_has_fixed_texts() {
        let d = resource_draft("Rust in Action", 456, Some("en"));
        assert_eq!(d.annotation, "PDF caricato da te, 456 pagine.");
        assert!(d.url.is_none());
        assert_eq!(resource_draft("t", 1, None).language, "und");
    }
}
