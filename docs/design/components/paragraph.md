# `paragraph` e markdown inline

## Scopo didattico
Il veicolo principale della spiegazione. Deve essere leggibile a lungo (P10) e rispettare la memoria di lavoro (P1): pochi concetti per paragrafo, termini nuovi introdotti con il glossario (pre-training), segnalazione con grassetto solo dove conta (Mayer).

## Anatomia e misure

```
In Rust ogni valore ha un unico **proprietario**. Quando
[[ownership|lo sposti]] a un'altra variabile, la prima non è
più utilizzabile[^rust_book_4_1].
```

| Misura | Valore |
|---|---|
| Stile | `reading.body` (Literata 400, 17 sp / 28 sp), `ink` su `bg`, contrasto 16,5 / 14,4 |
| Larghezza | colonna ≤ `readingMaxWidth` 600 dp: ≈ 65-70 caratteri per riga a font normale (obiettivo 45-75). Compact 360 dp: ≈ 40 caratteri (limite noto, non si riduce il font) |
| Allineamento | a sinistra, **non** giustificato; `LineBreak.Paragraph`, `Hyphens.Auto` (lingua `it`, o disattivato se `lang` ≠ `it`) |
| Spaziatura | 16 dp fra paragrafi consecutivi; 24 dp verso altri blocchi |
| Capoverso | nessun rientro, nessun a capo interno (il contratto lo vieta) |

## Markdown inline (campi *md* di tutti i blocchi)

Sottoinsieme di `screens/chat.md` §5, **solo inline**, più due estensioni. Un unico `InlineMarkdown(text, baseStyle, interactive)` condiviso da tutti i blocchi. Il parser è **tollerante**: ciò che non è nel sottoinsieme resta letterale; marcatori non chiusi restano letterali; mai crash.

| Sintassi | Rendering (stile `reading.body`; in contesti `ui` si usa la stessa famiglia della base) |
|---|---|
| `**grassetto**` | `reading.bodyStrong` (Literata 600), `ink`. Unico mezzo di segnalazione: niente colore, niente evidenziatore nel testo (il `mark` non si usa nelle lezioni) |
| `*corsivo*` | italic 400 |
| `` `codice` `` | `code.inline` (JetBrains Mono 15/28; dimensione = base − 2 sp se la base è `ui.bodyL`/`ui.body`), `codeInk` su `surfaceSunken` (span background), senza ligature, non va a capo a metà token se evitabile; in `ink` per i contrasti (≥ 14:1) |
| `[testo](https://…)` | `accent`, **sempre sottolineato**; tocco → Custom Tab; solo `https` (altri schemi: testo + URL in chiaro tra parentesi). Area di tocco estesa in verticale a 28 dp (altezza riga) |
| **`[[key]]` / `[[key|testo]]`** (termine) | testo in `ink`, **sottolineatura tratteggiata** 1,5 dp `knowledge` a 3 dp sotto la linea di base (tratto 4 dp, vuoto 3 dp), tappabile: apre lo sheet del glossario ([glossary_term.md](glossary_term.md) §Sheet). La forma (tratteggio) è il canale non cromatico. Se `key` non esiste: testo semplice senza decorazione |
| **`[^key]`** (citazione) | piccolo rimando **numerico** `ui.labelS` (12 sp, 600) in `knowledge`, racchiuso in parentesi quadre «[1]» a 2 dp dal testo; numero = posizione di `key` in `sources[]` (1-based, stabile). Tocco → sheet «Fonte» (titolo, dominio, «Apri» che va al browser, «Chiudi»). Se `key` non esiste: nulla (nessun «[?]»). Fonte con `url: null` (PDF dell'utente): sheet senza «Apri», vedi `screens/lesson.md` §3.3 |

**Interattività dei span.** `interactive = true` (default) in: `paragraph`, `callout`, `glossary_term`, domanda di un esercizio, `why` di `primary_source`, `feedback`, `back` della flashcard dopo la rivelazione, `caption` del codice. `interactive = false` — termini e citazioni diventano **testo semplice senza decorazione**; i link non sono link — in: opzioni di quiz, passi di ordinamento, voci di checklist, `front` della flashcard, testo del `fill_blank` (vedi sotto). Motivi: (a) il tocco sull'opzione deve sempre selezionare; (b) decorazioni diverse fra opzioni sarebbero indizi (P7); (c) un termine nella domanda è invece pre-training legittimo. Nel `fill_blank` il testo è una frase da completare: il tratteggio disturberebbe i campi.

**Annidamento.** Grassetto/corsivo si annidano; codice, termine e citazione sono "foglie" (nessun markup al loro interno: `` `**x**` `` mostra gli asterischi).

**Accessibilità dei span.** Un termine è letto «<testo>, termine del glossario, doppio tocco per la definizione» (azione custom «Apri definizione»); una citazione «Fonte 1: <titolo>»; un link «<testo>, link». I span interattivi sono anche azioni custom del nodo del paragrafo (TalkBack: menu azioni locali) perché i span dentro un testo lungo non sono sempre navigabili singolarmente. I span non sono target ≥ 48 dp (WCAG 2.2 2.5.8: esente, target in linea con il testo), ma l'area di tocco comprende l'intera altezza riga (28 dp) più 4 dp di margine orizzontale.

## Stati
Statico; span in `pressed` con fill `surfacePressed` per 100 ms; `focus` (tastiera: i span interattivi sono navigabili con Tab nell'ordine del testo, anello 3 dp attorno al span, ma **solo se `Configuration.keyboard` ≠ `NOKEYS`**, altrimenti sarebbero rumore per TalkBack che usa le azioni custom).

## Interazioni
Evidenziazione con la penna (solo penna, 4 colori, si aggancia alle parole): `highlight.md`. Con la penna si evidenzia e non si seleziona; la selezione lunga con il dito resta com'è.

Tocco su span → sheet. Selezione lunga del testo e copia di sistema. Nessun gesto speciale.

## Contenuti al limite
- 1200 caratteri: ≈ 17 righe a 600 dp, si disegna intero. Se il Validator non l'ha impedito, nessuna riduzione.
- Parola lunga (URL, identificatore) → `softWrap` con spezzatura di sicurezza (`LineBreak.Simple` per quel paragrafo se necessario), mai overflow orizzontale.
- `lang` ≠ `it`: `LocaleList`, `Hyphens.None`; il corsivo non è automatico (la citazione in inglese è tipograficamente un paragrafo normale; è il Generator a marcarla con `*…*` se serve).
- Paragrafo composto solo di spazi: non si disegna.

## Linee guida per il Generator
- 250-500 caratteri (max 1200), una idea. Frase del tema all'inizio («La regola è…»), poi esempio o conseguenza. Frasi ≤ 25 parole.
- Grassetto: 0-2 per paragrafo, solo per **il termine o la regola centrale**. Mai intere frasi, mai in sequenza ravvicinata.
- `[[key]]` alla **prima menzione** di un termine del glossario (anche nelle domande), poi testo semplice; non su ogni occorrenza (troppe sottolineature = rumore, annulla la segnalazione). Max 3 per paragrafo.
- `[^key]` a fine frase, dopo l'affermazione sostenuta dalla fonte. Max 2 per paragrafo.
- Niente "come visto sopra", niente rimandi a posizioni; niente elenchi mascherati con punto e virgola.
- Citazione in lingua originale: paragrafo a sé con `lang`, **tra virgolette caporali** «…» e subito dopo una parafrasi italiana.
