# ADR 0004 — Port `LlmProvider` e configurazione dei modelli

- Stato: accettata
- Data: 2026-11-08
- Dettaglio tecnico: `backend/prompts/llm-port.md` (proposta di `llm-engineer`, approvata dal tech lead)

## Contesto

Tutti gli agenti (Tutor, Interviewer, Researcher, Planner, Generator, Grader, Recorder) passano da un solo port, implementato oggi da un adapter Anthropic e in futuro da adapter OpenAI-compatible (OpenRouter, modelli locali). Il port deve reggere senza cambiare forma lo streaming della chat, i tool del Tutor e l'output strutturato del Generator.

## Decisione

1. **Port nel dominio, senza dipendenze di runtime**: `LlmProvider` con `capabilities(role)`, `stream(req)` obbligatorio e `complete(req)` di default costruito su `stream`. Lo stream è un trait *pull* (`async fn next()` via `async-trait`), non `futures::Stream`; droppare lo stream annulla la richiesta.
2. **La richiesta indica un ruolo, non un modello**: `LlmRequest.role` è risolto in modello e parametri dalla configurazione. Niente `temperature`/`top_p` (rifiutati dai modelli attuali).
3. **Errori classificati** (`LlmError`, `non_exhaustive`) e mappati sugli `error.code` del contratto: solo `RateLimited` → `rate_limited`; errori del provider, di rete, timeout, autenticazione e risposte malformate → `upstream_failed`; richieste non valide, funzioni non supportate e contesto troppo lungo → `internal`.
4. **Retry come decoratore in `application`** (`RetryingLlm`), non negli adapter: massimo 3 tentativi, backoff esponenziale con jitter, `retry-after` rispettato fino a 60 s, **solo prima del primo evento di contenuto** (dopo, il messaggio fallisce e si usa "Riprova"). Timeout: connessione 10 s, richieste non in streaming 120 s, stream con 60 s di inattività massima.
5. **`serde_json` ammesso nel dominio** (schemi JSON dei tool e dell'output strutturato, input dei tool): un tipo JSON proprio sarebbe solo una copia. ADR 0002 aggiornato.
6. **Niente `tool_choice` forzato in v1** (`Auto`/`None`): i modelli attuali lo rifiutano. L'obbligo di produrre un risultato si ottiene con l'output strutturato.
7. **Fallback lato server sui rifiuti spento** finché non è provato con una chiave reale.
8. **Configurazione**: default dei modelli per ruolo compilati nel backend; il TOML (`[llm.roles.<ruolo>]`) contiene solo override. Default iniziali: Tutor, Interviewer, Researcher, Recorder → `claude-sonnet-5-5`; Planner, Generator → `claude-opus-5-5`; Grader → `claude-haiku-5-5`. Sono ipotesi da confermare con gli eval.
9. **Chiave API**: `TEACH_ME_ANTHROPIC_API_KEY`, con ripiego su `ANTHROPIC_API_KEY`, in un tipo segreto mai scritto nei log. Senza chiave il backend si avvia con un warning e le chiamate LLM falliscono con `upstream_failed`. L'adapter usa l'URL ufficiale dell'API come default; **non** legge `ANTHROPIC_BASE_URL` (variabile usata da altri strumenti sulla macchina di sviluppo): un URL diverso si configura solo con `TEACH_ME_ANTHROPIC_BASE_URL`.
10. **Prompt versionati** in `backend/prompts/<ruolo>/`, inclusi a compile time (`include_str!`), con la versione (`VERSION`) nei log di ogni chiamata.
11. **Test senza chiamate reali**: l'adapter si testa con fixture SSE registrate servite da un server HTTP locale; i casi d'uso con un `FakeLlm` che registra le richieste.

## Alternative considerate

- `futures_core::Stream` come tipo di ritorno: più idiomatico con i combinator, ma porta una dipendenza e complica gli adapter scritti a mano. Il trait pull basta.
- Retry dentro l'adapter: ogni adapter futuro lo dovrebbe reimplementare, e l'adapter non sa se il contenuto è già stato mostrato all'utente.
- Modello indicato nella richiesta: lega il codice applicativo ai nomi dei modelli e impedisce di cambiarli per ruolo dalla configurazione.

## Conseguenze

- **Aggiornamento 2026-11-09 — OpenRouter.** In attesa dei crediti Anthropic, lo stesso adapter parla con l'endpoint di OpenRouter compatibile con la Messages API (`https://openrouter.ai/api`, autenticazione `Authorization: Bearer`). Si attiva con `TEACH_ME_LLM_PROVIDER=openrouter` e `TEACH_ME_OPENROUTER_API_KEY`; gli id dei modelli diventano slug OpenRouter (`claude-sonnet-5-5` → `anthropic/claude-sonnet-5.5`) e `refusal_fallback` non viene mai inviato. Provato davvero con Haiku 5.5: streaming, effort, tool e prompt caching funzionano. Verificato il 2026-11-09: anche lo strumento lato server `web_search_20250305` passa da OpenRouter (blocchi `server_tool_use`/`web_search_tool_result`, `usage.server_tool_use.web_search_requests`); una ricerca con Haiku è costata ~0,012 $, di cui ~16k token di input per i risultati. D1 (ADR 0005) resta valida.
- **Aggiornamento 2026-11-11 — ritorno ad Anthropic diretto.** Arrivata la chiave Anthropic, `backend/deploy/teach_me.toml` torna a `default_provider = "anthropic"`. Il provider è unico per il server e le chiavi sono per utente (ADR 0008): dopo il deploy ogni utente deve sostituire la chiave OpenRouter salvata con una chiave Anthropic, altrimenti riceve `llm_key_rejected`. OpenRouter resta supportato dallo stesso adapter. Gli eval sono stati misurati su Anthropic diretto con Sonnet 5.5.

- Gli adapter futuri dichiarano le capability (tool, output strutturato nativo/emulato, caching, context window) e l'application si adatta.
- Da verificare con la chiave reale: header `anthropic-version`, eventi `error` a metà stream, semantica di `usage`; il sottoinsieme di JSON Schema accettato dall'output strutturato.
