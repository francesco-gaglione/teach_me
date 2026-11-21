# `fill_blank`

> Comune: [_exercises.md](_exercises.md).

## Scopo didattico
**Richiamo libero** (non riconoscimento): l'utente deve produrre la parola, che è molto più efficace di sceglierla fra opzioni (generation effect, Bjork). La frase fornisce il contesto, non la risposta. Per questo: **nessun indizio dalla forma** (larghezza dei campi identica per tutti, indipendente dalla lunghezza della risposta), niente menu di suggerimenti, niente autocorrezione.

## Anatomia

Larghezza ≥ 360 dp e `fontScale ≤ 1,5` (modalità **in linea**):

```
┌──────────────────────────────────────────────────┐ ExerciseCard
│ (⌶) Completa                                     │
│ Completa la frase.                               │ istruzione
│                                                  │
│ Dopo `let b = a;` il valore è stato  ┌─────────┐ │ testo: reading.body 17/28
│ ┌─────────┐ a `b`, quindi `a` non è │  1      │ │ campi inline: 120 × 40 dp
│ │         │ più valido.             └─────────┘ │
│ └─────────┘                                      │
│ [ Controlla ]  Compila gli spazi per controllare.│
└──────────────────────────────────────────────────┘
```

(Il testo scorre normalmente e i campi sono "parole" larghe 120 dp che vanno a capo come le altre.)

Modalità **a elenco** (font > 1,5, o finestra < 360 dp): nel testo i vuoti sono marcatori numerati `(1)` (chip 28 dp, `surfaceSunken`, `ui.labelS`), e sotto c'è l'elenco dei campi etichettati:

```
 Dopo `let b = a;` il valore è stato (1) a `b`, quindi `a` non è più valido.
 Spazio 1   [______________________]   campo a tutta larghezza (min 48 dp)
```

## Misure e token

| Elemento | Valore |
|---|---|
| Testo | `reading.body` 17/28, `ink`, md inline con `interactive = false`; segnaposto `{{id}}` sostituiti dal campo |
| Campo in linea | `blankFieldWidth` **120 dp** (uguale per ogni vuoto, qualunque sia la risposta), `blankFieldHeight` 40 dp visivi (area tappabile 48 dp: l'altezza della riga cresce a 48 dp quando contiene un campo), raggio `sm`, fill `surfaceSunken`, bordo `border` 1,5, testo **sempre** `code.inline` 15 (risposte spesso identificatori; uniformità: nessun indizio sul tipo di risposta) |
| Contenuto che eccede 120 dp | il testo scorre orizzontalmente nel campo (`singleLine`), mai a capo né allargamento del campo |
| Stati del campo | default; **focus** (bordo `focus` 3 dp + offset 2); riempito (testo `ink`); **errore di validazione** non esiste (qualunque stringa è un tentativo); dopo il controllo vedi sotto; bloccato (testo `ink`, fill `surfaceSunken`, bordo `border` 1,5, icona di esito) |
| Etichetta/placeholder | **nessun placeholder** (un placeholder come «…» o «parola» è un indizio sulla forma); l'etichetta accessibile è «Spazio N di M» |
| IME | `KeyboardOptions(capitalization = None, autoCorrectEnabled = false, keyboardType = Text, imeAction = Next` o `Done` sull'ultimo`)`; nessun suggerimento (`KeyboardType.Password` NON si usa; si imposta `contentType` assente, `importantForAutofill = no`) |
| Trim | gli spazi iniziali/finali sono ignorati nel confronto, non rimossi nel campo |

### Dopo il controllo

Per ogni vuoto, nel campo compare a destra l'icona di esito (20 dp: `check-circle` in `correct` / `x-circle` in `incorrect`), il bordo diventa `correct`/`incorrect` 2 dp con fill `correctSoft`/`incorrectSoft`. Sotto il testo, un **elenco per vuoto** (ListRow compatto, ogni riga 48 dp min):

```
 ✓ Spazio 1 · Corretto               la risposta: «spostato»
 ✗ Spazio 2 · Non corretto           attesa: «proprietario»   (accettate anche: «owner»)
```

Parola (`ui.label`) + icona + colore. La risposta attesa si mostra solo se il vuoto è errato: `answers[0]`; se ci sono altre varianti, «accettate anche:» con al massimo 2 (`ui.body` `inkSoft`). Le risposte corrette dell'utente non si riscrivono ma si confermano.

Pannello di esito: `correct` → `feedback.correct`; `partial` (alcuni giusti) → `feedback.incorrect` con etichetta «Quasi»; `incorrect` → `feedback.incorrect`. Riprova offerta (testi restano nei campi, le marcature spariscono).

## Interazione e tastiera
- Tocco sul campo apre la tastiera; scorrimento automatico (`bringIntoView`) per tenere il campo sopra la tastiera.
- Invio/IME Next → campo successivo; sull'ultimo, se tutti compilati, «Controlla»; Ctrl+Invio controlla sempre che ci sia testo in tutti.
- «Controlla» attivo quando **ogni** vuoto ha almeno un carattere non-spazio; motivo: «Scrivi qualcosa in ogni spazio per controllare.».
- Tab/Maiusc+Tab sono equivalenti a Next/Previous.
- La bozza dei campi è salvata a ogni modifica (debounce 300 ms).

## Accessibilità
- Ogni campo: `TextField` semantics con `contentDescription` «Spazio 1 di 2» e il testo circostante come contesto: nell'ordine di lettura il paragrafo è letto prima, spezzato dove sta il vuoto («Dopo let b uguale a punto e virgola, il valore è stato, spazio 1 di 2, campo di testo, a `b`…»). Implementazione: `semantics` per ogni segmento del testo + campi come nodi intercalati.
- Esiti: `stateDescription` dopo il controllo («corretto», «non corretto, risposta attesa spostato»).
- Contrasto: testo `ink` ≥ 14 su `surfaceSunken`; bordi ≥ 3.
- Font 200 %: modalità a elenco.

## Contenuti al limite
- 4 vuoti in 600 caratteri: ok in linea (campi 120 dp = 4 × 120 su 560 dp di colonna: vanno a capo).
- Risposta lunga (60 caratteri): scorre nel campo; il campo non si allarga. Se è più lunga di 20 caratteri il Generator ha sbagliato: ma l'app non fallisce.
- Vuoto all'inizio o fine frase: ok.
- Segnaposto non corrispondente: blocco non renderizzabile (non obbligatorio).
- Normalizzazione: contratto §5 (trim, spazi interni, NFC, minuscolo salvo `case_sensitive`). Un testo con più forme accettate (`answers[]`) ammette tutte.

## Linee guida per il Generator
Vedi README §6. In breve: 1-3 vuoti (max 4), **ogni vuoto una parola o un token**; la frase dev'essere ambigua solo sulla parola cercata; includere in `answers` sinonimi/varianti e forme plausibili (articoli, plurali, `&mut` vs `mut`); mai vuoti per parole funzionali (a, il, di); il feedback `correct` conferma e aggiunge un dettaglio (non ripete la frase); `incorrect` spiega come ragionare («Chiediti a chi appartiene il valore dopo l'assegnamento.») **senza dare la risposta** (viene mostrata dall'app).
