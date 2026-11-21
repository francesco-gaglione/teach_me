# ADR 0005 — Decisioni aperte dell'MVP (D1-D4)

- Stato: accettata
- Data: 2026-11-08
- Decise dal proprietario su proposta del tech lead.
- Nota 2026-11-21: D3 superata in parte da ADR 0019 (lingua per argomento).

## D1 — Ricerca web delle fonti: strumenti web di Anthropic

Il Researcher usa gli strumenti **lato server** dell'API Claude per cercare sul web e leggere le pagine (web search e, se serve, web fetch), con la stessa chiave API già configurata (ADR 0004).

- Pro: nessun servizio in più da installare o mantenere, nessuna seconda chiave; risultati con citazioni.
- Contro: costo a consumo per ricerca sulla chiave Anthropic; dipendenza dal provider per la ricerca (un adapter OpenAI-compatible futuro non avrà lo stesso strumento).
- Conseguenze: il port `SearchProvider`/`PageFetcher` di `docs/mvp.md` §3 va ripensato (ricerca come capability del provider LLM oppure port separato con un adapter "Anthropic web tools"); i dettagli tecnici (nomi degli strumenti, limiti, domini da escludere, costi) si verificano sulla documentazione ufficiale.
- Scartate: SearXNG self-hosted (da gestire, qualità variabile), Brave Search API e Tavily (un'altra chiave e un altro fornitore).

## D3 — Lingua delle lezioni: italiano con termini originali

Lezioni sempre in **italiano**; i termini tecnici restano nella forma originale quando è quella usata dalla comunità (es. "borrow checker", "lifetime"), e il **glossario** li spiega in italiano. Le citazioni dalle fonti restano nella lingua originale, con una parafrasi in italiano se servono alla comprensione.

## D2 — Formato delle schede di ripasso: solo fronte/retro

Le `ReviewCard` dell'MVP sono **fronte/retro**. Il cloze si valuta dopo l'MVP.

## D4 — FSRS: crate esistente

Si usa un'implementazione Rust esistente di FSRS (open-spaced-repetition) **dietro il port `ReviewScheduler`**, dopo averne verificato licenza, manutenzione e correttezza contro i casi di riferimento. Se non regge la verifica, si implementa l'algoritmo nel dominio con gli stessi test.
