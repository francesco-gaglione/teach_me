# Indice di `docs/contracts/`

Apri il file alla sezione che serve (`grep -n '^#' <file>` per i numeri di riga); non leggere i file interi.

## Radice di `contracts/`

- [`api-conventions.md`](api-conventions.md) · 14.6 KB — Convenzioni API comuni: trasporto e compatibilità, auth, identificatori, date, errori, paginazione, job, SSE, limiti.
  - Titoli: §1 Trasporto, versionamento e compatibilità · §2 Autenticazione · §3 Endpoint di servizio · §4 Identificatori · §5 Date e ore · §6 Metodi e codici di stato · §7 Errori · §8 Paginazione · §9 Job (operazioni lunghe) · §10 Eventi SSE · §11 Header · §12 Limiti e timeout
- [`audio.md`](audio.md) · 8.2 KB — Contratto audio: blocchi leggibili, manifest per lezione, voci, clip, regole per l'app.
  - Titoli: §1 Concetti · §2 Blocchi leggibili e testo parlato · §3 Manifest dell'audio di una lezione · §4 Voci · §5 Clip · §6 Regole per l'app
- [`chat.md`](chat.md) · 10.5 KB — Contratto chat: risorsa messaggio, endpoint, eventi ed SSE del Tutor.
  - Titoli: §1 Risorsa ChatMessage · §2 Endpoint · §3 Eventi della chat · §4 Stream SSE · §5 Contesto inviato al modello (informa…
- [`component-requests.md`](component-requests.md) · 2.2 KB — Concetti ed endpoint per le richieste di componenti mancanti.
- [`documents.md`](documents.md) · 8.1 KB — Documenti caricati dall'utente (PDF): risorsa, endpoint, job di import, eventi.
  - Titoli: §1 Concetti · §2 Risorsa Document · §3 Endpoint · §4 Il job document_import · §5 Eventi · §6 Documenti nelle lezioni · §7 Fuori da questa versione
- [`folders-topics.md`](folders-topics.md) · 8.1 KB — Cartelle e argomenti: risorse (con la lingua dell'argomento), endpoint, errori di validazione, testi lato app.
  - Titoli: §1 Risorse · §2 Endpoint · §3 Errori di validazione · §4 Testi lato app
- [`jobs.md`](jobs.md) · 6.1 KB — Job asincroni: risorsa Job, endpoint, eventi SSE.
  - Titoli: §1 Concetti · §2 Risorsa Job · §3 Endpoint · §4 Eventi SSE
- [`lessons.md`](lessons.md) · 36.8 KB — Lezioni (v1.9: `language` in `LessonView`; v1.8: `document_sources`, `GET /lessons?status=in_progress`, fase `reading`): risorse, generazione, tentativi, completamento, evidenziazioni, note a mano, lezioni dalla chat.
  - Titoli: §1 Concetti · §2 Risorse · §3 Endpoint · §4 Job lesson_generation · §5 Tentativi · §6 Chiusura e job lesson_completion · §7 Chat dal bottone ask_agent · §8 Eventi · §9 Fuori da questa versione · §10 Evidenziazioni (v1.2) · §11 Note a mano (v1.4) · §12 Lezione richiesta dalla chat (v1.5,…
- [`mission.md`](mission.md) · 14.8 KB — Mission dell'argomento: risorse, proposta, endpoint, stati del Topic, tool del Tutor.
  - Titoli: §1 Concetti · §2 Risorse · §3 Parte di messaggio mission_proposal · §4 Endpoint · §5 Stati del Topic e nuovi eventi · §6 Tool del Tutor (informativo) · §7 Precisazioni (dopo l'implementazione… · §8 Punto di partenza (v1.2)
- [`resources.md`](resources.md) · 7.0 KB — Fonti web: risorse, endpoint, job research ed eventi.
  - Titoli: §1 Concetti · §2 Risorse · §3 Endpoint · §4 Il job research · §5 Eventi
- [`reviews.md`](reviews.md) · 14.0 KB — Ripasso FSRS: concetti, ReviewCard, endpoint, voto, annulla, eventi.
  - Titoli: §1 Concetti · §2 Risorsa ReviewCard · §3 Endpoint · §4 Voto · §5 Annulla · §6 Eventi · §7 Fuori da questa versione
- [`sdui-schema-v1.json`](sdui-schema-v1.json) · 30.4 KB — JSON Schema del documento SDUI v1.
- [`sdui.md`](sdui.md) · 22.9 KB — Contratto SDUI: documento, blocchi, catalogo v1, validator, fixture, diagrammi.
  - Titoli: §1 Principi · §2 Documento · §3 Blocchi · §4 Catalogo v1 · §5 Correzione locale (app) · §6 Validator (backend) · §7 Cosa riceve l'app · §8 Fixture · §9 Precisazioni (v1.1, dopo la stesura… · §10 Diagrammi (v1.3)
- [`users.md`](users.md) · 8.6 KB — Utenti: risorsa User, endpoint, operazioni che usano l'LLM, isolamento, gestione dal server.
  - Titoli: §1 Risorsa User · §2 Endpoint · §3 Operazioni che usano l'LLM · §4 Isolamento · §5 Gestione dal server · §6 Passaggio dalla versione a utente si…

## Fixture (`fixtures/`)

- [`fixtures/README.md`](fixtures/README.md) · 3.4 KB — Elenco delle fixture di lezioni valide e non valide, con i codici attesi.

## Fixture JSON (una riga per cartella)

- **`fixtures/`** (radice): 4 lezioni valide (`lesson-*.json`) e `unknown-type.json`, 5 file JSON. Da non leggere: servono ai test.
- **`fixtures/invalid/`**: 20 casi non validi, ciascuno con `<code>.json` e `<code>.expected.json` (40 file JSON). Da non leggere: servono ai test.
