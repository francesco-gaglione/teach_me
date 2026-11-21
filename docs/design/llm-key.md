# Chiave LLM mancante, rifiutata o credito esaurito: messaggi e punti di ingresso

> Trasversale a più schermate. Contratto: `../contracts/users.md` §3, `api-conventions.md` §7 (`409 llm_key_missing`, `502 llm_key_rejected`, `502 llm_credit_exhausted` dalla v1.3), `users.md` v1.1 «Credito esaurito». Componenti: `base-components.md` §5 (StatusPanel, StatusChip), §1 (Button), §16 (composizione `LlmKeyNotice`). Schermate coinvolte: `screens/settings.md`, `screens/home.md`, `screens/chat.md`, `screens/mission.md`, `screens/resources.md`, `screens/topic.md`, `screens/lesson.md`, `components/open_answer.md`.

## 1. Principio

Ogni utente porta la propria chiave; senza una chiave valida **l'app insegna a metà**: si può ripassare (FSRS non usa l'LLM), rileggere le lezioni, creare cartelle e argomenti, ma non parlare con il Tutor, cercare fonti, scrivere o chiudere lezioni, valutare risposte aperte. L'interfaccia deve:

1. **non bloccare mai** chi non ha ancora la chiave: tutto ciò che funziona resta raggiungibile; il limite si incontra nel punto in cui serve, con una frase che lo spiega e un'azione sola (Mayer, segnalazione e contiguità: il messaggio sta dove l'utente ha agito);
2. **non perdere mai nulla**: il testo del messaggio resta nel campo, la lezione non chiusa resta com'è, le risposte restano al sicuro; il messaggio lo dice (un errore che sembra una perdita di lavoro è l'errore peggiore: P9);
3. **non dare colpa**: «mancante» è uno stato, «rifiutata» è una cosa che capita (chiave scaduta o non abilitata). Il **credito esaurito** non è un problema della chiave: la chiave è giusta, manca la ricarica fuori dall'app (§9). Tono ambra, mai rosso, mai punti esclamativi.

## 2. Il componente: `LlmKeyNotice` (composizione, non componente nuovo)

`StatusPanel` variante `warning` (o `info` per i suggerimenti proattivi) + un `Button`. Non aggiunge ruoli né token. È una funzione composable condivisa in `core/designsystem`, che prende `variant` (`missing` | `rejected` | `credit`), `context` e `phase` (vedi sotto) e produce i testi di §5-6, così le sette schermate non duplicano stringhe.

```
┌────────────────────────────────────────────┐
│ (🔑) Manca la tua chiave LLM               │  icona key-round 24, titolo ui.labelL, colore warning
│ Per parlare con il Tutor serve la chiave   │  corpo ui.bodyL, ink su warningSoft
│ del servizio LLM, e non ne hai ancora …    │
│ [ 🔑 Inserisci la chiave ]                 │  Button secondario 48 dp, 12 dp sotto il corpo
└────────────────────────────────────────────┘
```

- Fill `warningSoft`, raggio `md`, padding 16 dp, `liveRegion = Polite`, `feedbackAppear` (200 ms). Letto da TalkBack come «<titolo>. <corpo>»; il bottone ha `Role.Button`. Il focus di TalkBack **non viene spostato** dall'app.
- L'azione apre `settings?section=llm-key&reason=missing|rejected` (`settings.md`, ingresso diretto). Indietro riporta alla schermata di partenza con bozze e scroll intatti.
- Un solo pannello per volta e per posto; il pannello non è chiudibile (si risolve o si ignora andando avanti; l'azione che ha provocato il messaggio resta disponibile).
- Etichette del bottone: `missing` → «Inserisci la chiave»; `rejected` → «Sostituisci la chiave» (icona `key-round` 20 dp in entrambi). `credit` ha un'altra struttura (icona `wallet`, bottone «Riprova», nessun rimando alle Impostazioni): vedi §9, che prevale su ciò che segue in questa sezione e nelle fasi.

### Fasi (`phase`)

| Fase | Quando | Aspetto |
|---|---|---|
| `blocking` | il backend ha risposto `409 llm_key_missing` / `502 llm_key_rejected` alla richiesta (nulla è stato creato), oppure un job / un messaggio è `failed` con quel `error_code` | `warning`, testi di §5 (richiesta) o §6 (fallimento), bottone di Impostazioni |
| `fixed` | dopo che l'utente ha salvato una chiave (`llmKeyVersion` è salito dopo la comparsa del pannello) | richiesta sincrona: **il pannello sparisce** (nessun testo: l'utente rifà l'azione); fallimento (job/messaggio): `info` · `info`, titolo «Chiave salvata», corpo «Ora puoi riprovare.», bottone **Riprova** (come quello dei pannelli di fallimento esistenti) al posto di quello di Impostazioni |
| `hint` | suggerimento proattivo (§4): lo stato è noto, nulla è stato tentato | `info` (non `warning`: non è successo niente), testi di §4 |

`llmKeyVersion` è un contatore **locale per utente**, incrementato quando Impostazioni salva o rimuove una chiave, o quando `GET /me` cambia `has_llm_key`. Ogni pannello `blocking` ricorda la versione con cui è nato; al `ON_START` della schermata, se la versione è salita, passa a `fixed`. Un fallimento *nuovo* con lo stesso codice (la chiave salvata non funziona) riporta il pannello a `blocking` con il testo `rejected`.

### Stato noto della chiave (per i suggerimenti)

L'app deriva `Missing | Set | RejectedKnown` da: `User.has_llm_key` (da `POST /auth/login` e `GET /me`), il rifiuto visto (`llmKeyRejectedAt`, per utente, azzerato al salvataggio). `GET /me` si richiama all'avvio, a ogni ritorno in primo piano (al massimo una volta al minuto) e all'apertura di Impostazioni: così la chiave inserita dal server con `user set-llm-key` fa sparire i suggerimenti da sola. **Il backend resta l'unica autorità**: un suggerimento non disabilita mai un'azione; se l'utente la tenta comunque, risponde il `409`/`502` e vale il pannello `blocking`.

## 3. Cosa cambia nel flusso dell'utente

1. Azione che usa l'LLM → `409` o `502` (o job/messaggio `failed`).
2. Pannello `blocking` nel punto dell'azione; il resto della schermata funziona.
3. «Inserisci la chiave» → Impostazioni, campo a fuoco; incolla, «Salva chiave» → Toast «Chiave salvata. Puoi tornare indietro e riprovare.»
4. Indietro → pannello `fixed`: azione sincrona **rifattibile subito** (il testo nel composer c'è ancora); job/messaggio fallito → «Riprova».

Tre tocchi più un incolla; nessuna schermata modale in mezzo.

## 4. Primo accesso senza chiave (guida senza bloccare)

Un utente appena creato dal proprietario ha `has_llm_key = false`. Dopo l'accesso (`login.md`) vede **Home**, non le Impostazioni: può già creare cartelle e argomenti. La guida è in tre punti, tutti non bloccanti, tutti spariscono da soli a chiave salvata:

### 4.1 Home: banda di suggerimento

`LlmKeyNotice` `hint`, in cima alla lista, sopra «Cartelle» (compact: in colonna, 16 dp di margine, 16 dp sotto; nello stato vuoto «Inizia da una cartella» sta **sotto** il titolo e il testo della CTA, mai sopra: la CTA «Crea la prima cartella» resta la prima cosa). **Expanded**: banda a tutta larghezza sotto la TopBar, margini 32 dp, titolo e corpo a sinistra, bottone a destra (corpo max 2 righe); in nessun caso dentro un pannello.

| Variante | Titolo | Corpo | Bottone |
|---|---|---|---|
| `missing` (`Missing`) | Manca la chiave per le lezioni | Puoi già creare cartelle e argomenti e ripassare le schede. Per parlare con il Tutor, scrivere le lezioni e valutare le risposte aperte serve la chiave del servizio LLM. | Inserisci la chiave |
| `rejected` (`RejectedKnown`) | Il servizio non accetta la tua chiave | Puoi continuare a ripassare e a rileggere le lezioni. Per tutto il resto serve una chiave valida. | Sostituisci la chiave |

**Non ha ✕ e non si può rimandare**: è una condizione vera e persistente, nasconderla sposta il problema al momento di una lezione; è una banda singola e calma (`info`), non un badge sull'icona né un contatore (`home.md` «Evitare» resta valido: questo non è un'attività da smaltire). Sparisce con la chiave. Nel flusso a chiave `Set` non occupa spazio.

### 4.2 Chat: sopra il composer

Quando l'utente apre un argomento senza chiave, nello stato vuoto (`chat.md` §9.1) e in qualunque chat, finché `Missing` o `RejectedKnown`: `LlmKeyNotice` `hint` **sopra il composer** (stesso posto del pannello d'errore di invio, `chat.md` §9.3), il composer **resta attivo** e il testo si può scrivere (le bozze si conservano):

| Variante | Titolo | Corpo | Bottone |
|---|---|---|---|
| `missing` | Prima di scrivere: manca la chiave LLM | Il Tutor risponde con la tua chiave del servizio LLM. Inseriscila nelle Impostazioni, poi torna qui: quello che stai scrivendo resta nel campo. | Inserisci la chiave |
| `rejected` | Il servizio non accetta la tua chiave | Il Tutor non può rispondere finché non la sostituisci. Quello che scrivi resta nel campo. | Sostituisci la chiave |

È il punto in cui un nuovo utente **incontra davvero** il bisogno della chiave (la chat è il primo passo di ogni argomento): spiegarlo lì, nel momento in cui vuole scrivere, vale più di un'istruzione all'accesso (contiguità, motivazione intrinseca: capisce *perché* serve).

### 4.3 Completa lezione (ultima sezione)

Se `Missing`/`RejectedKnown`, sopra «Completa lezione» (stessa colonna della `ContinueBar`) un `LlmKeyNotice` `hint` «Per chiudere la lezione serve la chiave LLM» / «Le tue risposte sono al sicuro. Il registro si aggiorna con la tua chiave del servizio LLM: inseriscila nelle Impostazioni, poi torna a chiudere la lezione.» (`rejected`: titolo «Il servizio non accetta la tua chiave», corpo «Le tue risposte sono al sicuro. Sostituiscila nelle Impostazioni, poi torna a chiudere la lezione.»). «Completa lezione» **resta attivo**: l'autorità è il backend.

Nessun altro suggerimento proattivo (non in Topic, Fonti, Mission): lì il messaggio compare solo se l'azione viene tentata, per non moltiplicare le bande.

## 5. Richiesta rifiutata (`blocking`, risposta sincrona)

Nulla è stato creato (`users.md` §3). Testi definitivi, composti da una struttura fissa e da `{azione}` e `{rassicurazione}` per contesto (in italiano, frasi complete).

| | Titolo | Corpo |
|---|---|---|
| **`missing`** (`409 llm_key_missing`) | Manca la tua chiave LLM | Per {azione} serve la chiave del servizio LLM, e non ne hai ancora salvata una. {rassicurazione} Inseriscila nelle Impostazioni: ci vuole un minuto. |
| **`rejected`** (`502 llm_key_rejected`) | Il servizio non accetta la tua chiave | Per {azione} ho usato la tua chiave, ma il servizio l'ha rifiutata: può essere sbagliata, scaduta o non abilitata a questo uso. {rassicurazione} Sostituiscila nelle Impostazioni. |

| Contesto (azione dell'utente) | Dove compare il pannello | `{azione}` | `{rassicurazione}` |
|---|---|---|---|
| **Messaggio in chat** (`POST …/messages`) | sopra il composer, al posto del pannello «Messaggio non inviato» (`chat.md` §9.3) | parlare con il Tutor | Il tuo messaggio è ancora nel campo. |
| **Conferma della mission** (che avvia la ricerca delle fonti) | slot errore della card Mission (`mission.md`, «errore di conferma»); la card resta *aperta* | confermare la mission e cercare le fonti | La mission non è stata confermata: la proposta è ancora lì. |
| **Nuova ricerca di fonti / aggiornamento** (`POST …/resources/research`) | nello stato dell'azione in Fonti (`resources.md` §4.4 e banner di aggiornamento §6), sopra le fonti già presenti | cercare le fonti | Le fonti che hai restano come sono. *(se non ce ne sono: «La mission è al sicuro.»)* |
| **Genera lezione** | dentro la card «Prossima lezione» (`topic.md` F3.B), sopra il bottone, che resta | scrivere la lezione | Non hai perso nulla. |
| **Completa lezione** | in `ContinueBar`, al posto del bottone in errore (`lesson.md` §7.2); il pulsante torna «Completa lezione» | chiudere la lezione e aggiornare il registro | La lezione non è stata chiusa e le tue risposte sono al sicuro. |
| **Riprova** di un job o di un messaggio (`POST …/retry`) | nel pannello di fallimento da cui si è partiti | (quello del contesto) | (quella del contesto) |

Esempio completo (chat, `missing`): titolo «Manca la tua chiave LLM», corpo «Per parlare con il Tutor serve la chiave del servizio LLM, e non ne hai ancora salvata una. Il tuo messaggio è ancora nel campo. Inseriscila nelle Impostazioni: ci vuole un minuto.», bottone «Inserisci la chiave».

### 5.1 Valutazione delle risposte aperte (nessun `409` visibile)

Il contratto (`lessons.md` §5.1) fa salvare il tentativo comunque con `outcome: pending` se il Grader non riesce: l'app non riceve `llm_key_missing`/`llm_key_rejected`. Finché non cambia (proposta nel rapporto), il pannello «Risposta registrata» (`open_answer.md`, titolo e variante `info` invariati: niente colori d'errore per una risposta che non è sbagliata) sceglie il corpo dallo **stato noto della chiave**:

| Stato noto | Corpo di «Risposta registrata» | Bottone |
|---|---|---|
| `Missing` | La tua risposta è salvata, ma non posso valutarla perché manca la tua chiave LLM. Inseriscila nelle Impostazioni: la valuterò con «Valuta ora» o alla chiusura della lezione. | **Inserisci la chiave** (secondario) al posto di «Valuta ora»; da `fixed` torna «Valuta ora» |
| `RejectedKnown` | La tua risposta è salvata, ma il servizio ha rifiutato la tua chiave e non posso valutarla. Sostituiscila nelle Impostazioni: poi la valuto con «Valuta ora» o alla chiusura della lezione. | **Sostituisci la chiave** al posto di «Valuta ora» |
| `Set` | testi esistenti di `open_answer.md` | «Valuta ora» |

Lezione `completed` e risposta ancora `pending`: vale il testo esistente; se il job `lesson_completion` è `failed` con codice chiave, vale §6.

## 6. Fallimento di job o messaggio (`error_code`)

Sostituisce il pannello `danger` generico del contesto con `LlmKeyNotice` `blocking` (stesso posto, stesso schema titolo/corpo/bottone di `resources.md` §4.4, `topic.md` F3.D, `lesson.md` Stato D, `chat.md` §9.2) quando `error.code` è `llm_key_missing` o `llm_key_rejected`. **Non c'è «Riprova» finché la chiave non è cambiata** (riprovare con la stessa chiave rifiutata sarebbe un secondo fallimento garantito); dopo il salvataggio il pannello passa a `fixed` con «Riprova». Niente `Disclosure` «Dettagli»: il messaggio è già specifico.

| | Titolo | Corpo |
|---|---|---|
| **`llm_key_missing`** | Manca la tua chiave LLM | Mentre lavoravo la chiave è stata tolta, quindi non ho potuto {azione fallita}. {rassicurazione} Salvane una nelle Impostazioni e riprova. |
| **`llm_key_rejected`** | Il servizio non accetta la tua chiave | Ho provato a {azione fallita}, ma il servizio ha rifiutato la tua chiave: può essere sbagliata, scaduta o non abilitata a questo uso. {rassicurazione} Sostituiscila nelle Impostazioni e riprova. |

| Contesto | Posto | `{azione fallita}` | `{rassicurazione}` |
|---|---|---|---|
| Messaggio del Tutor `failed` | `ChatErrorPanel` sotto il testo parziale (`chat.md` §9.2) | `missing`: rispondere · `rejected`: rispondere al tuo messaggio | Il tuo messaggio è al sicuro. *(con testo parziale: aggiungere «Sopra c'è la parte arrivata prima dell'interruzione.»)* |
| Job `research` | stato D di Fonti (`resources.md` §4.4); chip del Topic «Ricerca da riprovare» invariato | cercare le fonti | La mission è al sicuro. |
| Job `lesson_generation` | card «Prossima lezione», stato F3.D (`topic.md`) | scrivere la lezione | Non hai perso nulla. |
| Job `lesson_completion` | Riepilogo, Stato D (`lesson.md` §7.1) | chiudere la lezione | La lezione è completata e le tue risposte sono al sicuro. |

Esempio (generazione, `rejected`): «Ho provato a scrivere la lezione, ma il servizio ha rifiutato la tua chiave: può essere sbagliata, scaduta o non abilitata a questo uso. Non hai perso nulla. Sostituiscila nelle Impostazioni e riprova.» Annuncio: «Preparazione interrotta. Servono le Impostazioni.» (live region Polite, una volta). Su un messaggio `failed` non ultimo (non ritentabile) il pannello resta corpo-soltanto, senza bottone di Riprova (come in `chat.md` §9.2) ma con quello di Impostazioni.

Il rifiuto osservato (`llm_key_rejected`, **solo** quello; mai `llm_credit_exhausted`, §9) imposta `llmKeyRejectedAt` (Impostazioni: chip «Rifiutata dal servizio»; Home: banda `rejected`). Un `llm_key_missing` osservato forza `has_llm_key = false` nella cache locale.

## 7. Accessibilità

- Tutti i pannelli sono `liveRegion = Polite` e si leggono come «<titolo>. <corpo>»; il bottone ha una `contentDescription` esplicita («Inserisci la chiave nelle Impostazioni», «Sostituisci la chiave nelle Impostazioni»).
- Il significato non dipende dal colore: icona `key-round`, titolo e testo; il tono ambra è rinforzo.
- Il focus non viene mai spostato dall'app; dopo il ritorno da Impostazioni il focus torna al bottone che ha aperto le Impostazioni (se esiste ancora) o al titolo del pannello.
- Font 200%: il bottone sta sotto il testo, a tutta larghezza se non ci sta accanto; nessun troncamento.
- Contrasti: `ink` su `warningSoft` e su `infoSoft` ≥ 7:1; `warning` su `warningSoft` e `info` su `infoSoft` ≥ 4,5:1 (`design-system.md` §2.4).

## 8. Cosa evitare

Rosso (`danger`) per chiave mancante o rifiutata; il verbo «sbagliata» riferito all'utente («hai inserito una chiave sbagliata»: la chiave può essere scaduta o non abilitata); la parola «credito» nei testi di `llm_key_rejected` (il credito esaurito ha il suo codice, §9; il rifiuto del provider con 403 resta ambiguo e il testo non ipotizza cause economiche); per il credito esaurito: «Sostituisci la chiave», l'icona `key-round`, il chip «Rifiutata dal servizio», la banda `rejected` di Home, un link a un sito di ricarica o il nome di un provider; nominare un provider (l'app non lo sa: dipende dalla configurazione del server); mostrare, anche in parte, la chiave; dialog modali che chiedono la chiave in mezzo a un'azione (si perderebbe il contesto); contatori o badge sull'icona Impostazioni; un secondo suggerimento per la stessa condizione sulla stessa schermata.

## 9. Credito esaurito (`llm_credit_exhausted`)

Contratto: `users.md` v1.1 §3, `jobs.md` v1.4, `chat.md` v1.2. Il provider ha accettato la chiave ma non c'è più credito. Cosa può fare l'utente: ricaricare **fuori dall'app** e tornare a riprovare. Cosa non deve fare: toccare la chiave (è giusta; sostituirla non risolve nulla). Quindi il pannello ha **una sola azione, «Riprova»**, ed è la stessa dei pannelli di fallimento esistenti (Mayer, coerenza: un'azione sola, quella che serve).

### 9.1 Pannello

`LlmKeyNotice` variante `credit`: `StatusPanel` `warning` (ambra, `warningSoft`: l'utente può risolvere subito, non è colpa sua, non è un errore del sistema), icona **`wallet`** (Lucide, stesso set e tratto di `design-system.md` §1.7; da aggiungere all'elenco dei nomi; non `key-round`, che suggerirebbe un problema della chiave), titolo `ui.labelL`, corpo `ui.bodyL`, bottone **«Riprova»** (`Button` secondario 48 dp, icona `rotate-cw` 20 dp come negli altri «Riprova»). Stessi padding, raggio, `liveRegion = Polite`, `feedbackAppear` di §2; il focus di TalkBack non viene spostato. Non ha ✕.

```
┌────────────────────────────────────────────┐
│ (wallet) Credito del servizio LLM esaurito │
│ Per scrivere la lezione ho usato la tua    │
│ chiave: il servizio l'ha accettata, ma il  │
│ credito è esaurito. Non hai perso nulla.   │
│ Ricaricalo presso il provider e riprova.   │
│ [ ↻ Riprova ]                              │
└────────────────────────────────────────────┘
```

- **Nessun bottone o link alle Impostazioni.** Valutato e scartato: l'app non conosce il provider né ha un link di ricarica, quindi un rimando alle Impostazioni porterebbe a una schermata dove non c'è nulla da fare per questo problema, e inviterebbe a sostituire una chiave giusta. L'azione fuori dall'app è nel testo («Ricaricalo presso il provider»).
- **Nessuna fase `fixed`**: non c'è un evento locale che dica «hai ricaricato». Il pannello resta `blocking` finché l'utente non preme «Riprova»; il tocco rilancia la stessa azione (§9.2). Non si fa polling né si nasconde da solo.
- **Nessuna fase `hint`**: nessuna banda in Home, Chat o Completa lezione per il credito. Il credito si consuma e si ricarica in modo imprevedibile; una banda persistente diventerebbe subito falsa e l'app non può verificarlo. Il messaggio compare solo quando l'azione viene tentata.
- Durante il nuovo tentativo: bottone disabilitato con spinner 20 dp e testo «Riprovo…» (annuncio TalkBack: «Riprovo»); se fallisce di nuovo con lo stesso codice, il pannello resta e si annuncia di nuovo una volta («Credito ancora esaurito.»). Nessun conteggio dei tentativi, nessun rallentamento artificiale.

### 9.2 Richiesta rifiutata (`502 llm_credit_exhausted`, nulla creato)

| | Titolo | Corpo |
|---|---|---|
| `credit` | Credito del servizio LLM esaurito | Per {azione} ho usato la tua chiave: il servizio l'ha accettata, ma il credito è esaurito. {rassicurazione} Ricaricalo presso il provider e riprova. |

`{azione}`, `{rassicurazione}` e i **posti** dei pannelli sono quelli della tabella di §5 (stessi contesti). «Riprova» **rilancia la stessa richiesta** con gli stessi dati: chat → rinvia il testo presente nel composer (come «Riprova» di `chat.md` §9.3); conferma mission → riconferma; nuova ricerca/aggiornamento → riavvia; genera lezione → ripete il `POST`; completa lezione → ripete `complete`, al posto del bottone in errore (`lesson.md` §7.2) e il pulsante torna «Completa lezione» solo a pannello chiuso da un esito riuscito. Riprova di un job o messaggio (`POST …/retry`) che risponde `502 llm_credit_exhausted`: il pannello di fallimento da cui si è partiti diventa questo pannello.

Esempio (chat): titolo «Credito del servizio LLM esaurito», corpo «Per parlare con il Tutor ho usato la tua chiave: il servizio l'ha accettata, ma il credito è esaurito. Il tuo messaggio è ancora nel campo. Ricaricalo presso il provider e riprova.», bottone «Riprova».

### 9.3 Fallimento di job o messaggio (`error.code = llm_credit_exhausted`)

Stesso posto di §6 e stessa struttura; al posto del pannello `danger` generico. **«Riprova» presente da subito** (a differenza di `llm_key_rejected`: nulla da salvare prima) e usa lo stesso `POST …/retry` dei fallimenti ordinari. Niente `Disclosure` «Dettagli».

| | Titolo | Corpo |
|---|---|---|
| `credit` | Credito del servizio LLM esaurito | Ho provato a {azione fallita}, ma il credito del servizio LLM è esaurito: la tua chiave è a posto. {rassicurazione} Ricaricalo presso il provider e riprova. |

`{azione fallita}` e `{rassicurazione}` come nella tabella di §6, **con una sola differenza**: per il messaggio del Tutor `failed` l'azione è sempre «rispondere al tuo messaggio» (il Tutor era già partito). Esempio (generazione): «Ho provato a scrivere la lezione, ma il credito del servizio LLM è esaurito: la tua chiave è a posto. Non hai perso nulla. Ricaricalo presso il provider e riprova.» Annuncio: «Preparazione interrotta. Credito esaurito.» (Polite, una volta).

Un messaggio `failed` **non ultimo** (non ritentabile): pannello a solo corpo, senza «Riprova» e senza rimando alle Impostazioni (come `chat.md` §9.2). Con testo parziale si aggiunge «Sopra c'è la parte arrivata prima dell'interruzione.» alla rassicurazione.

### 9.4 Effetti sullo stato locale

- **Non** imposta `llmKeyRejectedAt` né fa comparire il chip «Rifiutata dal servizio» (`settings.md`), né la banda `rejected` di Home, né i suggerimenti di §4. `has_llm_key` non cambia, `llmKeyVersion` non sale.
- Il codice è la prova che il provider ha **accettato** la chiave: se `llmKeyRejectedAt` era valorizzato (rifiuto visto con la v1, quando il credito arrivava come `llm_key_rejected`), **va azzerato**. Così uno stato `rejected` stale sparisce da solo.
- Nessun indicatore persistente del credito (Impostazioni, Home, icona): non aiuta a fare nulla, perché l'app non sa quando è stato ricaricato e la ricarica non avviene lì. Vale solo per la sessione: `llmCreditExhaustedSeen` (in memoria, per utente), usato solo da §9.5, azzerato alla prima operazione LLM riuscita osservata (messaggio del Tutor `completed`, job `succeeded`, tentativo valutato) e al riavvio dell'app.
- **Regola per data** . Ogni prova che la chiave funziona (chiave salvata, operazione LLM riuscita) ha un istante del **server**: quello che porta la risposta (job, messaggio) oppure, se non ne porta, l'header HTTP `Date` della risposta che la prova. L'orologio del dispositivo non si confronta con gli istanti del server. `Date` ha precisione al secondo, i fallimenti al millisecondo: la prova vale fino all'ultimo millisecondo di quel secondo, quindi un fallimento nello stesso secondo conta come precedente (verso sicuro: un vecchio fallimento rimandato non riarma ciò che l'utente ha appena sistemato). Anche un fallimento nato dalla risposta di una richiesta (chat, mission, lezioni, risorse) è datato con il `Date` di quella risposta, all'**inizio** del secondo: nello stesso secondo il fallimento conta come più vecchio della prova. Senza `Date` valido, o con un `Date` più di un giorno avanti rispetto al massimo tra orologio del dispositivo e ultimo fallimento visto (proxy o server con l'ora sbagliata), l'app ripiega sull'orologio del dispositivo, mai prima del fallimento più recente già visto. Un fallimento non successivo all'ultima prova si ignora, da qualunque canale arrivi. `Date` può essere indietro di qualche centinaio di millisecondi rispetto all'istante reale della risposta: la finestra residua è di circa un secondo, accettata.
- **Credito legato alla sessione.** `llmCreditExhaustedSeen` vale per la sessione (token), non solo per l'utente: uscita e nuovo accesso, anche dello stesso utente, ripartono puliti. Lo azzera solo un'operazione riuscita con data successiva al fallimento di credito; un evento rimandato più vecchio non lo azzera, e non azzera nemmeno il conteggio dei «ancora esaurito» in chat (§9.3).
- **Filtro sull'avvio.** Un fallimento di credito conta solo se è avvenuto dopo l'avvio del monitor per quella sessione: uno più vecchio, letto dopo un riavvio o un accesso, non accende nulla. Un fallimento nato da una richiesta appena fatta è nuovo per costruzione e non passa dal filtro; quello letto da un job o da un messaggio sì, ed è ancora confrontato con l'orologio del dispositivo (limite accettato).

### 9.5 Risposte aperte in attesa di valutazione

Estende la tabella di §5.1. Precedenza: `Missing` > `RejectedKnown` > credito > `Set`. Con `Set` e `llmCreditExhaustedSeen` vero, il pannello «Risposta registrata» (titolo e variante `info` invariati) usa questo corpo, e **il bottone resta «Valuta ora»**, che è il «Riprova» di questo caso (nessun rimando alle Impostazioni):

| Stato | Corpo di «Risposta registrata» | Bottone |
|---|---|---|
| credito esaurito visto | La tua risposta è salvata, ma non posso valutarla: il credito del servizio LLM è esaurito. Ricaricalo presso il provider: poi la valuto con «Valuta ora» o alla chiusura della lezione. | **Valuta ora** (invariato) |

Se `llmCreditExhaustedSeen` è falso e il tentativo è `pending` (altro errore del Grader), vale il testo esistente di `open_answer.md`: l'app non ipotizza il credito senza averlo visto.

### 9.6 Accessibilità

- Titolo e corpo letti come «<titolo>. <corpo>» (Polite); il bottone ha `contentDescription` «Riprova: <azione>» (es. «Riprova: scrivere la lezione»). Non nelle Impostazioni, quindi niente testo «nelle Impostazioni».
- Il significato non dipende dal colore: icona `wallet`, titolo e testo; ambra come rinforzo. Contrasti come §7 (`ink` su `warningSoft` ≥ 7:1, `warning` su `warningSoft` ≥ 4,5:1).
- Font 200%: bottone sotto il testo, a tutta larghezza; nessun troncamento. Target ≥ 48 dp.
- Durante «Riprovo…» il bottone è `disabled` con `stateDescription` «in corso»; al termine, se riesce, il pannello sparisce e il focus resta dove l'utente era; se fallisce, il focus non si sposta.

### 9.7 Stringhe (italiano, per `android-dev`)

| Chiave suggerita | Testo |
|---|---|
| `llm_credit_title` | Credito del servizio LLM esaurito |
| `llm_credit_body_request` | Per %1$s ho usato la tua chiave: il servizio l'ha accettata, ma il credito è esaurito. %2$s Ricaricalo presso il provider e riprova. |
| `llm_credit_body_failure` | Ho provato a %1$s, ma il credito del servizio LLM è esaurito: la tua chiave è a posto. %2$s Ricaricalo presso il provider e riprova. |
| `llm_credit_retry` / `llm_credit_retrying` | Riprova / Riprovo… |
| `llm_credit_retry_cd` | Riprova: %1$s |
| `llm_credit_announce_failed` / `llm_credit_announce_again` | Preparazione interrotta. Credito esaurito. / Credito ancora esaurito. |
| `open_answer_pending_credit` | La tua risposta è salvata, ma non posso valutarla: il credito del servizio LLM è esaurito. Ricaricalo presso il provider: poi la valuto con «Valuta ora» o alla chiusura della lezione. |

`%1$s` e `%2$s` sono gli stessi `{azione}` / `{azione fallita}` e `{rassicurazione}` delle tabelle di §5 e §6. Se il backend manda un `message` diverso da quello fisso del contratto, l'app usa comunque queste stringhe locali (il codice decide, non il testo).
