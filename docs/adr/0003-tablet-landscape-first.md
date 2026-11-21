# ADR 0003 — App Android: prima il tablet in orizzontale

- Stato: accettata
- Data: 2026-11-08

## Contesto

L'app è stata progettata e realizzata pensando al telefono in verticale: `design-system.md` §6 prevede per le finestre larghe (expanded, > 840 dp) solo centratura e larghezza massima, "nessun layout a due pannelli". Il proprietario studia soprattutto su **tablet in orizzontale**; il telefono viene dopo.

## Decisione

1. Il layout di riferimento è il **tablet in orizzontale**, cioè le window size class **expanded** in larghezza (≥ 840 dp; tipicamente 1280×800 dp). Ogni specifica di schermata descrive prima il layout expanded, poi medium e compact.
2. Usiamo layout adattivi basati sulle **window size class** (non sul tipo di dispositivo né sull'orientamento): dove la navigazione è "lista → dettaglio" (Home → Cartella → Argomento) si usa un layout **list-detail a più pannelli** su expanded; su compact resta la navigazione a schermate impilate. La lettura delle lezioni mantiene una larghezza di colonna leggibile (`readingMaxWidth`) anche su schermi larghi, e lo spazio laterale si usa per contesto utile (es. indice della lezione, glossario), non per allargare il testo.
3. Il telefono resta supportato e testato (compact), ma con priorità seconda: una schermata è "fatta" quando funziona bene su expanded orizzontale e non è rotta su compact.
4. Test: l'emulatore di riferimento per l'accettazione è un tablet in orizzontale (2560×1600 px, 320 dpi ≈ 1280×800 dp). Il telefono si verifica dopo.
5. L'app non blocca l'orientamento: funziona anche col tablet in verticale (medium) e con la finestra ridimensionata (multi-window).

## Conseguenze

- `docs/design/design-system.md` §6 va aggiornato (via `learning-designer`): la regola "nessun layout a due pannelli" decade.
- Le specifiche di Home, Cartella, Argomento vanno completate con il layout expanded; le bozze di Lezione e Ripasso vanno riviste alla loro realizzazione.
- In Android serve una dipendenza per le window size class (es. `androidx.compose.material3.adaptive` o `androidx.window`), da scegliere coerentemente col design system custom (che non usa Material).
- Il test di accettazione iniziale è stato fatto su telefono: le verifiche su tablet fanno parte dell'accettazione di Home, Cartella e Argomento.
