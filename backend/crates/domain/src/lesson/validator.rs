use std::collections::HashSet;

use serde_json::Value;
use uuid::Uuid;

use super::inline::{MAX_INLINE_CODE_CHARS, placeholders};
use super::model::{
    Block, BlockKind, DiagramEdge, DiagramLayout, LessonDocument, LessonErrorCode as Code,
    LessonIssue, show,
};
use super::parse::parse_document;

/// Tunable thresholds (contract §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatorConfig {
    /// Max difference in words between the longest and shortest quiz option.
    pub option_word_tolerance: usize,
    /// Max words of prose (paragraphs, callouts, quiz texts).
    pub max_prose_words: usize,
    /// Max sections (level-2 headings).
    pub max_sections: usize,
    /// Max size of the document as JSON (contract §9.24).
    pub max_document_bytes: usize,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            option_word_tolerance: 3,
            max_prose_words: 2200,
            max_sections: 6,
            max_document_bytes: 256 * 1024,
        }
    }
}

/// What the Validator knows about the Topic.
///
/// Two modes, decided by which fields are set:
///
/// * **Fixture mode** (`ValidationContext::fixture()`): no Topic is
///   available, so `sources[].resource_id` may be `null`, `resource_id` and
///   `lesson_link.lesson_id` existence checks are skipped, and `[[key]]` may
///   only refer to the document's own glossary.
/// * **Production mode** : build the context with
///   [`ValidationContext::production`] from the Topic's resources and
///   lessons. `null` resource ids become `unknown_resource` and every
///   resource/lesson id must belong to the Topic. The document must stand on
///   its own: Topic glossary keys are *not* accepted (`lessons.md` §4).
///   [`ValidationContext::strict`] keeps the older, looser variant.
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// `Some` activates the `unknown_resource` check.
    pub resource_ids: Option<HashSet<Uuid>>,
    /// `Some` activates the `invalid_lesson_link` check.
    pub lesson_ids: Option<HashSet<Uuid>>,
    /// Glossary keys already stored for the Topic.
    pub topic_glossary_keys: HashSet<String>,
    /// Resources of kind `document` (uploaded PDFs): their sources have
    /// `url: null`, all the others an `https` URL (`documents.md` §6).
    pub document_resource_ids: HashSet<Uuid>,
}

impl ValidationContext {
    /// Fixture mode: no Topic context. Never use in production paths.
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            resource_ids: None,
            lesson_ids: None,
            topic_glossary_keys: HashSet::new(),
            document_resource_ids: HashSet::new(),
        }
    }

    /// Production mode (`lessons.md` §4): resources and lessons of the Topic,
    /// and a document that stands on its own: `[[key]]` and `glossary_term`
    /// may only use the document's own glossary, not the Topic's.
    #[must_use]
    pub fn production(resource_ids: HashSet<Uuid>, lesson_ids: HashSet<Uuid>) -> Self {
        Self::strict(resource_ids, lesson_ids, HashSet::new())
    }

    #[must_use]
    pub fn strict(
        resource_ids: HashSet<Uuid>,
        lesson_ids: HashSet<Uuid>,
        topic_glossary_keys: HashSet<String>,
    ) -> Self {
        Self {
            resource_ids: Some(resource_ids),
            lesson_ids: Some(lesson_ids),
            topic_glossary_keys,
            document_resource_ids: HashSet::new(),
        }
    }

    /// Marks which of the resources are uploaded PDFs.
    #[must_use]
    pub fn with_document_resources(mut self, ids: HashSet<Uuid>) -> Self {
        self.document_resource_ids = ids;
        self
    }
}

/// Checks a lesson document against the catalog v1 contract. Pure logic: the
/// same input always yields the same issues, in a stable order.
#[derive(Debug, Clone)]
pub struct LessonValidator {
    config: ValidatorConfig,
    context: ValidationContext,
}

impl LessonValidator {
    #[must_use]
    pub const fn new(config: ValidatorConfig, context: ValidationContext) -> Self {
        Self { config, context }
    }

    /// Returns every issue found; empty means the document is valid.
    ///
    /// If the shape is wrong (`schema` issues) the cross-document rules are
    /// skipped: counts and references computed on a broken document would
    /// only add noise to the repair prompt. Fix the shape, then re-validate.
    #[must_use]
    pub fn validate(&self, document: &Value) -> Vec<LessonIssue> {
        let size = json_size(document);
        if size > self.config.max_document_bytes {
            return vec![issue(
                String::new(),
                Code::TooLong,
                format!(
                    "Il documento pesa {size} byte di JSON, il massimo è {}.",
                    self.config.max_document_bytes
                ),
            )];
        }
        let (doc, mut issues) = parse_document(document);
        if issues.iter().any(|i| i.code == Code::Schema) {
            return issues;
        }
        issues.extend(self.rules(&doc));
        issues.extend(super::quality::errors(document));
        issues
    }

    /// Non-blocking quality warnings. Call only on a document that already
    /// passed [`Self::validate`].
    #[must_use]
    pub fn quality_warnings(
        &self,
        document: &Value,
        language: crate::ContentLanguage,
    ) -> Vec<super::quality::QualityWarning> {
        super::quality::warnings(document, language)
    }

    fn rules(&self, doc: &LessonDocument) -> Vec<LessonIssue> {
        let mut out = Vec::new();
        let mut all = Vec::new();
        for b in &doc.blocks {
            b.flatten(&mut all);
        }
        markdown(doc, &all, &mut out);
        duplicate_block_ids(&all, &mut out);
        for b in &all {
            self.block_rules(b, &mut out);
        }
        answer_position(&doc.blocks, &mut out);
        self.references(doc, &all, &mut out);
        self.structure(doc, &mut out);
        out
    }

    fn block_rules(&self, b: &Block, out: &mut Vec<LessonIssue>) {
        match &b.kind {
            BlockKind::QuizSingle {
                options, correct, ..
            } => {
                if !options.iter().any(|o| &o.id == correct) {
                    out.push(issue(
                        format!("{}/correct", b.path),
                        Code::QuizCorrectMissing,
                        format!(
                            "Il quiz '{}' ha 'correct' = '{}', che non è l'id di nessuna opzione.",
                            b.id,
                            show(correct)
                        ),
                    ));
                }
                self.option_length(b, options, out);
                length_cue(b, options, std::slice::from_ref(correct), out);
            }
            BlockKind::QuizMulti {
                options, correct, ..
            } => {
                for (i, c) in correct.iter().enumerate() {
                    if !options.iter().any(|o| &o.id == c) {
                        out.push(issue(
                            format!("{}/correct/{i}", b.path),
                            Code::QuizCorrectMissing,
                            format!(
                                "Il quiz '{}' indica come corretta '{}', che non è un'opzione.",
                                b.id,
                                show(c)
                            ),
                        ));
                    }
                }
                let distinct: HashSet<_> = correct.iter().collect();
                if distinct.is_empty() || distinct.len() >= options.len() {
                    out.push(issue(
                        format!("{}/correct", b.path),
                        Code::QuizCorrectMissing,
                        format!(
                            "Il quiz a risposta multipla '{}' deve avere almeno una opzione corretta e almeno una errata ({} corrette su {}).",
                            b.id,
                            distinct.len(),
                            options.len()
                        ),
                    ));
                }
                self.option_length(b, options, out);
                length_cue(b, options, correct, out);
            }
            BlockKind::FillBlank {
                text, blank_ids, ..
            } => fill_blank(b, text, blank_ids, out),
            BlockKind::Diagram {
                layout,
                nodes,
                edges,
            } => diagram_structure(b, *layout, nodes, edges, out),
            _ => {}
        }
    }

    fn option_length(
        &self,
        b: &Block,
        options: &[super::model::QuizOption],
        out: &mut Vec<LessonIssue>,
    ) {
        // First longest / shortest, so the report is deterministic.
        let Some((long_i, long)) = options.iter().enumerate().fold(
            None,
            |best: Option<(usize, usize)>, (i, o)| match best {
                Some((_, w)) if w >= o.words => best,
                _ => Some((i, o.words)),
            },
        ) else {
            return;
        };
        let short = options.iter().map(|o| o.words).min().unwrap_or(long);
        if long - short > self.config.option_word_tolerance {
            let tol = self.config.option_word_tolerance;
            // The window [start, start + tol] that already holds the most
            // options (lowest start on ties): the fewest rewrites to fix.
            let start = options
                .iter()
                .map(|o| o.words)
                .max_by_key(|&s| {
                    (
                        options
                            .iter()
                            .filter(|o| o.words >= s && o.words <= s + tol)
                            .count(),
                        std::cmp::Reverse(s),
                    )
                })
                .unwrap_or(short);
            let counts = options
                .iter()
                .enumerate()
                .map(|(i, o)| format!("opzione {i}: {} parole", o.words))
                .collect::<Vec<_>>()
                .join("; ");
            out.push(issue(
                format!("{}/options/{long_i}/text", b.path),
                Code::QuizOptionLength,
                format!(
                    "Le opzioni del quiz '{}' hanno lunghezze troppo diverse ({short}-{long} parole, massimo scarto {tol}). Parole per opzione: {counts}. Riscrivi le opzioni fuori intervallo in modo che tutte abbiano tra {start} e {} parole.",
                    b.id,
                    start + tol
                ),
            ));
        }
    }

    #[allow(clippy::too_many_lines)] // one pass per reference kind
    fn references(&self, doc: &LessonDocument, all: &[&Block], out: &mut Vec<LessonIssue>) {
        let source_keys: HashSet<&str> = doc.sources.iter().map(|s| s.key.as_str()).collect();
        let glossary_keys: HashSet<&str> = doc.glossary.iter().map(|g| g.key.as_str()).collect();
        let mut used_sources: HashSet<&str> = HashSet::new();
        let mut used_terms: HashSet<&str> = HashSet::new();

        let known_term =
            |k: &str| glossary_keys.contains(k) || self.context.topic_glossary_keys.contains(k);

        for f in &doc.md_fields {
            // References inside glossary definitions do not count as use (§9.7).
            let counts = !f.path.starts_with("/glossary/");
            for c in &f.scan.citations {
                if counts {
                    used_sources.insert(c);
                }
                if !source_keys.contains(c.as_str()) {
                    out.push(issue(
                        f.path.clone(),
                        Code::UnknownCitation,
                        format!(
                            "La citazione [^{}] non corrisponde a nessuna fonte in 'sources'.",
                            show(c)
                        ),
                    ));
                }
            }
            for k in &f.scan.glossary_refs {
                if counts {
                    used_terms.insert(k);
                }
                if !known_term(k) {
                    out.push(issue(
                        f.path.clone(),
                        Code::UnknownGlossaryKey,
                        format!("Il termine [[{}]] non esiste in 'glossary'.", show(k)),
                    ));
                }
            }
        }
        for b in all {
            match &b.kind {
                BlockKind::GlossaryTerm { key } => {
                    used_terms.insert(key);
                    if !known_term(key) {
                        out.push(issue(
                            format!("{}/key", b.path),
                            Code::UnknownGlossaryKey,
                            format!("Il blocco glossary_term '{}' usa la chiave '{key}', assente da 'glossary'.", b.id),
                        ));
                    }
                }
                BlockKind::PrimarySource { source } => {
                    used_sources.insert(source);
                    if !source_keys.contains(source.as_str()) {
                        out.push(issue(
                            format!("{}/source", b.path),
                            Code::UnknownCitation,
                            format!("Il blocco primary_source '{}' usa la fonte '{source}', assente da 'sources'.", b.id),
                        ));
                    }
                }
                BlockKind::LessonLink { lesson_id } => {
                    if let Some(ids) = &self.context.lesson_ids
                        && !ids.contains(lesson_id)
                    {
                        out.push(issue(
                            format!("{}/lesson_id", b.path),
                            Code::InvalidLessonLink,
                            format!("Il lesson_link '{}' punta a '{lesson_id}', che non è una lezione di questo argomento.", b.id),
                        ));
                    }
                }
                _ => {}
            }
        }
        for (i, g) in doc.glossary.iter().enumerate() {
            if !used_terms.contains(g.key.as_str()) {
                out.push(issue(
                    format!("/glossary/{i}"),
                    Code::UnusedEntry,
                    format!(
                        "La voce di glossario '{}' non è mai usata: usala con [[{}]] o rimuovila.",
                        g.key, g.key
                    ),
                ));
            }
        }
        for (i, s) in doc.sources.iter().enumerate() {
            if !used_sources.contains(s.key.as_str()) {
                out.push(issue(
                    format!("/sources/{i}"),
                    Code::UnusedEntry,
                    format!("La fonte '{}' non è mai citata: usa [^{}] o un blocco primary_source, oppure rimuovila.", s.key, s.key),
                ));
            }
            if let Some(ids) = &self.context.resource_ids {
                let known = s.resource_id.is_some_and(|r| ids.contains(&r));
                if !known {
                    let why = if s.resource_id.is_none() {
                        "è null (ammesso solo nelle prove)"
                    } else {
                        "non è una risorsa di questo argomento"
                    };
                    out.push(issue(
                        format!("/sources/{i}/resource_id"),
                        Code::UnknownResource,
                        format!("La fonte '{}' ha un resource_id che {why}.", s.key),
                    ));
                }
                if let Some(rid) = s.resource_id.filter(|r| ids.contains(r)) {
                    let is_document = self.context.document_resource_ids.contains(&rid);
                    if is_document && !s.url_is_null {
                        out.push(issue(
                            format!("/sources/{i}/url"),
                            Code::Schema,
                            format!(
                                "La fonte '{}' è un PDF caricato: 'url' deve essere null.",
                                s.key
                            ),
                        ));
                    } else if !is_document && s.url_is_null {
                        out.push(issue(
                            format!("/sources/{i}/url"),
                            Code::Schema,
                            format!("La fonte '{}' non è un PDF caricato: 'url' deve essere un URL https.", s.key),
                        ));
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)] // flat list of independent document-level rules
    fn structure(&self, doc: &LessonDocument, out: &mut Vec<LessonIssue>) {
        let blocks = &doc.blocks;
        let count = |pred: fn(&BlockKind) -> bool| blocks.iter().filter(|b| pred(&b.kind)).count();

        let primaries = count(|k| matches!(k, BlockKind::PrimarySource { .. }));
        if primaries != 1 {
            out.push(issue(
                "/blocks".into(),
                Code::PrimarySourceCount,
                format!("Serve esattamente un blocco primary_source (trovati {primaries})."),
            ));
        }
        if count(|k| matches!(k, BlockKind::AskAgent)) == 0 {
            out.push(issue(
                "/blocks".into(),
                Code::AskAgentMissing,
                "Serve almeno un blocco ask_agent.".into(),
            ));
        }
        if !blocks.iter().any(Block::is_required_exercise) {
            out.push(issue(
                "/blocks".into(),
                Code::NoRequiredExercise,
                "Serve almeno un esercizio con required: true (una vittoria concreta).".into(),
            ));
        }

        let cfg = &self.config;
        let sections = count(|k| matches!(k, BlockKind::Heading { level: 2 }));
        if sections > cfg.max_sections {
            out.push(issue(
                "/blocks".into(),
                Code::TooLong,
                format!(
                    "La lezione ha {sections} sezioni (heading di livello 2), il massimo è {}.",
                    cfg.max_sections
                ),
            ));
        }
        let words = doc.prose_words();
        if words > cfg.max_prose_words {
            out.push(issue(
                "/blocks".into(),
                Code::TooLong,
                format!(
                    "La prosa della lezione conta {words} parole, il massimo è {}.",
                    cfg.max_prose_words
                ),
            ));
        }

        match blocks.first() {
            Some(Block {
                kind: BlockKind::Heading { level: 1 },
                ..
            })
            | None => {} // empty list is already a `schema` error
            Some(b) => out.push(issue(
                b.path.clone(),
                Code::FirstBlockNotTitle,
                "Il primo blocco deve essere un heading di livello 1 con il titolo.".into(),
            )),
        }

        // A section runs to the next level-2 heading (or the end) and is empty
        // when it holds only headings of any level (§9.8).
        for (i, b) in blocks.iter().enumerate() {
            if !matches!(b.kind, BlockKind::Heading { level: 2 }) {
                continue;
            }
            let empty = blocks[i + 1..]
                .iter()
                .take_while(|n| !matches!(n.kind, BlockKind::Heading { level: 2 }))
                .all(|n| matches!(n.kind, BlockKind::Heading { .. }));
            if empty {
                out.push(issue(
                    b.path.clone(),
                    Code::SectionWithoutContent,
                    format!(
                        "La sezione '{}' non contiene blocchi prima della successiva.",
                        b.id
                    ),
                ));
            }
        }
    }
}

fn issue(path: String, code: Code, message: String) -> LessonIssue {
    LessonIssue {
        path,
        code,
        message,
    }
}

fn markdown(doc: &LessonDocument, all: &[&Block], out: &mut Vec<LessonIssue>) {
    for f in &doc.md_fields {
        if !f.scan.long_code_spans.is_empty() {
            let in_exercise = all.iter().any(|b| {
                matches!(
                    b.kind,
                    BlockKind::QuizSingle { .. }
                        | BlockKind::QuizMulti { .. }
                        | BlockKind::OpenAnswer { .. }
                        | BlockKind::Flashcard { .. }
                ) && f
                    .path
                    .strip_prefix(&b.path)
                    .is_some_and(|r| r.starts_with('/'))
            });
            let remedy = if in_exercise {
                "Sposta il codice nel campo 'code' dell'esercizio (un oggetto {language, code}) \
                 e nel testo lascia solo un riferimento breve, per esempio un nome o un'espressione."
            } else {
                "Usa un blocco 'code' separato e nel testo lascia solo un riferimento breve."
            };
            for (chars, semicolons) in &f.scan.long_code_spans {
                out.push(issue(
                    f.path.clone(),
                    Code::InlineCodeTooLong,
                    format!(
                        "Il campo {} contiene codice inline troppo lungo ({chars} caratteri, \
                         {semicolons} ';'; massimo {MAX_INLINE_CODE_CHARS} caratteri e un solo ';'). {remedy}",
                        f.path
                    ),
                ));
            }
        }
        for p in &f.scan.problems {
            out.push(issue(
                f.path.clone(),
                Code::Markdown,
                format!("Il campo {} {}.", f.path, p.describe()),
            ));
        }
    }
}

/// Graph rules of a `diagram` (contract §10.1-10.2). A cross-document rule
/// like the others in `rules`: it only runs on a well-formed document, and
/// per-edge problems are reported before the whole-graph ones, which would
/// otherwise be noise (an edge to a missing node makes a node look isolated).
fn diagram_structure(
    b: &Block,
    layout: DiagramLayout,
    nodes: &[String],
    edges: &[DiagramEdge],
    out: &mut Vec<LessonIssue>,
) {
    let mut push = |path: String, message: String| {
        out.push(issue(path, Code::DiagramStructure, message));
    };
    let unique: HashSet<&str> = nodes.iter().map(String::as_str).collect();
    if unique.len() != nodes.len() {
        return; // duplicate node ids are already reported as `duplicate_id`
    }
    let id = &b.id;

    if layout == DiagramLayout::Cycle {
        // The cycle is implied by the order of the nodes: no edges to check.
        if !edges.is_empty() {
            push(
                format!("{}/edges", b.path),
                format!(
                    "Il diagram '{id}' ha layout 'cycle': togli 'edges' (o lascialo vuoto), il ciclo segue l'ordine dei nodi e l'ultimo torna al primo."
                ),
            );
            return; // errors on the edges mask those on the graph (§10.8)
        }
        if nodes.len() < 3 {
            push(
                format!("{}/nodes", b.path),
                format!(
                    "Il diagram '{id}' ha layout 'cycle' con {} nodi: ne servono almeno 3. Aggiungi nodi oppure usa il layout 'flow'.",
                    nodes.len()
                ),
            );
        }
        return;
    }

    // Per-edge problems.
    let index = |n: &str| nodes.iter().position(|x| x == n);
    let mut edge_problem = false;
    let mut pairs = HashSet::new();
    for (i, e) in edges.iter().enumerate() {
        let path = format!("{}/edges/{i}", b.path);
        let mut bad = false;
        for (field, end) in [("from", &e.from), ("to", &e.to)] {
            if !unique.contains(end.as_str()) {
                bad = true;
                push(
                    format!("{path}/{field}"),
                    format!(
                        "L'arco {i} del diagram '{id}' ha '{field}' = '{}', che non è l'id di nessun nodo. Usa uno degli id in 'nodes'.",
                        show(end)
                    ),
                );
            }
        }
        if !bad && e.from == e.to {
            bad = true;
            push(
                path,
                format!(
                    "L'arco {i} del diagram '{id}' va dal nodo '{}' a se stesso. Rimuovilo o collega il nodo a un altro.",
                    show(&e.from)
                ),
            );
        } else if !bad && !pairs.insert((e.from.as_str(), e.to.as_str())) {
            bad = true;
            push(
                path,
                format!(
                    "L'arco {i} del diagram '{id}' ripete la coppia '{}' -> '{}'. Rimuovi il duplicato.",
                    show(&e.from),
                    show(&e.to)
                ),
            );
        }
        edge_problem |= bad;
    }
    if edge_problem {
        return; // repair the edges first, then the graph is judged
    }
    if edges.is_empty() {
        let what = if layout == DiagramLayout::Flow {
            "flow"
        } else {
            "tree"
        };
        push(
            format!("{}/edges", b.path),
            format!(
                "Il diagram '{id}' ha layout '{what}' ma nessun arco: aggiungi gli archi che collegano i {} nodi.",
                nodes.len()
            ),
        );
        return;
    }
    let edge_idx: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|e| Some((index(&e.from)?, index(&e.to)?)))
        .collect();
    match layout {
        DiagramLayout::Flow => flow_rules(b, nodes, edges, &edge_idx, out),
        DiagramLayout::Tree => tree_rules(b, nodes, edges, &edge_idx, out),
        DiagramLayout::Cycle => {}
    }
}

fn flow_rules(
    b: &Block,
    nodes: &[String],
    edges: &[DiagramEdge],
    edge_idx: &[(usize, usize)],
    out: &mut Vec<LessonIssue>,
) {
    let id = &b.id;
    let mut push = |path: String, message: String| {
        out.push(issue(path, Code::DiagramStructure, message));
    };
    // First edge (in document order) that closes a cycle in a depth-first walk.
    if let Some(i) = back_edge(nodes.len(), edge_idx) {
        push(
            format!("{}/edges/{i}", b.path),
            format!(
                "L'arco {i} del diagram '{id}' ('{}' -> '{}') chiude un ciclo: un 'flow' non può tornare indietro. Rimuovi l'arco, oppure usa il layout 'cycle' se il processo si ripete.",
                show(&edges[i].from),
                show(&edges[i].to)
            ),
        );
    }
    let mut touched = vec![false; nodes.len()];
    for &(f, t) in edge_idx {
        touched[f] = true;
        touched[t] = true;
    }
    for (n, _) in touched.iter().enumerate().filter(|(_, t)| !**t) {
        push(
            b.path.clone(),
            format!(
                "Il nodo '{}' del diagram '{id}' non ha archi: collegalo agli altri o toglilo.",
                show(&nodes[n])
            ),
        );
    }
    // Connectivity among the nodes that have edges (isolated ones are reported above).
    let comp = components(nodes.len(), edge_idx);
    let mut reps: Vec<usize> = Vec::new();
    for n in (0..nodes.len()).filter(|&n| touched[n]) {
        if !reps.contains(&comp[n]) {
            reps.push(comp[n]);
        }
    }
    if reps.len() > 1 {
        let groups = reps
            .iter()
            .map(|c| {
                let members = (0..nodes.len())
                    .filter(|&n| touched[n] && comp[n] == *c)
                    .map(|n| format!("'{}'", show(&nodes[n])))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{members}]")
            })
            .collect::<Vec<_>>()
            .join(" e ");
        push(
            b.path.clone(),
            format!(
                "Il diagram '{id}' non è connesso: ci sono gruppi di nodi separati {groups}. Aggiungi un arco tra i gruppi o dividi la figura."
            ),
        );
    }
}

fn tree_rules(
    b: &Block,
    nodes: &[String],
    edges: &[DiagramEdge],
    edge_idx: &[(usize, usize)],
    out: &mut Vec<LessonIssue>,
) {
    let id = &b.id;
    let mut push = |path: String, message: String| {
        out.push(issue(path, Code::DiagramStructure, message));
    };
    let mut parents = vec![0usize; nodes.len()];
    let mut second_parent = false;
    for (i, &(_, t)) in edge_idx.iter().enumerate() {
        parents[t] += 1;
        if parents[t] == 2 {
            second_parent = true;
            push(
                format!("{}/edges/{i}", b.path),
                format!(
                    "Il nodo '{}' del diagram '{id}' ha più di un genitore (anche con l'arco {i}, da '{}'): in un 'tree' ogni nodo ha un solo genitore. Rimuovi un arco o usa il layout 'flow'.",
                    show(&nodes[t]),
                    show(&edges[i].from)
                ),
            );
        }
    }
    if second_parent {
        return; // errors on the edges mask those on the graph (§10.8)
    }
    let roots: Vec<usize> = (0..nodes.len()).filter(|&n| parents[n] == 0).collect();
    match roots.as_slice() {
        [] => push(
            b.path.clone(),
            format!(
                "Il diagram '{id}' non ha una radice: ogni nodo ha un genitore. Un 'tree' ha un solo nodo senza genitore; rimuovi l'arco che torna verso l'alto."
            ),
        ),
        [root] => {
            let mut seen = vec![false; nodes.len()];
            let mut stack = vec![*root];
            seen[*root] = true;
            while let Some(n) = stack.pop() {
                for &(f, t) in edge_idx {
                    if f == n && !seen[t] {
                        seen[t] = true;
                        stack.push(t);
                    }
                }
            }
            for n in (0..nodes.len()).filter(|&n| !seen[n]) {
                push(
                    b.path.clone(),
                    format!(
                        "Il nodo '{}' del diagram '{id}' non è raggiungibile dalla radice '{}'. Collegalo con un arco dal suo genitore.",
                        show(&nodes[n]),
                        show(&nodes[*root])
                    ),
                );
            }
        }
        many => {
            let list = many
                .iter()
                .map(|&n| format!("'{}'", show(&nodes[n])))
                .collect::<Vec<_>>()
                .join(", ");
            push(
                b.path.clone(),
                format!(
                    "Il diagram '{id}' ha {} radici ({list}): un 'tree' ne ha una sola. Collega le radici in più sotto un genitore, o usa il layout 'flow'.",
                    many.len()
                ),
            );
        }
    }
}

/// Index of the first edge that points back to a node on the current
/// depth-first path (so removing it breaks that cycle), if the graph has one.
fn back_edge(n: usize, edges: &[(usize, usize)]) -> Option<usize> {
    // 0 = unvisited, 1 = on the path, 2 = done.
    fn visit(node: usize, edges: &[(usize, usize)], state: &mut [u8]) -> Option<usize> {
        state[node] = 1;
        for (i, &(f, t)) in edges.iter().enumerate() {
            if f != node {
                continue;
            }
            match state[t] {
                1 => return Some(i),
                0 => {
                    if let Some(found) = visit(t, edges, state) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        state[node] = 2;
        None
    }
    let mut state = vec![0u8; n];
    (0..n).find_map(|start| {
        if state[start] == 0 {
            visit(start, edges, &mut state)
        } else {
            None
        }
    })
}

/// Weakly connected component of each node (edge direction ignored).
fn components(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut comp: Vec<usize> = (0..n).collect();
    loop {
        let mut changed = false;
        for &(f, t) in edges {
            let low = comp[f].min(comp[t]);
            if comp[f] != low || comp[t] != low {
                comp[f] = low;
                comp[t] = low;
                changed = true;
            }
        }
        if !changed {
            return comp;
        }
    }
}

fn duplicate_block_ids(all: &[&Block], out: &mut Vec<LessonIssue>) {
    let mut seen = HashSet::new();
    for b in all {
        if b.id.is_empty() {
            continue; // already reported as schema
        }
        if !seen.insert(b.id.as_str()) {
            out.push(issue(
                format!("{}/id", b.path),
                Code::DuplicateId,
                format!("L'id di blocco '{}' è usato più volte nel documento.", b.id),
            ));
        }
    }
}

fn answer_position(blocks: &[Block], out: &mut Vec<LessonIssue>) {
    let positions: Vec<(&Block, usize)> = blocks
        .iter()
        .filter_map(|b| match &b.kind {
            BlockKind::QuizSingle {
                options, correct, ..
            } => options
                .iter()
                .position(|o| &o.id == correct)
                .map(|p| (b, p)),
            _ => None,
        })
        .collect();
    if positions.len() >= 3 && positions.iter().all(|(_, p)| *p == positions[0].1) {
        let (first, pos) = positions[0];
        out.push(issue(
            format!("{}/correct", first.path),
            Code::QuizAnswerPosition,
            format!(
                "Tutti i {} quiz a risposta singola hanno la risposta corretta in posizione {}: varia la posizione.",
                positions.len(),
                pos + 1
            ),
        ));
    }
}

fn fill_blank(b: &Block, text: &str, blank_ids: &[String], out: &mut Vec<LessonIssue>) {
    let used = placeholders(text);
    let mut seen = HashSet::new();
    for ph in &used {
        if !seen.insert(ph) {
            out.push(issue(
                format!("{}/text", b.path),
                Code::FillBlankMismatch,
                format!("Il segnaposto {{{{{}}}}} compare più volte nel blocco '{}': usalo una volta sola.", show(ph), b.id),
            ));
            continue;
        }
        if !blank_ids.contains(ph) {
            out.push(issue(
                format!("{}/text", b.path),
                Code::FillBlankMismatch,
                format!(
                    "Il segnaposto {{{{{}}}}} del blocco '{}' non ha un blank corrispondente.",
                    show(ph),
                    b.id
                ),
            ));
        }
    }
    for (i, id) in blank_ids.iter().enumerate() {
        if !used.contains(id) {
            out.push(issue(
                format!("{}/blanks/{i}", b.path),
                Code::FillBlankMismatch,
                format!(
                    "Il blank '{id}' del blocco '{}' non compare nel testo come {{{{{id}}}}}.",
                    b.id
                ),
            ));
        }
    }
}

/// Serialized size without allocating the string.
fn json_size(value: &Value) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    // Serializing a `Value` into a counter cannot fail.
    let _ = serde_json::to_writer(&mut counter, value);
    counter.0
}

/// How much longer than the median wrong option the correct one may be.
const CUE_RATIO: f64 = 1.3;

/// The right answer must not be recognizable by its length: each correct
/// option is compared, in visible characters, with the median of the wrong
/// ones (contract §6, `quiz_option_length_cue`).
fn length_cue(
    b: &Block,
    options: &[super::model::QuizOption],
    correct: &[String],
    out: &mut Vec<LessonIssue>,
) {
    let mut wrong: Vec<usize> = options
        .iter()
        .filter(|o| !correct.contains(&o.id))
        .map(|o| o.chars)
        .collect();
    if wrong.is_empty() {
        return;
    }
    wrong.sort_unstable();
    let mid = wrong.len() / 2;
    #[allow(clippy::cast_precision_loss)] // lengths of a few hundred characters
    let median = if wrong.len() % 2 == 1 {
        wrong[mid] as f64
    } else {
        (wrong[mid - 1] + wrong[mid]) as f64 / 2.0
    };
    for (i, o) in options.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        if correct.contains(&o.id) && o.chars as f64 > CUE_RATIO * median {
            out.push(issue(
                format!("{}/options/{i}/text", b.path),
                Code::QuizOptionLengthCue,
                format!(
                    "Nel quiz '{}' l'opzione corretta '{}' è molto più lunga delle sbagliate ({} caratteri contro una mediana di {median}): la risposta si riconosce dalla lunghezza. Accorcia la corretta o allunga le sbagliate (stessa precisione, stesso registro) senza superare {} volte la mediana.",
                    b.id,
                    o.id,
                    o.chars,
                    CUE_RATIO.to_string().replace('.', ",")
                ),
            ));
        }
    }
}
