# ADR 0015 — Scrittura a mano sulle lezioni

- Stato: accettata
- Data: 2026-11-15
- Issue Gitea #27 ("Scrittura a mano"). Il proprietario ha deciso i punti 1-4 il 2026-11-15; i punti 5-11 sono scelte del tech lead che ne derivano. Contratto: `docs/contracts/lessons.md` §11. Design: `docs/design/components/handwriting.md`.

## Problema

Mentre si studia una lezione si può evidenziare (ADR 0011) ma non annotare: niente appunti, frecce, schemi o formule scritti di proprio pugno, come a margine di un libro. Sul tablet con la penna, target primario dell'app (ADR 0003), è il gesto più naturale.

Due vincoli: la lezione non ha pagine fisse (il testo si riadatta a rotazione, dimensione del carattere, tablet o telefono), e oggi un tratto di penna sul testo è già un'evidenziazione.

## Scelta

1. **Note a mano libera sulla lezione.** Le risposte scritte a mano negli esercizi (`open_answer`, `fill_blank`) restano fuori da questa versione.
2. **Sopra il testo, agganciate al blocco.** Un tratto appartiene al blocco della lezione su cui è iniziato e si disegna sopra di esso, non in uno spazio separato.
3. **Uno strumento da scegliere nella tavolozza**: «evidenziatore» o «penna». Con l'evidenziatore tutto resta come nell'ADR 0011; con la penna la punta scrive.
4. **Salvate sul backend, per chi legge**: si ritrovano su ogni dispositivo dell'utente.
5. **Coordinate relative alla larghezza del blocco.** Un punto è (`x`, `y`) dall'angolo in alto a sinistra del blocco, entrambi divisi per la larghezza del blocco al momento del tratto. A una larghezza diversa la nota si ridisegna in scala, senza deformarsi. **Limite accettato**: se il testo va a capo in modo diverso (rotazione, carattere più grande, telefono), la nota resta vicina al suo blocco ma può non coincidere più con le stesse parole. Riagganciare i tratti alle parole non è possibile in generale (un disegno non ha parole).
6. **Si scrive anche fuori dal blocco**, nello spazio della colonna e nei margini laterali sul tablet: il tratto resta del blocco in cui è iniziato e le sue coordinate possono uscire da [0, 1]. Dove lo spazio non c'è (telefono), la parte fuori dallo schermo si taglia.
7. **Tutti i blocchi del primo livello**, esercizi inclusi: con la penna attiva la punta scrive e non risponde agli esercizi, il dito tocca e risponde come oggi.
8. **Insieme intero per lezione** (`PUT /lessons/{id}/strokes`), come le evidenziazioni (ADR 0011 punto 7): niente coda di operazioni, reinvio idempotente, con due dispositivi vince l'ultimo invio. Ogni tratto ha un id generato dall'app, così la gomma e l'«Annulla» lavorano su tratti interi senza riconciliazioni.
9. **Legate alla lezione** come le evidenziazioni: se la lezione viene eliminata o sostituita, le note si perdono.
10. **Solo per chi legge.** Non entrano nei contesti degli agenti e non si riconoscono come testo. Elenco per argomento, ricerca e Tutor restano possibili in seguito.
11. **Senza penna la lezione è identica a oggi**: chi non ha la penna vede le note fatte su un altro dispositivo, senza poterle modificare.

## Alternative scartate

- **Riquadro «appunti» sotto un blocco**: sempre al suo posto con qualsiasi layout, ma non permette di scrivere vicino alle parole; il proprietario ha preferito la nota sopra il testo.
- **Margine laterale dedicato**: solo sul tablet orizzontale; sul telefono serviva un'altra soluzione.
- **Coordinate in dp o pixel di schermo**: semplici, ma la nota esce di posto appena cambia la larghezza della colonna.
- **La zona decide lo strumento** (testo = evidenzia, altrove = scrive): niente da scegliere, ma non si potrebbe scrivere sopra un paragrafo, che è la scelta del punto 2.
- **Solo sul dispositivo**: meno lavoro, ma il proprietario vuole ritrovarle altrove.
- **Operazioni singole** (`POST`/`DELETE` per tratto): servono coda offline ordinata e riconciliazione; l'insieme intero copre gli stessi casi.

## Conseguenze

- Contratto `lessons.md` v1.4 (§11 e `LessonView.strokes`), additivo.
- Backend: migrazione additiva, validazione dei limiti e dei `block_id`, isolamento per utente ereditato dalla lezione.
- App: tavolozza con la scelta dello strumento, disegno a bassa latenza sopra la colonna della lezione, gomma, sincronizzazione dell'insieme come per le evidenziazioni.
- Limiti accettati: la nota non segue le parole quando il testo va a capo diversamente (punto 5); con due dispositivi offline vince l'ultimo invio.
