# ADR 0020 — Istanza singola

- Stato: accettata
- Data: 2026-11-22
- Contesto: audit esterno del 22/11/2026, punto C6.

## Problema

teach_me gira sull'homelab con SQLite e con componenti che tengono stato in memoria di processo: il job runner in-process, il worker audio, l'ottimizzatore dei pesi FSRS e il canale degli eventi SSE. I volumi locali contengono database, audio e modelli. Con due repliche dello stesso backend che leggono gli stessi dati, un job può essere eseguito due volte, e gli eventi SSE e i file audio non sono condivisi tra le repliche.

## Scelta

1. **Una sola istanza del backend per ogni deployment.** SQLite, job runner in-process, worker audio, ottimizzatore, eventi SSE e volumi locali per database, audio e modelli.
2. **Due repliche sugli stessi dati non sono supportate.** Possono duplicare i job, e gli eventi e l'audio non sono condivisi.
3. **Scalare in orizzontale è fuori scopo.** PostgreSQL, coda esterna, object storage e broker di eventi richiedono una nuova ADR prima di essere introdotti.

## Alternative scartate

- **Repliche con database condiviso e coda esterna già ora:** complessità operativa non giustificata per un utente, su un solo host di casa.

## Conseguenze

- Il deploy non prevede più di un contenitore del backend contro gli stessi volumi.
- Un aggiornamento del backend ferma per un momento il servizio: accettabile per un uso domestico.
- Chi propone di scalare in orizzontale deve aprire una nuova ADR che sostituisca questa.
