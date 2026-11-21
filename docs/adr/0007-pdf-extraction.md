# ADR 0007 — Testo dei PDF: PDFium (pdfium-render) dietro PdfTextExtractor

- Stato: accettata
- Data: 2026-11-09
- Contratto `docs/contracts/documents.md`. Servono testo **per pagina**, **segnalibri** con pagina di destinazione, rilevamento dei PDF **cifrati**, tempi accettabili su ~500 pagine, licenza compatibile, e un modo semplice di portare la libreria nell'immagine Docker (Debian bookworm). Verifica e prova: `rust-backend-dev`.

## Scelta

**PDFium tramite `pdfium-render` 0.9.5** (MIT OR Apache-2.0; PDFium: BSD-3-Clause/Apache-2.0), caricato come libreria condivisa a run time (`libpdfium.so` da `bblanchon/pdfium-binaries`, release `chromium/8138`, scaricata nell'immagine con versione e SHA-256 fissati). L'estrazione sta dietro il port `PdfTextExtractor` (`domain::document`), l'adapter è `infrastructure::pdf::PdfiumTextExtractor`; nessun tipo di `pdfium_render` esce da quel file. Il crate è usato con `default-features = false, features = ["pdfium_latest", "thread_safe"]`: senza la feature `image` (non serve, niente rendering) l'albero normale ha circa 30 righe di `cargo tree`, con `chrono`, `itertools`, `bytemuck`, `libloading`.

Perché:
- **Testo**: è l'unico dei candidati che scompone le legature in lettere (0 caratteri `ﬁ ﬂ ﬀ ﬃ` su SICP, contro 2.647 di `pdftotext`, `pdf_oxide` e `pdf-extract`): per un LLM e per le citazioni conta, e le legature non vanno ricostruite a valle. Spazi tra le parole, righe di codice e ordine di lettura sono equivalenti agli altri sui due libri provati (token oltre 28 caratteri: 22 su Think Python e 57 su SICP con ogni motore).
- **Segnalibri**: API diretta ad albero (`bookmarks().root()`, `first_child`, `next_sibling`, `destination().page_index()`), con livello e pagina già risolti anche per le destinazioni nominate. `pdftotext`/poppler non li espone; `pdf-extract` nemmeno; `pdf_oxide` li espone ma con meno prove alle spalle.
- **Cifrati**: PDFium rifiuta l'apertura senza password con `FPDF_ERR_PASSWORD`/`SECURITY`, mappati su `PdfError::Encrypted` → `encrypted_pdf`. Provato su un PDF cifrato a 128 bit generato con Ghostscript (fixture `tests/fixtures/encrypted.pdf`, 3 KB). I PDF con sola password del proprietario (restrizioni di stampa/copia, nessuna password per aprirli) si aprono e si importano.
- **Robustezza**: è il motore di Chrome, testato su tutto il web; file corrotti o troncati finiscono in `invalid_pdf` senza panic (provato: file vuoto, `%PDF-` seguito da spazzatura). Le alternative in Rust puro sono più giovani o hanno storie di panic su font particolari (il fork `hygg-pdf-extract` esiste per un panic su font Adobe Expert).
- **Prestazioni**: 883 pagine (SICP, 7 MB, 1,26 M caratteri) in **0,45-0,51 s** di sola lettura (build release); 244 pagine (Think Python) in 0,10 s. Il salvataggio in SQLite di 883 pagine, 172 sezioni e la `Resource` è di **~10 ms**. Un libro da 500 pagine si importa in meno di un secondo; il limite di 1.500 pagine non è un problema di tempo.
- **Licenza**: tutto MIT/BSD/Apache, nessun copyleft. Il pacchetto binario include `LICENSE` e `licenses/` di PDFium e delle sue dipendenze (FreeType, libjpeg-turbo, ICU, lcms, openjpeg, zlib…); l'immagine li copia in `/usr/share/doc/pdfium`.
- **Immagine Docker**: stage `pdfium` (bookworm-slim + curl) che scarica `pdfium-linux-{x64,arm64}.tgz`, verifica lo SHA-256 e ne estrae solo `libpdfium.so` e le licenze; lo stage finale lo copia in `/usr/local/lib/pdfium` (8 MB) e imposta `TEACH_ME_PDFIUM_DIR`. `libpdfium.so` dipende solo da glibc, `libm`, `libpthread`, `libgcc_s` (verificato con `ldd` in bookworm-slim): nessun pacchetto `apt` in più. Provato in locale il solo stage `pdfium` con podman (hash verificato); la build completa dell'immagine non l'ho eseguita qui.

Il binario parte anche senza la libreria: `PdfiumTextExtractor::bind` fallisce, il backend registra un avviso e usa un estrattore che fa fallire gli import con `internal`. Così `cargo test` e lo sviluppo del resto non richiedono PDFium; i test dell'adapter (`infrastructure/tests/pdf.rs`) si saltano se non lo trovano (`TEACH_ME_PDFIUM_DIR` o libreria di sistema).

## Prova

PDF reali, scaricati fuori dal repo: *Think Python 2e* (244 pagine, codice Python, 240 segnalibri) e *SICP* (883 pagine, codice Scheme, formule, 132 segnalibri fino al livello 3). PDF di prova generati: testo con segnalibri a 3 livelli, solo pagine vuote (scansione), spazzatura, cifrato.

| Motore | SICP 883 pag. | Think Python 244 pag. | Legature | Segnalibri | Cifrati | Note |
|---|---|---|---|---|---|---|
| **pdfium-render 0.9.5** | 0,45 s | 0,10 s | scomposte (0 residue) | sì, ad albero, con pagina | `PasswordError` | libreria condivisa da 8 MB |
| pdf_oxide 0.3.78 | 1,35 s | 0,42 s | 2.647 `ﬁ ﬂ ﬀ ﬃ` residue | sì (primo livello e figli) | `is_encrypted`, `authenticate` | Rust puro, MIT/Apache; crate recente (0.3.x), molta superficie (241 righe di albero) |
| pdf-extract 0.12.1 (lopdf) | 0,94 s | 0,60 s | 2.647 residue | no (con `lopdf` a parte, `get_toc`) | via lopdf | Rust puro, MIT; testo per pagina sì |
| `pdftotext` (poppler) come processo | 0,48 s | 0,27 s | 2.647 residue | no | errore di password | GPL: ok come processo separato, ma niente segnalibri; serve `poppler-utils` e un `fork/exec` per file |

Qualità: su tutti la perdita della legatura "tt" in SICP (`wrien` per `written`, 51 occorrenze) **dipende dal PDF** (il font non mappa il glifo) e nessun motore la recupera. Il rientro del codice (spazi iniziali) è perso da tutti: PDFium restituisce le righe già spogliate, quindi l'indentazione di Python o Scheme non si legge dal testo; il Generator non deve fare affidamento su di essa (`prompts/`: nota per `llm-engineer`). Un capolettera ("Th" di "This") può mancare quando il PDF lo disegna come immagine.

Scartati senza prova: `mupdf` (AGPL-3.0: incompatibile con la distribuzione sotto altra licenza), `poppler-rs`/`pdfium` statico (richiedono compilare C++).

## Conseguenze e rischi

- **Una libreria nativa da gestire**: va aggiornata a mano (cambio di `PDFIUM_RELEASE` e hash nel `Dockerfile`, poi `infrastructure/tests/pdf.rs` con la nuova libreria). PDFium si associa una sola volta per processo (`Pdfium::new`): l'estrattore è creato una volta all'avvio.
- **PDF ostili**: PDFium è C++ e legge file di sconosciuti. Qui l'utente è uno solo e carica i suoi file; il processo gira come utente non privilegiato nel container. Limiti già applicati: 50 MB, 1.500 pagine controllate prima di leggere il testo, al massimo 5.000 segnalibri letti (profondità 3) contro cicli e outline giganti. Un crash nativo fermerebbe il processo: il job interrotto riparte al riavvio e dopo 3 partenze fallisce con `interrupted` (`jobs.md`).
- **`pdfium-render` esegue il lavoro sotto un lock globale** (feature `thread_safe`): due import in contemporanea si serializzano. Non è un problema con un utente solo e un solo `document_import` attivo per argomento.
- **Memoria**: il testo di tutte le pagine è in memoria durante l'import (1,3 MB per 883 pagine; 1.500 pagine dense ~5 MB).
- **Lingua**: stimata dal testo con una lista di stopword (en, it, es, fr, de), `null` se incerta; la `Resource` usa `und` (BCP-47: indeterminata) quando non lo sa.
- **Se PDFium diventasse un problema**: il port permette di passare a `pdf_oxide` (la scelta pura-Rust più vicina) cambiando solo l'adapter; le legature andrebbero normalizzate (tabella NFKC su `ﬁ ﬂ ﬀ ﬃ ﬄ`).
