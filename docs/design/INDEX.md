# Indice di `docs/design/`

Apri il file alla sezione che serve (`grep -n '^#' <file>` per i numeri di riga); non leggere i file interi.

## Radice di `design/`

- [`base-components.md`](base-components.md) · 36.1 KB — Specifica dei componenti base (Button, campi, Surface, TopBar, dialog) da costruire su Compose foundation.
  - Titoli: §1 Button · §2 Campo di testo (TeachTextField) · §3 Card / superficie (Surface) · §4 Lista / riga (ListRow) · §5 Indicatore di stato / esito (StatusP… · §6 Barra in alto (TopBar) · §7 Tab (TabRow) · §8 Dialog di conferma (ConfirmDialog) · §9 Altri componenti usati dalle scherma… · §10 Componenti aggiunti in Fase 1 · §11 Checklist per android-dev · §11 Dialog su expanded (tablet orizzont… · §12 Componenti aggiunti in Fase 2 (Chat) · §13 Componenti aggiunti in Fase 3 (Miss… · §14 Componenti aggiunti in Fase 5 (Lezi… · §15 Componenti aggiunti in Fase 6-7 (Le… · §16 Aggiunte della Fase 11 (multiutenza…
- [`design-system.md`](design-system.md) · 38.4 KB — Design system v1: principi, tipografia, colori, spaziature, motion, accessibilità, terminologia.
  - Titoli: §0 Struttura di tokens.json · §1 Principi, con la loro base · §2 Linguaggio visivo "carta e inchiostr… · §3 Tipografia · §4 Stati dei componenti (regole comuni) · §5 Spacing e layout · §6 Shape e stroke · §7 Elevazione · §8 Motion · §9 Accessibilità (checklist trasversale) · §10 Voce dell'interfaccia · §11 Terminologia dell'interfaccia (ital… · §12 Indice
- [`generator-guidelines.md`](generator-guidelines.md) · 23.8 KB — Linee guida per Grader e Recorder, lessico da usare e da evitare, controlli di qualità.
  - Titoli: §1 Lessico: parole da usare e da evitar… · §2 Grader · §3 Recorder · §4 Note per il Planner (cosa l'interfac… · §5 Controlli di qualità (prima di conse… · §6 Lezioni dal PDF dell'utente (Fase 10) · §7 Sintesi per il prompt del Generator:…
- [`llm-key.md`](llm-key.md) · 28.8 KB — Messaggi e punti di ingresso per chiave LLM mancante, rifiutata o credito esaurito.
  - Titoli: §1 Principio · §2 Il componente: LlmKeyNotice (composi… · §3 Cosa cambia nel flusso dell'utente · §4 Primo accesso senza chiave (guida se… · §5 Richiesta rifiutata (blocking, rispo… · §6 Fallimento di job o messaggio (error… · §7 Accessibilità · §8 Cosa evitare · §9 Credito esaurito (llm_credit_exhaust…
- [`quality-warnings.md`](quality-warnings.md) · 8 KB — Controlli del Validator (errori e avvisi non bloccanti, issue #14): soglie, codici, messaggi e resa nell'app.
  - Titoli: §0 Regole comuni · §1 Errori bloccanti · §2 Avvisi non bloccanti · §3 App · §4 Per android-dev e backend · §5 Aperto
- [`review-mvp.md`](review-mvp.md) · 10.2 KB — Revisione dell'esperienza MVP contro i principi P1-P13: cosa funziona, problemi, misure.
  - Titoli: §1 Cosa funziona · §2 Problemi, per impatto sull'apprendim… · §3 Controllo per principio (sintesi) · §4 Da rimandare dopo l'MVP · §5 Misure da raccogliere con i dati che…
- [`tokens.json`](tokens.json) · 12.6 KB — Token del design system (colori, tipografia, spaziature), fonte dei valori.

## Componenti (`components/`)

- [`components/README.md`](components/README.md) · 27.5 KB — Catalogo dei 17 blocchi SDUI: regole comuni e linee guida per il Generator.
  - Titoli: Indice · Regole comuni a tutti i blocchi · Linee guida generali per il Generator · Costanti di lezione (per android-dev)
- [`components/_exercises.md`](components/_exercises.md) · 24.9 KB — Regole comuni a tutti gli esercizi: cornice, stati, feedback, riprova, tastiera, TalkBack.
  - Titoli: §1 Cornice: ExerciseCard · §2 Stati · §3 Azione di controllo · §4 Feedback (StatusPanel) · §5 Riprova (facoltativa) · §6 Tastiera fisica (tablet con tastiera) · §7 TalkBack · §8 Contenuti al limite · §9 Cosa il Generator deve evitare (comu… · §10 Codice di un esercizio (code, contr…
- [`components/ask_agent.md`](components/ask_agent.md) · 6.5 KB — Blocco che apre la Chat del Topic dalla lezione: scopo, anatomia, stati.
  - Titoli: Scopo didattico · Anatomia · Cosa succede al tocco · Stati · Accessibilità · Contenuti al limite · Linee guida per il Generator
- [`components/callout.md`](components/callout.md) · 3.3 KB — Blocco di segnalazione (info, tip, warning) costruito come StatusPanel.
- [`components/checklist.md`](components/checklist.md) · 3.2 KB — Blocco di 2-5 azioni autodichiarate da fare fuori dall'app.
- [`components/code.md`](components/code.md) · 5.9 KB — Blocco di codice di esempio: misure, stati, interazioni, accessibilità.
  - Titoli: Scopo didattico · Anatomia · Stati · Interazioni · Accessibilità · Contenuti al limite · Linee guida per il Generator
- [`components/diagram.md`](components/diagram.md) · 20.6 KB — Blocco diagram (flow, tree, cycle): quando serve, layout, tema, fallback testuale.
  - Titoli: Scopo didattico · Anatomia · Tre layout · Tema chiaro e scuro · Stati · Fallback testuale · Accessibilità · Contenuti al limite · Linee guida per il Generator
- [`components/fill_blank.md`](components/fill_blank.md) · 6.5 KB — Esercizio di richiamo libero: completare una parola in una frase.
  - Titoli: Scopo didattico · Anatomia · Misure e token · Interazione e tastiera · Accessibilità · Contenuti al limite · Linee guida per il Generator
- [`components/flashcard.md`](components/flashcard.md) · 6.0 KB — Scheda fronte/retro dentro la lezione con autovalutazione.
  - Titoli: Scopo didattico · Anatomia · Misure e token · Stati · Interazione e tastiera · Accessibilità · Contenuti al limite · Linee guida per il Generator
- [`components/glossary_term.md`](components/glossary_term.md) · 4.6 KB — Blocco glossary_term in-flow e sheet del glossario.
- [`components/handwriting.md`](components/handwriting.md) · 46.5 KB — Scrittura a mano sulla lezione: tavolozza, gomma, sincronizzazione, proposta per il contratto.
  - Titoli: §1 Scopo didattico · §2 Il modello in una pagina · §3 Tavolozza (estende highlight.md §6) · §4 Aspetto del tratto · §5 Dove si scrive · §6 Gomma, Annulla e Ripeti · §7 Penna attiva sugli esercizi, su term… · §8 Limiti e messaggi · §9 Sincronizzazione · §10 Accessibilità · §11 Per android-dev · §12 Per il Generator (linee guida, da p… · §13 Proposta per il contratto (lessons.… · Contenuti al limite e casi particolari
- [`components/heading.md`](components/heading.md) · 4.0 KB — Titoli di livello 1-3 che dividono la lezione in sezioni.
- [`components/highlight.md`](components/highlight.md) · 29.3 KB — Evidenziatore con la penna: aspetto, tavolozza, gomma, contrasti WCAG, sincronizzazione.
  - Titoli: §1 Scopo didattico · §2 Quando esiste qualcosa di visibile (… · §3 Aspetto · §4 Il tratto (penna) · §5 Interazione con evidenziazioni esist… · §6 Tavolozza (strumento e colore) · §7 Gomma e tasto della penna · §8 «Rimuovi» con un tocco · §9 Selezione del testo di sistema · §10 Contrasti calcolati (WCAG 2.x, tool… · §11 Sincronizzazione e errori · §12 Accessibilità · §13 Per android-dev (riassunto) · §14 Per il Generator (linee guida) · Contenuti al limite e casi particolari
- [`components/lesson_link.md`](components/lesson_link.md) · 2.4 KB — Blocco che collega la lezione alle altre dello stesso argomento.
- [`components/missing_component.md`](components/missing_component.md) · 3.6 KB — Blocco missing_component e blocchi sconosciuti: fallback senza rompere la lezione.
- [`components/open_answer.md`](components/open_answer.md) · 12.5 KB — Esercizio a risposta aperta con feedback del Grader e risposta in attesa.
  - Titoli: Scopo didattico · Anatomia · Stati · Feedback del Grader · Risposta registrata (pending) e reinvio · Interazione e tastiera · Accessibilità · Contenuti al limite · Linee guida per il Generator · Linee guida per il Grader · Proposte ai contratti (per il tech lead…
- [`components/order_steps.md`](components/order_steps.md) · 8.5 KB — Esercizio per riordinare una sequenza causale o procedurale.
  - Titoli: Scopo didattico · Anatomia · Misure e token · Stati · Interazione · Accessibilità · Contenuti al limite · Linee guida per il Generator
- [`components/paragraph.md`](components/paragraph.md) · 6.4 KB — Blocco paragrafo e markdown inline dei campi testo.
  - Titoli: Scopo didattico · Anatomia e misure · Markdown inline (campi *md* di tutti i… · Stati · Interazioni · Contenuti al limite · Linee guida per il Generator
- [`components/primary_source.md`](components/primary_source.md) · 4.3 KB — Blocco della fonte primaria in chiusura, anche per il PDF dell'utente.
- [`components/quiz_multi.md`](components/quiz_multi.md) · 5.4 KB — Quiz a più risposte corrette: differenze rispetto a quiz_single.
  - Titoli: Scopo didattico · Anatomia · Differenze di stato/misura rispetto a q… · Contenuti al limite · Linee guida per il Generator
- [`components/quiz_single.md`](components/quiz_single.md) · 8.5 KB — Quiz a risposta singola con feedback spiegato su ogni opzione.
  - Titoli: Scopo didattico · Anatomia · Misure e token (ChoiceRow, base-compone… · Interazione · Accessibilità · Contenuti al limite · Linee guida per il Generator

## Schermate (`screens/`)

- [`screens/chat.md`](screens/chat.md) · 61.0 KB — Schermata Chat con il Tutor dentro l'Argomento: layout, messaggi, streaming, composer.
  - Titoli: §1 Scopo didattico · §2 Layout expanded (tablet orizzontale,… · §3 Layout compact (telefono) · §4 Lista dei messaggi · §5 Markdown del Tutor · §6 Risposta in arrivo e streaming · §7 Scroll · §8 Composer · §9 Stato vuoto, errori e casi limite · §10 Posto per la card nativa di propost… · §10b Card della lezione richiesta (Less… · §11 Accessibilità · §12 Indicazioni per il Generator/Tutor… · §13 Costanti (non in tokens.json; mette… · Aggiunte del tech lead (implementazione…
- [`screens/folder.md`](screens/folder.md) · 13.1 KB — Schermata Cartella: argomenti con stato in parole, creazione, rinomina, eliminazione.
  - Titoli: Scopo didattico · Layout (compact, 360 dp) · Stati · Crea argomento · Rinomina argomento · Elimina argomento · Elimina / rinomina la cartella da qui · Spostare un argomento in un'altra carte… · Accessibilità · Evitare · Expanded / tablet orizzontale (layout p…
- [`screens/home.md`](screens/home.md) · 26.7 KB — Schermata Home: cartelle, ripasso di oggi (con filtro per argomento); layout tablet e telefono.
  - Titoli: Scopo didattico · Layout (compact, 360 dp) · Stati · Crea cartella · Rinomina cartella · Elimina cartella · Accessibilità · Evitare · Expanded / tablet orizzontale (layout p… · Ripasso di oggi (Fase 8) · Riprendi la lezione (Fase 8b)
- [`screens/lesson-audio.md`](screens/lesson-audio.md) · 14.5 KB — Lettura ad alta voce: controllo Ascolta per blocco, lettura continua, impostazioni.
  - Titoli: §0 Principi · §1 Controllo «Ascolta» per blocco · §2 Segno «ora in lettura» · §3 «Ascolta la lezione» (lettura contin… · §4 Impostazioni → sezione «Ascolto» · §5 Cosa deve evitare il Generator · §6 Token e icone · §7 Aperto
- [`screens/lesson.md`](screens/lesson.md) · 46.3 KB — Schermata Lezione: documento in sezioni, flusso, obbligatorietà, riepilogo, stati.
  - Titoli: §1 Scopo didattico · §2 Modello: documento → sezioni · §3 Layout expanded (≥ 840 dp) di riferi… · §4 Regole di flusso · §5 Obbligatorietà e sblocco · §6 Tastiera fisica (tablet con tastiera) · §7 Riepilogo (dopo «Completa lezione»;… · §8 Stati della schermata · §9 Accessibilità · §10 Proposte ai contratti (per il tech…
- [`screens/login.md`](screens/login.md) · 20.3 KB — Schermata di accesso: primo accesso, campi, errori, sessione finita.
  - Titoli: Scopo · Quando si vede · Layout (compact, 360 dp), primo accesso · Campi · Pulsante «Accedi» · Stati ed errori (testi definitivi) · Variante *Sessione finita* e *Aggiornam… · Focus e tastiera: riepilogo · Accessibilità · Cosa non c'è (di proposito)
- [`screens/mission.md`](screens/mission.md) · 39.9 KB — Card di proposta della mission, vista Mission e testi definitivi.
  - Titoli: §1 Scopo didattico · §2 La card di proposta (MissionCard) · §3 Cosa succede dopo la conferma · §4 Vista Mission dell'argomento · §5 Chip di stato e frase onesta per res… · §6 Accessibilità · §7 Testi definitivi (riepilogo) · §8 Cosa evitare
- [`screens/progress.md`](screens/progress.md) · 20.1 KB — Tab «Cosa sai»: Registro delle competenze dimostrate e Glossario.
  - Titoli: §1 Scopo didattico · §2 Struttura della tab · §3 Registro («Cosa hai dimostrato») · §4 Glossario · §5 Motion · §6 Costanti · §7 Proposte ai contratti (per il tech l… · §8 Aperto
- [`screens/resources.md`](screens/resources.md) · 54.2 KB — Tab Fonti: stati della ricerca, gaps, card delle fonti, documenti caricati.
  - Titoli: §1 Scopo didattico · §2 Posizione, presenza e tab iniziale · §3 Layout · §4 Stati della tab · §5 Gaps: «Cosa non ho trovato» · §6 Rifacimento dopo un cambio di mission · §7 Come chat e pannello Mission segnala… · §8 Card di una fonte (ResourceRow) · §9 Motion · §10 Proposte di contratto (per il tech… · §11 Aperto · §12 Indicazioni per llm-engineer (Resea… · §13 «I tuoi documenti» (Fase 10, PDF ca…
- [`screens/review.md`](screens/review.md) · 36.5 KB — Schermata Ripasso: fronte, retro, «Apri la lezione», voto, annulla, sessione (anche filtrata per argomento), coda offline.
  - Titoli: §1 Scopo didattico · §2 Flusso · §3 Misure comuni · §4 Fronte, rivelazione, retro · §5 Dopo il voto: annulla (UndoRow) · §6 Menu ⋮ (azioni della scheda) · §7 Sessione, arretrato e fine · §8 Voti offline: coda locale · §9 Responsive · §10 Tastiera fisica e accessibilità · §11 Stati e testi · §12 Cosa evitare
- [`screens/settings.md`](screens/settings.md) · 28.9 KB — Impostazioni: account, chiave LLM, password, connessione, ripasso, licenze.
  - Titoli: Scopo · Layout (compact, 360 dp) · Sezione Account · Sezione Chiave LLM · Sezione Password → sotto-schermata «Cam… · Sezione Connessione · Righe di servizio · Stati dei controlli · Accessibilità · Casi particolari · Cosa non c'è (di proposito) · Sotto-schermata: Componenti mancanti (F… · Sotto-schermata: Licenze open source
- [`screens/suspended.md`](screens/suspended.md) · 10.0 KB — Schede sospese dall'utente e loro riattivazione.
  - Titoli: §1 Scopo e accesso · §2 Layout · §3 Riattivazione · §4 Stati della lista · §5 Accessibilità e stringhe · §6 Evitare
- [`screens/topic.md`](screens/topic.md) · 39.5 KB — Schermata Argomento con tab (Chat, Mission, Fonti, Lezioni, Cosa sai), per fase.
  - Titoli: Fase 1: schermata Argomento minima · Fase 2: la Chat è il contenuto dell'Arg… · Fase 3: tab Chat e Mission, chip aggior… · Scopo didattico · Layout (compact) · Tab Lezioni (schizzo di Fase 0, **super… · Tab Chat · Altre tab (schema) · Accessibilità · Evitare · Fase 4: tab Fonti · Fase 6-7: Lezioni, «Cosa sai» e struttu… · Fase 10: lezioni che seguono il tuo PDF

## Strumenti (`tools/`)

- [`tools/contrast_check.py`](tools/contrast_check.py) · 11.5 KB — Script Python che verifica i contrasti WCAG delle coppie colore.

## Campioni JSON (una riga per cartella)

- **`samples/`**: 2 file JSON di lezioni di esempio (`lesson-01`, `lesson-02`). Da non leggere: servono ai test.
