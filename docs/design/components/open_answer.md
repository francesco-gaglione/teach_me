# `open_answer`

> Comune: [_exercises.md](_exercises.md). Il Grader (backend, Fase 7) valuta la risposta con la `rubric`, che **l'app non riceve** (contratto §7).

## Scopo didattico
**Produzione con parole proprie** (effetto di auto-spiegazione, Chi; generation effect): la forma di richiamo più forte e quindi più faticosa (difficoltà desiderabile). Serve a verificare la comprensione, non la formulazione: feedback sul *contenuto* (cosa c'è, cosa manca), con tono di sostegno. Non è un test: nessun punteggio.

## Anatomia

```
┌──────────────────────────────────────────────────┐ ExerciseCard
│ (✎) Risposta aperta                  Facoltativo  │
│ Spiega con parole tue perché `a` non è più       │ question: reading.question
│ valida dopo `let b = a;`.                        │
│ Rispondi con parole tue.                         │ istruzione: ui.body inkSoft (fissa)
│ ┌──────────────────────────────────────────────┐ │ TeachTextField multilinea (base-components §2)
│ │                                              │ │ min 4 righe (≥ 120 dp), cresce fino a 10 righe,
│ │                                              │ │ poi scorre; fill surfaceSunken, bordo border 1,5
│ └──────────────────────────────────────────────┘ │
│                                   0 / 600        │ contatore ui.bodyS inkSoft, a destra
│ [ Invia ]   Scrivi la risposta per inviare.      │
└──────────────────────────────────────────────────┘
```

- Testo del campo: `reading.body` 17/28 `ink` (si scrive prosa, non codice), autocorrezione **attiva** (è prosa italiana), maiuscole a inizio frase, `ImeAction.Default` (Invio = a capo). Nessun placeholder con esempi di risposta (suggerirebbero una risposta); placeholder neutro assente: l'etichetta è la domanda.
- `max_chars`: input troncato al limite (`filter`), contatore `n / max`; da 90 % del limite il contatore passa a `ui.label` `inkSoft`; al limite aggiunge «Limite raggiunto» (non errore). Mai `danger`.
- «Invia» attivo quando il testo (trim) ha **≥ 1 carattere**; nessun minimo di qualità (il feedback è la risposta). Motivo mostrato finché disabilitato: «Scrivi la risposta per inviare.».
- **Bozza** salvata a ogni modifica (debounce 300 ms) per `block.id` e ripristinata.

## Stati

| Stato | Aspetto |
|---|---|
| non svolto / in scrittura | campo vuoto/pieno, «Invia» disabilitato/abilitato |
| **in invio / valutazione** | campo bloccato (testo `ink`, fill `surfaceSunken`, bordo `border`), `StatusPanel progress` «Sto valutando la tua risposta…» con spinner 24 dp; «Invia» scompare. Il backend valuta **dentro la richiesta** (fino a 20 s); dopo 8 s il testo diventa «Ci sta mettendo più del solito…»; il client rinuncia a 25 s e passa a «Risposta registrata» (sotto). Nessuna barra, nessuna stima |
| **valutata** (`outcome` = `verdict`) | `StatusPanel` con variante `correct` / `partial` / `incorrect` decisa dal Grader (parole fisse «Corretto» / «Quasi» / «Non proprio»), corpo = `grade.feedback` (md inline, §Feedback del Grader). Sotto: «La tua risposta» (riquadro `surfaceSunken`, `reading.body`, chiuso se > 6 righe con «Mostra tutto») |
| **Risposta registrata** (`outcome: pending`, o rinuncia del client, o coda locale per rete assente) | la risposta conta come **svolta** (non blocca «Continua»). `StatusPanel info`, titolo «Risposta registrata»; corpo e azione in §Risposta registrata. Nessun colore d'errore, nessun punto esclamativo |
| Errore irreversibile (`422` o altro 4xx) | `StatusPanel info` «Risposta registrata», corpo «Non sono riuscito a valutare questa risposta. L'ho salvata.» (nessuna azione) |
| bloccato / lezione riaperta | come valutata (feedback visibile), o come registrata se ancora `pending` (con «Valuta ora» finché la lezione non è `completed`) |
| **Fase 5 (Grader assente)** | non più attuale: dalla Fase 7 il Grader valuta. Se il backend risponde sempre `pending`, vale «Risposta registrata» |

**Nessuna riprova**: dopo la valutazione non si riscrive (la risposta aperta non si riprova a caso, e il backend registra un solo tentativo). Proprio per questo **il feedback deve insegnare**, non solo giudicare: dice cosa c'è, cosa manca e l'idea giusta (`../generator-guidelines.md`). Per proseguire il discorso: «Ne parlo col Tutor» (sotto).

## Feedback del Grader

Il tentativo valutato porta `verdict`, `feedback` e, a volte, `misconception`.

**Layout dentro il `StatusPanel`**: etichetta (parola fissa del verdetto) → `feedback` in `ui.bodyL` `ink` (md inline: grassetto, corsivo, codice inline; eventuali `[[ ]]` si mostrano come testo semplice: il Grader non conosce le chiavi del glossario; `interactive = false`). Nessun elenco puntato né titoli: il feedback è prosa di 2-5 frasi (≤ 800 caratteri, si disegna **intero**, mai troncato, mai scroll interno).

| `verdict` | Variante | Parole | Cosa trasmette |
|---|---|---|---|
| `correct` | `correct` | «Corretto» | Ciò che c'era di valido, e un'osservazione di approfondimento |
| `partial` | `partial` | «Quasi» | Ciò che c'è già, e la cosa principale che manca |
| `incorrect` | `incorrect` | «Non proprio» | L'idea giusta in una frase, e da dove nasce il fraintendimento |

**`misconception`: la si mostra alla persona? No.** Si conserva e la legge il Recorder, ma **non compare** né come riga né come tag. Motivi:

1. **È già nel feedback**, e in forma adatta: il Grader deve trattare la convinzione sbagliata *dentro* il `feedback`, in seconda persona e con tono di sostegno («Sembra che tu pensi che `a` resti valida: …»). Una seconda riga sarebbe una ripetizione (carico estraneo, Mayer: coerenza).
2. **È scritta in terza persona** («Pensa che…», contratto §2): rivolta a un lettore che non è la persona. Leggerla in un riquadro suona come un'**etichetta sulla persona**, non come informazione sul concetto (P9: errore = informazione, mai colpa).
3. **Rievocare l'errore lo rinforza** (effetto di familiarità): il feedback deve mettere in primo piano l'idea giusta; l'idea sbagliata compare una volta, di servizio.
4. **Il momento giusto è dopo**: quando la convinzione viene *superata* (esercizio successivo, secondo tentativo), il registro mostra una card «Idea chiarita» (`../screens/progress.md`): lì sì, come informazione e con il lessico di competenza.

Se in futuro si volesse mostrarla (per esempio un tag «Idea da chiarire»), servirebbe un testo in seconda persona scritto apposta (proposta ai contratti sotto).

**Dopo il feedback** (solo per `partial` e `incorrect`): Button testuale 48 dp «Ne parlo col Tutor» sotto «La tua risposta»; apre la chat con il composer precompilato: «Ho risposto così alla domanda «{prime 120 lettere della domanda}…»: «{risposta, max 600 caratteri}». Mi aiuti a capire cosa manca?» e con `lesson_id` (`ask_agent.md`). Non c'è per `correct` (non si invita a discutere ciò che va bene) né per `pending`.

**Annunci**: il pannello ha `liveRegion = Polite`; contenuto letto: «<parola>. <feedback senza markup>». Quando la valutazione arriva *dopo* (da «Risposta registrata»): «Valutazione arrivata: Quasi. …» senza spostare il focus né lo scroll.

## Risposta registrata (pending) e reinvio

```
┌──────────────────────────────────────────────────┐ StatusPanel info
│ (i) Risposta registrata                          │
│ La valuto appena riesco. Al più tardi lo faccio  │ corpo ui.bodyL ink su infoSoft
│ quando completi la lezione.                      │
│ [ Valuta ora ]                                   │ Button secondario 48 dp
└──────────────────────────────────────────────────┘
La tua risposta  (riquadro surfaceSunken)
```

- **Fase 11, chiave LLM nota come mancante o rifiutata**: corpo e bottone cambiano come in `../llm-key.md` §5.1 («Inserisci la chiave» / «Sostituisci la chiave» al posto di «Valuta ora»). Con chiave `Set` ma credito esaurito visto in sessione, corpo dedicato e «Valuta ora» invariato (`../llm-key.md` §9.5).
- **Corpo** per origine (stesso titolo):
  - il backend ha salvato la risposta ma il Grader non ha risposto in tempo (`outcome: pending`): «La valuto appena riesco. Al più tardi lo faccio quando completi la lezione.»;
  - la risposta è solo nella coda locale (rete assente o nessuna risposta dal backend): «Non riesco a raggiungere il server: la invio e la valuto appena torna la connessione.»
- **«Valuta ora»**: reinvia lo stesso tentativo (la coppia `block_id` + `attempt_number` ritenta la valutazione di un `pending`, `lessons.md` §5.1). Durante: il pannello torna a «Sto valutando la tua risposta…» (bottone sparisce). Se anche stavolta resta `pending`: torna «Risposta registrata» con riga `ui.bodyS` `ink` in più «Non è ancora andata a buon fine. Riprovo più tardi.» (nessuna vergogna, nessun conteggio di tentativi).
- Riprovo automatico: **una volta** alla prossima ripresa in primo piano o al ritorno della rete (non a ogni secondo: ogni valutazione costa), poi a richiesta; al limite la valuta il job di chiusura (step `grading`).
- **Quando arriva l'esito**, il pannello si aggiorna **in place** con `feedbackAppear` (dissolvenza; con «riduci animazioni» istantaneo), anche se l'utente è già in un'altra sezione; nessun Toast, nessuno scroll.
- Lezione `completed`: «Valuta ora» sparisce (il job di chiusura valuta i `pending`); resta «Risposta registrata» con corpo «La valuterò con la chiusura della lezione.» finché il job non finisce, poi il verdetto. Se dopo il job la risposta è ancora `pending` (Grader non disponibile): resta registrata e **non** conta come esercizio superato (`lessons.md` §6); nel Riepilogo: «Risposta registrata».

## Interazione e tastiera
Tocco → tastiera software (`bringIntoView`, il campo sopra l'IME). Ctrl+Invio = Invia. Esc non fa nulla. Il testo è sempre selezionabile e copiabile anche dopo l'invio.

## Accessibilità
Campo: label = domanda (leggere la domanda come etichetta del campo), `contentDescription` «Risposta, modifica testo, N caratteri su max»; il contatore non è annunciato a ogni carattere (aggiornamento di `stateDescription` solo a 90 % e al limite). In valutazione `busy` e live region polite «Sto valutando la tua risposta». Feedback letto come per gli altri esercizi.

## Contenuti al limite
- `max_chars` 2000: il campo sale a 10 righe e scorre; il contatore resta visibile in basso a destra (sticky nel campo).
- Incolla di testo oltre il limite: troncato con annuncio «Testo troncato al limite».
- Risposta con a capo: preservati; mostrati in «La tua risposta» con a capo.
- IME copre il bottone: `imePadding`, il bottone sempre raggiungibile scorrendo.

## Linee guida per il Generator
Vedi README §9. Domanda che chiede **una produzione specifica** («Spiega… in 2-3 frasi»), non un fatto; collegata all'`objective`; `max_chars` 300-600 (il campo deve incoraggiare la sintesi). `rubric` (non visibile): 3-5 criteri osservabili, errori tipici, come trattare risposte parziali, e **tono** del feedback («sostenere, indicare cosa manca, mai giudicare»). Max 1 per lezione, di norma in chiusura; `required: false` se è una sfida in più. Non chiedere opinioni, né elenchi di fatti memorizzati.

## Linee guida per il Grader
Tono, criteri di verdetto e struttura del feedback: `../generator-guidelines.md` (sezione Grader). Per il Generator vale quanto sopra: la `rubric` deve elencare i **criteri essenziali** e gli errori tipici *con la formulazione dell'idea giusta*, perché il Grader la userà per scrivere un feedback che insegna.

## Proposte ai contratti (per il tech lead; non applicate)
1. `grade.misconception_feedback` (facoltativo, 1-300, seconda persona) **solo** se si decidesse di mostrare la convinzione riconosciuta: oggi non serve (§Feedback del Grader).
2. `Attempt.grade` per `quiz_*`: nessuna proposta; la spiegazione degli esercizi chiusi sta nel documento.

> **Codice nella domanda** (`code` facoltativo, contratto `sdui.md` §4.1): posizione, scorrimento, accessibilità e comportamento dopo la risposta in [_exercises.md](_exercises.md) §10.
