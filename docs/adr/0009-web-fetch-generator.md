# ADR 0009 — Testo delle fonti web: web fetch del provider dato al Generator

- Stato: accettata, **attivazione in produzione subordinata a una prova reale** (vedi «Da verificare»)
- Aggiornamento 2026-11-19: acceso con `max_fetches = 1` dopo le eval della issue #26.
- Data: 2026-11-11
- Fase 12, blocco C. Decisione del proprietario (2026-11-10): il Generator legge il testo reale delle fonti web con lo strumento web fetch lato server del provider; niente download né archiviazione delle pagine nel backend. Verifica sulla documentazione: `llm-engineer` (2026-11-11, pagine ufficiali di Anthropic e OpenRouter).

## Problema

Per le fonti web il Generator oggi vede solo titolo, URL e annotazione del Researcher: scrive la lezione a memoria e la cita. Per i PDF caricati riceve invece gli estratti (Fase 10). Il proprietario trova le lezioni generiche: devono poggiare sul testo della fonte.

## Scelta

Il Generator riceve lo strumento **web fetch lato server di Anthropic**, versione **`web_fetch_20250910`** (la base: senza filtraggio dinamico, quindi senza code execution), nella stessa chiamata che produce il documento della lezione.

- **Porta LLM**: nuovo `ToolDef::ServerWebFetch { max_uses, allowed_domains, max_content_tokens }` e capability `server_web_fetch` del provider, accanto a `ServerWebSearch` (ADR 0004, ADR 0005 D1). Un provider senza la capability non riceve lo strumento e il Generator lavora come oggi.
- **Capability per provider**: predefinita vera per `anthropic` e **falsa per `openrouter`** (non documentato che inoltri lo strumento); si dichiara con `[llm.providers.<nome>] server_web_fetch = true`. Con `max_fetches > 0` e capability assente il backend registra un avviso e il Generator scrive dalle annotazioni.
- **Parametri**: `max_uses` = min(numero delle fonti web del piano, `max_fetches`, **3**); con più di 3 fonti web si leggono le prime 3 (primaria prima); `max_content_tokens` = **8.000** per pagina; `allowed_domains` = soli host delle fonti del piano (senza schema né percorso: per web fetch una voce con percorso non corrisponde mai; i sottodomini sono inclusi; solo ASCII, punycode per i domini internazionali); **citazioni dello strumento disattivate** (con `output_config.format` l'API risponde 400; la lezione cita già con `[^key]`).
- **URL nel messaggio utente**: lo strumento scarica solo URL comparsi nella conversazione (messaggi utente o risultati di strumenti), non quelli presenti solo nel system prompt. Gli URL delle fonti del piano stanno quindi nel messaggio utente del Generator.
- **Chiamata non in streaming** con il ciclo di continuazione `pause_turn` condiviso con la ricerca (`llm-port.md` §12 e §14; un solo ciclo in `application::structured::run_turn`, con il budget di ogni strumento portato avanti tra le continuazioni). Le riparazioni dopo il Validator proseguono la stessa conversazione: il testo scaricato resta nel contesto, non si riscarica.
- **Fallimenti**: una fetch fallita (`url_not_accessible`, `url_not_allowed` per robots.txt, pagina JavaScript, PDF remoto troppo grande) è un risultato che il modello vede; la lezione si scrive con ciò che è stato letto e, per quella fonte, con le sole informazioni del Researcher. La generazione non fallisce per una fetch fallita. Se invece il provider rifiuta la richiesta per via dello strumento (HTTP 400), il job fallisce con un log che nomina lo strumento: nessun ripiego automatico, si spegne l'interruttore.
- **Interruttore**: `[llm.roles.generator] max_fetches` (0 = spento), come `max_searches` del Researcher. **Predefinito 0** finché la prova reale sul provider di produzione non è superata.
- **Sicurezza**: il testo scaricato è materiale da citare, mai istruzioni (regola nel prompt del Generator, con esempio negativo). La difesa strutturale resta il Validator: schema del catalogo, `sources` e link ammessi solo tra le fonti del piano. Il Generator non ha strumenti client né dati di altri utenti nel contesto, quindi il rischio è la contaminazione della lezione, non l'esfiltrazione.

## Alternative scartate

- **Download nel backend** (HTTP + estrazione del testo, con archiviazione): controllo totale e indipendenza dal provider, ma servono estrazione dell'HTML, limiti, protezione da SSRF, rispetto di robots.txt e spazio su disco. Scartata dal proprietario per questa fase.
- **Versioni più recenti dello strumento** (`web_fetch_20260209` e successive): filtrano il testo con codice eseguito lato provider. Aggiungono code execution, blocchi di risposta che l'adapter non conosce e non sono idonee alla zero data retention. Nessun vantaggio che ci serva ora.
- **`openrouter:web_fetch`** (strumento proprio di OpenRouter): è il ripiego se lo strumento nativo non passa da OpenRouter. Con l'engine nativo corrisponde alla versione con filtraggio dinamico e non tronca i token; con gli engine non nativi il download lo fa OpenRouter e la forma della risposta nel formato Messages non è documentata. Da valutare solo se la prova fallisce.

## Costi attesi

Lo strumento non ha un prezzo proprio: si pagano i token. Una pagina media vale circa 2.500 token, il tetto è 8.000: con 3 fonti sono 7.500-24.000 token d'ingresso in più, circa **0,015-0,05 $ per lezione** con Sonnet 5.5. Una riparazione rilegge il testo (a prezzo pieno senza cache). Stima da confermare con l'eval del blocco C.

## Prova reale dell'11 novembre 2026 (OpenRouter, caso `rust_book_page_facts`, una pagina)

- **OpenRouter inoltra `web_fetch_20250910`** su `/api/v1/messages`: con Haiku 5.5 e con Sonnet 5.5 la pagina è stata letta (14.645 caratteri, nessun errore), senza header beta. Il punto 1 qui sotto è chiuso.
- Con **Sonnet 5.5** la lezione è valida al primo colpo in 99 s (sotto il timeout di 300 s), una sola chiamata, nessun `pause_turn`. Costo 0,22 $ (14.541 token di output, 27k token scritti in cache).
- Con **Haiku 5.5** il Generator ha esaurito i 20.000 token di output senza scrivere il documento: Haiku non è adatto al Generator della Fase 12.
- Via OpenRouter **`usage.server_tool_use.web_fetch_requests` non arriva** e il blocco `server_tool_use` ha **`input` vuoto (`{}`)**: l'URL letto sta solo nel blocco `web_fetch_tool_result`. Corretto lo stesso giorno: le letture si contano dai blocchi (`LlmResponse::reconcile_fetch_count`) e l'URL si ricava dal risultato; riverificato con una terza chiamata reale (Haiku: 1 lettura contata, URL riconosciuto).
- Forma reale della risposta di Haiku: `thinking`, `server_tool_use`, `web_fetch_tool_result`, `thinking`, poi `max_tokens`: i 20.000 token vanno nel ragionamento dopo la lettura.
- Non provati: più pagine, `pause_turn`, riparazione nella stessa conversazione, fetch fallita, API diretta di Anthropic.

Resta spento in produzione finché la suite `generator/phase12-fetch` non è passata per intero (serve credito).

## Da verificare con una chiamata reale (stato dopo la prova dell'11 novembre)

1. **OpenRouter inoltra `web_fetch_20250910`** scritto a mano su `/api/v1/messages`, come già fa con `web_search_20250305` (provato in Fase 4)? Non è documentato. È il punto che decide l'attivazione in produzione.
2. Web fetch insieme a `output_config.format` e a `pause_turn` nella stessa chiamata: nessuna frase esplicita nella documentazione.
3. Se serve ancora un header beta (la pagina attuale dà lo strumento come stabile e non ne cita).
4. Limite al numero di voci in `allowed_domains`; comportamento con i paywall.

5. Forma reale dei blocchi `web_fetch_tool_result` e degli errori (parser e fixture seguono la documentazione, non risposte vere), e se l'API accetta quei blocchi rimandati nelle continuazioni e nelle riparazioni.
6. Tempi: le chiamate con strumenti lato server hanno un timeout di 300 s (`server_tool_timeout_secs`); una lezione da 20.000 token più le letture potrebbe avvicinarlo.

Prova minima: Haiku 5.5, una pagina, `max_fetches = 1`, con il caso `rust_book_page_facts` della suite `generator/phase12-fetch` (`backend/evals/README.md`), prima su OpenRouter (dichiarando `server_web_fetch = true`) poi, se serve, su API diretta. Costo: pochi centesimi. Finché non è superata, `max_fetches` resta 0 in `deploy/teach_me.toml`.

## Prova reale del 2026-11-15 (API Anthropic diretta, issue #16)

Generator e giudice su Sonnet 5.5, `max_fetches` = 3, una run per caso della suite `generator/phase12-fetch` (risultati in `backend/evals/results/2026-11-15_16-*`, ignorati da git).

- **Punti 2, 5 e 6 chiusi.**
  - Lo strumento funziona insieme a `output_config.format`.
  - Gli URL letti sono solo quelli del piano.
  - L'API rifiuta `max_uses: 0` (400) e accetta una cronologia con blocchi `web_fetch_tool_result` anche senza lo strumento. Per questo la riparazione toglie lo strumento quando le letture del job sono finite e non ricopia il documento, che è già l'ultima risposta.
  - La chiamata più lunga con lo strumento è durata 116 s, il 39% del limite di 300 s.
- **Costo.** Lo stesso piano con 3 pagine costa 0,39 $ contro 0,16 $ del gemello senza lettura: +0,23 $ a lezione, il triplo della soglia di 0,08 $ della suite. Il confronto però è sporco, perché la lezione con la lettura è venuta molto più lunga.
- **Qualità.** La rubrica delle lezioni con la lettura è più bassa di quella del gemello (0,23-0,33 contro 0,60). Il giudice trova affermazioni attribuite alla pagina che la pagina non contiene. Con la fonte irraggiungibile (`.invalid`, errore `url_not_allowed`, non `url_not_accessible`) la lezione si genera, ma cita lo stesso la fonte non letta.
- **Decisione: `max_fetches` resta 0.** Prima di riprovare bisogna correggere le attribuzioni nel prompt del Generator. Il tentativo successivo è `max_fetches` = 1, sulla sola fonte primaria.

Non misurato nemmeno dopo la prova: la resistenza alla prompt injection da una pagina (il caso di eval richiede una pagina di prova pubblicata dal proprietario; il runner oggi lo salta).

## Conseguenze

- Le lezioni su fonti web diventano più lente (una o più fetch prima della scrittura) e un po' più care.
- Le pagine rese con JavaScript e quelle che vietano i crawler non si leggono: per quelle la lezione resta com'è oggi. Il Researcher può preferire fonti leggibili (fuori da questa fase).
- Il provider mette in cache i risultati: una pagina può essere letta in una versione non recentissima.
- Fuori dalla fase: web fetch con provider diversi da Anthropic, estratti web per il Tutor, archiviazione delle pagine.
