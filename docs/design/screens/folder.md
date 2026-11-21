# Schermata Cartella

> Raggiungibile da Home (tocco su una cartella). Componenti: `../base-components.md` (TopBar, ListRow, StatusChip §5 e §10.3, Button, `InputDialog` §10.1, `ConfirmDialog` §8, `ActionMenu` §10.2, Skeleton §10.4, `ErrorState` §10.5, Toast). Terminologia: `../design-system.md` §11. Pattern di errore e testi dei dialog identici a `home.md`: qui si specificano le differenze.

## Scopo didattico

Mostrare gli argomenti di una cartella e, per ognuno, **a che punto è** in parole ("Definiamo la mission", "Pronto per le lezioni"), così il passo successivo è ovvio (P1). Lo stato è testo + icona, mai solo colore (P8) e non è una percentuale di "completamento" (P9).

## Layout (compact, 360 dp)

```
┌──────────────────────────────────────┐
│ ←  Lavoro                      ⋮     │  TopBar: ui.title2 (fino a 2 righe se fontScale > 1,3,
├──────────────────────────────────────┤  altrimenti 1 riga con ellissi: il nome intero è nel dialog)
│ Argomenti                            │  ui.label inkSoft, heading
│ ┌──────────────────────────────────┐ │
│ │ Rust                          ⋮  │ │  ListRow ≥ 72 dp: titolo ui.title3 (a capo, mai ellissi)
│ │ ( 💬 Definiamo la mission )      │ │  + StatusChip (28 dp) | ⋮ IconButton 48 dp
│ ├──────────────────────────────────┤ │
│ │ Fotografia                    ⋮  │ │
│ │ ( ✓ Pronto per le lezioni )      │ │
│ └──────────────────────────────────┘ │
│                                      │
│        [ +  Nuovo argomento ]        │  Button primario grande 56 dp, ancorato in basso
└──────────────────────────────────────┘
```

Colonna scorrevole, margini 16 dp, `contentMaxWidth` 840 dp, padding inferiore 88 dp.

- **TopBar ⋮** (cartella): `ActionMenu` con **Rinomina cartella** (`pencil`) e, separata, **Elimina cartella** (`trash-2`, `danger`). Il titolo mostra il nome della cartella e si aggiorna dopo la rinomina.
- **Riga argomento**: intera riga (esclusa ⋮) → schermata Argomento. Titolo `ui.title3`, sotto lo `StatusChip`. ⋮ → `ActionMenu` "Rinomina" · "Elimina" (e, in futuro, "Sposta in…", vedi sotto). Pressione lunga = stesso menu. Ordine: come dal backend (`created_at`): il più vecchio sopra, il nuovo in fondo.
- **Chip di stato** (testi e icone di `topic.md`; varianti §10.3):

| `state` | Testo | Icona | Variante |
|---|---|---|---|
| `new` | Definiamo la mission | `message-circle` | neutral |
| `interviewing` | Definiamo la mission | `message-circle` | neutral |
| `mission_proposed` | Conferma la mission | `flag` | info |
| `researching` | Cerco le fonti… | `search` (statica nella lista) | neutral |
| `ready` | Pronto per le lezioni | `check-circle` | accent |
| sconosciuto (`Unknown`) | Stato non riconosciuto | `help-circle` | neutral |

  `new` e `interviewing` hanno lo stesso testo di proposito: per l'utente è la stessa fase. Solo `mission_proposed` e `ready` hanno colore, perché sono i due momenti in cui l'utente deve (o può) fare qualcosa; il resto è calmo. Nessun conteggio di lezioni o "% completato" nella riga.
- **Nuovo argomento**: ancorato come in Home; assente nello stato vuoto (la CTA è nel centro).
- Il **conteggio** non compare nella schermata (lo dice già la lista).

## Stati

| Stato | Contenuto |
|---|---|
| Caricamento | TopBar con il nome (noto dalla navigazione), skeleton 3 righe; "Sto ancora caricando…" dopo 8 s. |
| **Vuoto** | Niente illustrazione. `ui.title1` "Nessun argomento qui"; `ui.bodyL` "Un argomento è qualcosa che vuoi imparare, ad esempio *Rust* o *Fotografia*. Ti aiuto a capire perché ti serve e preparo le lezioni."; primario grande "Crea il primo argomento". |
| Lista | Come in layout. |
| Errore di caricamento | `ErrorState` come Home. `404` (cartella eliminata altrove): torna a Home con Toast "Questa cartella non esiste più." |
| Errore di aggiornamento | Banner come Home: "Non riesco ad aggiornare gli argomenti." |

Se ci sono più di 50 argomenti, caricamento della pagina successiva allo scorrimento (cursore, `api-conventions.md` §8) con un'ultima riga-spinner "Carico altri argomenti…"; errore di pagina → riga con testo "Non riesco a caricare altri argomenti" e bottone testuale "Riprova".

## Crea argomento

`InputDialog`:

| Elemento | Testo |
|---|---|
| Titolo | Nuovo argomento |
| Label / placeholder | Titolo dell'argomento / Es. Rust |
| Limite | 120 caratteri (contatore da 96) |
| Primario / loading | Crea / Creo… |

Errori sul campo `title`: `empty` "Dai un titolo all'argomento." · `too_long` "Al massimo 120 caratteri: ora sono 134." (dal server: "Al massimo 120 caratteri.") · `duplicate` "In questa cartella c'è già un argomento con questo titolo." Errori di azione: come Home con "L'argomento non è stato creato." (rete) e `404` cartella: dialog chiuso, torna a Home, Toast "Questa cartella non esiste più."

Successo: dialog chiuso, riga in fondo con scorrimento, focus sulla riga, Toast "Argomento «Rust» creato" con azione testuale **"Apri"** (6 s) che porta alla schermata Argomento. Dopo la creazione si **apre direttamente l'argomento** (la Chat per definire la mission): è il passo successivo naturale.

## Rinomina argomento

`InputDialog` "Rinomina argomento", campo precompilato e selezionato, primario "Salva"/"Salvo…". Successo: riga aggiornata, Toast "Argomento rinominato in «Rust avanzato»". Errori: come sopra ("L'argomento non è stato rinominato."). `404` argomento: dialog chiuso, lista ricaricata, Toast "Questo argomento non esiste più."

## Elimina argomento

`ConfirmDialog`, conferma distruttiva.

| Titolo | Corpo | Conferma |
|---|---|---|
| Eliminare l'argomento «Rust»? | Verrà eliminato insieme a tutto ciò che contiene. Non si può annullare. | Elimina argomento |

Annulla ha il focus iniziale. Successo: riga rimossa, Toast "Argomento «Rust» eliminato", focus sulla riga vicina (o sul titolo dello stato vuoto). `404` = successo. Errore di rete: nel dialog, "L'argomento non è stato eliminato."

## Elimina / rinomina la cartella da qui

Stessi dialog e testi di `home.md` (il conteggio per il dialog di eliminazione è `topic_count` da `GET /folders/{id}`, ricaricato all'ingresso e dopo ogni creazione/eliminazione di argomento; l'elenco in pagina può essere incompleto, il conteggio no). Dopo la rinomina il titolo della TopBar cambia, Toast come in Home. Dopo l'eliminazione si torna a Home con Toast "Cartella «Lavoro» eliminata"; il focus va alla riga successiva di Home.

## Spostare un argomento in un'altra cartella: **rimandato**

Decisione: **non previsto per ora.** Motivi: (1) richiede un selettore di cartelle e un percorso di errore in più (`duplicate` nella cartella di destinazione, che compare in un luogo diverso dal campo di rinomina); (2) con pochi argomenti vuoti, ricrearlo costa quasi nulla; (3) l'API è già pronta, aggiungerlo dopo non cambia nulla di quanto sopra. Conviene farlo prima che gli argomenti accumulino contenuto (mission, fonti, lezioni).

Progetto già pronto per quando servirà: voce di menu "Sposta in…" (`folder-input`) → `BottomSheet` (§9) "Sposta «Rust» in" con elenco radio delle cartelle (`ChoiceRow` §9, la cartella attuale selezionata e etichettata "Qui" non selezionabile di nuovo), bottone "Sposta" (primario, disabilitato finché non si sceglie un'altra cartella, con testo "Scegli una cartella diversa"). `duplicate` → `StatusPanel danger` nello sheet: "Nella cartella «Hobby» c'è già un argomento con questo titolo. Rinominalo prima di spostarlo." Dopo lo spostamento: riga rimossa, Toast "«Rust» spostato in «Hobby»".

## Accessibilità

- Ordine TalkBack: Indietro → titolo (heading) → "Altre azioni per la cartella" → heading "Argomenti" → righe → "Nuovo argomento".
- Riga: un elemento `Role.Button`, `contentDescription` "Rust, stato: definiamo la mission" (il chip non è un target separato né ripetuto); azioni personalizzate "Rinomina", "Elimina"; ⋮ secondo target ("Altre azioni per Rust"), 48 × 48 dp.
- Annunci: ogni Toast è live region Polite; la creazione sposta il focus sulla nuova riga ("Rust, stato: definiamo la mission").
- Font 200%: titolo e chip vanno a capo (il chip può diventare a due righe, altezza cresce); TopBar a 2 righe; azioni dei dialog in colonna.
- Il chip ha icona + testo; nessun significato solo cromatico. Contrasti: §10.3.

## Evitare

Mostrare `state` grezzi (`mission_proposed`), "Topic" nel testo, percentuali di completamento, indicatori numerici sulle righe, spinner animati in più righe contemporaneamente.

## Expanded / tablet orizzontale (layout principale)

In expanded questa schermata **è il pannello destro** del list-detail descritto in `home.md` (Expanded); non è una destinazione separata. La schermata a tutta finestra di questo documento vale per compact e medium.

```
┌─────────────────────────┬──────────────────────────────────────────────────────────────┐
│  (pannello Cartelle)    │ Lavoro                       3 argomenti   ⋮  [ + Nuovo      │ intestazione 72 dp
│                         │ (ui.title1)                  (ui.body       argomento ]   │ (padding 32 dp)
│                         │                               inkSoft)                       │
│                         │ ┌──────────────────────────────────────────────────────────┐ │
│                         │ │ Rust                                                  ⋮ │ │ riga ≥ 72 dp,
│                         │ │ ( 💬 Definiamo la mission )                               │ │ colonna max
│                         │ ├──────────────────────────────────────────────────────────┤ │ 840 dp centrata
│                         │ │ Fotografia                                            ⋮ │ │
│                         │ │ ( ✓ Pronto per le lezioni )                               │ │
│                         │ └──────────────────────────────────────────────────────────┘ │
└─────────────────────────┴──────────────────────────────────────────────────────────────┘
```

- **Intestazione**: nome cartella `ui.title1` (a capo fino a 2 righe, mai ellissi), conteggio `ui.body` `inkSoft` ("3 argomenti"; qui sì, perché lo spazio c'è e il dialog di eliminazione lo usa), ⋮ cartella (**Rinomina cartella**, **Elimina cartella** `danger`; stessi dialog), Button **primario** "Nuovo argomento" (48 dp, icona `plus`) allineato a destra. Con fontScale > 1,3 o pannello < 560 dp il bottone scende sotto il titolo, a sinistra.
- **Lista argomenti**: identica a compact (riga, chip di stato, ⋮, pressione lunga), `Surface` a `contentMaxWidth`, centrata nel pannello, margini 32 dp. Il tocco su una riga apre l'**argomento a tutta finestra** (non un terzo pannello: motivazione in `design-system.md` §5.1). Il pulsante Indietro dell'argomento riporta qui con scroll intatto.
- **Stato vuoto** (cartella senza argomenti): nel pannello destro, centrato (max `readingMaxWidth`): `ui.title1` "Nessun argomento qui", testo come compact, primario grande "Crea il primo argomento". In questo stato il bottone nell'intestazione **non c'è** (una sola CTA primaria).
- **Caricamento / errori**: skeleton e `ErrorState` nel pannello destro, la selezione a sinistra non si muove. Cambio rapido di selezione: la richiesta precedente è annullata, si mostra solo la cartella corrente (nessun contenuto della cartella A sotto il titolo di B).
- **Creazione argomento**: dialog 480 dp; successo come compact (riga in fondo con scorrimento, Toast con "Apri"). Cartella eliminata altrove (`404`): nel list-detail la selezione passa alla successiva come in `home.md`, Toast "Questa cartella non esiste più."
- Il conteggio a sinistra e quello nell'intestazione si aggiornano insieme dopo ogni creazione/eliminazione di argomento.
