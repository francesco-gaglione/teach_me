# Contratto: Mission e preferenze

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.4** (2026-11-18: tool `request_lesson`, §6), v1.3 (2026-11-15: proposta superata, §3 `replaces_version`), v1.2 (2026-11-11), Fase 3.
> **Fase 12 (v1.2):** nuovo campo `starting_point` della mission (che cosa la persona sa già), `null` per le versioni precedenti; il Tutor lo chiede sempre nell'intervista (§2, §6, §8).
> **Fase 11 (v1.1):** le preferenze, comprese quelle globali, sono dell'utente: valgono solo per i suoi argomenti e non entrano mai nei prompt di altri. La conferma della mission riesce anche senza chiave LLM (`users.md` §3).
> Valgono `api-conventions.md`, `folders-topics.md` e `chat.md`. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- La **mission** di un Topic dice *perché* l'utente impara quell'argomento e *come capirà di esserci riuscito*. Guida tutte le lezioni (regola: **nessuna lezione senza mission confermata**, `docs/mvp.md` §5).
- La mission nasce in chat: il Tutor intervista l'utente e, quando ha abbastanza informazioni concrete, salva una **bozza** con il tool interno `save_mission_draft`. La bozza compare nella chat come **card nativa** (parte `mission_proposal` del messaggio del Tutor) con i bottoni Conferma / Modifica.
- La mission è **versionata**: ogni bozza ha un numero di versione crescente per Topic. Confermare una bozza la rende la mission corrente; le versioni precedenti restano nello storico.
- Le **preferenze** sono istruzioni stabili dell'utente sul modo di insegnare ("esempi in Python", "niente analogie sportive"), salvate dal Tutor con il tool interno `save_preference`, globali o per Topic. Equivalgono al `NOTES.md` della skill `teach`.

## 2. Risorse

### `Mission`

```json
{
  "topic_id": "01933b4e-...",
  "version": 2,
  "status": "draft",
  "why": "Mantenere il backend di pagamenti scritto in Rust nel mio nuovo lavoro.",
  "success_criteria": [
    "Leggo e modifico un servizio axum esistente senza aiuto",
    "Scrivo test per un modulo con tokio"
  ],
  "starting_point": "Programmo in Kotlin da tre anni e ho scritto un po' di C; di Rust ho letto solo i primi capitoli del libro ufficiale, mai scritto codice.",
  "constraints": ["30 minuti al giorno"],
  "out_of_scope": ["Rust embedded", "macro procedurali"],
  "created_at": "2026-11-08T14:36:01.129Z",
  "confirmed_at": null,
  "superseded_at": null
}
```

| Campo | Tipo | Note |
|---|---|---|
| `version` | int ≥ 1 | Crescente per Topic, assegnata dal backend |
| `status` | enum | `draft` (proposta, in attesa) · `confirmed` (mission corrente) · `superseded` (sostituita da una versione successiva: confermata o bozza scartata) |
| `why` | string | 1-500 caratteri. Il motivo concreto, nelle parole dell'utente quando possibile |
| `success_criteria` | string[] | 1-5 voci, ciascuna 1-200 caratteri. Osservabili ("so fare X"), non vaghe ("capire Rust") |
| `starting_point` | string \| null | 1-600 caratteri. Che cosa la persona sa già e sa fare che conta per questo argomento, e che cosa non ha mai visto: è la base da cui partono le lezioni (cosa non rispiegare, cosa non dare per scontato). `null` solo per le versioni salvate prima della v1.2. Dalla v1.2 |
| `constraints` | string[] | 0-5 voci, 1-200 caratteri. Tempo, strumenti, vincoli pratici. Dalla v1.2 le conoscenze di partenza vanno in `starting_point`, non qui |
| `out_of_scope` | string[] | 0-5 voci, 1-200 caratteri. Cosa *non* imparare ora |
| `confirmed_at` | datetime \| null | Valorizzato per le versioni confermate (anche se poi superate) |
| `superseded_at` | datetime \| null | Quando la versione è diventata `superseded`. Una `superseded` con `confirmed_at: null` è una bozza mai confermata |

Le regole di qualità (concretezza, niente vaghezza) le fa rispettare il prompt dell'Interviewer; i limiti di lunghezza e numero li verifica il backend quando il tool salva la bozza (se violati, il tool restituisce un errore al modello, che corregge; l'utente non vede l'errore).

### `TopicMission` (risposta di `GET /topics/{id}/mission`)

```json
{ "current": { "...": "Mission confermata o null" }, "draft": { "...": "Mission in bozza o null" } }
```

Al massimo una bozza aperta per Topic: una nuova bozza rende `superseded` quella aperta.

### `Preference`

```json
{ "id": "01933b4e-...", "scope": "topic", "topic_id": "01933b4e-...", "text": "Esempi in Python, non in Java", "created_at": "2026-11-08T14:36:01.129Z" }
```

| Campo | Tipo | Note |
|---|---|---|
| `scope` | enum | `global` (vale per tutti i Topic) · `topic` |
| `topic_id` | uuid \| null | Valorizzato solo con `scope: topic` |
| `text` | string | 1-300 caratteri |

## 3. Parte di messaggio `mission_proposal`

Nuovo tipo di parte in `ChatMessage.parts` (`chat.md` §1). Compare nel messaggio del Tutor che ha chiamato `save_mission_draft`, **dopo** il testo che introduce la proposta.

```json
{ "type": "mission_proposal", "mission": { "...": "Mission con status draft al momento della proposta" }, "replaces_version": 1 }
```

- `replaces_version`: la versione confermata che la bozza sostituirebbe se confermata (`null` se non c'è ancora una mission confermata). L'app la usa per i testi "Sostituirà la versione N" e per evidenziare le differenze. Se nel frattempo la versione confermata è cambiata (`replaces_version` diverso dalla versione confermata corrente), la proposta è **superata**: l'app la mostra come non più applicabile, senza bottoni (v1.3, issue #19).
- La parte `mission_proposal` è sempre **l'ultima** del messaggio, dopo tutto il testo.
- La parte è uno **snapshot**: non cambia quando la bozza viene confermata o superata. L'app decide lo stato della card confrontando `mission.version` con `GET /topics/{id}/mission` (o con gli eventi del §5): bozza aperta → bottoni attivi; confermata → "Confermata"; superata → "Sostituita da una proposta più recente" senza bottoni.
- App vecchie che non conoscono il tipo lo ignorano (regola di `chat.md`).

## 4. Endpoint

| Metodo | Path | Successo | Errori specifici |
|---|---|---|---|
| GET | `/topics/{id}/mission` | `200` `TopicMission` | `400 invalid_id`, `404` |
| GET | `/topics/{id}/mission/versions` | `200` lista paginata di `Mission`, `version` decrescente | `400`, `404` |
| POST | `/topics/{id}/mission/confirm` | `200` `TopicMission` aggiornata | `400 invalid_id`, `404`, `409 conflict`, `422` |
| GET | `/preferences` | `200` lista paginata di `Preference` | `400` |
| DELETE | `/preferences/{id}` | `204` | `400 invalid_id`, `404` |

### Conferma

```json
POST /topics/{id}/mission/confirm
{ "version": 2 }
```

- `version` obbligatoria: conferma **solo** quella bozza. Se non è la bozza aperta (già confermata, superata, inesistente) → `409 conflict` (l'app ricarica la mission). `version` mancante o non intera → `422` con `{field: "version", issue: "invalid"}`.
- Effetti: la bozza diventa `confirmed`, la mission confermata precedente diventa `superseded`; il Topic passa a `researching` (da `mission_proposed`). Alla prima conferma il backend crea anche un job `research` (`jobs.md`, `resources.md`); il Topic passa a `ready` quando il job riesce.
- La conferma **non** genera un messaggio in chat. Il Tutor la vede nel contesto del turno successivo.

### Modifica

Non c'è un endpoint: "Modifica" nell'app mette il focus sul composer; l'utente scrive cosa cambiare e il Tutor proporrà una nuova bozza. Inviare un messaggio mentre il Topic è `mission_proposed` riporta il Topic a `interviewing` (la bozza resta aperta finché non viene superata o confermata: l'utente può ancora confermarla).

### Preferenze

- `GET /preferences?topic_id=<uuid>`: senza filtro restituisce tutte le preferenze; con `topic_id` restituisce le globali **e** quelle di quel Topic. Ordine: `created_at` crescente, poi `id`.
- Le preferenze di un Topic vengono eliminate con il Topic.
- In v1 non si creano né modificano preferenze dall'app: le crea il Tutor; l'app le mostra e permette di eliminarle.

## 5. Stati del Topic e nuovi eventi

Transizioni della Fase 3 (`docs/mvp.md` §5, semplificato dal tech lead per i cambi di mission):

**Senza mission confermata** (prima mission):

| Da | A | Causa |
|---|---|---|
| `new` / `interviewing` | `mission_proposed` | il Tutor salva una bozza (`save_mission_draft`) |
| `mission_proposed` | `interviewing` | l'utente invia un messaggio (la bozza resta aperta e confermabile) |
| `mission_proposed` | `researching` | conferma della bozza |

**Con una mission confermata** (cambio di mission, `propose_mission_change`): la bozza **non cambia lo stato del Topic** (resta `researching` o `ready`), così lezioni e ripassi non si bloccano mentre una proposta è in attesa; la bozza è visibile come card e nella vista Mission. La conferma rende la bozza la mission corrente senza cambiare stato in Fase 3 (dalla Fase 4 l'app propone all'utente di rifare la ricerca delle fonti, `resources.md` §3; nessuna ricerca automatica). Questo sostituisce la transizione `Ready → MissionProposed` di `docs/mvp.md` §5.

Ogni cambio di stato emette `topic_updated` (`chat.md` §3). Nuovi eventi SSE:

| `type` | `payload` | Quando |
|---|---|---|
| `mission_proposed` | `{ "mission": Mission }` | Salvata una nuova bozza (subito prima del `message_completed` del messaggio che la contiene) |
| `mission_confirmed` | `{ "mission": Mission }` | Bozza confermata |
| `preference_saved` | `{ "preference": Preference }` | Il Tutor ha salvato una preferenza |
| `preference_deleted` | `{ "preference_id": "...", "scope": "topic", "topic_id": "..." }` | Preferenza eliminata via `DELETE /preferences/{id}`. Emesso sul Topic della preferenza; per le globali su **tutti** i Topic con uno stream aperto |

## 6. Tool del Tutor (informativo)

Non fanno parte del contratto HTTP; fissano l'intento per `llm-engineer` e backend. Il ciclo dei tool è nell'application (ADR 0004); i tool call e i loro risultati **non** sono esposti all'app come messaggi.

| Tool | Input | Effetto |
|---|---|---|
| `save_mission_draft` | `why`, `starting_point`, `success_criteria[]`, `constraints[]`, `out_of_scope[]` | Ammesso solo se il Topic **non** ha una mission confermata. Nuova bozza + parte `mission_proposal` + stato `mission_proposed` |
| `propose_mission_change` | come sopra | Ammesso solo se il Topic **ha** una mission confermata. Nuova bozza + parte `mission_proposal`, stato invariato |
| `request_lesson` | nessuno | Ammesso solo con Topic `ready`. Mette in coda un job `lesson_generation` come `POST /topics/{id}/lessons` + parte `lesson_request` (`lessons.md` §12, v1.4) |
| `save_preference` | `scope`, `text` | Nuova preferenza (`topic_id` = Topic corrente se `scope: topic`); duplicati (stesso ambito, stesso testo a meno di maiuscole e spazi) non vengono salvati |

L'ammissibilità dipende dai dati (c'è o no una mission confermata), non dallo stato del Topic. Al più un tool di mission per turno del Tutor.

`start_research` non è un tool: la ricerca parte dalla conferma della mission (`resources.md`). `request_lesson` dalla v1.4 (issue #17); un turno ha al più una card, quindi niente `request_lesson` nello stesso turno di un tool di mission.

## 7. Precisazioni (dopo l'implementazione, v1.1)

- **Prima conferma** ammessa con Topic `mission_proposed` **o** `interviewing` (l'utente può aver scritto dopo la proposta). Con Topic in uno stato incompatibile (`new`, oppure `researching`/`ready` senza mission confermata: dati incoerenti) → `409 conflict`. Una conferma quando esiste già una mission confermata (cambio di mission) non cambia stato e non emette `topic_updated`.
- `POST .../mission/confirm` con corpo che non è un oggetto JSON → `422 validation_failed` con `details: null` (come gli altri endpoint).
- `GET /preferences`: `topic_id` malformato, `limit` fuori intervallo o cursore non valido → `400 bad_request`. Un `topic_id` inesistente restituisce solo le preferenze globali (nessun 404).
- **`preference_saved`** per una preferenza globale esce sullo stream del Topic della conversazione in cui è stata salvata.
- **`preference_deleted`** per una preferenza globale arriva solo agli stream aperti in quel momento e non entra nel buffer di replay dei Topic senza ascoltatori: l'app **ricarica le preferenze** quando apre uno stream o dopo un `resync_required`.
- **`mission_proposed`** può ripetersi con la stessa `version` (per esempio dopo "Riprova" di un messaggio che aveva già proposto quella bozza): l'app fa upsert per `(topic_id, version)`.
- Se il messaggio che contiene la card **fallisce**, la card resta nelle parti parziali del messaggio fallito e la bozza resta aperta e confermabile. "Riprova" azzera le parti; se il Tutor riproduce la stessa bozza, la card ricompare con la stessa versione.
- **Ciclo dei tool**: al massimo 4 giri di tool per messaggio. Se il modello chiede un quinto giro, il backend non lo esegue e chiude il messaggio: `complete` (con un warning nei log) se c'è già del testo, altrimenti `failed` con `code: internal`.
- `GET /topics/{id}/mission` per un Topic senza mission restituisce `200 {"current": null, "draft": null}`, mai 404 (404 solo se il Topic non esiste).
- Le versioni di un Topic sono **contigue** a partire da 1 (una versione non viene mai cancellata finché esiste il Topic): il numero di versioni precedenti si ricava dalla versione corrente.
- La parte `mission_proposal` viene aggiunta al messaggio **alla sua chiusura** (`message_completed` o `message_failed`), non durante lo streaming: uno snapshot REST di un messaggio `streaming` non contiene ancora la card, e gli indici dei `message_delta` si riferiscono solo alle parti di testo.
- Al massimo 50 preferenze per ambito (globali; ciascun Topic): oltre il limite il Tutor riceve un errore e non salva.

## 8. Punto di partenza (v1.2, Fase 12)

- `starting_point` è **obbligatorio nei tool** `save_mission_draft` e `propose_mission_change` (1-600 caratteri dopo il trim): senza, il tool restituisce un errore al modello come per gli altri limiti. Il Tutor chiede sempre che cosa la persona sa già prima di proporre una mission; se la persona parte da zero lo scrive esplicitamente ("Parto da zero: mai programmato"), non lascia il campo vuoto.
- Nelle risposte HTTP e negli eventi il campo è **sempre presente**: stringa, oppure `null` per le versioni create prima della v1.2. Nessuna versione esistente viene riscritta.
- Negli **snapshot** `mission_proposal` già salvati nei messaggi (§3) la chiave può **mancare**: backend e app trattano la chiave assente come `null`.
- Una mission confermata con `starting_point: null` resta valida: lezioni e ricerca continuano a funzionare e i prompt ricevono "non dichiarato". Per valorizzarlo serve un cambio di mission (`propose_mission_change`), che il Tutor propone quando la persona parla di ciò che sa già.
- L'app mostra la sezione «Punto di partenza» nella scheda Mission e nella card in chat; con `null` la sezione non compare.
- `starting_point` entra nei contesti di Tutor, Researcher, Planner e Generator. Non sostituisce i learning record: un prerequisito è «dimostrato» solo con un record, «dichiarato» se sta nel punto di partenza.
