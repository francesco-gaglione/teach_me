//! PDF text extraction with `PDFium` (the engine of Chromium) through
//! `pdfium-render`. The engine is a shared library found at run time, so the
//! binary builds and starts without it (see `docs/adr/0007-pdf-extraction.md`).

use std::path::Path;

use domain::{ExtractedPdf, OutlineEntry, PdfError, PdfTextExtractor};
use pdfium_render::prelude::*;

/// Bookmarks read at most, whatever the file says: a corrupt or hostile
/// outline can loop or be enormous.
const MAX_OUTLINE_ENTRIES: usize = 5_000;
/// Sections use bookmarks down to this level (`document::MAX_OUTLINE_LEVEL`).
const MAX_OUTLINE_DEPTH: u8 = 3;
/// The progress callback is called every this many pages.
const PROGRESS_EVERY: u32 = 20;

pub struct PdfiumTextExtractor {
    pdfium: Pdfium,
}

impl PdfiumTextExtractor {
    /// Binds `PDFium`, looking for `libpdfium.so` (`pdfium.dll`, `libpdfium.dylib`)
    /// in `library_dir` first, then in the system's library path. `PDFium` can be
    /// bound once per process, so create a single extractor.
    pub fn bind(library_dir: Option<&Path>) -> Result<Self, PdfError> {
        let bindings = match library_dir {
            Some(dir) => Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(dir))
                .or_else(|_| Pdfium::bind_to_system_library()),
            None => Pdfium::bind_to_system_library(),
        }
        .map_err(|e| PdfError::Unavailable(e.to_string()))?;
        Ok(Self {
            pdfium: Pdfium::new(bindings),
        })
    }
}

fn open_error(err: &PdfiumError) -> PdfError {
    match err {
        PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::PasswordError | PdfiumInternalError::SecurityError,
        ) => PdfError::Encrypted,
        PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::FormatError | PdfiumInternalError::FileError,
        ) => PdfError::Invalid,
        other => PdfError::Internal(format!("{other:?}")),
    }
}

/// `visited` counts every node looked at, titled or not, so a cyclic or huge
/// outline ends after `MAX_OUTLINE_ENTRIES` steps whatever it contains.
fn walk_outline(
    first: Option<PdfBookmark<'_>>,
    level: u8,
    visited: &mut usize,
    out: &mut Vec<OutlineEntry>,
) {
    let mut current = first;
    while let Some(bookmark) = current {
        if *visited >= MAX_OUTLINE_ENTRIES {
            return;
        }
        *visited += 1;
        let page = bookmark
            .destination()
            .and_then(|d| d.page_index().ok())
            .and_then(|p| u32::try_from(p).ok())
            .map(|p| p + 1);
        if let (Some(title), Some(page)) = (bookmark.title(), page) {
            out.push(OutlineEntry { title, level, page });
        }
        if level < MAX_OUTLINE_DEPTH {
            walk_outline(bookmark.first_child(), level + 1, visited, out);
        }
        current = bookmark.next_sibling();
    }
}

impl PdfTextExtractor for PdfiumTextExtractor {
    fn extract(
        &self,
        path: &Path,
        max_pages: u32,
        on_page: &mut dyn FnMut(u32, u32),
    ) -> Result<ExtractedPdf, PdfError> {
        let document = self
            .pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| open_error(&e))?;
        let total = u32::try_from(document.pages().len()).unwrap_or(u32::MAX);
        if total == 0 {
            return Err(PdfError::Invalid);
        }
        if total > max_pages {
            return Err(PdfError::TooManyPages(total));
        }
        let title = document
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|tag| tag.value().to_owned());

        let mut pages = Vec::with_capacity(total as usize);
        for (i, page) in document.pages().iter().enumerate() {
            // A page whose text cannot be read counts as a page without text.
            let text = page.text().map_or_default(|t| t.all());
            pages.push(text);
            let done = u32::try_from(i + 1).unwrap_or(total);
            if done % PROGRESS_EVERY == 0 || done == total {
                on_page(done, total);
            }
        }

        let mut outline = Vec::new();
        walk_outline(document.bookmarks().root(), 1, &mut 0, &mut outline);
        Ok(ExtractedPdf {
            title,
            pages,
            outline,
        })
    }
}
