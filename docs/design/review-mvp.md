# Revisione dell'esperienza MVP (Fase 9)

> Data 2026-11-09. Contro: principi P1-P13 (`design-system.md` §1) e criterio di successo `mvp.md` §1. Materiale: spec in `screens/` e `components/`, `generator-guidelines.md`, prompt `backend/prompts/*/system.md` (sola lettura) e due lezioni reali generate da Sonnet con i loro tentativi, voti, record e schede (`samples/lesson-01-ownership.json`, `samples/lesson-02-result-question-mark.json`).
> **Limite onesto**: non ho usato l'app costruita (nessun test su schermo, nessun utente). Le conclusioni sulle schermate vengono dalle specifiche; quelle sui contenuti dai dati reali. Un campione di due lezioni mostra problemi, non ne misura la frequenza.

## 1. Cosa funziona

- **Il ciclo del criterio di successo è chiuso nei dati.** Lezione, tentativi, valutazione, record, termine di glossario e schede nascono entro ~8 s dal completamento. Le schede sono in FSRS con `good` = 1 giorno (lezione 1) e 2 giorni (lezione 2): il ripasso del giorno dopo è coerente col criterio.
- **Un'unica vittoria per lezione** (P1): ogni `objective` è una capacità osservabile ("saper prevedere se compila e correggere") e gli esercizi la verificano con codice nuovo, non con definizioni.
- **Feedback dei distrattori** nel formato "Sembra giusto perché… ma…" (P6): nomina l'errore plausibile. Il voto della risposta aperta cita le parole dell'utente e dà una correzione.
- **Intercalazione reale**: la lezione 2 richiama in `q1` il move della lezione 1 e ha un `lesson_link`. È la difficoltà desiderabile che volevamo.
- **Contiguità e codice** (P4): esempi di 3-9 righe, caption che dicono cosa osservare, numeri di riga citati nel testo.
- **Fonti e citazioni**: `primary_source` unico, glossario con `avoid`, tono in seconda persona e senza colpa.
- **Ripasso senza ansia** (P9): nessuno streak, riga discreta a `due_count` = 0, voti con intervalli visibili, annullamento.

## 2. Problemi, per impatto sull'apprendimento

### 1. Un quiz tradisce la risposta con la lunghezza (P7) → llm-engineer, tech lead
Lezione 2, `q3` (`quiz_multi`): le opzioni giuste misurano 74 e 75 caratteri, le sbagliate 46, 48 e 49. Basta guardare le due più lunghe per rispondere (infatti il primo tentativo è stato parziale, il secondo giusto). Il prompt vieta già l'asimmetria ("pari lunghezza"), ma il modello non la rispetta e nessun controllo la rileva. La lezione 1 è invece a posto (41-44).
- *Prompt*: per il codice nelle opzioni, "scrivi i distrattori come varianti realistiche della stessa forma (stessa firma, stesso numero di token ±20%), non come frammenti più corti".
- *Validator* (tech lead → rust-backend-dev): avviso `option_length_cue` se la lunghezza di una opzione corretta supera di oltre 1,4 volte la mediana delle sbagliate (o viceversa). Va nel ciclo di repair, perché è un difetto di validità del test e non estetico.

### 2. Alcuni esercizi sono lettura travestita da richiamo (P5) → llm-engineer
- Lezione 1, `q1` (fill_blank): la `caption` del codice subito sopra dice "Dopo `let b = a;` il proprietario è `b`", cioè la risposta del vuoto `owner`. Il vuoto `var` è quasi altrettanto ovvio dal paragrafo precedente.
- Lezione 2, `q2`: il paragrafo e la caption sopra dicono "il controllo sta nel simbolo alla riga 2" e "L'operatore `?` sostituisce…", e le tre risposte del vuoto sono `?`.
In entrambi i casi l'utente copia ciò che ha appena letto: fluency senza storage (Bjork). La checklist finale del Generator controlla titoli, opzioni e feedback ma non caption e paragrafi adiacenti.
- *Prompt*: aggiungere alla regola 4 e alla checklist: "La risposta di un esercizio non compare nel testo, nella caption o nel codice adiacenti. La caption dice cosa osservare ('guarda la riga 2'), mai il risultato. Se l'esercizio ripete l'esempio, cambia il caso (nuovi nomi non bastano: cambia la struttura)".
- *Eval* (llm-engineer): controllo "risposta del vuoto presente come sottostringa nei 2 blocchi precedenti".

### 3. Il registro "ciò che ho dimostrato" può dichiarare più di ciò che è stato dimostrato → llm-engineer, tech lead
Lezione 1: l'utente ha sbagliato `q2`, e in `q4` ha scelto `clone` "perché è più semplice" (voto `partial`: manca proprio il criterio di scelta). Il record dice però "Hai capito… `clone()` serve solo se la funzione deve possedere il dato", mentre l'`evidence` cita solo `q3`. È esattamente il contenuto che l'utente **non** ha mostrato. Un registro che promette più del vero rompe la fiducia sul punto più importante del prodotto.
- *Recorder*: il `body` riporta solo ciò che l'`evidence` dimostra; ciò che il voto `partial` o un errore ha lasciato scoperto non entra nel record, ma diventa una scheda di ripasso (vedi 4).
- *Contratto* (tech lead, dopo l'MVP): un tipo `to_consolidate` o una sezione "Da consolidare" per i punti incerti. Nell'MVP basta che compaiano come schede e nel riepilogo locale "Da rivedere" (`lesson.md` §7.1).

### 4. Le schede testano definizioni, non la capacità dell'obiettivo → llm-engineer, android-dev (verifica)
Le due schede viste sono entrambe `kind: term`, con fronte a **due domande** ("quale variabile non si può più usare **e** chi è il nuovo proprietario?"; "che cosa fa `?` **e** che cosa richiede…?") e retro di 2-3 frasi. L'obiettivo era *prevedere se compila*. Un fronte doppio si ricorda a metà e il voto FSRS diventa ambiguo (quale metà era "Con fatica"?).
- *Recorder*: una domanda per scheda, fronte ≤ 140 caratteri, retro ≤ 2 frasi. Per ogni lezione almeno una scheda di **applicazione** (un frammento breve: "compila? perché?") e una sull'errore commesso, se c'è. La scheda dell'errore ha priorità.
- *android-dev*: verificare che il fronte con codice inline e a più righe si veda bene nella scheda (`review.md`); se il fronte può contenere un blocco di codice, il Recorder potrà usarlo.

### 5. Il feedback della risposta aperta è lungo e senza seconda occasione (P6, Hattie) → llm-engineer; prodotto dopo l'MVP
Feedback di 475, 505 e 572 caratteri: al limite alto del range delle linee guida (200-500), e quello `partial` (572) porta tre idee (clone valido, costo, domanda guida) invece di "una sola". Dopo un voto `partial`, `open_answer.md` non permette di riprovare, quindi la correzione resta una lettura passiva.
- *Prompt Grader*: per `correct`, 1-2 frasi (≤ 300 caratteri) senza ripetere la risposta; per `partial`, una sola lacuna e la domanda che porta a colmarla.
- *Dopo l'MVP* (android-dev + tech lead): dopo un `partial`, un bottone "Riscrivi con questa idea" che apre un secondo tentativo (`attempt_number` 2) senza cambiare il voto già dato. È la difficoltà desiderabile che oggi manca ai quiz aperti.

### 6. Il confronto Go/Rust è fatto di blocchi in fila, e `missing_component` non viene mai usato → llm-engineer
Lezione 2, sezione 2: due blocchi `code` (Go, poi Rust) uno dopo l'altro; per confrontarli l'utente deve scorrere e tenere l'altro a mente (contiguità spaziale, Mayer). In tablet orizzontale un confronto affiancato costerebbe meno carico. Nessuna delle due lezioni contiene `missing_component`, quindi la Fase 9 non raccoglierebbe questo bisogno.
- *Prompt*: "Quando ti serve un confronto affiancato (A contro B, prima contro dopo) usa `missing_component` con `wanted: code_compare` e come `fallback` i due `code` consecutivi". Così la classifica dei componenti mancanti parte con un dato reale e l'utente non perde nulla.
- *Candidato di catalogo v2*: `code_compare` (due colonne in expanded, impilato in compact).

### 7. La chiusura non sempre porta fuori dall'app → llm-engineer
La lezione 2 chiude con una `checklist` di azioni reali ("scrivi un test che restituisce `Result`") legate alla mission; la lezione 1 chiude con una sintesi da leggere e poi nient'altro. La sintesi passiva ripete le risposte (illusione di padronanza).
- *Prompt*: la sezione finale contiene sempre una `checklist` di 2-3 azioni da fare nel proprio codice, e la sintesi scende a 2 righe che **non** ripetono le risposte di un esercizio.

### 8. Il "giorno dopo" dipende dall'utente (criterio §1) → decisione prodotto, dopo l'MVP
Senza notifiche (fuori scope), il ripasso compare solo quando si apre l'app, in Home. Per il proprietario-sviluppatore va bene, ma il criterio "il giorno dopo l'app mi propone il ripasso" è verificabile solo aprendola. Il progetto deve sapere che questo è il punto più fragile della retention nell'uso reale.
- *Dopo l'MVP*: notifica unica e discreta a un orario scelto, mai conteggi né colpa ("Hai 3 schede da ripassare, ~2 minuti").

## 3. Controllo per principio (sintesi)

| Principio | Esito |
|---|---|
| P1 un'idea, P3 segmentazione | Ok nelle spec e nei dati (sezioni con un esercizio ciascuna) |
| P2 coerenza | Ok; nessun elemento decorativo nelle lezioni |
| P4 contiguità | Ok nel codice; da migliorare nei confronti (problema 6) |
| P5 prima richiama | **Parziale**: ok nei quiz e nelle risposte aperte, no in alcuni fill_blank (problema 2) |
| P6 feedback | Ok per quiz e voti; lunghezza da sistemare (problema 5) |
| P7 nessun indizio | **Violato** in un quiz (problema 1) |
| P8 non solo colore, P9 niente manipolazione, P10 leggibilità | Ok nelle spec; da verificare sul dispositivo con TalkBack e font al 200% |
| P11 dual coding | Ok (icona + testo); nessun diagramma nel catalogo v1, accettato |
| P12 pre-training | Parziale: i termini sono nel glossario e linkati alla prima menzione, ma definiti nella stessa frase del concetto. Accettabile per ora |
| P13 impegno non punitivo | Ok (un quiz sbagliato non blocca); la lezione 2 ha solo un esercizio `required: true`, quindi la parte "obbligatoria" è sottile: sorvegliare con l'eval |

## 4. Da rimandare dopo l'MVP

Notifiche del ripasso (8); secondo tentativo dopo `partial` (5); `to_consolidate` nel registro (3); `code_compare` e altri componenti (6); "Riprendi la lezione" in Home (`home.md`); diagrammi; verifica con utenti reali: 5 sessioni osservate di una lezione, misurando quanti vuoti sono compilati senza esitazione (problema 2) e quanti quiz si vincono senza leggere (problema 1).

## 5. Misure da raccogliere con i dati che già abbiamo

Tasso di risposte corrette al primo tentativo per tipo di blocco (se un fill_blank supera il 95% sistematicamente è lettura, non richiamo); percentuale di quiz con opzione corretta più lunga della mediana; voto delle schede a 1 giorno per scheda `term` contro schede di applicazione.
