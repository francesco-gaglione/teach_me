#![allow(
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::format_push_string,
    clippy::assert_is_empty
)] // a throwaway PDF writer for tests
//! The PDFium adapter on real (small, generated) PDFs. PDFium is a shared
//! library found at run time: a machine without it (`TEACH_ME_PDFIUM_DIR`
//! unset and no system copy) skips these tests with a message instead of
//! failing, so `cargo test` works everywhere; the Docker image has the library.
//! A skip looks green, so CI sets `TEACH_ME_REQUIRE_PDFIUM=1`: with it a missing
//! library fails the tests instead.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use domain::{PdfError, PdfTextExtractor};
use infrastructure::PdfiumTextExtractor;

/// PDFium binds once per process: one extractor for all the tests.
fn engine() -> Option<&'static PdfiumTextExtractor> {
    static ENGINE: OnceLock<Option<PdfiumTextExtractor>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let dir = std::env::var("TEACH_ME_PDFIUM_DIR").ok().map(PathBuf::from);
            match PdfiumTextExtractor::bind(dir.as_deref()) {
                Ok(e) => Some(e),
                Err(e) => {
                    assert!(
                        std::env::var_os("TEACH_ME_REQUIRE_PDFIUM")
                            .is_none_or(|v| v.is_empty() || v == "0"),
                        "TEACH_ME_REQUIRE_PDFIUM is set but PDFium is not available \
                         (TEACH_ME_PDFIUM_DIR={:?}): {e}",
                        std::env::var("TEACH_ME_PDFIUM_DIR").ok()
                    );
                    eprintln!("skipping PDF tests, no PDFium: {e}");
                    None
                }
            }
        })
        .as_ref()
}

struct Outline {
    title: &'static str,
    /// 0-based page.
    page: usize,
    children: Vec<Outline>,
}

/// A PDF with one line of text per page (`Pagina N ...`), optional bookmarks
/// and an optional Title. Offsets in the xref table are exact.
fn build_pdf(pages: &[String], outline: &[Outline], title: Option<&str>) -> Vec<u8> {
    let mut objects: Vec<String> = Vec::new();
    // 1 catalog, 2 pages, 3 font, 4 info; then pages and contents; then outlines.
    let n = pages.len();
    let page_obj = |i: usize| 5 + i * 2;
    let content_obj = |i: usize| 6 + i * 2;
    let outline_root = 5 + n * 2;

    // Flatten the outline tree into numbered items (root is `outline_root`).
    struct Item<'a> {
        title: &'a str,
        page: usize,
        parent: usize,
        prev: Option<usize>,
        next: Option<usize>,
        first: Option<usize>,
        last: Option<usize>,
        count: usize,
    }
    let mut items: Vec<Item<'_>> = Vec::new();
    fn add<'a>(
        list: &'a [Outline],
        parent: usize,
        items: &mut Vec<Item<'a>>,
        base: usize,
    ) -> Vec<usize> {
        let mut ids = Vec::new();
        for o in list {
            let id = base + 1 + items.len();
            items.push(Item {
                title: o.title,
                page: o.page,
                parent,
                prev: None,
                next: None,
                first: None,
                last: None,
                count: 0,
            });
            let kids = add(&o.children, id, items, base);
            let idx = id - base - 1;
            items[idx].first = kids.first().copied();
            items[idx].last = kids.last().copied();
            items[idx].count = kids.len();
            ids.push(id);
        }
        for w in ids.windows(2) {
            items[w[0] - base - 1].next = Some(w[1]);
            items[w[1] - base - 1].prev = Some(w[0]);
        }
        ids
    }
    let top = add(outline, outline_root, &mut items, outline_root);

    objects.push(if outline.is_empty() {
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned()
    } else {
        format!("<< /Type /Catalog /Pages 2 0 R /Outlines {outline_root} 0 R >>")
    });
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", page_obj(i))).collect();
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {n} >>",
        kids.join(" ")
    ));
    objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned());
    objects.push(match title {
        Some(t) => format!("<< /Title ({t}) >>"),
        None => "<< >>".to_owned(),
    });
    for (i, text) in pages.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            content_obj(i)
        ));
        let stream = if text.is_empty() {
            String::new()
        } else {
            format!("BT /F1 10 Tf 40 750 Td ({text}) Tj ET")
        };
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}\nendstream",
            stream.len()
        ));
    }
    if !outline.is_empty() {
        objects.push(format!(
            "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
            top[0],
            top[top.len() - 1],
            items.len()
        ));
        for it in &items {
            let mut d = format!(
                "<< /Title ({}) /Parent {} 0 R /Dest [{} 0 R /Fit]",
                it.title,
                it.parent,
                page_obj(it.page)
            );
            for (k, v) in [
                ("Prev", it.prev),
                ("Next", it.next),
                ("First", it.first),
                ("Last", it.last),
            ] {
                if let Some(v) = v {
                    d.push_str(&format!(" /{k} {v} 0 R"));
                }
            }
            if it.count > 0 {
                d.push_str(&format!(" /Count {}", it.count));
            }
            d.push_str(" >>");
            objects.push(d);
        }
    }

    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    let info = if title.is_some() { " /Info 4 0 R" } else { "" };
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R{info} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write(name: &str, bytes: &[u8]) -> (PathBuf, Cleanup) {
    let path = std::env::temp_dir().join(format!("teach_me_pdf_{}_{name}", uuid::Uuid::now_v7()));
    std::fs::write(&path, bytes).unwrap();
    (path.clone(), Cleanup(path))
}

fn pages(n: usize) -> Vec<String> {
    (1..=n)
        .map(|p| format!("Pagina {p} del libro di prova"))
        .collect()
}

fn read(engine: &PdfiumTextExtractor, path: &Path) -> Result<domain::ExtractedPdf, PdfError> {
    engine.extract(path, 1500, &mut |_, _| {})
}

#[test]
fn text_is_read_page_by_page_with_title_and_progress() {
    let Some(engine) = engine() else { return };
    let (path, _guard) = write(
        "text.pdf",
        &build_pdf(&pages(45), &[], Some("Rust in Action")),
    );
    let mut ticks = Vec::new();
    let pdf = engine
        .extract(&path, 1500, &mut |done, total| ticks.push((done, total)))
        .unwrap();
    assert_eq!(pdf.pages.len(), 45);
    assert!(pdf.pages[0].contains("Pagina 1 del libro di prova"));
    assert!(pdf.pages[44].contains("Pagina 45 del libro"));
    assert_eq!(pdf.title.as_deref(), Some("Rust in Action"));
    assert!(pdf.outline.is_empty());
    assert_eq!(ticks, [(20, 45), (40, 45), (45, 45)]);
}

#[test]
fn bookmarks_come_with_their_level_and_destination_page() {
    let Some(engine) = engine() else { return };
    let outline = vec![
        Outline {
            title: "Capitolo 1",
            page: 0,
            children: vec![
                Outline {
                    title: "1.1 Perche",
                    page: 2,
                    children: vec![],
                },
                Outline {
                    title: "1.2 Come",
                    page: 4,
                    children: vec![Outline {
                        title: "1.2.1 Dettaglio",
                        page: 5,
                        children: vec![],
                    }],
                },
            ],
        },
        Outline {
            title: "Capitolo 2",
            page: 7,
            children: vec![],
        },
    ];
    let (path, _guard) = write("outline.pdf", &build_pdf(&pages(10), &outline, None));
    let pdf = read(engine, &path).unwrap();
    let got: Vec<_> = pdf
        .outline
        .iter()
        .map(|o| (o.title.as_str(), o.level, o.page))
        .collect();
    assert_eq!(
        got,
        [
            ("Capitolo 1", 1, 1),
            ("1.1 Perche", 2, 3),
            ("1.2 Come", 2, 5),
            ("1.2.1 Dettaglio", 3, 6),
            ("Capitolo 2", 1, 8),
        ]
    );
    // The domain turns them into sections.
    let sections = domain::document::build_sections(&pdf.outline, 10);
    assert_eq!(sections.len(), 5);
    assert_eq!((sections[0].page_start, sections[0].page_end), (1, 7));
    assert_eq!((sections[4].page_start, sections[4].page_end), (8, 10));
}

/// A committed PDF made by another writer than `build_pdf` (regenerate it with
/// `scripts/gen-fixture-pdf.py`): heading, body text, accents, bookmarks.
#[test]
fn a_real_structure_pdf_is_extracted_with_text_accents_and_bookmarks() {
    let Some(engine) = engine() else { return };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
    let pdf = read(engine, &path).unwrap();
    assert_eq!(pdf.pages.len(), 3);
    assert!(pdf.pages[0].contains("Capitolo 1: Introduzione"));
    assert!(pdf.pages[0].contains("Perché testare con un PDF vero?"));
    assert!(pdf.pages[1].contains("La città è più grande del paese."));
    assert!(pdf.pages[2].contains("Fine del libro di prova."));
    let got: Vec<_> = pdf
        .outline
        .iter()
        .map(|o| (o.title.as_str(), o.level, o.page))
        .collect();
    assert_eq!(
        got,
        [
            ("Capitolo 1: Introduzione", 1, 1),
            ("Capitolo 2: Ownership", 1, 2),
            ("Capitolo 3: Conclusione", 1, 3),
        ]
    );
}

#[test]
fn a_password_protected_pdf_is_reported_as_encrypted() {
    let Some(engine) = engine() else { return };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/encrypted.pdf");
    assert!(matches!(read(engine, &path), Err(PdfError::Encrypted)));
}

#[test]
fn garbage_and_missing_files_are_invalid_not_a_crash() {
    let Some(engine) = engine() else { return };
    let (path, _guard) = write("junk.pdf", b"%PDF-1.4\nthis is not really a pdf");
    assert!(matches!(read(engine, &path), Err(PdfError::Invalid)));
    let (empty, _guard2) = write("empty.pdf", b"");
    assert!(matches!(read(engine, &empty), Err(PdfError::Invalid)));
    // A file that vanished is our problem, not the user's: an internal error.
    let gone = std::env::temp_dir().join("teach_me_does_not_exist.pdf");
    assert!(matches!(read(engine, &gone), Err(PdfError::Internal(_))));
}

#[test]
fn too_many_pages_are_refused_before_reading_text() {
    let Some(engine) = engine() else { return };
    let (path, _guard) = write("long.pdf", &build_pdf(&pages(30), &[], None));
    let mut ticks = 0;
    let err = engine
        .extract(&path, 20, &mut |_, _| ticks += 1)
        .unwrap_err();
    assert!(matches!(err, PdfError::TooManyPages(30)));
    assert_eq!(ticks, 0);
}

#[test]
fn a_pdf_without_text_reads_as_empty_pages() {
    let Some(engine) = engine() else { return };
    let blank: Vec<String> = vec![String::new(); 12];
    let (path, _guard) = write("scan.pdf", &build_pdf(&blank, &[], None));
    let pdf = read(engine, &path).unwrap();
    assert_eq!(pdf.pages.len(), 12);
    assert!(pdf.pages.iter().all(|p| p.trim().is_empty()));
    assert_eq!(
        domain::document::check_has_text(&pdf.pages),
        Err(domain::JobErrorCode::NoText)
    );
}

/// Manual check on a real book, to run before a release (CI never has one):
/// `TEACH_ME_REQUIRE_PDFIUM=1 TEACH_ME_PDFIUM_DIR=… TEACH_ME_TEST_PDF=book.pdf
/// cargo test -p infrastructure --test pdf -- --ignored --nocapture`.
/// Read the report: pages, text volume, sections and timing must look sane.
#[test]
#[ignore = "needs a real PDF in TEACH_ME_TEST_PDF"]
fn real_pdf_report() {
    let Some(engine) = engine() else { return };
    let Ok(path) = std::env::var("TEACH_ME_TEST_PDF") else {
        eprintln!("set TEACH_ME_TEST_PDF");
        return;
    };
    let started = std::time::Instant::now();
    let pdf = read(engine, Path::new(&path)).unwrap();
    let read_time = started.elapsed();
    let chars: usize = pdf.pages.iter().map(|p| p.chars().count()).sum();
    let sections =
        domain::document::build_sections(&pdf.outline, u32::try_from(pdf.pages.len()).unwrap());
    let sample: String = pdf
        .pages
        .iter()
        .take(60)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    eprintln!(
        "pages={} chars={} read_time={read_time:?} title={:?} bookmarks={} sections={} language={:?} has_text={:?}",
        pdf.pages.len(),
        chars,
        pdf.title,
        pdf.outline.len(),
        sections.len(),
        domain::document::guess_language(&sample),
        domain::document::check_has_text(&pdf.pages),
    );
    // The rest of the import: storing text, sections and resource in SQLite.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let stored = rt.block_on(async {
        use domain::{DocumentRepository, FolderRepository, TopicRepository};
        let db = std::env::temp_dir().join(format!("teach_me_real_{}.db", uuid::Uuid::now_v7()));
        let pool = infrastructure::sqlite::connect(&format!("sqlite://{}", db.display()))
            .await
            .unwrap();
        infrastructure::sqlite::migrate(&pool).await.unwrap();
        let at = time::macros::datetime!(2026-11-09 10:00 UTC);
        let folder = domain::FolderId::new(uuid::Uuid::from_u128(1));
        infrastructure::SqliteFolderRepository::new(pool.clone())
            .insert(domain::NewFolder {
                user_id: domain::UserId::OWNER,
                id: folder,
                name: domain::FolderName::parse("F").unwrap(),
                created_at: at,
            })
            .await
            .unwrap();
        let topic = domain::Topic::new(
            domain::TopicId::new(uuid::Uuid::from_u128(2)),
            domain::UserId::OWNER,
            folder,
            domain::TopicTitle::parse("T").unwrap(),
            at,
        );
        infrastructure::SqliteTopicRepository::new(pool.clone())
            .insert(&topic)
            .await
            .unwrap();
        let docs = infrastructure::SqliteDocumentRepository::new(pool.clone());
        let id = domain::DocumentId::new(uuid::Uuid::from_u128(3));
        let doc = domain::Document::importing(id, topic.id, "book.pdf".into(), 1, at);
        let job = domain::Job::queued_for_document(
            domain::JobId::new(uuid::Uuid::from_u128(4)),
            topic.id,
            id,
            at,
        );
        docs.insert_with_job(&doc, &job).await.unwrap();
        let started = std::time::Instant::now();
        docs.complete(
            &domain::ImportedDocument {
                document_id: id,
                title: "t".into(),
                language: None,
                pages: pdf.pages.clone(),
                sections: sections.clone(),
                resource_id: domain::ResourceId::new(uuid::Uuid::from_u128(5)),
                resource: domain::document::resource_draft("t", 1, None),
            },
            at,
        )
        .await
        .unwrap();
        let t = started.elapsed();
        let _ = std::fs::remove_file(&db);
        t
    });
    eprintln!("  sqlite complete() = {stored:?}");
    for s in sections.iter().take(8) {
        eprintln!(
            "  [{}] L{} {} (pp. {}-{})",
            s.id, s.level, s.title, s.page_start, s.page_end
        );
    }
}

#[test]
fn a_cyclic_outline_ends_instead_of_spinning() {
    let Some(engine) = engine() else { return };
    let outline = vec![
        Outline {
            title: "Uno",
            page: 0,
            children: vec![],
        },
        Outline {
            title: "Due",
            page: 1,
            children: vec![],
        },
    ];
    let mut bytes = build_pdf(&pages(3), &outline, None);
    // Three pages: the outline root is object 11, the items 12 and 13. Make
    // item 12 point back at itself (same length, so the xref stays right).
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("/Next 13 0 R"));
    bytes = text.replace("/Next 13 0 R", "/Next 12 0 R").into_bytes();
    let (path, _guard) = write("cycle.pdf", &bytes);
    let started = std::time::Instant::now();
    let pdf = read(engine, &path).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(pdf.outline.len() <= 5_000);
}
