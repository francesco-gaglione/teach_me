# Fixture del contratto SDUI (catalogo v1)

Riferimento: `../sdui.md` (§6 codici del Validator, §8 struttura) e `../sdui-schema-v1.json`. Tutte le fonti hanno `resource_id: null` (ammesso solo nelle fixture). Il campo `open_answer.rubric` è presente: sono documenti come salvati dal backend, non come li riceve l'app (§7).

## Lezioni valide (devono superare schema e tutti i controlli del §6 senza contesto del Topic)

- `lesson-rust-ownership.json`: Rust, ownership/move/borrowing; heading, paragraph, glossary_term, code, callout, 3 quiz_single (risposta corretta in posizioni 1, 0, 2), quiz_multi, fill_blank, flashcard facoltativa, un quiz_single con `code` (§4.1), primary_source, lesson_link, ask_agent.
- `lesson-photo-exposure.json`: fotografia, diaframma/tempo/ISO; callout, quiz_single, missing_component (`exposure_simulator` con fallback paragraph + code), open_answer con rubric, flashcard, checklist, primary_source, ask_agent con label.
- `lesson-git-staging.json`: git, staging area e commit; code bash, quiz_multi, quiz_single, callout warning, order_steps, fill_blank facoltativo, checklist senza titolo, primary_source e ask_agent nell'ultima sezione.
- `lesson-scientific-method.json`: scienze, metodo scientifico; i tre `layout` di `diagram` (`cycle` senza archi, `tree`, `flow` con diramazione ed etichette sugli archi), fill_blank, order_steps, primary_source, ask_agent. Nessun quiz.
- `unknown-type.json`: `lesson-git-staging.json` più un blocco di tipo inventato `timeline`; l'app mostra la riga neutra, il Validator risponde solo `unknown_block_type`.

## Lezioni non valide (`invalid/<code>.json` + `invalid/<code>.expected.json` con i codici attesi)

Ognuna è una variazione minima di una lezione valida che produce solo il codice indicato.

- `schema`: git, callout con `variant: "danger"` (fuori enum).
- `unknown_block_type`: rust, blocco `memory_diagram` inventato dopo il callout.
- `duplicate_id`: rust, il secondo blocco code riusa l'id `s1_code`.
- `quiz_correct_missing`: rust, `q2.correct` = `"e"`, opzione inesistente.
- `quiz_option_length`: rust, un'opzione di `q1` allungata a 11 parole contro 4.
- `quiz_answer_position`: rust, la risposta corretta dei 3 quiz_single è sempre la seconda.
- `fill_blank_mismatch`: rust, il segnaposto `{{op_int}}` rinominato `{{op_i32}}` (segnaposto senza blank e blank non usato).
- `unknown_citation`: rust, citazione aggiuntiva `[^rust_book_4_3]` non presente in `sources`.
- `unknown_glossary_key`: rust, riferimento `[[heap]]` senza voce di glossario.
- `unused_entry`: rust, voce di glossario `lifetime` mai usata.
- `primary_source_count`: rust, un secondo primary_source.
- `ask_agent_missing`: rust, ask_agent rimosso.
- `no_required_exercise`: rust, tutti gli esercizi con `required: false`.
- `too_long`: git, 13 paragrafi aggiunti, oltre 2200 parole di prosa (blocchi e sezioni nei limiti).
- `first_block_not_title`: git, il paragrafo introduttivo spostato prima dell'heading di livello 1.
- `section_without_content`: git, heading 2 vuoto subito prima di un altro heading 2.
- `nested_missing_component`: foto, missing_component dentro il fallback di `exposure_simulator`.
- `diagram_structure`: metodo scientifico, un arco in più `smentita → dati` chiude un ciclo nel `flow` di `fig_flow`.
- `markdown`: git, link `http://` (non https) in un paragrafo.
- `inline_code_too_long`: rust, la domanda di `q3` con un intero programma come codice inline invece che nel campo `code`.
