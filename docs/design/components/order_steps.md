# `order_steps`

> Comune: [_exercises.md](_exercises.md).

## Scopo didattico
Far ricostruire una **sequenza causale o procedurale** (come si svolge un processo, in che ordine si applicano le regole): richiamo dell'ordine, non dei singoli fatti. Utile quando l'ordine *è* la conoscenza. Non è un quiz: non ha "opzioni sbagliate" ma una disposizione.

## Anatomia

```
┌──────────────────────────────────────────────────┐ ExerciseCard
│ (≡) Metti in ordine                               │
│ Ordina i passi per compilare un programma Rust.  │ prompt: reading.question
│ Trascina, oppure usa le frecce.                   │ istruzione
│                                                  │
│ ┌──────────────────────────────────────────────┐ │
│ │ ⠿  Scrivere il codice sorgente      [↑] [↓] │ │ StepRow ×N (3-8): min 56 dp, raggio md
│ └──────────────────────────────────────────────┘ │ ⠿ = maniglia (icona grip-vertical 24)
│ ┌──────────────────────────────────────────────┐ │ [↑] [↓] = IconButton 48 dp
│ │ ⠿  Eseguire il binario              [↑] [↓] │ │
│ └──────────────────────────────────────────────┘ │
│ …                                                │
│ [ Controlla ]                                    │ sempre attivo (l'ordine iniziale è un tentativo valido)
└──────────────────────────────────────────────────┘
```

Dopo il controllo errato:

```
│ ┌──────────────────────────────────────────────┐ │ ogni riga: numero di posizione + esito
│ │ 1  Scrivere il codice sorgente               │ │ bordo correct/incorrect 2 dp, fill xSoft
│ │ (✓) Al posto giusto                          │ │ icona + parola (ui.label)
│ └──────────────────────────────────────────────┘ │
│ │ 2  Eseguire il binario                       │ │
│ │ (✗) Non qui                                  │ │
│ ┌ (x) Non proprio ───────────────────────────┐   │ StatusPanel
│ │ feedback.incorrect                         │   │
│ └────────────────────────────────────────────┘   │
│ ▾ Mostra l'ordine corretto                       │ Button testuale 48 dp: apre elenco numerato
│ [↺ Riprova]                                      │
```

## Misure e token

| Elemento | Valore |
|---|---|
| Riga | min 56 dp, padding 8 × 12 dp, raggio `md`, fill `surface`, bordo `border` 1,5; gap fra righe 8 dp |
| Maniglia | `grip-vertical` 24 dp `inkSoft`, area 48 × 48 (trascinamento con long-press o tocco+trascinamento immediato sulla maniglia) |
| Testo | `ui.bodyL` `ink`, md inline (`interactive = false`) |
| Pulsanti ↑ ↓ | IconButton 48 × 48 dp (icone `chevron-up`/`chevron-down` 24, `ink`), `contentDescription` «Sposta su» / «Sposta giù»; ↑ disabilitato sulla prima riga e ↓ sull'ultima (stato `disabled()`: sono controlli veri che non si possono usare; il motivo è ovvio) |
| Riga in trascinamento | ombra `raised`, bordo `accent` 2 dp, fill `surfaceRaised`, scostamento ≤ 4 dp; le altre righe scorrono 150 ms `inOut` (nessuna animazione a riposo) |
| Larghezza riga a 360 dp | testo ≈ 360 − 32 (margini) − 40 (padding card) − 48 (maniglia) − 96 (↑↓) = 144 dp ≈ 18 car./riga: va a capo, accettato (alternativa: sotto 480 dp i pulsanti ↑↓ passano a un `⋮` con menu «Sposta su / Sposta giù / In cima / In fondo»; scelta di android-dev se lo spazio non basta) |
| Numero di posizione | sempre mostrato a sinistra della maniglia: `ui.label` `inkSoft` in cerchio 28 dp `surfaceSunken`. Dice la posizione *corrente* scelta dall'utente (informazione sua, non della soluzione); si aggiorna a ogni spostamento |

### Mescolamento deterministico

Contratto §5: i passi arrivano nell'ordine corretto e l'app li mescola «in modo deterministico per `id` del blocco». Algoritmo da implementare (stesso risultato a ogni apertura, su ogni dispositivo):
1. `seed = FNV-1a 32 bit(UTF-8(block.id))`.
2. Generatore `xorshift32`: `x ^= x << 13; x ^= x >>> 17; x ^= x << 5`, seed 0 → 0x9E3779B9.
3. Fisher-Yates dalla fine: per `i` da `n−1` a 1: `j = x_next() mod (i+1)`, scambia `i`,`j`.
4. Se il risultato coincide con l'ordine corretto, **ruota di una posizione a sinistra**. Se `n ≥ 4` e nessun passo si trova nella posizione corretta non occorre altro; se la permutazione lascia **più di n/2** passi al posto giusto (troppo vicina alla soluzione), ruota ancora fino a ≤ n/2 (max n tentativi).
L'ordine **tentato** dall'utente è persistito (per `id` passo) e ripristinato.

## Stati
- **non svolto**: ordine mescolato; «Controlla» già attivo (non c'è un "non risposto": ogni disposizione è una risposta); l'utente può controllare anche senza muovere nulla (poi vede il risultato: non si impedisce, è informazione).
- **in trascinamento / afferrato** (tastiera/TalkBack): riga con bordo `accent` 2 dp e ombra; annuncio «Passo X afferrato, posizione N di M».
- **controllato**: righe bloccate, maniglia e frecce nascoste (tolte dall'albero di accessibilità), marcature per riga (in posizione corretta: `check-circle` «Al posto giusto»; altrimenti `x-circle` «Non qui»); `correct` quando tutte; contratto: esito binario (nessun parziale registrato), ma la marcatura per riga è solo informativa. «Mostra l'ordine corretto» apre l'elenco numerato dei passi nell'ordine corretto (in un riquadro `surfaceSunken`, testo `ink`); aperto di default **solo** se `incorrect`.
- **riprova**: marcature rimosse, righe riordinabili, ordine tentato mantenuto, l'elenco corretto richiuso.

## Interazione
- Tocco: trascinare la maniglia (o long-press sulla riga) per spostare; oppure ↑↓. Auto-scroll della colonna ai bordi durante il trascinamento.
- Tastiera fisica: Tab entra nella lista (un solo gruppo); ↑↓ spostano il **focus** fra le righe; **Spazio** afferra/rilascia la riga a fuoco; con la riga afferrata ↑↓ la **spostano** (annuncio della nuova posizione); Esc annulla l'ultimo afferra (ripristina la posizione di partenza); Alt+↑/Alt+↓ spostano la riga a fuoco senza afferrare; Ctrl+Invio controlla.
- Alternativa al trascinamento (WCAG 2.5.7): i pulsanti ↑↓ sempre visibili.

## Accessibilità
- Lista: `collectionInfo`; ogni riga `collectionItemInfo(row = i)`, testo «<passo>, posizione 2 di 5». **Custom actions** TalkBack: «Sposta su», «Sposta giù», «Sposta all'inizio», «Sposta alla fine»; dopo ogni spostamento: annuncio polite «<passo>, ora in posizione 3 di 5» e il focus resta sulla riga mossa.
- Controllato: `stateDescription` «Al posto giusto» / «Non qui, dovrebbe stare in posizione 3» (la posizione corretta si dice solo dopo il controllo).
- Contrasto come per le opzioni; marcatura = icona + parola + colore.

## Contenuti al limite
- 8 passi da 200 caratteri: righe alte, la card ≈ 8 × 90 = 720 dp; scorrimento durante il trascinamento con auto-scroll; i pulsanti ↑↓ restano comodi.
- Passi con codice inline lungo: `softWrap` sicuro.
- Passi duplicati nel testo (stesso testo, `id` diversi): l'ordine si valuta per `id` (contratto), le marcature per `id`.
- Con un solo passo mal formato: blocco non renderizzabile.

## Linee guida per il Generator
Vedi README §7. Aggiungo: il `prompt` dice **che criterio di ordine** si usa («in ordine cronologico di esecuzione», «dalla causa all'effetto»); passi tutti **della stessa forma** e dello stesso livello di dettaglio, senza «prima/poi/infine» né numeri; non usare passi con dipendenze deducibili solo per eliminazione grammaticale (articoli "il/lo" non collegati). Il `feedback.incorrect` spiega **la dipendenza** fra i passi («Non si può eseguire prima di compilare perché…») e non rivela l'ordine (lo fa l'app); `feedback.correct` aggiunge il perché dell'ordine, non lo ripete.
