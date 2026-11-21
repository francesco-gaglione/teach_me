# Port `LlmProvider`, configurazione dei modelli, Context Assembler (Fase 2)

Proposta di `llm-engineer` per `rust-backend-dev` e per l'ADR del tech lead. Verificata sulla documentazione Anthropic disponibile (skill `claude-api`, cache del 2026-10-06; modelli e prezzi vanno riverificati sulla pagina ufficiale prima del rilascio). Dove un dettaglio di wire format non è stato verificato lo dico in "Da verificare".

## 1. Principi di forma

1. **Il port è una conversazione a un turno**: una richiesta, un assistant message in risposta. Il ciclo dei tool (Fase 3) sta in `application`, non nel port: così lo stesso port serve chat, Planner, Generator, Grader.
2. **Il ruolo, non il modello, nella richiesta.** Il dominio conosce `LlmRole`; id del modello, effort, limiti e provider stanno nella configurazione.
3. **Nessun parametro che un provider non supporta in modo uniforme.** Niente `temperature`/`top_p`: sui modelli Anthropic attuali sono rimossi (400) e con `anthropic-version: 2026-10-21` sono rifiutati su tutti. Se un adapter locale ne ha bisogno, li legge dalla configurazione del ruolo, non dalla richiesta.
4. **Tipi con `#[non_exhaustive]`** dove prevedo estensioni (`ToolChoice`, `StopReason`, `ToolDef`, `LlmError`): aggiungere varianti non rompe i chiamanti.
5. **Nessun tipo del provider nel dominio** (niente nomi di blocchi Anthropic): ciò che è specifico passa come `Part::ProviderBlock` opaco.

## 2. Tipi e trait (nel crate `domain`, modulo `ports::llm`)

Dipendenze nuove per il dominio, da decidere (vedi §11): `serde_json` (per `Value`: schemi e input dei tool sono JSON arbitrario). Nessuna dipendenza da tokio/reqwest/futures: lo stream è un trait pull con `async_trait`.

```rust
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

/// Ruoli degli agenti (docs/mvp.md §4). La chiave in config è `as_str()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LlmRole { Tutor, Interviewer, Researcher, Planner, Generator, Grader, Recorder }

// ---------- richiesta ----------

pub struct LlmRequest {
    pub role: LlmRole,
    /// Blocchi di sistema in ordine: prima i più stabili (cacheable).
    pub system: Vec<SystemBlock>,
    /// Alternati user/assistant; l'ultimo è `User`.
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<ToolDef>,                 // vuoto in Fase 2
    pub tool_choice: ToolChoice,             // Auto di default
    pub output: OutputFormat,                // Text di default
    /// Se None vale il default del ruolo in config.
    pub max_output_tokens: Option<u32>,
    /// Solo per log/metriche (es. "tutor.chat"). Mai contenuto utente.
    pub trace_label: &'static str,
}

pub struct SystemBlock {
    pub text: String,
    /// Suggerimento: "il prefisso fino a qui è stabile, cachealo".
    pub cache_breakpoint: bool,
}

pub struct LlmMessage {
    pub role: MessageRole,                   // User | Assistant
    pub parts: Vec<Part>,
    /// Suggerimento di cache: breakpoint dopo questo messaggio.
    pub cache_breakpoint: bool,
}

pub enum MessageRole { User, Assistant }

#[non_exhaustive]
pub enum Part {
    Text(String),
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    /// Blocco da rimandare al provider senza interpretarlo (es. thinking
    /// firmati di Anthropic, necessari nei cicli di tool). `provider` è il
    /// `kind` dell'adapter che lo ha prodotto; un adapter diverso lo scarta.
    ProviderBlock { provider: String, payload: Value },
}

#[non_exhaustive]
pub enum ToolDef {
    Function { name: String, description: String, input_schema: Value },
    // Futuro: ServerWebSearch { .. } come alternativa al port SearchProvider.
}

#[non_exhaustive]
pub enum ToolChoice { Auto, None } // vedi §5: niente "forzato" in v1

pub enum OutputFormat {
    Text,
    /// JSON conforme allo schema. `Native` o `Emulated` secondo capability.
    JsonSchema { name: String, schema: Value },
}

// ---------- risposta ----------

pub struct LlmResponse {
    pub message: LlmMessage,                 // role Assistant, parts complete
    pub stop_reason: StopReason,
    pub usage: Usage,
    pub model: String,                       // modello effettivo (per i log)
    pub request_id: Option<String>,
}
impl LlmResponse { pub fn text(&self) -> String { /* concatena i Text */ } }

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason { EndTurn, MaxTokens, ToolUse, Refusal, PauseTurn, Other(String) }

#[derive(Clone, Copy, Default, Debug)]
pub struct Usage {
    pub input_tokens: u32,          // non cache
    pub output_tokens: u32,         // include il thinking
    pub thinking_tokens: u32,       // parte di output_tokens (usage.output_tokens_details); 0 se il provider non lo dà
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
}

// ---------- streaming ----------

#[non_exhaustive]
pub enum LlmEvent {
    Started { model: String, request_id: Option<String> },
    TextDelta { block: u32, text: String },
    ToolCallStarted { block: u32, id: String, name: String },
    /// Frammento di JSON dell'input (può essere incompleto; solo per UI/log).
    ToolCallInputDelta { block: u32, partial_json: String },
    /// Input finale già parsato e validato come JSON. Se non parsabile
    /// l'adapter emette Err(LlmError::MalformedResponse), non questo evento.
    ToolCallFinished { block: u32, id: String, name: String, input: Value },
    ProviderBlock { block: u32, provider: String, payload: Value },
    /// Usage cumulativo finora (può arrivare più volte).
    Usage(Usage),
    Finished { stop_reason: StopReason, usage: Usage },
}

#[async_trait]
pub trait LlmEventStream: Send {
    /// `None` = stream concluso (dopo `Finished`). Un `Err` termina lo stream.
    /// Droppare lo stream annulla la richiesta (chiude la connessione).
    async fn next(&mut self) -> Option<Result<LlmEvent, LlmError>>;
}

// ---------- capability ----------

#[derive(Clone, Debug)]
pub struct LlmCapabilities {
    pub tools: bool,
    pub parallel_tool_calls: bool,
    pub forced_tool_choice: bool,            // false su Sonnet 5.5 / Opus 5.5
    pub structured_output: StructuredOutput, // Native | Emulated | None
    pub streaming_tool_input: bool,
    pub prompt_caching: bool,
    pub max_cache_breakpoints: u8,           // 4 su Anthropic
    pub context_window_tokens: u32,
    pub max_output_tokens: u32,
}

// ---------- il port ----------

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Capability del modello configurato per il ruolo. Sincrona: l'adapter le
    /// tiene in memoria (statiche per tipo di provider, oppure lette all'avvio,
    /// vedi §9).
    fn capabilities(&self, role: LlmRole) -> LlmCapabilities;

    /// Obbligatorio. Ritorna Err prima del primo evento se la richiesta è
    /// rifiutata (auth, 400, 429...). Errori a stream aperto arrivano da `next`.
    async fn stream(&self, req: LlmRequest) -> Result<Box<dyn LlmEventStream>, LlmError>;

    /// Facoltativo: default = `stream` + raccolta degli eventi in `LlmResponse`
    /// (helper `collect_response` nel dominio). Un adapter può sovrascriverlo.
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse, LlmError> {
        collect_response(self.stream(req).await?).await
    }
}
```

Perché `complete` ha un default basato su `stream`: un solo percorso da testare, e le richieste con `max_tokens` alto (Generator, fino a 128K di output) devono comunque usare lo streaming verso Anthropic per non incorrere nei timeout HTTP.

## 3. Come regge le fasi successive senza cambiare forma

**Fase 3, tool del Tutor.** `request.tools` con `ToolDef::Function` (`save_mission_draft`, `save_preference`, `propose_mission_change`), `tool_choice = Auto`. Lo stream emette `ToolCallStarted/InputDelta/Finished`, poi `Finished { stop_reason: ToolUse }`. Il caso d'uso in `application`:
1. accumula le `Part` dell'assistant (testo, `ToolUse`, `ProviderBlock`) e le **rimanda così come sono** nel messaggio successivo (i blocchi di thinking dei modelli attuali vanno restituiti intatti nello stesso ciclo);
2. se `stop_reason` è `MaxTokens` o `Refusal` **non esegue i tool** (l'input potrebbe essere troncato);
3. esegue i tool, aggiunge un unico messaggio `User` con **tutti** i `ToolResult` (anche in caso di errore, con `is_error: true`) e richiama il port;
4. ripete fino a `EndTurn`, con un tetto di iterazioni (proposta: 5).
L'SSE verso l'app inoltra solo `TextDelta`; i tool call sono interni (contratto chat §1). Se il modello scrive testo prima del tool call, quel testo è già stato inviato come delta: va bene, è parte del messaggio.

**Fase 6, output strutturato del Generator/Planner.** `output = OutputFormat::JsonSchema { name, schema }`, `tools` vuoto. Con capability `Native` l'adapter Anthropic usa `output_config.format = {type: "json_schema", schema}` e il testo restituito è JSON conforme; con `Emulated` l'adapter aggiunge al sistema l'istruzione di rispondere solo con JSON secondo lo schema (e, se il provider ha una modalità JSON, la usa). **In ogni caso `application` parsa e valida con il Validator e fa repair** (max 3 tentativi: re-invio con l'errore): lo schema nativo garantisce la sintassi, non le regole semantiche del catalogo. Perciò il port restituisce testo, non un `Value` già "garantito". Il Generator può restituire JSON molto lungo: usa `stream` (o `complete` che lo usa) e `max_output_tokens` alto.

**Fase 7, Grader/Recorder.** Stessa forma, ruolo diverso (modello economico), richieste brevi; `complete`.

**Cosa non cambia:** `LlmRequest`, `LlmEvent`, `LlmProvider`. Cosa può crescere senza rompere: varianti `non_exhaustive` (nuovo `ToolDef` per la ricerca web lato server, nuovi `StopReason`), campi opzionali nuovi su `LlmRequest` (si costruisce con un builder o `..Default`, vedi §11).

## 4. Capability e adapter futuri

| Capability | Anthropic (modelli attuali) | OpenAI-compatible / locali (previsione) |
|---|---|---|
| `tools` / `parallel_tool_calls` | sì / sì | dipende dal modello; molti locali: no o inaffidabile |
| `forced_tool_choice` | **no** su Sonnet 5.5, Opus 5.5, Fable 5.1 (400); sì su Haiku 4.5 e Opus 4.8 | sì tipicamente |
| `structured_output` | `Native` (`output_config.format`), incompatibile con le citazioni dei documenti | `Native` su OpenAI; `Emulated` o `None` sui locali |
| `streaming_tool_input` | sì | varia |
| `prompt_caching` / breakpoint | sì / 4 | automatico a prefisso su OpenAI (breakpoint ignorati); no sui locali |
| `context_window_tokens` | 1M (Sonnet/Opus/Fable/Haiku 5.x), 200K (Haiku 4.5) | tipicamente 8K-128K: il Context Assembler deve leggerlo |

Regole d'uso nell'`application`: prima di richiedere tool o output strutturato nativo si controlla la capability; se manca si degrada (emulazione + repair, oppure errore di configurazione chiaro all'avvio: "il ruolo `generator` richiede output strutturato, il modello X non lo offre"). Il budget di cronologia del Context Assembler è `min(budget di config, context_window - output - sistema)`.

`forced_tool_choice` è nel set di capability ma **non nel port in v1**: `ToolChoice` ha solo `Auto` e `None` perché i modelli Claude attuali rifiutano il forzato. Se serve "devi chiamare questo tool" si ottiene con output strutturato oppure con istruzione nel prompt; un `ToolChoice::Named(..)` si aggiungerà (grazie a `non_exhaustive`) solo per adapter che lo supportano, sempre dietro la capability.

## 5. Errori

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LlmError {
    #[error("rate limited")]            RateLimited { retry_after: Option<Duration> },
    #[error("provider overloaded")]     Overloaded,                  // 529 / equivalente
    #[error("provider unavailable")]    Unavailable,                 // 5xx, connessione caduta a stream aperto
    #[error("authentication failed")]   Authentication,              // 401, 403, 402 (fatturazione)
    #[error("invalid request: {0}")]    InvalidRequest(String),      // 400/404/422: bug nostro o config sbagliata
    #[error("context too long")]        ContextTooLong,              // 413 o 400 per token
    #[error("network: {0}")]            Network(String),             // DNS, connect, TLS
    #[error("timeout")]                 Timeout,                     // connect, idle dello stream o totale
    #[error("malformed response: {0}")] MalformedResponse(String),   // SSE/JSON illeggibile, tool input non parsabile
    #[error("unsupported: {0}")]        Unsupported(String),         // capability mancante
}
impl LlmError { pub fn is_retryable(&self) -> bool { /* RateLimited, Overloaded, Unavailable, Network, Timeout */ } }
```
Il messaggio di `InvalidRequest` può contenere testo del provider: va nei log, **mai** nell'`error.message` dell'API.

Mappatura sul contratto (`error.code` di `ChatMessage.error` e di `api-conventions.md` §7), in un solo punto in `application`/`api`:

| `LlmError` | `error.code` | Messaggio per l'utente (italiano, generico) |
|---|---|---|
| `RateLimited` | `rate_limited` | "Troppe richieste al modello, riprova tra poco." |
| `Overloaded`, `Unavailable`, `Network`, `Timeout`, `MalformedResponse`, `Authentication` | `upstream_failed` | "Il servizio del modello non risponde, riprova." (per `Authentication` log `error!` con indicazione di controllare la chiave) |
| `InvalidRequest`, `Unsupported` | `internal` | "Errore interno." |
| `ContextTooLong` | `internal` dopo **un** nuovo tentativo con budget di cronologia dimezzato; se fallisce ancora, `internal` | idem |
| `stop_reason = Refusal` | `upstream_failed` | "Il modello ha rifiutato di rispondere a questo messaggio." |
| `stop_reason = MaxTokens` (Tutor) | non è un errore: messaggio `complete`, `warn!` nei log | — |

## 6. Retry, backoff, timeout

- **Dove**: nell'`application`, come decoratore `RetryingLlm<P: LlmProvider>` che implementa lo stesso port; funziona per qualunque adapter. **Gli adapter non ritentano** (client HTTP senza retry automatici) ma classificano bene gli errori e riempiono `retry_after` (header `retry-after`, secondi).
- **Quando**: solo errori `is_retryable()`. In `stream`: solo se l'errore arriva **prima del primo evento di contenuto** (`TextDelta`/`ToolCall*`). Dopo, il testo parziale è già stato inviato all'app: niente retry automatico, il messaggio diventa `failed` e l'utente usa "Riprova" (contratto chat).
- **Politica**: massimo 3 tentativi totali, backoff esponenziale con jitter completo (base 1 s, fattore 2, tetto 20 s); se c'è `retry_after` lo si rispetta (+ jitter piccolo) fino a 60 s, oltre quel valore si smette e si restituisce `rate_limited`. `Authentication`, `InvalidRequest`, `ContextTooLong` non si ritentano.
- **Timeout** (configurabili): connessione 10 s; richieste non streaming (o `complete` con max_tokens basso) totale 120 s; stream: **nessun tetto totale** ma timeout di inattività tra due eventi 60 s (l'API invia `ping` periodici, quindi il silenzio vero indica una connessione morta). Questi timeout sono indipendenti dal timeout REST di 30 s dell'API, che non copre la generazione in background.
- Annullamento: il caso d'uso droppa lo stream (es. messaggio cancellato/spegnimento): l'adapter chiude la connessione.

## 7. Come si testa

- **Adapter con risposte registrate.** Fixture SSE/JSON in `crates/infrastructure/tests/fixtures/anthropic/*.sse|*.json` (testo semplice, testo + tool_use con input a frammenti, thinking, `stop_reason: max_tokens`, `refusal`, errore a metà stream `event: error`, 429 con `retry-after`, 529, 401, 400 "prompt too long"). Un server HTTP locale (es. `wiremock`, dev-dependency) serve le fixture; `base_url` dell'adapter è configurabile. Test che verificano anche il **corpo della richiesta** inviato (system a blocchi con `cache_control`, `output_config`, niente `temperature`). Le fixture si registrano una volta a mano con una chiave vera, togliendo header e id; nessun test chiama la rete. Se utile, scrivo io le fixture a partire dal formato documentato.
- **Fake per i casi d'uso**: `FakeLlm` nel modulo fakes di `application`: riceve uno script `Vec<Turn>` dove `Turn` è una lista di `Result<LlmEvent, LlmError>` (più un `Err` iniziale opzionale), registra le `LlmRequest` ricevute in un `Mutex<Vec<_>>` per le asserzioni (ordine dei blocchi di sistema, troncamento, breakpoint) e consente pause controllate (`Notify`) per testare lo streaming SSE e l'annullamento.
- **Test del decoratore di retry** con un fake che fallisce N volte e un `Clock`/sleeper iniettato (nessuna attesa reale).
- **Test unitari dell'assemblatore** (§10) senza LLM.
- **Test live opzionali** (`#[ignore]`, richiedono la chiave): un giro reale dell'adapter e gli eval in `backend/evals/`.

## 8. Mappatura Anthropic (per l'adapter)

Senza SDK Rust ufficiale: `reqwest` + SSE a mano. Endpoint `POST {base_url}/v1/messages`, header `x-api-key`, `anthropic-version` (configurabile; gli esempi attuali della documentazione usano `2026-10-21`), `content-type: application/json`. Sempre `"stream": true` dentro `stream()`.

| Port | Anthropic |
|---|---|
| `system: Vec<SystemBlock>` | `system: [{type:"text", text, cache_control?:{type:"ephemeral"}}]` |
| `messages` | `messages[].content` come array di blocchi; `cache_breakpoint` -> `cache_control` sull'ultimo blocco del messaggio |
| `ToolDef::Function` | `tools[]` `{name, description, input_schema}` (+ `eager_input_streaming: true` per i tool streammati; l'adapter valida l'input finale prima di emettere `ToolCallFinished`) |
| `ToolChoice::Auto` / `None` | `{type:"auto"}` / `{type:"none"}` |
| `OutputFormat::JsonSchema` | `output_config.format = {type:"json_schema", schema}` |
| effort del ruolo | `output_config.effort` |
| `max_output_tokens` | `max_tokens` (include il thinking: lasciare margine) |
| thinking | **non inviato** (Sonnet/Opus 5.5 usano adaptive di default; `disabled` darebbe 400 su Sonnet/Opus 5.5) |
| SSE `message_start` | `Started` (+ `usage` iniziale) |
| `content_block_delta` `text_delta` | `TextDelta` |
| `content_block_start` `tool_use` / `input_json_delta` / `content_block_stop` | `ToolCallStarted` / `ToolCallInputDelta` / `ToolCallFinished` (parse del JSON accumulato) |
| blocchi `thinking` e `signature_delta` | accumulati e emessi come `ProviderBlock { provider:"anthropic" }`; i testi di thinking non arrivano mai all'utente |
| `message_delta` (`stop_reason`, `usage`) | `Usage` poi `Finished` (`end_turn`, `max_tokens`, `tool_use`, `refusal`, `pause_turn`) |
| `ping` | ignorato (azzera il timeout di inattività) |
| `event: error` a metà stream | `Err(Overloaded)` per `overloaded_error`, `Err(Unavailable)` per `api_error`, `Err(RateLimited)` per `rate_limit_error` |
| HTTP 429 (`retry-after`), 529, 5xx, 401/402/403, 400/404, 413 | `RateLimited{..}`, `Overloaded`, `Unavailable`, `Authentication`, `InvalidRequest` (o `ContextTooLong` se il messaggio parla di token/prompt), `ContextTooLong` |

Usage: `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`.

Opzione `refusal_fallback` (default **off** in Fase 2): la documentazione prevede, per Sonnet 5.5 sulla Claude API, `fallbacks: "default"` con beta `server-side-fallback-2026-07-01` per riprovare lato server dopo un rifiuto dei classificatori di sicurezza. Per un tutor di chat il rifiuto è raro; l'header beta sbagliato romperebbe ogni richiesta con un 400, quindi va attivata solo dopo averla provata con la chiave reale.

**Da verificare in integrazione con chiave reale (rust-backend-dev):** valore corretto di `anthropic-version`; subset di JSON Schema accettato da `output_config.format` (servirà alla Fase 6: schemi strict, `additionalProperties: false`); forma esatta degli eventi `error`; che `usage` nel `message_start` e nel `message_delta` sia cumulativo.

## 9. Configurazione dei modelli per ruolo

Sezione TOML (stessa convenzione di ADR 0001: file opzionale, le variabili `TEACH_ME_*` vincono). Gli override per ruolo da ambiente: `TEACH_ME_LLM_ROLES_TUTOR_MODEL`, ecc. Da aggiungere a `backend/config/teach_me.example.toml`:

```toml
[llm]
# Provider usato dai ruoli che non ne dichiarano uno.
default_provider = "anthropic"

[llm.providers.anthropic]
kind = "anthropic"
base_url = "https://api.anthropic.com"   # override: solo TEACH_ME_ANTHROPIC_BASE_URL
# anthropic-version (verificare sulla documentazione ufficiale prima di cambiare)
api_version = "2026-10-21"
# La chiave NON va qui: TEACH_ME_ANTHROPIC_API_KEY (oppure ANTHROPIC_API_KEY come ripiego).
connect_timeout_secs = 10
request_timeout_secs = 120     # richieste non streaming
stream_idle_timeout_secs = 60  # silenzio massimo tra due eventi SSE
refusal_fallback = false

[llm.roles.tutor]
model = "claude-sonnet-5-5"
effort = "low"                 # low | medium | high | xhigh | max
max_output_tokens = 4096       # include il thinking
history_token_budget = 8000    # solo per i ruoli con chat
```

Regole:
- L'adapter non legge `ANTHROPIC_BASE_URL`: l'unico override dell'URL è `TEACH_ME_ANTHROPIC_BASE_URL` (ADR 0004 punto 9).
- I default dei ruoli sono **compilati nel backend** (tabella sotto): il file deve contenere solo ciò che si vuole cambiare. Chiave di ruolo sconosciuta o `provider` non definito = errore all'avvio. Un ruolo può avere `provider = "..."` per usare un altro adapter in futuro (`kind = "openai_compatible"`, `base_url` locale).
- **Chiave API**: `TEACH_ME_ANTHROPIC_API_KEY` (coerente con il prefisso `TEACH_ME_` e con il resto della config), con ripiego su `ANTHROPIC_API_KEY` se la prima è assente. Rimandare al ripiego evita doppia configurazione per chi usa già la variabile standard. La chiave è un tipo `Secret` il cui `Debug`/`Display` stampa `[redacted]`, non è serializzabile, non compare nei log, negli errori né nell'endpoint di diagnostica. Se manca all'avvio: warn, il backend parte comunque; le chiamate falliscono con `Authentication` (-> `upstream_failed`). Non ammessa nel file TOML versionato (consentita nel file privato, come `api_token`).
- I log di una chiamata contengono: ruolo, modello, `prompt_version`, latenza, usage (incluse cache read/write), `stop_reason`, `request_id`. Il contenuto dei messaggi non si logga sopra `debug`.
- Le capability possono essere lette all'avvio da `GET /v1/models/{id}` (campi `max_input_tokens`, `max_tokens`, `capabilities`) per evitare tabelle cablate che invecchiano; in alternativa tabella statica nell'adapter. Scelta libera dell'implementatore, il port non cambia.

### Default per ruolo (modelli attuali, prezzi per 1M token input/output dalla documentazione del 2026-10-06)

| Ruolo | Modello | Effort | Motivazione |
|---|---|---|---|
| **Tutor** (Fase 2-3) | `claude-sonnet-5-5` ($2/$10) | `low` | Conversazione breve su tablet: latenza e costo contano più della profondità. Serve però giudizio per fare push-back sulla vaghezza e rispettare regole di stile: Haiku 5.5 è più economico ma meno affidabile su istruzioni sottili (da misurare con l'eval, vedi sotto). Per la sola Fase 2 le risposte sono ~100 token: costo trascurabile |
| **Interviewer** (Fase 3, se separato dal Tutor) | `claude-sonnet-5-5` | `medium` | Qualità della mission = qualità di tutto il resto; poche chiamate |
| **Researcher** (Fase 4) | `claude-sonnet-5-5` | `medium` | Molto input (pagine), classificazione di affidabilità e annotazioni; Opus non necessario |
| **Planner** (Fase 6) | `claude-opus-5-5` ($4/$20) | `medium` | Una chiamata per lezione, decide la zona di sviluppo prossimale: l'errore costa una lezione intera |
| **Generator** (Fase 6) | `claude-opus-5-5` | `high` | Output lungo e vincolato da un catalogo; vale la qualità. Da confrontare con Sonnet 5.5 a effort alto: se l'eval (validità al primo tentativo, qualità didattica) non peggiora, si passa a Sonnet e si dimezza il costo |
| **Grader** (Fase 7) | `claude-haiku-5-5` ($0.10/$0.50) | `medium` | Chiamate frequenti e brevi con rubrica; confrontare con Sonnet sull'accordo con giudizi umani prima di fissare. Nota: il tokenizer di Haiku 5.5 conta ~30% di token in più, e per i rifiuti non c'è fallback lato server |
| **Recorder** (Fase 7) | `claude-sonnet-5-5` | `medium` | "Coverage ≠ learning": distinguere ciò che è stato dimostrato richiede giudizio; chiamata una per lezione |

Tutte le scelte sono ipotesi iniziali, da confermare con gli eval di ciascuna fase.

## 10. Context Assembler, Fase 2 (Tutor)

Funzione pura in `application` (nessun I/O): riceve prompt compilati (`include_str!`), Topic, messaggi dal repository e i limiti del ruolo; restituisce `LlmRequest`.

```
assemble_tutor_request(prompts, topic, messages, limits) -> LlmRequest
```

1. **Sistema, 2 blocchi.**
   - Blocco A = `prompts/tutor/system.md` letterale, `cache_breakpoint: true`. Stabile tra tutte le chiamate e tutti i Topic.
   - Blocco B = `prompts/tutor/context.md` con `{{topic_title}}`, `{{topic_state}}`, `{{history_note}}`, `cache_breakpoint: true`. Cambia per Topic e quando cambia lo stato.
   Nessun timestamp, id, contatore o data nel sistema.
2. **Cronologia.** Dal repository: gli ultimi 200 messaggi del Topic, riordinati dal più vecchio. Esclusi: messaggi `failed` e il messaggio `assistant` in `streaming` (è quello che si sta generando); i messaggi `user` consecutivi (possibile dopo un fallimento) si uniscono con `"\n\n"`. Ogni messaggio diventa un `LlmMessage` con un solo `Part::Text`. Deve finire con un `User` (garantito dal contratto: si genera in risposta all'ultimo messaggio user, anche nel retry).
3. **Troncamento a budget di token, senza chiamate extra.** Stima locale: `tokens(testo) = ceil(chars / 3) + 6` per messaggio (`chars` = code point, non byte). Per l'italiano `chars/3` sovrastima del 10-30% rispetto ai tokenizer attuali: è voluto, l'errore va dalla parte sicura; il budget (default 8000) è molto sotto la finestra (200K-1M) quindi la precisione è irrilevante. Algoritmo: sommare dal più recente al più vecchio finché si resta nel budget; si scartano solo messaggi **interi** dal più vecchio, mai si taglia il testo di un messaggio (anche se l'ultimo da solo supera il budget, lo si include); la cronologia inviata deve iniziare con un messaggio `User` (si scarta un eventuale `Assistant` iniziale). Se è stato scartato qualcosa, `history_note` assume il testo del README, altrimenti stringa vuota. Per ruoli futuri con modelli locali il budget effettivo è `min(history_token_budget, context_window - max_output - stima del sistema)`.
   Calibrazione facoltativa: il log confronta stima e `usage.input_tokens` reale; se lo scarto supera il 50% si rivede il divisore. `count_tokens` dell'API resta per gli eval, non per il percorso di richiesta.
4. **Breakpoint di prompt caching** (max 4 per richiesta, qui 3): fine blocco A, fine blocco B, ultimo messaggio della richiesta. L'API cerca all'indietro il prefisso già in cache, quindi il turno successivo riusa tutto fino al messaggio del turno precedente. Ordine di rendering Anthropic: tools, system, messages; in Fase 3 le definizioni dei tool (stabili) precedono il sistema e rientrano nel prefisso del blocco A. Sui provider senza breakpoint i flag sono ignorati.
   Realismo: in Fase 2 il prefisso statico è ~700 token, poco sopra la soglia minima di cache (512 token sui modelli 5.x), e la TTL di default è 5 minuti: il risparmio è minimo. Si implementa comunque il meccanismo (costa nulla) perché in Fase 3 (tool) e soprattutto in Fase 6 (prompt grandi) è la leva principale. Si controlla con `usage.cache_read_tokens` nei log; se resta 0 con prefissi uguali, c'è un invalidatore (variabile che cambia, ordine dei campi nei tool, ecc.).
   Effetto collaterale: il troncamento a scorrimento sposta l'inizio della cronologia a ogni turno e invalida la cache dei messaggi; ottimizzazione facoltativa (non per la Fase 2): tagliare a blocchi di 8 messaggi allineati all'ordinale assoluto, così il prefisso resta stabile per 8 turni.
5. **Parametri**: `tools: []`, `tool_choice: Auto`, `output: Text`, `max_output_tokens: None` (default del ruolo), `trace_label: "tutor.chat"`.
6. **Test**: titolo con ritorni a capo ripuliti; stato `new` e `interviewing`; cronologia con messaggio `failed` e unione degli `user` consecutivi; troncamento che parte con un `Assistant` (viene scartato); messaggio singolo oltre il budget; `history_note` presente solo se si è scartato; nessuna variabile `{{...}}` residua nel risultato; blocco A identico byte per byte tra due Topic.

## 10bis. Context Assembler e ciclo dei tool, Fase 3 (Tutor con mission)

Estende §10; dove non detto, §10 resta valido. Prompt: `prompts/tutor/` (`tutor-phase3-v1`).

### 10.1 Cosa si aggiunge al contesto
- **Tool**: `tools.json` -> `LlmRequest.tools` (stesso ordine sempre, schemi byte-identici: i tool precedono il sistema nel prefisso di cache; cambiare l'ordine o un carattere invalida tutto), `tool_choice: Auto`.
- **Blocco B** (`context.md`) guadagna `{{mission_section}}` e `{{preferences_section}}` (formato esatto in `prompts/README.md`). Sono nel blocco B e non nei messaggi perché cambiano raramente (una conferma, una preferenza) e devono essere sempre aggiornati: nessuna chiamata di tool serve per leggerli.
- **Dati letti dal repository** a ogni turno: mission corrente (`confirmed`) e bozza aperta (`draft`) del Topic, preferenze globali + del Topic (max 20 recenti), stato del Topic **così com'è all'inizio del turno, dopo** la transizione `mission_proposed -> interviewing` causata dal messaggio dell'utente. Per questo il prompt non si fida di `Stato: mission_proposed` e legge la sezione Mission per sapere se c'è una bozza aperta.
- **Conferma**: non genera messaggi (contratto). Il Tutor la vede perché `mission_section` mostra "Mission confermata" al posto della bozza. Nessun altro segnale necessario.
- **Cache**: breakpoint invariati (fine A, fine B, ultimo messaggio = 3 su 4). B cambia a ogni conferma/bozza/preferenza, quindi quel turno paga la riscrittura di B (poche centinaia di token) ma non di A + tool (~3.5K token, il grosso). Sotto la soglia minima di cache il flag è inerte, non dannoso.

### 10.2 Ciclo dei tool (application)
1. Si chiama `stream`; i `TextDelta` vanno all'SSE come in Fase 2. Il testo scritto **prima** del tool call è già stato inviato: è l'introduzione alla card.
2. A `Finished { ToolUse }` si eseguono i tool **in ordine di blocco** (anche più d'uno), si produce un solo messaggio `User` con tutti i `ToolResult` (`is_error` dal file dei risultati) e si richiama il port. I `ProviderBlock` e le `Part` dell'assistant si rimandano intatti (§3).
3. **Tetto: 4 iterazioni del ciclo** (5 chiamate al modello al massimo), poi si chiude il messaggio con il testo ottenuto e `warn!`. Il caso normale è 1-2 giri: tool -> risultato -> (testo vuoto o poche parole). Ritentare dopo un errore di validazione conta come iterazione.
4. Se `stop_reason` è `MaxTokens`/`Refusal` non si eseguono i tool (§3).
5. Un solo tool di mission (`save_mission_draft` o `propose_mission_change`) per turno: il secondo restituisce `already_in_turn`. `save_preference` può comparire più volte (duplicati -> `duplicate`).
6. **Ammissibilità per dati, non per stato**: `save_mission_draft` ammesso se il Topic **non** ha una mission confermata (qualunque stato); `propose_mission_change` ammesso se ce l'ha. Altrimenti il risultato `not_allowed_*`. `save_preference` sempre. (Vedi proposta di contratto: lo stato da solo è ambiguo, es. proposta di cambio a Topic `ready` seguita da un messaggio dell'utente.)
7. La bozza è sempre **completa** (mai un diff): il backend imposta `replaces_version` sulla parte (versione **confermata** sostituita) e l'app evidenzia le differenze. La parte `mission_proposal` va **in coda** al messaggio assistant, dopo tutto il testo, anche se il modello avesse scritto testo dopo il tool (contratto: la card segue l'introduzione).
8. Il testo del modello dopo un tool call con esito di successo resta nel messaggio. Se il modello chiude con testo vuoto, va bene: non è un errore.
9. Il log di ogni chiamata include i nomi dei tool e `is_error`; mai gli input a livello `info`.
10. Dedup: `unchanged` se i quattro campi (dopo trim) coincidono con la bozza aperta (o, per `propose_mission_change`, con la confermata); preferenza duplicata = stesso `scope` e testo uguale ignorando maiuscole e spazi.

### 10.3 Modelli con tool inaffidabili
Se `capabilities.tools == false` (modelli locali), il Tutor funziona come in Fase 2 (nessun tool, niente card): `system.md` va reso senza le sezioni "Proporre la bozza", "Cambiare", "Preferenze" (da fare con un secondo file `system_no_tools.md` quando serve il primo provider locale; non ora). Per tool call con JSON malformato da modelli piccoli: restituire `invalid_input_shape`, massimo un nuovo tentativo.

### 10.4 Interviewer come modalità del Tutor, non ruolo separato
Scelta: **stessa chiamata e stesso prompt del Tutor**, con le regole dell'intervista nel prompt statico e lo stato nel contesto.
- *Latenza*: un ruolo separato richiederebbe un passaggio di routing (o una seconda chiamata a ogni turno per decidere "intervista o chat") prima di rispondere. Con la modalità, la risposta parte subito e in streaming.
- *Costo*: un solo prefisso (A + tool) in cache per tutte le conversazioni; due ruoli = due prefissi, meno riuso. L'intervista è 3-6 turni brevi: nessuna ragione di un modello diverso.
- *Qualità*: la conversazione è una sola, con la stessa voce; un passaggio di consegne tra Interviewer e Tutor perde contesto (preferenze dette in passing, correzioni dopo la card) e crea il caso "chi gestisce questo messaggio?". Il prompt unico gestisce naturalmente "preferenza detta durante l'intervista" e "cambio mission in `ready`".
- *Rischio accettato*: un prompt più lungo (~3K token) e regole che competono. Mitigazione: eval di Fase 3 (`evals/tutor/phase3.cases.json`) sui turni dell'intervista e sul rispetto dello stile; se il Tutor in `ready` degrada, si separa il blocco dell'intervista in un secondo file di sistema incluso solo quando manca la mission confermata (stessa struttura, A diventa A1+A2).
- Il ruolo `Interviewer` resta nell'enum (`LlmRole`) ma in Fase 3 non è usato; la config `[llm.roles.interviewer]` non serve.
- Modello consigliato: `claude-sonnet-5-5`; effort `low` come baseline Tutor. Da confrontare con `medium` sui casi vaghi dell'eval (la valutazione della vaghezza è la parte che richiede giudizio).

## 11. Decisioni per il tech lead (ADR)

1. **`serde_json` nel dominio** (ADR 0002 elenca solo `serde`): serve per `Value` di schemi e input dei tool. Alternativa: tipo `JsonValue` proprio, più codice e conversioni.
2. **Stream pull con `async_trait`** (zero dipendenze) invece di `futures_core::Stream` (una piccola dipendenza senza runtime, ma più ergonomica con `StreamExt`/`select!`). Preferisco il trait pull; la conversione in `Stream` per SSE axum si fa nel livello `api`.
3. **Retry nell'`application` come decoratore** (vs nell'adapter): è la mia proposta (§6); ogni adapter resta semplice e la politica è testabile senza rete.
4. **Costruzione di `LlmRequest`**: struct con campi pubblici e `Default` parziale, oppure builder. Dettaglio per l'implementatore.
5. **`ToolChoice` senza forzato in v1** (§4): conseguenza dei modelli Claude attuali; accettabile?
6. **`anthropic-version` e `refusal_fallback`**: valori da validare con chiave reale; default prudente (fallback off).
7. **Chiave API**: `TEACH_ME_ANTHROPIC_API_KEY` con ripiego su `ANTHROPIC_API_KEY` (§9).
8. **Fallimento all'avvio senza chiave**: warn e avvio comunque (proposta) oppure errore bloccante.
9. **Rendering del testo in app**: risolto, l'app rende un sottoinsieme di markdown (docs/design/screens/chat.md §12) e il prompt lo segue.

## 12. Strumenti lato server e Researcher, Fase 4 (approvata dal tech lead il 2026-11-09, con le correzioni del contratto `docs/contracts/resources.md`: sostituzione in blocco delle fonti, limiti di lunghezza, `invalid_output`)

Proposta di `llm-engineer` per `rust-backend-dev`, da implementare dopo l'approvazione del tech lead. Basata su ADR 0005 D1, sulla documentazione ufficiale (web search tool, server tools, structured outputs, consultate il 2026-11-09) e su due chiamate reali con Haiku 5.5 via OpenRouter (§12.7). Il prompt è in `backend/prompts/researcher/`.

### 12.1 Versione dello strumento: `web_search_20250305`

Esistono tre versioni: `web_search_20250305` (base), `web_search_20260209` (filtro dinamico: Claude scrive ed esegue codice per filtrare i risultati prima che entrino nel contesto) e `web_search_20260318` (aggiunge `response_inclusion`). Scelta: **la base**.
- Il filtro dinamico gira dentro code execution: `allowed_callers` vale `["code_execution_20260120"]` di default, non è idoneo a ZDR/HIPAA e i modelli senza programmatic tool calling vogliono `allowed_callers:["direct"]` (altrimenti 400). Con `["direct"]` la versione 2026 equivale alla base, senza vantaggio.
- Il risparmio di token è reale ma non misurato, la compatibilità con OpenRouter non è verificata e aggiunge un container/stato (`container` da rimandare nelle continuazioni).
- Rivalutare dopo la prima misura dei costi: se l'input per ricerca (~22k token osservati) pesa troppo, provare `web_search_20260209` su Anthropic diretto e confrontare con l'eval del Researcher. Il tipo e la versione devono quindi essere **configurabili** nell'adapter, non nel port.
- `web_fetch` (`web_fetch_20250910`): non nel primo giro. Le ricerche danno titolo, URL e il testo per annotare; leggere pagine intere moltiplica i token. Si aggiunge dopo, se l'eval mostra annotazioni superficiali.

### 12.2 Modifiche al port

Tutte additive (`non_exhaustive` dove c'è già).

```rust
pub enum ToolDef {
    Function { name, description, input_schema },
    /// Ricerca web eseguita dal provider. Il backend non esegue nulla.
    ServerWebSearch {
        max_uses: u32,                       // tetto per turno, anche attraverso le continuazioni pause_turn
        allowed_domains: Vec<String>,        // domini nudi, senza schema; mutuamente esclusivo con blocked
        blocked_domains: Vec<String>,
    },
}

pub struct LlmCapabilities { /* ... */ pub server_web_search: bool }

pub struct Usage { /* ... */ pub web_searches: u32 }   // usage.server_tool_use.web_search_requests

pub enum LlmEvent {
    /// Il modello ha lanciato una ricerca (input finale già letto, es. {"query": "..."}):
    /// serve al progresso del job ("Cerco: ..."). Non va rimandato al provider.
    ServerToolUse { block: u32, name: String, input: Value },
    /* ... */
}
```

- **Blocchi da conservare e rimandare.** Per continuare un turno servono *tutti* i blocchi dell'assistant così come arrivati: `server_tool_use`, `web_search_tool_result` (con `encrypted_content` intatto: se manca o è modificato, 400), i `thinking` firmati e il testo. L'adapter li emette come `LlmEvent::ProviderBlock { provider: "anthropic", payload }` (payload = blocco JSON completo, `server_tool_use` con l'input già assemblato) nella posizione giusta; `collect_response` li mette nel messaggio come `Part::ProviderBlock` e `build_body` li rimanda (già fa così per il thinking). Le citazioni dei blocchi di testo si possono perdere nel replay (il testo torna come `Part::Text` normale); **da verificare al primo `pause_turn` reale** che la API non chieda `encrypted_index`.
- **`pause_turn`** (`StopReason::PauseTurn`, già esistente): il caso d'uso rimanda il messaggio dell'assistant *invariato* e con gli *stessi* tools, senza nessun messaggio utente dopo; ripete fino a un altro stop reason, con tetto di 5 continuazioni. Questo richiede di **rilassare l'invariante** di `LlmRequest.messages` ("l'ultimo è `User`"): ammesso `Assistant` come ultimo solo se termina con una continuazione di `pause_turn`. Le ricerche delle richieste precedenti contano nel `max_uses`.
- **Conteggio ricerche.** L'adapter legge `usage.server_tool_use.web_search_requests` (cumulativo, nel `message_delta`) in `Usage.web_searches`; il log di ogni chiamata lo riporta. Costo: 10 $ ogni 1000 ricerche più i token (le ricerche fallite non si pagano). `application` somma anche attraverso le continuazioni.
- **Errori di ricerca** non sono errori HTTP: arrivano come `web_search_tool_result` con `content: {type:"web_search_tool_result_error", error_code}` (`max_uses_exceeded`, `too_many_requests`, `unavailable`, ...) dentro una risposta 200. Il caso d'uso non fallisce: il modello li vede e continua; se la run chiude con zero risultati utili, il Researcher restituisce `resources` vuoto con le lacune e il job termina "senza fonti" (non `Ready` silenzioso).
- **Capability.** Anthropic e OpenRouter con modelli Claude: `server_web_search = true`. Gli adapter OpenAI-compatible/locali: `false`.

### 12.3 Cosa fa il caso d'uso `RunResearch` (application)

1. Carica mission confermata, preferenze (max 20), risorse esistenti. Costruisce `LlmRequest { role: Researcher, system: [system.md (cache), context.md renderizzato], messages: [user: "Procedi con la ricerca delle fonti."], tools: [ServerWebSearch { max_uses: 4 }], output: Text }`.
2. Esegue il ciclo `pause_turn` (§12.2), inoltrando `ServerToolUse` al job come avanzamento ("Cerco: <query>").
3. Estrae l'oggetto JSON dal testo finale (l'ultimo blocco di testo; tollera una recinzione ```json), valida con `researcher/output_schema.json`, poi con il **Validator delle risorse** (sotto). Se fallisce: **repair** (§12.4), max 2 tentativi.
4. Salva `Resource[]` e `Gap[]` in transazione, **sostituendo in blocco** fonti e gap precedenti (`resources.md` §1); se la ricerca o la validazione falliscono l'elenco precedente resta intatto. Il Researcher riceve le fonti esistenti nel contesto e consegna l'elenco finale completo: può riproporre quelle ancora valide (stesso URL) e scartare le superate. Topic -> `Ready` (solo alla prima ricerca).

**Validator delle risorse** (codice, non prompt):
- `url`: `https://`, parsabile, nessun duplicato, **presente nell'insieme degli URL visti nei risultati di ricerca della run OPPURE tra gli URL delle fonti già salvate del Topic** (URL dei `web_search_tool_result`, più quelli delle citazioni dei blocchi di testo se il provider non restituisce i risultati; confronto dopo normalizzazione: schema+host in minuscolo, senza `#frammento`, senza `/` finale). È la difesa dura contro URL inventati; il prompt da solo non basta. `url` null ammesso solo con `source_type` in `live_class`/`local_group`/`other` e `kind = community`.
- `kind`/`source_type` coerenti: `forum`, `social_group`, `live_class`, `local_group` solo con `community`; `book`, `paper`, `article`, `documentation`, `video`, `course`, `standard` solo con `knowledge` (`other` ammesso in entrambi).
- Se `communities_skipped = true` nessuna risorsa `community` (altrimenti scarta le community e annota un warning nel log). Se la preferenza di opt-out esiste ma `communities_skipped = false`, lo stesso scarto: la fonte di verità è il codice che legge le preferenze, non il modello (decisione del modello sulle preferenze in testo libero, nessun campo strutturato per ora).
- Lunghezze (`resources.md` §2): `title` 1-300, `annotation` 1-600, `use_for` 1-300, `gaps[].area` 1-200, `note` 1-400; 5-12 risorse (meno di 5 solo se `gaps` spiega perché) e massimo 8 lacune; l'ordine dell'array è l'importanza (le `knowledge` prima delle `community`). Lo schema non le esprime (nessun `minLength`/`maxItems` in strict): sono errori da riparare, non da troncare.
- Warning non bloccanti (per l'eval): `annotation` non in italiano, `use_for` che inizia con "Usala per", community con URL che sembra un thread, `community` prima di una `knowledge`, `gaps[].note` con consigli.

### 12.4 Output strutturato e strumenti lato server nella stessa richiesta

La documentazione dice che `output_config.format` è **incompatibile con le citazioni** (400) e che le citazioni del web search sono "sempre attive"; non dice esplicitamente se i due convivono. Provato: via OpenRouter con Haiku 5.5 `output_config.format` + `web_search_20250305` nella stessa richiesta **funziona** e il JSON è conforme (§12.7). Non è provato sull'API Anthropic diretta e un 400 lì romperebbe tutto, quindi:

- **Default: `OutputFormat::Text`** con il JSON richiesto dal prompt (e lo schema applicato a valle). Funziona identico su Anthropic, OpenRouter e (con un `SearchProvider` locale) su altri modelli. Provato: 3/3 risorse valide e JSON parsabile al primo colpo.
- **Repair/ristrutturazione in due passi** solo se il parsing o il Validator falliscono: seconda richiesta **senza strumenti** con `OutputFormat::JsonSchema` nativo, che riceve il testo della prima risposta e l'elenco degli errori e restituisce il JSON corretto. Costa poco (nessuna ricerca, poco input) e non può inventare URL: il Validator controlla comunque l'insieme degli URL visti, che resta quello della prima fase.
- Una futura capability `structured_output_with_server_tools: bool` consentirebbe il nativo in un solo passo dopo averlo provato con la chiave Anthropic; non serve ora.
- Il **doppio passo sistematico** (prima ricerca libera, poi strutturazione) non è il default: raddoppia la latenza e aggiunge una chiamata senza vantaggi misurati. Diventa la strategia per modelli piccoli/locali con tool lato client che non reggono ricerca e formato nella stessa richiesta.

### 12.5 Portabilità: se il provider non ha la ricerca lato server

Il prompt non dipende dallo strumento Anthropic: parla di "strumento di ricerca" e di "risultati visti". Con `server_web_search = false` il caso d'uso passa a un ciclo di tool *lato client* come in Fase 3: `ToolDef::Function { name: "web_search", input_schema: {query} }` (ed eventualmente `fetch_page {url}`), eseguiti da un port `SearchProvider`/`PageFetcher` (`docs/mvp.md` §3: SearXNG, Brave, Tavily), risultati restituiti come `tool_result` con titolo, URL e snippet. Il Validator degli URL usa lo stesso insieme (URL restituiti dal `SearchProvider`). Nell'MVP non si implementa: se un ruolo `researcher` è configurato su un provider senza `server_web_search`, errore di configurazione chiaro all'avvio.

### 12.6 Streaming con OpenRouter: i risultati non arrivano

Osservato il 2026-11-09 (Haiku 5.5, `web_search_20250305`, 1 ricerca):
- **Non in streaming**: la risposta ha `thinking` -> `server_tool_use` (id `srvtoolu_...`, `input:{"query":...}`) -> `web_search_tool_result` (10 risultati, ognuno con `url`, `title`, `page_age`, `encrypted_content`; ~45 KB) -> `thinking` -> `text`. `usage.server_tool_use.web_search_requests = 1`. Campi extra di OpenRouter (`cost`, `cost_details`, `is_byok`) vanno ignorati. Esempio ridotto: `backend/evals/researcher/sample_response_nonstream.trimmed.json`.
- **In streaming**: i blocchi sono `thinking` (0), `server_tool_use` (1, con `input_json_delta` che porta la query; id riscritto `toolu_...`), `text` (2) con soli `citations_delta` di tipo `web_search_result_location` (`url`, `title`, `cited_text`), `thinking`, `text` finale con il JSON. **Il blocco `web_search_tool_result` non compare nello stream** (0 occorrenze). Quindi con OpenRouter in streaming: (a) non si può costruire l'insieme completo degli URL visti (restano solo i citati), (b) un `pause_turn` non sarebbe continuabile (manca `encrypted_content`). La documentazione Anthropic mostra invece il blocco risultato in un `content_block_start` dello stream: su API diretta atteso presente (non verificato).
- **Raccomandazione**: per le richieste con `ServerWebSearch` l'adapter usa la richiesta **non in streaming** (override di `complete()`, un parser per la risposta intera: stessi blocchi, stesso mapping) e per il ruolo Researcher si configura `request_timeout_secs` più alto (300). Il progresso "Cerco: ..." si ricava a fine chiamata o dai `ServerToolUse` emessi nell'ordine dei blocchi; non è in tempo reale, accettabile per un job che dura decine di secondi. Se in seguito l'API diretta restituisce i risultati nello stream, si torna a `stream()` senza toccare `application`.

### 12.7 Cosa è stato provato (costo totale 0,023 $)

| Prova | Esito | Costo |
|---|---|---|
| Haiku 5.5, streaming, `web_search_20250305` `max_uses:1` + `output_config.format` (schema del Researcher) | 200; JSON conforme (4 risorse, lacune), ricerca eseguita, `cost` 0,009 $ (con caching automatico del prefisso: `cache_creation` 8,5k, `cache_read` 3,9k token, senza `cache_control` inviato) | 0,009 $ |
| Haiku 5.5, non streaming, stesso strumento, `OutputFormat::Text` (JSON da prompt) | 200; JSON parsabile, 3 risorse, tutti gli URL presenti nei risultati di ricerca; 28,4k token di input per 1 ricerca; 0,014 $ (0,01 $ è la tariffa della ricerca) | 0,014 $ |

Non provati: `pause_turn`, `max_uses` superato, errore di ricerca, `web_search_20260209`, `web_fetch`, API Anthropic diretta.

### 12.8 Modello, costi e cache

- Modello: `claude-sonnet-5-5`, effort `medium` (tabella §9). Haiku 5.5 è usabile per le prove dell'eval; la qualità del giudizio di affidabilità va confrontata prima di ridurre.
- `max_output_tokens`: 4096 (JSON di 10 risorse ~2k token, più thinking).
- `max_uses`: **4** di partenza. Ogni ricerca porta 16-22k token di input e ogni passo successivo del loop rilegge tutto: il costo cresce più che linearmente con le ricerche in sequenza. Il prompt chiede di raggruppare le query indipendenti nello stesso passo. Da ritarare con gli eval (2 vs 4 vs 6).
- Cache: `system.md` (~1,7k token) è sotto la soglia minima cacheable di molti modelli, il breakpoint è innocuo ma probabilmente inerte; il vero risparmio è il caching del prefisso tra i passi del loop, che il lato server applica da solo (osservato). Variabile in fondo: `context.md` dopo `system.md`, come per il Tutor. `{{today}}` solo in `context.md`.

## 13. Fasi 6-7: Planner, Generator, Grader, Recorder (proposta di `llm-engineer`, `phase6-7-v1`/`v2`)

Contratto: `docs/contracts/lessons.md`. Prompt e segnaposto: `README.md` (sezione «Fasi 6-7»). Verificato il 2026-11-09 sulla documentazione ufficiale degli structured outputs (`platform.claude.com/docs/en/build-with-claude/structured-outputs`); prezzi dal §9 di questo file. Non è stata fatta nessuna chiamata a un modello.

### 13.1 Chiamate per ruolo

| Ruolo | Quando | Chiamate | Modello / effort | `max_output_tokens` | Output | Streaming | `trace_label` |
|---|---|---|---|---|---|---|---|
| Planner | job `lesson_generation`, step `planning` | 1, + 1 riparazione | `claude-opus-5-5` / `medium` | 4000 | **nativo** (`planner/output_schema.json`) | no | `planner.plan`, `planner.repair` |
| Generator | step `writing` | 1 + fino a 2 riparazioni (3 documenti) | `claude-opus-5-5` / `high` | **20000** (include il thinking) | **emulato** (`OutputFormat::Text`, JSON nel testo) | **sì** (documento di ~4k token + thinking: oltre il `request_timeout` di 120 s non in streaming) | `generator.write`, `generator.repair` |
| Grader | `POST /lessons/{id}/attempts` (sincrono, tetto 20 s) e job `lesson_completion` (`grading`) | 1 per risposta aperta, **nessuna riparazione** | `claude-haiku-5-5` / `low` (da misurare: `medium` solo se la latenza regge) | 1500 | **nativo** (`grader/output_schema.json`) | no; `request_timeout` della richiesta = 20 s meno il tempo già speso | `grader.grade` |
| Recorder | job `lesson_completion`, step `recording` | 1, + 1 riparazione; **saltata** se nessun tentativo è `correct` | `claude-sonnet-5-5` / `medium` | 6000 | **nativo** (`recorder/output_schema.json`) | no | `recorder.record`, `recorder.repair` |

Costruzione della richiesta (come §10): sistema = blocco A `system.md` (`cache_breakpoint: true`) + blocco B `context.md` renderizzato (`cache_breakpoint: true`); un solo messaggio `user`. Planner/Generator/Recorder: «Procedi.» (testo fisso, breve: il compito è nel blocco B). Grader: un messaggio fisso «Valuta la risposta.». Riparazione: **stessi blocchi A e B** (si riusa la cache) e un solo messaggio `user` = `repair.md` renderizzato; il tentativo precedente è dentro `repair.md` (non come turno `assistant`: più semplice e portabile, niente prefill).

### 13.2 Output strutturato: nativo o emulato

Dalla documentazione: lo strumento di schema accetta `anyOf`/`allOf`, `$ref`/`$defs`, `enum`, `const`, `required`, `additionalProperties: false` (obbligatorio), `minItems` 0/1; **non** accetta ricorsione, `oneOf`, `minLength`/`maxLength`, `minimum`/`maximum`, `maxItems`, né regex con quantificatori `{n,m}` ampi o lookaround. Limiti per richiesta: 24 parametri opzionali, 16 union (`anyOf` o `["x","null"]`), grammatica compilata con timeout di 180 s; il primo uso di uno schema è più lento (grammatica in cache 24 h); cambiare `output_config.format` invalida la prompt cache.

- **Planner, Grader, Recorder: nativo.** Schemi piatti nel sottoinsieme strict (tutti i campi `required`, nessun vincolo di lunghezza o numero: stanno nel Validator, come per il Researcher). Union usate: Grader 1 (`misconception`), Recorder 2 (`record_index`, `term_key`). Verificato in locale che non usino keyword vietate. Con provider senza schema nativo l'adapter emula (aggiunge lo schema al sistema): i prompt descrivono già i campi a parole, quindi regge.
- **Generator: emulato.** `sdui-schema-v1.json` (18 KB) non è inviabile: contiene `oneOf` e il blocco `missing_component.fallback` rimanda a `block` (ricorsione), più `minLength`/`maxLength`/`maximum`/`pattern`. Una versione ridotta valida solo la forma e lascerebbe comunque tutte le regole al Validator; costerebbe grammatica da compilare (timeout, latenza alla prima richiesta, invalidazione della cache di prompt ad ogni cambio) per un guadagno minimo, e non sarebbe portabile. Quindi: `OutputFormat::Text`, il prompt descrive il documento a parole (catalogo, limiti, regole del Validator come istruzioni), il backend estrae il JSON, valida col Validator reale e ripara. Rischio: un modello può chiudere il JSON male o aggiungere testo; l'estrazione è tollerante e un JSON non parsabile conta come errore `schema` a `path: ""` e va alla riparazione.
- Portabilità: nessun prompt dipende da feature di un provider. Per modelli locali piccoli il Generator è il ruolo critico (JSON lungo con molte regole): serve un modello di classe medio-grande; strategia di ripiego futura, non prevista ora: generare per sezioni e comporre il documento.

### 13.3 Riparazioni e fallimenti

| Ruolo | Riparazioni | Errori resi in `{{errors}}` | Dopo l'ultima |
|---|---|---|---|
| Planner | 1 | `- {campo}: {messaggio}` | job `invalid_output` |
| Generator | 2 (3 documenti), stessa conversazione di base, `progress` torna a `writing` con `done/total` = n/3 | `- {path} [{code}] {message}` (max 30 righe) | job `invalid_output` |
| Grader | nessuna | — | `pending` (§5.1 del contratto); nel job di chiusura una nuova chiamata |
| Recorder | 1 | `- {campo}: {messaggio}` | job `invalid_output` |

Un rifiuto del modello o un `max_tokens` raggiunto sono errori del port (§5), non da riparare. Se il Generator si ferma per `max_tokens` il JSON è troncato: riprovare con la stessa richiesta non serve; conta come documento non valido e la riparazione chiede di riscriverlo più corto (`too_long` implicito).

### 13.4 Context Assembler per questi ruoli

Funzioni pure in `application`, come §10: `assemble_planner_request`, `assemble_generator_request`, `assemble_grader_request`, `assemble_recorder_request` (+ le varianti `repair`). Regole:
1. Tutti i segnaposto di `context.md` presenti, sostituzione a passata unica; test «nessun `{{...}}` residuo» su `context.md`/`repair.md` renderizzati.
2. Troncamenti e formati nel README (60 record, 80 termini, 20 lezioni, 12 risorse, 20 preferenze). Nessun troncamento taglia il testo di una voce a metà tranne `body` dei record (400) e le risposte nei tentativi (600).
3. Dati da escludere: community e risorse senza URL https dalle fonti; `rubric` mai verso l'app (solo Grader/Recorder/Generator).
4. Ordine di rendering e cache: variabile sempre dopo il prefisso stabile; nessuna data/id nel blocco A. Il blocco B del Generator contiene piano, fonti e glossario: cambia a ogni lezione, ma resta identico tra documento 1, 2 e 3 (le riparazioni leggono dalla cache).
5. Test: formato di ogni sezione con insiemi vuoti; titoli e risposte con `{{x}}`/`[[x]]`; troncamenti con `(mostrati…)`; blocco A identico byte per byte tra due Topic; nel Grader `<<<`/`>>>` neutralizzati.

### 13.5 Dimensioni e cache

Misure per difetto (caratteri / 3,3): `system.md` Planner ~2,1k token, Generator ~6k, Grader ~1,3k, Recorder ~2,4k. Il Generator supera di molto la soglia minima di cache (512 token sui modelli 5.x, §10): il blocco A è la leva di risparmio; le riparazioni leggono dalla cache anche il blocco B. Gli altri blocchi A sono piccoli (Grader sotto la soglia di Haiku, probabilmente inerte). I breakpoint si impostano comunque (innocui).

### 13.6 Stima dei costi per lezione

Assunzioni (da verificare con `usage` reale): lezione con ~10 record, ~15 termini, ~5 lezioni, 8 fonti; Planner 5k input / 3k output (con thinking); Generator 8k input, 10k output (documento ~4k + thinking `high`), riparazione 12k input (6k da cache a 0,1×) e 8k output; Grader 2,1k/0,5k; Recorder 5,5k/3,5k. Moltiplicatori di cache standard di Anthropic (lettura 0,1×, scrittura 1,25×) **non riverificati** qui. Ipotesi: 40% delle lezioni richiede almeno una riparazione. Prezzi per 1M token (§9): Opus 4/20, Sonnet 2/10, Haiku 0,10/0,50 (Haiku conta ~30% di token in più).

| Configurazione | Planner | Generator (1 doc / 1 riparazione) | Grader (per risposta) | Recorder | Lezione: minimo / attesa / massimo (3 doc + 1 rip. Recorder) |
|---|---|---|---|---|---|
| **Default** (Opus Planner e Generator, Haiku Grader, Sonnet Recorder) | 0,08 $ | 0,24 $ / 0,19 $ | 0,0005 $ | 0,05 $ | **0,36 / 0,44 / 0,77 $** |
| **Economica** (Sonnet Planner e Generator, Haiku Grader, Sonnet Recorder) | 0,04 $ | 0,12 $ / 0,09 $ | 0,0005 $ | 0,05 $ | **0,21 / 0,25 / 0,43 $** |
| **Prove** (Haiku per Planner, Generator, Recorder) | 0,003 $ | 0,008 $ / 0,006 $ | 0,0005 $ | 0,003 $ | **0,01 / 0,02 / 0,03 $** |

Il Generator pesa circa il 70% del costo (output con thinking). Per le prove con credito limitato: Generator Haiku (o Sonnet a effort `medium`), consapevoli che più riparazioni e qualità didattica inferiore sono attese; la qualità si decide con `evals/generator/phase6.cases.json` prima di fissare il default (Opus vs Sonnet a `high`: se la qualità didattica e la validità al primo colpo non peggiorano, Sonnet dimezza il costo). Il Recorder, saltato quando non c'è nessun tentativo superato, risparmia ~0,05 $.

### 13.7 Rischi e da verificare

1. **Fonti senza testo.** Oggi il Generator vede solo titolo, URL e annotazione delle fonti, non le pagine: «never trust parametric knowledge» è garantito solo in parte (il prompt vieta numeri, citazioni testuali e affermazioni fuori dal perimetro dell'annotazione; il caso `strength_thin_sources` lo misura). Il segnaposto `{{excerpts_section}}` è già pronto per estratti reali (proposta di contratto: un passo di recupero degli estratti per la fonte primaria, con limite e URL).
2. **Citazioni:** il Validator controlla che `[^key]` esista, non che la fonte sostenga la frase.
3. **Latenza del Generator:** 60-150 s a `high` su Opus (da misurare); l'app mostra già gli step. Streaming indispensabile.
4. **Grader entro 20 s:** Haiku a `low` dovrebbe stare sotto 5-8 s; se `medium` sfora, resta `low`. Misurare p95.
5. **Primo uso degli schemi nativi:** latenza di compilazione sulla prima richiesta (grammatica in cache 24 h).
6. **Verificare all'integrazione:** `max_tokens` del Generator (20000) contro il limite del modello; `effort` ammesso per Haiku 5.5.

## 14. Web fetch lato server per il Generator, Fase 12 blocco C (ADR 0009)

Decisione e parametri: `docs/adr/0009-web-fetch-generator.md`. Questa sezione è la specifica per l'adapter e il caso d'uso. Legenda: **[doc]** = ricavato dalla documentazione ufficiale di Anthropic e OpenRouter (ricerca di `llm-engineer`, 2026-11-11, pagine non rilette in questo passaggio); **[NON PROVATO]** = nessuna chiamata reale fatta (manca la chiave).

### 14.1 Strumento e parametri

- Tipo `web_fetch_20250910`, `name: "web_fetch"` **[doc]**. Parametri: `max_uses` (tetto di letture per richiesta), `allowed_domains` (host nudi, senza schema né percorso, sottodomini inclusi; esclusivo con `blocked_domains`), `max_content_tokens` (tronca la pagina) **[doc]**. Il campo `citations` **non viene inviato**: le citazioni dello strumento sono spente per impostazione predefinita, ed è il comportamento voluto. Valori scelti dall'ADR: `max_uses` come in §14.6, `max_content_tokens` 8000.
- L'ADR dice che con `output_config.format` le citazioni attive darebbero 400: restano spente (default, campo non inviato). Web fetch + `output_config.format` + `pause_turn` nella stessa richiesta: **[NON PROVATO]**. Ripiego già previsto in §12.4: `OutputFormat::Text` con il JSON da prompt e schema applicato a valle (il Generator oggi usa il nativo: se la prova fallisce con il nativo, passare al Text solo quando lo strumento è attivo).
- Nessun header beta noto **[doc]**; se la API risponde con un errore su `anthropic-beta`, è il punto 3 dell'ADR.

### 14.2 Forma della risposta (non in streaming)

**[doc]**, stesso schema di `web_search`:
- `server_tool_use` con `name: "web_fetch"` e `input: {"url": "..."}`.
- `web_fetch_tool_result` con `tool_use_id` e `content` = `{type: "web_fetch_result", url, retrieved_at, content: {type: "document", title, source: {type: "text", media_type: "text/plain", data}}}`; per un PDF remoto `source` è `{type: "base64", media_type: "application/pdf", data}`.
- Errore: `content: {type: "web_fetch_tool_result_error", error_code}` dentro una risposta **200**. Codici documentati: `invalid_tool_input`, `url_too_long`, `url_not_allowed` (anche robots.txt o dominio fuori da `allowed_domains`), `url_not_accessible`, `unsupported_content_type`, `too_many_requests`, `max_uses_exceeded`, `unavailable`. L'adapter li mappa su un evento di progresso, mai su `LlmError`: il modello li vede.
- `usage.server_tool_use.web_fetch_requests` (intero, cumulativo) accanto a `web_search_requests`: nuovo campo `Usage.web_fetches`. Lo strumento non ha prezzo proprio: si pagano i token d'ingresso del testo letto (stima ADR: 0,015-0,05 $ per lezione). L'eval lo misura.
- Blocchi da rimandare tali e quali nelle continuazioni e nelle riparazioni: `server_tool_use`, `web_fetch_tool_result` (testo incluso), `thinking` firmati, testo. Come per la ricerca (§12.2): `Part::ProviderBlock`.
- Il testo di una pagina può arrivare **troncato** a `max_content_tokens`: la documentazione non dice se il risultato lo segnala **[NON PROVATO]**. Per questo il prompt tratta «testo che si interrompe» come lettura parziale.

### 14.3 Ciclo, streaming, riparazioni

- **Non in streaming** (come §12.6 per la ricerca; con OpenRouter in streaming il blocco del risultato non arriva **[doc/provato per web_search]**, per web_fetch **[NON PROVATO]**).
- **`pause_turn`**: stesso ciclo di §12.2 (rimandare il messaggio dell'assistant invariato, tetto 5 continuazioni). Le letture delle richieste precedenti contano nel `max_uses`.
- **Riparazioni**: devono proseguire la **stessa conversazione**: `[user: Procedi. + fonti da leggere]`, `[assistant: tutti i blocchi ricevuti, testo finale incluso]`, `[user: repair.md]`, con lo stesso `tools` (il testo letto resta nel contesto; prompt: «non rileggere»). **[provato 2026-11-15, Anthropic diretto, Haiku 5.5]**: `max_uses: 0` è rifiutato (400, «Input should be greater than 0»), mentre una cronologia con blocchi `web_fetch_tool_result` senza lo strumento nei `tools` è accettata. Quindi la riparazione tiene lo strumento con le letture rimaste del job e lo toglie quando sono finite (una continuazione `pause_turn` non può toglierlo, perché c'è una lettura in sospeso: lì resta a 1, quindi al massimo una lettura oltre il tetto per turno in pausa); il documento da correggere non si ricopia nel `repair.md` (è già l'ultima risposta dell'assistant).
- **Cache**: i `tools` precedono il `system` nel prefisso della cache; con `allowed_domains` diversi per lezione, il blocco `system.md` non si riusa da una lezione all'altra quando lo strumento è attivo (si riusa tra la prima chiamata e le sue continuazioni; nelle riparazioni `max_uses` scende con le letture rimaste e lo strumento sparisce quando sono finite, quindi il prefisso cambia e la cache si riscrive). Accettabile: la cache a 5 minuti tra lezioni diverse era comunque rara. Se pesa, ordinare gli host alfabeticamente e usare un elenco più ampio e stabile per argomento.

### 14.4 Capability per provider

| Provider | `server_web_fetch` | Note |
|---|---|---|
| Anthropic diretto, modelli Claude | `true` | `web_fetch_20250910` **[provato 2026-11-15]** con Haiku 5.5: 1 lettura, ~12k token d'ingresso per una pagina del libro di Rust |
| OpenRouter con modelli Claude | `false` finché non si prova | inoltra `web_search_20250305` (provato); `web_fetch_20250910` **[NON PROVATO]**: è il punto 1 dell'ADR. Ripiego: `openrouter:web_fetch`, forma della risposta non documentata nel formato Messages |
| OpenAI-compatible, locali | `false` | il Generator lavora come oggi (dalle annotazioni). Alternativa futura: `fetch_page {url}` lato client eseguito dal backend, fuori fase |

Con `server_web_fetch = false` o `max_fetches = 0` il backend **non** invia lo strumento né l'elenco «Fonti da leggere» (`generator/kickoff_web.md`): il prompt non cambia, la sezione «Quando hai lo strumento di lettura web» di `system.md` non si applica.

### 14.5 Prova minima prima di accendere (dall'ADR)

Haiku 5.5, una pagina breve, `max_uses: 1`, con `output_config.format`, prima su OpenRouter poi su API diretta. Osservare: 200 o 400; presenza di `web_fetch_tool_result` e di `usage.server_tool_use.web_fetch_requests`; se il testo troncato è segnalato; comportamento con una pagina vietata da robots.txt. Poi l'eval `generator/phase12-fetch` (`backend/evals/README.md`).

### 14.6 Fatti dell'implementazione

- Capability `server_web_fetch`: predefinita **vera** per `anthropic`, **falsa** per `openrouter`; si dichiara con `[llm.providers.<nome>] server_web_fetch = true`.
- Interruttore `[llm.roles.generator] max_fetches` (predefinito 0 = strumento spento).
- `max_uses` = min(fonti web del piano, `max_fetches`, 3). Il budget vale per l'**intero job**, riparazioni comprese (non si resetta a ogni chiamata).
- Il ciclo `pause_turn` è unico, in `application::structured::run_turn`.
- Le chiamate con strumenti lato server hanno un timeout proprio, `server_tool_timeout_secs` (300 s).
- Il documento precedente compare due volte in una riparazione: nella cronologia (turno assistant) e in `repair.md` (`{{previous_document}}`), circa 7k token in più per riparazione.
