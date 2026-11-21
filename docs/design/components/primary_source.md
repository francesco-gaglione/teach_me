# `primary_source`

## Scopo didattico
Indicare **dove approfondire** con la fonte che ha informato la lezione, e *perché* vale la pena (motivazione intrinseca e autonomia: l'utente sceglie se andare oltre). Una sola per lezione, in chiusura. Le altre fonti citate restano come rimandi `[^key]` e nel pannello di contesto.

## Anatomia

```
┌──────────────────────────────────────────────────┐ Surface interactive (un solo target), fill surface, bordo line, raggio md, padding 16
│ (📖) Per approfondire                          ↗ │ icona book-open 24 (knowledge) + ui.label knowledge ; trailing icona external-link 20 inkSoft
│ The Rust Book, 4.1 What is Ownership?            │ source.title: ui.title3 ink (2 righe max a font normale, poi a capo libero)
│ doc.rust-lang.org                                │ dominio: ui.body inkSoft (host dell'url, senza www.)
│ Spiega l'ownership con lo stack e lo heap,       │ why: reading.body 17/28 ink (md inline interactive = false)
│ utile se vuoi capire perché `String` si sposta.  │
└──────────────────────────────────────────────────┘
```

- Il blocco è **un unico target** (≥ 48 dp, intera card): `Role.Button`, apre `source.url` nel browser di sistema (Custom Tab); se il Custom Tab non è disponibile, `Intent.ACTION_VIEW`. Pressed `surfacePressed`; focus anello. Solo `https` (altrimenti la card non è tappabile e mostra l'URL in testo).
- Il `why` non contiene tappabili annidati (un target solo).
- Se `source` non esiste in `sources[]` (key sconosciuta): blocco non renderizzabile.
- Contrasti: `knowledge` su `surface` 6,4 / 10,1; `ink` ≥ 17 / 13,4.
- Gap 24 dp.

## Stati
Normale / premuto / focus. Offline: tappabile comunque (il browser gestisce l'errore); nessuno stato disabilitato. Dopo il tocco: nessuna marcatura «visitato».

## Accessibilità
Un nodo: «Per approfondire: The Rust Book, 4.1 What is Ownership?, doc.rust-lang.org. <why>. Apre il browser» con `onClickLabel` = «Apri la fonte nel browser». Link esterno annunciato come tale.

## Contenuti al limite
Titolo 200 caratteri: va a capo, nessuna ellissi; URL lunghissimo: si mostra solo il dominio. `why` 300 caratteri ≈ 4 righe.

## Linee guida per il Generator
Esattamente **uno** per lezione (`primary_source_count`), nella sezione finale. `why` = **perché questa fonte e cosa troverai** («Spiega X con Y; utile per Z»), non un riassunto della lezione né un "per saperne di più". Scegliere la fonte che meglio estende l'obiettivo, non la più generica. Ogni fonte in `sources` deve comparire (citata o primaria).


## Variante «Dal tuo PDF» (Fase 10: `source.url == null`)

La fonte primaria può essere un PDF caricato dall'utente (di norma lo è, se la lezione segue un suo documento). Senza URL la card **non è un bottone**:

```
┌──────────────────────────────────────────────────┐ Surface static (fill surface, bordo line, raggio md, padding 16), NESSUN trailing, nessuno stato pressed
│ (📄) Per approfondire                            │ icona file-text 24 + ui.label knowledge
│ Rust in Action, cap. 4 (pp. 101-118)             │ source.title ui.title3 ink (contiene già le pagine)
│ Dal tuo PDF                                      │ al posto del dominio: ui.body inkSoft
│ Rileggi il capitolo sul borrow checker: spiega…  │ why: reading.body ink
└──────────────────────────────────────────────────┘
```

- Non esiste un URL, quindi niente tocco né `Role.Button` (una card che sembra tappabile e non lo è è un indizio falso). Il testo è selezionabile; le pagine sono in chiaro per poter cercare nel proprio file.
- Accessibilità: un nodo di sola lettura «Per approfondire: Rust in Action, cap. 4 (pp. 101-118), dal tuo PDF. <why>».
- Contrasti: come la variante web.
- **Generator**: il `why` per un PDF dice **cosa rileggere e perché** («Rileggi la parte sui lifetime: l'autore li introduce con il diagramma dello stack, utile per…»), non «Apri il PDF». Le pagine stanno nel `title` (vedi `generator-guidelines.md` §PDF).
