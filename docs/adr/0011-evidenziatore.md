# ADR 0011 — Evidenziatore con la penna nelle lezioni

- Stato: accettata
- Data: 2026-11-13
- Issue Gitea #2 ("Evidenziatore"). Le sei domande aperte della issue le ha chiuse il proprietario il 2026-11-13 (punti 1-5); i punti 6-8 sono scelte del tech lead che ne derivano.

## Problema

Mentre si studia una lezione non c'è modo di segnare i passaggi importanti. Sul tablet, target primario dell'app (ADR 0003), la penna permette di farlo senza una "modalità selezione", lasciando al dito lo scorrimento e i tocchi.

## Scelta

1. **Solo la penna evidenzia.** Il tratto si aggancia alle parole (non è un disegno libero). Il dito scorre e tocca come prima. Senza penna la lezione è identica a oggi.
2. **Dove: il testo di `paragraph` e di `callout`** (campo `text`). Fuori: titoli, titolo del callout, esercizi, codice, chat, fonti, PDF.
3. **Salvate sul backend, per chi legge.** Si ritrovano su ogni dispositivo dell'utente. Non sono un segnale per il Tutor e non c'è un elenco per argomento: restano possibili in seguito.
4. **Legate alla lezione.** Un documento di lezione non cambia dopo il salvataggio; se una lezione viene eliminata o sostituita, le sue evidenziazioni si perdono. Nessun riaggancio al testo.
5. **Più colori** (`yellow`, `green`, `blue`, `pink`) e **due modi di togliere**: gomma o tasto della penna passando sopra, oppure un tocco sull'evidenziazione che offre «Rimuovi».
6. **Ancora = `block_id` + intervallo nel sorgente del campo `text`**, in unità UTF-16 (`lessons.md` §10). Il sorgente markdown è l'unico testo che backend e app condividono: il testo mostrato dipende dal renderer (termini del glossario sostituiti, numeri delle citazioni, marcatori rimossi). L'app converte tra sorgente e testo mostrato; il backend controlla solo blocco, limiti e colore.
7. **L'app invia l'insieme intero** (`PUT /lessons/{id}/highlights`), non singole operazioni. L'insieme locale è la verità finché non è sincronizzato: niente coda di operazioni, niente id da riconciliare, reinvio idempotente. Con due dispositivi vince l'ultimo invio.
8. **Si evidenzia anche a lezione completata**: rileggere e segnare è l'uso tipico.

## Alternative scartate

- **Solo sul dispositivo** (come i progressi di sezione): meno lavoro, ma il proprietario le vuole ritrovare altrove.
- **Intervalli nel testo mostrato**: più semplici per l'app, ma ogni client dovrebbe replicare alla lettera il renderer di Android e il backend non potrebbe controllarli.
- **Risorse singole con `POST`/`DELETE`**: servono una coda offline ordinata e la gestione di fusioni e divisioni come sequenze di chiamate; l'insieme intero copre gli stessi casi con una chiamata.
- **Riaggancio dopo una rigenerazione**: richiede di salvare il testo citato e cercarlo nel nuovo documento, con risultati a volte sorprendenti.

## Conseguenze

- Contratto `lessons.md` v1.2 (§10 e `LessonView.highlights`).
- Backend: migrazione additiva, validazione contro il documento, isolamento per utente ereditato dalla lezione.
- App: il parser del markdown inline tiene le posizioni nel sorgente; input della penna distinto dal dito; token di colore per tema chiaro e scuro; sincronizzazione dell'insieme.
- Limite accettato: due dispositivi che modificano la stessa lezione mentre uno è offline, vince l'ultimo che invia.
