# teach_me — MVP

> Documento di design. Stato: **bozza v0.1** (2026-11-08).
> I diagrammi sono in Mermaid: si vedono renderizzati su GitHub, GitLab, VS Code (estensione "Markdown Preview Mermaid Support") e IntelliJ/Android Studio.

## 1. Obiettivo dell'MVP

**Criterio di successo:** creo il Topic "Rust" in una cartella, l'agente mi intervista e definisce la mission, trova le fonti, mi genera la prima lezione nativa con quiz che completo in pochi minuti, registra cosa ho dimostrato di sapere, e **il giorno dopo** l'app mi propone il ripasso FSRS di quei concetti.

Se questo ciclo funziona end-to-end, l'MVP è fatto. Tutto il resto è ottimizzazione.

## 2. Scope

### Dentro l'MVP

| Area | Cosa |
|---|---|
| Organizzazione | Cartelle e Topic: crea, rinomina, elimina. Un Topic = un workspace con una mission |
| Chat | Chat con il Tutor dentro ogni Topic, risposte in streaming (SSE) |
| Mission | Intervista guidata → proposta di mission → conferma dell'utente. Modifica successiva con conferma |
| Fonti | Ricerca web delle fonti (knowledge; community/wisdom solo se l'utente le chiede, mai nelle lezioni), annotate, visibili nell'app |
| Lezioni | Generazione asincrona di una lezione: planner → generator → validator → repair |
| UI | Renderer Compose con il **catalogo v1** (sezione 6) |
| Feedback loop | Quiz chiusi corretti sul telefono; risposte aperte valutate dall'agente |
| Learning record | Proposti automaticamente **solo da evidenze** (tentativi), visibili e modificabili |
| Glossario | Termini come entità, mostrati nelle lezioni e in una schermata dedicata |
| Preferenze | Il Tutor salva le preferenze dalla chat (l'equivalente di `NOTES.md`), globali o per Topic |
| Ripasso | Schede FSRS generate da termini e record, schermata "Ripasso di oggi" su tutti i Topic |
| Monitoraggio | Livello 1: blocco `missing_component` con fallback + lista delle richieste |
| Provider | Solo l'adapter **Anthropic**, ma dietro il port `LlmProvider` già definitivo |
| Infrastruttura | Un solo binario Rust, SQLite, accesso via Tailscale. Più utenti con nome utente e password, dati isolati e chiave LLM per utente (ADR 0008, `contracts/users.md`) |

### Fuori dall'MVP

- Adapter OpenAI-compatible (OpenAI, OpenRouter, Ollama/llama.cpp/vLLM)
- Reference document (cheat sheet). Il glossario copre l'essenziale
- Monitoraggio livelli 2 e 3 (clustering delle richieste, feedback per blocco)
- Componenti avanzati: immagini, simulatori (i diagrammi sono entrati con il blocco `diagram`, ADR 0010)
- Notifiche push per i ripassi, modalità offline, generazione delle lezioni in streaming blocco per blocco

## 3. Architettura generale

```mermaid
flowchart LR
    subgraph Android["App Android - Kotlin + Compose"]
        UI["Schermate<br/>Cartelle, Topic, Chat,<br/>Lezione, Ripasso"]
        R["SDUI Renderer<br/>registry dei componenti"]
        G["Correzione locale<br/>quiz chiusi"]
        C["Client HTTP + SSE"]
        UI --> R --> G
        UI --> C
        G --> C
    end

    C -- "HTTPS via Tailscale<br/>Bearer token" --> API

    subgraph Backend["Backend Rust - homelab"]
        API["api<br/>axum: REST + SSE"]
        APP["application<br/>casi d'uso + job runner"]
        DOM["domain<br/>entità, regole, port"]
        subgraph INF["infrastructure - adapter"]
            LLM["llm-anthropic"]
            SRCH["search"]
            DB["sqlite - sqlx"]
            FS["scheduler-fsrs"]
        end
        API --> APP --> DOM
        INF -. implementa i port .-> DOM
    end

    LLM --> Claude["Claude API"]
    SRCH --> Web["Motore di ricerca<br/>+ fetch pagine"]
    DB --> File[("teach_me.db")]
```

### Regola delle dipendenze (clean architecture)

```mermaid
flowchart TB
    api["api - axum"] --> application
    infrastructure --> domain
    application --> domain
    api --> infrastructure
    domain["domain<br/>nessuna dipendenza esterna"]
```

- `domain` non conosce axum, sqlx, Anthropic. Contiene entità, regole (es. validazione dei quiz) e i **port** come trait.
- `application` orchestra i casi d'uso usando solo i trait.
- `infrastructure` implementa i trait.
- `api` è il punto di composizione: costruisce gli adapter e li inietta (composition root).

### Port del dominio

| Port | Responsabilità | Adapter MVP | Adapter futuri |
|---|---|---|---|
| `LlmProvider` | messaggi, tool calling, output strutturato, streaming, capability | Anthropic | OpenAI-compatible (OpenAI, OpenRouter, locali) |
| `SearchProvider` | cerca sul web e restituisce risultati | da scegliere (sezione 11) | altri motori |
| `PageFetcher` | scarica e ripulisce il testo di una pagina | reqwest + estrazione testo | — |
| `ReviewScheduler` | calcola il prossimo ripasso | FSRS | — |
| `*Repository` | persistenza di ogni aggregato | SQLite | Postgres |
| `Clock`, `IdGenerator` | tempo e id, per test deterministici | sistema | — |

Il modello da usare si configura **per ruolo dell'agente**: ad esempio un modello forte per planner e generator, uno economico per il grader. Così si ottimizzano i costi, e in futuro si può usare un modello locale per i ruoli semplici.

## 4. Gli agenti

Invece di un solo prompt gigante come nella skill, ci sono ruoli specializzati. Ognuno riceve **solo il contesto che gli serve** e risponde con **output strutturato**.

```mermaid
flowchart LR
    CA["Context Assembler<br/>codice, non LLM"] --> T & P & GEN & GR & REC

    T["Tutor<br/>chat con tool"]
    I["Interviewer<br/>mission"]
    RS["Researcher<br/>fonti"]
    P["Planner<br/>cosa insegnare ora"]
    GEN["Generator<br/>documento SDUI"]
    GR["Grader<br/>risposte aperte"]
    REC["Recorder<br/>record, glossario, schede"]

    T -- "tool: start_research" --> RS
    T -- "tool: request_lesson" --> P --> GEN
    T -. "durante l'intervista" .-> I
```

| Ruolo | Input | Output strutturato |
|---|---|---|
| **Tutor** | chat, mission, preferenze, record attivi | testo + tool call (`save_mission_draft`, `save_preference`, `start_research`, `request_lesson`, `propose_mission_change`) |
| **Interviewer** | risposte dell'utente | `MissionDraft` (why, starting_point, success, constraints, out_of_scope) |
| **Researcher** | mission, risultati di ricerca, pagine scaricate | `Resource[]` con tipo, annotazione e "use for" |
| **Planner** | mission, record, glossario, lezioni precedenti, fonti | `LessonPlan` (obiettivo unico, prerequisiti, concetti, fonte primaria, tipi di esercizio) |
| **Generator** | `LessonPlan`, spec del catalogo, glossario, fonti | `LessonDocument` (JSON SDUI) |
| **Grader** | domanda, rubrica, risposta | `Grade` (corretto/parziale/errato, feedback, misconcezione rilevata) |
| **Recorder** | tentativi della lezione, record esistenti | `LearningRecord[]`, `GlossaryTerm[]`, `ReviewCard[]` |

Il **Context Assembler** è codice deterministico: sceglie cosa mettere nel prompt (mission, solo i record attivi, il glossario del Topic, le preferenze) invece di far rileggere tutto all'agente.

## 5. Ciclo di vita di un Topic

```mermaid
stateDiagram-v2
    [*] --> New: crea Topic
    New --> Interviewing: primo messaggio
    Interviewing --> MissionProposed: Interviewer ha abbastanza info
    MissionProposed --> Interviewing: utente corregge
    MissionProposed --> Researching: utente conferma
    Researching --> Ready: fonti salvate
    Ready --> Ready: lezioni, chat, ripassi
    Ready --> Ready: cambio di mission (bozza e conferma, stato invariato; vedi contracts/mission.md)
```

Regola ereditata dalla skill: **nessuna lezione senza mission confermata.**

## 6. Catalogo componenti v1

Ogni blocco ha `id` (stabile, serve per tentativi e feedback) e `type`; la compatibilità è data da `catalog_version` del documento (contratto completo: `docs/contracts/sdui.md`).

| Tipo | Uso | Correzione |
|---|---|---|
| `heading` | titolo di sezione | — |
| `paragraph` | testo con markdown inline (grassetto, corsivo, `code`, link) e citazioni `[^res_id]` | — |
| `callout` | info / tip / warning | — |
| `code` | snippet con linguaggio | — |
| `glossary_term` | definizione di un termine del glossario, tappabile | — |
| `quiz_single` | scelta singola | locale |
| `quiz_multi` | scelta multipla | locale |
| `fill_blank` | completa la frase | locale (match normalizzato) |
| `order_steps` | metti in ordine | locale |
| `flashcard` | domanda → rivela → autovalutazione | locale |
| `open_answer` | risposta libera con rubrica nascosta | backend (Grader) |
| `checklist` | passi da fare nel mondo reale | autodichiarata |
| `primary_source` | la fonte principale consigliata | — |
| `lesson_link` | link a un'altra lezione | — |
| `ask_agent` | "hai dubbi? chiedi al Tutor", apre la chat con il contesto della lezione | — |
| `missing_component` | componente desiderato + `fallback` di blocchi esistenti | — |

### Regole verificate dal Validator (codice, non prompt)

- Lo JSON rispetta lo schema del catalogo e usa solo tipi esistenti.
- Quiz: opzioni con **lo stesso numero di parole** (tolleranza configurabile); la posizione della risposta giusta varia tra le domande della lezione.
- Ogni `[^res_id]` punta a una fonte esistente del Topic.
- Ci sono esattamente un `primary_source` e almeno un `ask_agent`.
- La lezione ha almeno un esercizio con feedback (vincolo "single tangible win").
- Lunghezza massima (la lezione deve restare corta).
- I termini usati in `glossary_term` esistono nel glossario o sono nuovi e dichiarati.

## 7. Flussi

### 7.1 Creazione del Topic e intervista sulla mission

```mermaid
sequenceDiagram
    actor U as Utente
    participant A as App
    participant B as Backend
    participant T as Tutor/Interviewer
    participant L as Claude

    U->>A: crea Topic "Rust" nella cartella "Lavoro"
    A->>B: POST /folders/{id}/topics
    B-->>A: Topic (stato New)
    U->>A: "voglio imparare Rust"
    A->>B: POST /topics/{id}/messages
    B->>T: contesto + messaggio
    T->>L: richiesta
    L-->>B: stream: domanda "perché ti serve?"
    B-->>A: SSE token per token
    Note over U,L: qualche scambio di domande e risposte
    T->>B: tool save_mission_draft
    B-->>A: evento mission_proposed
    A->>U: card nativa con la mission e i bottoni Conferma / Modifica
    U->>A: Conferma
    A->>B: POST /topics/{id}/mission/confirm
    B->>B: crea job Research
```

### 7.2 Ricerca delle fonti

```mermaid
sequenceDiagram
    participant J as Job runner
    participant RS as Researcher
    participant S as SearchProvider
    participant F as PageFetcher
    participant DB as SQLite

    J->>RS: mission
    RS->>RS: genera query di ricerca
    loop per ogni query
        RS->>S: cerca
        S-->>RS: risultati
    end
    RS->>F: scarica le pagine candidate migliori
    F-->>RS: testo pulito
    RS->>RS: valuta affidabilità, annota, scarta il marketing
    RS->>DB: salva Resource[] (knowledge, community se chieste)
    RS->>DB: salva eventuali Gap
    J-->>DB: Topic -> Ready
```

### 7.3 Generazione di una lezione

È l'operazione più lunga (decine di secondi): per questo è un **job asincrono**. L'app vede l'avanzamento via SSE.

```mermaid
flowchart TD
    Start(["Utente: Prossima lezione<br/>o richiesta in chat"]) --> Job["crea Job lesson_generation"]
    Job --> CA["Context Assembler<br/>mission, record attivi, glossario,<br/>preferenze, fonti, lezioni precedenti"]
    CA --> P["Planner → LessonPlan"]
    P --> G["Generator → LessonDocument"]
    G --> V{"Validator"}
    V -- "ok" --> S["Salva la lezione"]
    V -- "errori" --> RP{"tentativi < 3?"}
    RP -- "sì" --> FIX["Generator riceve<br/>la lista degli errori"] --> V
    RP -- "no" --> FAIL["Job fallito<br/>errore visibile nell'app"]
    S --> MC["Estrai i blocchi missing_component<br/>→ ComponentRequest"]
    MC --> N(["SSE: lesson_ready"])
```

### 7.4 Svolgimento della lezione

```mermaid
sequenceDiagram
    actor U as Utente
    participant A as App
    participant B as Backend
    participant GR as Grader
    participant REC as Recorder

    A->>B: GET /lessons/{id}
    B-->>A: LessonDocument
    A->>U: renderer Compose
    U->>A: risponde a quiz_single
    A->>A: correzione locale, feedback immediato
    A->>B: POST /lessons/{id}/attempts (in batch)
    U->>A: risponde a open_answer
    A->>B: POST /lessons/{id}/attempts
    B->>GR: domanda + rubrica + risposta
    GR-->>B: Grade + feedback
    B-->>A: feedback
    U->>A: Completa lezione
    A->>B: POST /lessons/{id}/complete
    B->>REC: tentativi + record esistenti
    REC-->>B: LearningRecord[], GlossaryTerm[], ReviewCard[]
    B-->>A: riepilogo "cosa hai dimostrato"
```

Il Recorder segue le regole della skill: **la copertura non è apprendimento**. Se un concetto è stato solo spiegato ma non dimostrato in un esercizio, non diventa un record né una scheda. Una misconcezione corretta invece genera sempre un record.

### 7.5 Ripasso FSRS

```mermaid
sequenceDiagram
    actor U as Utente
    participant A as App
    participant B as Backend
    participant S as ReviewScheduler

    A->>B: GET /reviews/due
    B-->>A: schede in scadenza (tutti i Topic)
    loop per ogni scheda
        A->>U: fronte
        U->>A: rivela
        A->>U: retro
        U->>A: Again / Hard / Good / Easy
        A->>B: POST /reviews/{card_id}
        B->>S: stato scheda + voto + ora
        S-->>B: nuovo stato + prossima scadenza
        B->>B: salva ReviewLog
    end
```

Le schede nascono dal Recorder (sezione 7.4) e sono legate al termine o al record da cui derivano. Se un record viene superato (`superseded`), le sue schede vengono sospese.

### 7.6 Monitoraggio dei componenti mancanti

```mermaid
flowchart LR
    GEN["Generator vuole<br/>un componente inesistente"] --> MC["blocco missing_component<br/>wanted + purpose + fallback"]
    MC --> APP["L'app mostra il fallback"]
    MC --> REQ[("ComponentRequest")]
    REQ --> LIST["GET /component-requests<br/>classifica per frequenza"]
    LIST --> DEV["Implementi il componente<br/>in Compose"]
    DEV --> CAT["Catalogo v2<br/>+ min_app_version"]
    CAT --> GEN
```

Nell'MVP il raggruppamento è per nome esatto di `wanted`; il clustering semantico è fuori dall'MVP.

## 8. Modello dati

```mermaid
erDiagram
    FOLDER ||--o{ TOPIC : contiene
    TOPIC ||--o| MISSION : ha
    TOPIC ||--o{ CHAT_MESSAGE : ha
    TOPIC ||--o{ RESOURCE : ha
    TOPIC ||--o{ LESSON : ha
    TOPIC ||--o{ LEARNING_RECORD : ha
    TOPIC ||--o{ GLOSSARY_TERM : ha
    TOPIC ||--o{ PREFERENCE : "ha (o globale)"
    TOPIC ||--o{ JOB : ha
    LESSON ||--o{ ATTEMPT : riceve
    LESSON ||--o{ COMPONENT_REQUEST : genera
    LEARNING_RECORD ||--o{ REVIEW_CARD : origina
    GLOSSARY_TERM ||--o{ REVIEW_CARD : origina
    REVIEW_CARD ||--o{ REVIEW_LOG : ha
    LEARNING_RECORD |o--o| LEARNING_RECORD : supersedes

    FOLDER { uuid id string name int position }
    TOPIC { uuid id uuid folder_id string title string state }
    MISSION { uuid topic_id string why string starting_point json success json constraints json out_of_scope int version }
    RESOURCE { uuid id string kind string title string url string annotation string use_for }
    LESSON { uuid id int number string title json document int catalog_version string status }
    ATTEMPT { uuid id string block_id json answer string outcome json feedback datetime at }
    LEARNING_RECORD { uuid id int number string kind string title string body string evidence string status }
    GLOSSARY_TERM { uuid id string term string definition json avoid }
    REVIEW_CARD { uuid id string front string back json fsrs_state datetime due string status }
    REVIEW_LOG { uuid id int rating datetime reviewed_at json state_before }
    PREFERENCE { uuid id string scope string text }
    COMPONENT_REQUEST { uuid id string wanted string purpose json fallback }
    JOB { uuid id string kind string status json progress string error }
    CHAT_MESSAGE { uuid id string role json content datetime at }
```

`LessonDocument` si salva come JSON insieme alla `catalog_version` con cui è stato generato: così le lezioni vecchie restano leggibili quando il catalogo evolve.

## 9. API (bozza)

| Metodo | Path | Scopo |
|---|---|---|
| GET/POST | `/folders` | lista / crea cartelle |
| PATCH/DELETE | `/folders/{id}` | rinomina / elimina |
| GET/POST | `/folders/{id}/topics` | lista / crea Topic |
| GET/PATCH/DELETE | `/topics/{id}` | dettaglio / modifica / elimina |
| GET/POST | `/topics/{id}/messages` | chat (la risposta arriva via SSE) |
| GET | `/topics/{id}/events` | stream SSE: token, job, eventi di mission |
| GET | `/topics/{id}/mission` | mission corrente |
| POST | `/topics/{id}/mission/confirm` | conferma della bozza |
| GET | `/topics/{id}/resources` | fonti |
| GET | `/topics/{id}/glossary` | glossario |
| GET | `/topics/{id}/learning-records` | record |
| POST | `/topics/{id}/lessons` | richiede la prossima lezione (crea un job) |
| GET | `/topics/{id}/lessons` | lista delle lezioni |
| GET | `/lessons/{id}` | documento SDUI |
| POST | `/lessons/{id}/attempts` | tentativi (batch) |
| POST | `/lessons/{id}/complete` | chiude la lezione, avvia il Recorder |
| GET | `/reviews/due` | schede da ripassare oggi |
| POST | `/reviews/{card_id}` | voto FSRS |
| GET | `/component-requests` | backlog dei componenti mancanti |
| GET | `/catalog` | catalogo e versione supportata dal backend |

## 10. Schermate Android dell'MVP

> **Target primario: tablet in orizzontale** (window size class expanded), telefono in secondo piano: vedi `docs/adr/0003-tablet-landscape-first.md`. Su expanded Home → Cartella → Argomento è un layout list-detail a più pannelli; il diagramma sotto descrive la navigazione logica, valida anche per compact.

```mermaid
flowchart TD
    Home["Home<br/>cartelle + badge ripassi di oggi"] --> Folder["Cartella<br/>lista Topic"]
    Home --> Review["Ripasso di oggi"]
    Home --> Settings["Impostazioni<br/>URL backend, token,<br/>componenti mancanti"]
    Folder --> Topic["Topic<br/>tab: Chat / Lezioni / Glossario / Fonti / Mission"]
    Topic --> Chat["Chat con il Tutor"]
    Topic --> Lessons["Lista lezioni<br/>+ bottone Prossima lezione"]
    Lessons --> Lesson["Lezione<br/>renderer SDUI"]
    Lesson --> Summary["Riepilogo<br/>cosa hai dimostrato"]
    Lesson -- "ask_agent" --> Chat
```

Stack proposto: Kotlin, Jetpack Compose, kotlinx.serialization (i blocchi sono una `sealed class` polimorfica, e un tipo sconosciuto diventa automaticamente `Unknown` → fallback), Ktor client per HTTP e SSE, Koin o Hilt per la DI.

## 11. Decisioni aperte

Tutte decise il 2026-11-08: vedi `docs/adr/0005-mvp-open-decisions.md` (D1 ricerca con gli strumenti web lato server di Anthropic; D2 schede solo fronte/retro; D3 lezioni in italiano con termini tecnici originali; D4 FSRS con un crate esistente dietro `ReviewScheduler`).
