# ADR 0012 — Notifiche delle schede di ripasso in scadenza

- Stato: accettata
- Data: 2026-11-13
- Issue Gitea #7. Il proprietario ha chiuso il 2026-11-13 le tre domande aperte (punti 1-3); i punti 4-8 sono scelte del tech lead che ne derivano.

## Problema

Le schede in scadenza (FSRS, `reviews.md`) si vedono solo aprendo l'app, nella card «Ripasso di oggi» della Home. Chi non apre l'app non sa che ha schede da ripassare, e il ripasso fatto in ritardo rende meno.

## Scelta

1. **L'app interroga il backend, niente push.** Un lavoro periodico di WorkManager chiama `GET /reviews/due?limit=1` e mostra una notifica locale. Nessun servizio esterno (Firebase), nessuna modifica al backend né ai contratti: `due_count` c'è già (`reviews.md` §3.1). Funziona anche raggiungendo il backend via Tailscale o LAN.
2. **Avvisa appena le schede scadono**, non a un'ora fissa. Il controllo gira **ogni ora** con vincolo di rete: la notifica arriva entro circa un'ora dalla scadenza. Android può ritardarlo ancora in Doze, e va bene così.
3. **Su tutti i dispositivi, disattivabile per dispositivo.** Ogni dispositivo controlla per conto suo. Nelle Impostazioni c'è l'interruttore «Notifiche dei ripassi», attivo di default e salvato solo sul dispositivo.
4. **Una sola notifica, aggiornata sul posto.** Il testo è «Hai N schede da ripassare», con il tempo stimato da `estimated_minutes`. Avvisa (suono o vibrazione secondo il canale) solo quando `due_count` **cresce** rispetto all'ultimo valore notificato. Se il numero cala, la notifica si aggiorna in silenzio. A `due_count = 0` la notifica si toglie e il conteggio di riferimento torna a 0. Il conteggio di riferimento è salvato per utente.
5. **Il tocco apre la sessione di ripasso**, non la Home. Se nel frattempo le schede sono finite, la sessione mostra la sua fine come oggi.
6. **Permesso `POST_NOTIFICATIONS`** (Android 13+). L'app lo chiede quando si attiva l'interruttore e la prima volta che si vede la card del ripasso con schede in scadenza, mai all'avvio. Se il permesso è negato, l'interruttore lo dice e porta alle impostazioni di sistema. Il controllo non gira quando le notifiche sono spente o non permesse.
7. **Legate alla sessione.** Il lavoro parte solo per un utente con accesso valido. All'uscita (o a un `401`) si cancellano il lavoro e la notifica. Un errore di rete o del server non mostra niente e non conta come fallimento visibile.
8. **Un canale di notifica dedicato**, «Ripassi», a importanza predefinita, così da poterlo regolare dalle impostazioni di sistema.

## Alternative scartate

- **Push con Firebase Cloud Messaging**: più puntuale, ma richiede un progetto Google, le librerie Play Services nell'app, la registrazione dei token sul backend e un servizio esterno per un server che oggi è tutto in casa.
- **Una volta al giorno a un'ora scelta**: più semplice da regolare, ma il proprietario vuole l'avviso appena le schede sono pronte.
- **Lavoro unico programmato a `next_due_at`**: più preciso, ma le schede nascono anche su altri dispositivi (lezioni completate altrove) e la data cambia a ogni voto; serve comunque un controllo periodico, quindi da solo basta quello.

## Conseguenze

- Una richiesta leggera all'ora per dispositivo quando le notifiche sono attive.
- Con tre dispositivi dello stesso utente la notifica arriva su tutti: chi non la vuole la spegne su quel dispositivo.
- Nuova dipendenza Android: `androidx.work` (con l'integrazione Hilt per i worker).
