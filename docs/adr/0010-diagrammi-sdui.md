# ADR 0010 — Diagrammi nelle lezioni: blocco SDUI `diagram` con schema proprio

- Stato: accettata
- Data: 2026-11-12
- Richiesta "Diagrammi e disegni". La richiesta lasciava quattro domande aperte; il proprietario ha chiesto di implementarla senza rispondere, quindi le ha chiuse il tech lead con le scelte qui sotto, confermate dal proprietario il 2026-11-13. Sono tutte reversibili per aggiunta (nessuna preclude le alternative).

## Problema

Il catalogo SDUI non ha blocchi visivi. Flussi, gerarchie e cicli si capiscono prima da una figura che da un paragrafo; il Generator oggi ripiega sul testo o su `missing_component`.

## Scelta

Un nuovo blocco **`diagram`** nel catalogo v1 (`docs/contracts/sdui.md` §4 e §10, contratto v1.3): nodi, archi ed etichette descritti in JSON, con tre `layout` (`flow`, `tree`, `cycle`), disegnati dall'app in Compose con i token del design system.

1. **Solo diagrammi, niente disegni liberi.** Un diagramma si descrive e si valida in modo strutturato; un disegno libero (lo schema di una cellula) richiede SVG o immagini generate, con costi, tempi e rischi di errore non controllabili dal Validator.
2. **Schema nostro, non Mermaid in WebView.** Il contratto vieta già le WebView (§1). Uno schema piccolo dà tema chiaro/scuro e accessibilità senza lavoro in più, è validabile dal backend con errori utili al repair e non lascia al modello una sintassi libera da sbagliare.
3. **Solo nelle lezioni.** Le risposte del Tutor in chat sono markdown in streaming, non blocchi SDUI: portarci i diagrammi è un lavoro diverso.
4. **Figura statica.** Niente zoom né tocco sui nodi: con al massimo 8 nodi la figura sta nella colonna. I dettagli vanno in `detail` o nel testo.
5. **`alt` obbligatorio**: descrizione testuale per lo screen reader e fallback quando l'app non riesce a disegnare.
6. **`catalog_version` resta 1.** Aggiungere un tipo non rompe le app precedenti, che mostrano la riga neutra dei blocchi sconosciuti.

Niente layout `sequence` (attori e messaggi nel tempo) in questa versione: un processo nel tempo si esprime con `flow`.

## Alternative scartate

- **Mermaid in WebView**: sintassi nota ai modelli e molti tipi di diagramma, ma rompe il principio "tutto nativo", va tematizzato a mano, il testo non è validabile dal backend e una sintassi sbagliata si scopre solo sul dispositivo.
- **SVG generato dal modello**: copre anche i disegni liberi, ma è lungo, costoso, non verificabile e non segue il design system.
- **Immagini generate**: altro provider, altri costi, testo nelle immagini inaffidabile.

## Conseguenze

- Backend: parser e Validator conoscono `diagram` e il nuovo codice `diagram_structure`.
- Prompt del Generator: quando una figura aiuta e quando è decorazione.
- App: nuovo renderer; le app già installate vanno aggiornate per vedere le figure.
- Estensioni possibili senza rompere nulla: nuovi `layout`, diagrammi nella chat, tocco sui nodi.
