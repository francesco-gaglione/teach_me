# Controlli di qualità del Validator: errori e avvisi (issue #14)

Fonte delle soglie: `backend/crates/evals/src/roles/{shape,generator}.rs` e `backend/evals/generator/phase12.cases.json`. **Una sola regola per cosa**: il Validator del dominio implementa le stesse regole delle eval (le eval le possono poi delegare a lui). Dove la issue proponeva un valore diverso (callout > 3) vince il valore delle eval, perché è già misurato sulle suite.

Ogni controllo produce un'`issue` con: `code` (stabile, snake_case), `block_id` (id del blocco interessato, o `null` se riguarda la lezione intera), messaggio di riparazione (italiano, per il Generator: dice cosa è sbagliato e come ripararlo; interno al backend, non è nel contratto) e, per gli avvisi, `message` (italiano, testo per chi studia; è il campo `message` di `quality_warnings`, `lessons.md` §2).

## 0. Regole comuni

- **Blocchi esaminati**: i blocchi della lezione in ordine di lettura, **scendendo nei `fallback`** solo per leggere il testo, ma **i blocchi dentro un `fallback` non sono mai oggetto di un controllo** (non si segnala un `fill_blank` o un quiz che sta in un fallback, e il loro testo non conta nei totali). I `glossary_term` si ignorano ovunque (non rompono né sono contati). I `diagram` non sono prosa (non contano parole e non interrompono una serie).
- **Parole**: sequenze separate da spazi del testo con markup inline tolto (`plain`), `title` e `text` dei paragrafi e dei callout; il testo dei `code` non conta.
- **Esercizio**: `quiz_single`, `quiz_multi`, `fill_blank`, `order_steps`, `open_answer`, `flashcard`. `checklist` e `ask_agent` no.
- **Sezione**: da un `heading` livello 2 al successivo. **Intro** = blocchi prima del primo h2. **Chiusura** = ultima sezione. Intro e chiusura sono escluse da (1) e dal conteggio per sezione di (2).
- **Riparazione**: gli errori (a, b) bloccano e vanno al Generator. Se restano **solo avvisi** si tenta **una** riparazione; la nuova lezione è tenuta solo se ha **meno avvisi** della precedente (conteggio di issue) **e nessun errore**; altrimenti si tiene la precedente. Gli avvisi rimasti si salvano con la lezione (`quality_warnings`).

## 1. Errori bloccanti

### (a) `fill_blank_answer_in_text` (error)
La risposta di un blank compare nel testo vicino: la persona la legge invece di richiamarla (Bjork, retrieval practice).
- Per ogni blocco `fill_blank` (anche a più blank), per ogni stringa in `blanks[].answers`.
- Si salta la risposta con meno di **3 caratteri** (dopo `trim`): troppo comune per confrontarla.
- Testo vicino (stessa logica di `fill_blank_leak`): (i) il testo dell'`heading` precedente più vicino; (ii) il `caption` del `code` precedente più vicino (inline code tolto); (iii) le **due** prose precedenti più vicine, saltando gli esercizi (paragrafi e callout, inline code e `code` tolti); (iv) la prosa del blocco **immediatamente successivo**, se è prosa.
- Confronto: parola intera, senza maiuscole/minuscole, `_` e alfanumerici sono parte della parola (`contains_answer`). Ogni occorrenza in uno dei testi è una violazione.
- `block_id` = id del `fill_blank`. Messaggio: «La risposta «{answer}» del completamento compare già nel testo vicino ({dove}). Riformula il testo, cambia la parola da completare o sposta il completamento più avanti, così chi studia deve ricordarla.»

### (b) `quiz_option_markup_uneven` (error)
Markup in alcune opzioni sì e in altre no: tradisce la risposta (opzioni graficamente identiche, P8).
- Per ogni `quiz_single` e `quiz_multi`: il markup è presente se il testo di una `options[].text` contiene inline code (`` `x` ``), grassetto (`**x**` o `__x__`) o link (`[x](u)`).
- Violazione se **almeno una ma non tutte** le opzioni hanno markup (di qualunque tipo). Tutte o nessuna: ok. Si confronta **il tipo**: se tutte hanno markup ma una ha solo grassetto e le altre solo codice, ok. Il corsivo non è «markup» in questo controllo.
- `block_id` = id del quiz. Messaggio: «Nel quiz solo alcune opzioni hanno markup ({quali}). Usa lo stesso stile in tutte le opzioni o in nessuna.»

## 2. Avvisi non bloccanti

| # | `code` | Soglia (una riga) |
|---|---|---|
| 1 | `explanation_run_too_long` | serie di prosa tra inizio sezione (o un esercizio) e l'esercizio successivo **> 350 parole** |
| 2 | `too_many_callouts` | per sezione > **1**, oppure `tip`+`info` in lezione > **2**, oppure `warning` in lezione > **5** |
| 3 | `fill_blank_answer_too_long` | una risposta di blank con più di **3 parole** |
| 4 | `consecutive_code_blocks` | due `code` adiacenti che confrontano, senza `missing_component` con `wanted = code_compare` |

### 1. `explanation_run_too_long`
Troppa spiegazione senza richiamo (Sweller, carico; Bjork: l'esercizio ogni poche idee).
- Come `explanation_run`: in ogni sezione **tranne chiusura**, si sommano le parole di paragrafi e callout dall'inizio della sezione (o dall'ultimo esercizio) fino al prossimo esercizio; `code`, `diagram`, `glossary_term`, heading non contano e **non interrompono** la serie. L'intro è nell'ambito «prima del primo h2»: **esclusa**.
- Soglia **> 350** (= `defect_above` delle eval; le eval mantengono anche il livello `warn` a 300, che resta solo del loro dominio: nell'app il fastidio è oltre 350). Una sezione senza esercizi finali conta fino alla fine della sezione.
- `block_id` = id del primo blocco della serie. Un'issue per serie.
- messaggio di riparazione (interno): «Ci sono {n} parole di spiegazione di seguito nella sezione «{titolo}», senza un esercizio. Inserisci un esercizio a metà o accorcia a meno di 350 parole.»
- `message` (testo per chi studia): «Questa parte è lunga prima del primo esercizio. Fermati a ripetere a mente i punti chiave prima di andare avanti.»

### 2. `too_many_callouts`
Se tutto è segnalato, niente lo è (Mayer, segnalazione; `components/callout.md`).
- Si contano i `callout` per variante, **escluso l'intro** dal limite per sezione ma incluso nei totali di lezione. Soglie identiche a `callouts` delle eval: per sezione **max 1**; `tip`+`info` **max 2** per lezione; `warning` **max 5** per lezione (uno per concetto).
- Un'issue per ogni sezione oltre il limite (`block_id` = id del secondo callout della sezione) e una di lezione per i totali (`block_id` = id del primo callout oltre il limite).
- messaggio di riparazione (interno): «Troppi callout ({dettaglio}). Tieni al massimo 1 per sezione, 2 tra suggerimenti e info in tutta la lezione, e un solo avviso per concetto; trasforma gli altri in paragrafo.»
- `message` (testo per chi studia): «Qui ci sono diverse note evidenziate. Non tutte hanno la stessa importanza: dai la priorità a quelle di avviso.»

### 3. `fill_blank_answer_too_long`
Richiamo libero di una parola o di un breve termine, non di una frase (digitare è difficile, la correzione è incerta).
- Per ogni stringa di `blanks[].answers`: parole = token separati da spazi dopo `trim`; violazione se **> 3**. Ogni stringa dello stesso blank è valutata; basta una.
- `block_id` = id del `fill_blank`. Messaggio di riparazione (interno): «La risposta «{answer}» ha {n} parole. Accorciala a un termine di 1-3 parole.»
- `message` (testo per chi studia): «In questo completamento la risposta è lunga: se non coincide con quella attesa, controllala con il feedback.»

### 4. `consecutive_code_blocks`
Due esempi uno dopo l'altro da confrontare senza supporto: il confronto va visto (Mayer, contiguità) e il componente `code_compare` non esiste nel catalogo v1.
- Coppia adiacente nell'elenco dei blocchi (stessa sezione, ignorando i `glossary_term`) di due `code`, dove il secondo ha nel `caption` una di queste parole intere (senza maiuscole): `invece`, `rispetto`, `confronto`, `mentre`, `contro`, `vs`, `Go`, `prima`, `dopo` (regex di `consecutive_code_blocks_need_missing_component`).
- **Eccezione**: in tutta la lezione esiste un `missing_component` con `wanted = code_compare` (fuori dai fallback). Allora nessun avviso.
- `block_id` = id del secondo `code`. Messaggio di riparazione (interno): «Due blocchi di codice di seguito sembrano un confronto. Aggiungi un `missing_component` con `wanted = code_compare` oppure separali con un paragrafo che spiega la differenza.»
- `message` (testo per chi studia): «Qui due esempi di codice sono uno dopo l'altro. Confrontali riga per riga.»

## 3. App: dove e come si vedono

**Scelta**: una sola riga discreta **«Nota sulla lezione»** in fondo alla colonna di lettura (dopo l'ultimo blocco, prima della `ContinueBar` di §5 di `screens/lesson.md`), chiusa di default; si apre in una lista con «Vai al blocco» per le voci con `block_id`. Niente marcatori dentro i blocchi: durante lo studio non devono competere per l'attenzione (Sweller, coerenza).
Non appare se `quality_warnings` è vuoto, non appare nel Riepilogo, non è mai un toast, non cambia l'obbligatorietà né lo sblocco.

```
┌────────────────────────────────────────┐
│ (i)  Nota sulla lezione · 2          ⌄ │  chiusa, min 48 dp
└────────────────────────────────────────┘
┌────────────────────────────────────────┐
│ (i)  Nota sulla lezione · 2          ⌃ │
│ ────────────────────────────────────── │  line 1 dp
│ Questa parte è lunga prima del primo   │
│ esercizio. Fermati a ripetere…         │
│ [Vai al blocco]                        │
│ ────────────────────────────────────── │
│ Qui ci sono diverse note evidenziate…  │
└────────────────────────────────────────┘
```

| Elemento | Valore |
|---|---|
| Contenitore | `surfaceSunken`, raggio `md`, nessun bordo, margine sopra 24 dp, larghezza = colonna di lettura |
| Riga (chiusa) | altezza min 48 dp, padding orizzontale 16, verticale 12; icona `info` 20 dp in `inkSoft`; titolo «Nota sulla lezione · {n}» `ui.label` `inkSoft`; chevron 20 dp `inkSoft` a destra |
| Aperta | padding 16 dp; voci separate da `line` 1 dp con 12 dp sopra/sotto; testo `ui.body` `ink` (testo per chi studia); al massimo 6 voci mostrate, oltre «e altre {k}» nella stessa riga di testo |
| «Vai al blocco» | Button testuale, target 48 dp, `ui.labelL` in `accent`; solo se `block_id` esiste nel documento. Porta il focus al blocco e scorre fino a farlo comparire a 24 dp dal bordo alto (nessuno scroll animato con «Riduci animazioni»). Non chiude la lista |
| Colori | niente `warning`/`danger`: tono neutro, mai allarmante. Contrasti calcolati: `inkSoft` su `surfaceSunken` 8,15:1 chiaro e 11,3:1 scuro; `ink` su `surfaceSunken` ≥ 7:1 come da `design-system.md` §2.4 |
| Motion | apertura con `feedbackAppear` (200 ms, dissolvenza + altezza); durata 0 con «Riduci animazioni» |

Titolo e voci sono **testo fisso in italiano** (`message` di `quality_warnings`), mai il messaggio di riparazione del Generator. Voci in ordine di documento; stesso `code` ripetuto più di 2 volte → una sola voce con «{n} punti».

**Accessibilità**
- La riga chiusa è un solo nodo con ruolo pulsante: `contentDescription` «Nota sulla lezione, {n} osservazioni», `stateDescription` «chiusa» / «aperta», azione «Apri» / «Chiudi». Non è una live region (non si annuncia da sola).
- Nell'albero la riga sta nell'ordine visivo, dopo l'ultimo blocco; ogni voce è un nodo di testo seguito dal suo pulsante; «Vai al blocco» ha etichetta «Vai al blocco: {titolo o tipo del blocco}».
- Font fino a 200 %: il testo va a capo, la riga cresce; il chevron resta a destra, l'icona a sinistra.
- Significato mai al solo colore: icona `info` + parole.
- Tema scuro: gli stessi ruoli (`surfaceSunken`, `inkSoft`, `ink`, `accent`) hanno già i valori scuri in `tokens.json`.

**Evitare**: contatori in rosso, badge sulla barra, «attenzione» o «errore» nel titolo, conteggi nella lista delle lezioni, avvisi dentro il Riepilogo.

## 4. Per android-dev e per il backend

- Campo `quality_warnings: [{ code, block_id|null, message }]` nel dettaglio lezione e in `LessonView` (`contracts/lessons.md` §2 e §3.1). Nomi canonici: campo `quality_warnings`, elemento `{ code, block_id, message }`, codici di §2 (`explanation_run_too_long`, `too_many_callouts`, `fill_blank_answer_too_long`, `consecutive_code_blocks`). L'app non usa altri campi.
- `message` è il testo per chi studia: testo fisso in italiano generato dal backend dai testi di §2. Nessun testo del Generator arriva alla persona.
- Il messaggio di riparazione per il Generator (§0, §1, §2) resta interno al backend; non è nel contratto.

## 5. Decisioni del tech lead

Issue #14, registrate da `docs-keeper` su decisione del tech lead.

- **Punto 2 (`consecutive_code_blocks`)**: la regex delle parole di confronto si applica solo al `caption` del secondo `code`, non al `code`.
- **Punto 3 (callout)**: valgono le soglie delle eval (per sezione 1; `tip`+`info` in lezione 2; `warning` in lezione 5), non il «> 3» della issue.
- **Punto 4 (tenuta della riparazione)**: punteggio pesato. `explanation_run_too_long` vale 2, gli altri avvisi 1. Si tiene il documento riparato solo se è valido e con punteggio strettamente minore; altrimenti si tiene quello di partenza.
- **Punto 5 (limite riparazioni)**: la riparazione per soli avvisi parte solo da un documento già valido, quindi non sottrae tentativi a un errore. Se il riparato ha errori si tiene il precedente valido, senza altre riparazioni. Massimo una riparazione per avvisi per lezione, sempre dentro `max_document_repairs`.
