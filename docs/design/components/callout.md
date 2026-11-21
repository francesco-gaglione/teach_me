# `callout`

## Scopo didattico
**Segnalazione** (Mayer) di un'informazione che non è il filo principale ma è importante: un errore comune (`warning`), una tecnica per ricordare (`tip`), un contesto laterale (`info`). Usato con parsimonia: se tutto è segnalato, niente lo è (P2).

## Anatomia
È un `StatusPanel` (`base-components` §5) con le varianti `info`, `tip`, `warning`; **niente di nuovo da implementare**.

```
┌──────────────────────────────────────────┐ fill xSoft, raggio md, padding 16, nessun bordo
│ (⚠) Errore comune                        │ icona 24 + titolo ui.labelL, colore x
│                                          │ gap 8
│ `b = a` non copia la stringa: la sposta. │ corpo ui.bodyL, ink (md inline)
│ Dopo, `a` non è più utilizzabile.        │
└──────────────────────────────────────────┘
```

| Variante | Icona | Titolo di default (se `title` assente) | Ruoli |
|---|---|---|---|
| `info` | `info` | «Info» | `info` / `infoSoft` |
| `tip` | `lightbulb` | «Suggerimento» | `tip` / `tipSoft` |
| `warning` | `alert-triangle` | «Attenzione» | `warning` / `warningSoft` |

- `title` (≤ 80) sostituisce l'etichetta, l'icona resta (dual coding, e il tipo non dipende dal solo colore: P8).
- Corpo: `ui.bodyL` `ink` (il sans-serif lo distingue dal testo principale in Literata: seconda modalità di segnalazione oltre al riquadro). Contrasti: titolo su `xSoft` ≥ 5,5; corpo `ink` su `xSoft` ≥ 13,7 / 7,4.
- Larghezza piena colonna. Fra callout e blocchi: 24 dp.

## Stati
Statico. **Non** è una live region (non appare dopo un'azione): `liveRegion` solo nei feedback degli esercizi. Un `warning` non è un errore: colore e icona sono ambra/triangolo, mai il rosso `danger` (P9).

## Interazioni
Il corpo (non il titolo) è evidenziabile con la penna: `highlight.md` (aspetto sugli sfondi `infoSoft`/`tipSoft`/`warningSoft` e contrasti in §3.3 e §10).

Termini/citazioni nel corpo tappabili. Testo selezionabile.

## Accessibilità
Un solo nodo `mergeDescendants` con testo «<titolo>. <corpo>» e `role` assente (la tipologia è nel titolo). L'icona `contentDescription = null`. Non annunciare «avviso» in modo allarmistico.

## Contenuti al limite
600 caratteri nel corpo: si disegna intero (≈ 8 righe). Titolo lungo: va a capo accanto all'icona. Callout all'inizio della sezione, prima del paragrafo: accettato ma sconsigliato.

## Linee guida per il Generator
- **Quando**: `warning` per una trappola concreta o misconcezione (meglio se una delle opzioni di un quiz successivo la usa); `tip` per un modo di ricordare/applicare; `info` per un contesto laterale. Mai per la definizione centrale.
- **Quanto**: max 1 per sezione; `warning` = errore tipico del concetto (uno per sezione di concetto, fino a 5), `tip` e `info` insieme max 2 per lezione. Mai in coda a un esercizio (c'è il feedback). Mai come unico blocco di una sezione.
- **Come**: `title` breve e specifico; per l'errore tipico comincia con «Errore tipico:» («Errore tipico: pensare che `b = a` copi»), 1-3 frasi (≤ 300 caratteri consigliati). Non ripetere il paragrafo che precede.
- Non usare `warning` per sicurezza reale (non è un allarme) né per "importante": l'importante è nel grassetto.
