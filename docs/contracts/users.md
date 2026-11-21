# Utenti, accesso e chiave LLM

> Contratto condiviso tra backend e app. Proprietario: tech lead.
> Versione: **v1.2** (2026-11-13: campo `tts_voice` e `PUT /me/tts-voice`, `audio.md` §4.2; v1.1 2026-11-11: nuovo errore `llm_credit_exhausted`, §3; v1 2026-11-10). Design: ADR 0008. Regole generali: `api-conventions.md` (v1.2).

## 1. Risorsa `User`

```json
{
  "id": "01933b4e-8f2a-7c3d-9e1f-2a3b4c5d6e7f",
  "username": "valentina",
  "display_name": "Valentina",
  "has_llm_key": true,
  "llm_provider": "openrouter",
  "tts_voice": "vivian",
  "created_at": "2026-11-10T09:12:44.031Z"
}
```

- `username`: 3-32 caratteri tra `a-z`, `0-9`, `.`, `_`, `-`. Il backend lo confronta e lo salva in minuscolo; è unico.
- `display_name`: 1-60 caratteri, per l'interfaccia.
- `has_llm_key`: dice solo se l'utente ha salvato una chiave. **La chiave non esce mai dal backend**, né intera né in parte.
- `llm_provider`: il servizio configurato sul server, cioè quello di cui l'utente deve portare la chiave: `"anthropic"` oppure `"openrouter"`. È uguale per tutti gli utenti; l'app tollera valori sconosciuti e in quel caso dice "il servizio LLM".
- `tts_voice` (v1.2): la voce di lettura ad alta voce effettiva dell'utente, id opaco, oppure `null` se la funzione è spenta sul server. Regole e modifica: `audio.md` §4.2.
- Password: 10-128 caratteri, nessun'altra regola di composizione. Mai restituita, mai nei log.

## 2. Endpoint

| Metodo | Path | Auth | Successo |
|---|---|---|---|
| POST | `/auth/login` | no | `200` con `Session` |
| POST | `/auth/logout` | sì | `204`, la sessione usata per la chiamata smette di valere |
| GET | `/me` | sì | `200` con `User` |
| PUT | `/me/password` | sì | `204` |
| PUT | `/me/llm-key` | sì | `200` con `User` (`has_llm_key: true`) |
| DELETE | `/me/llm-key` | sì | `204` |
| PUT | `/me/tts-voice` | sì | `200` con `User` (`audio.md` §4.2, v1.2) |

Non esiste un endpoint di registrazione: gli utenti li crea il proprietario dal server (§5). `POST /auth/register` è riservato a una versione futura.

### 2.1 `POST /auth/login`

```json
{ "username": "valentina", "password": "…", "device_name": "Tablet di Valentina" }
```

`device_name` è opzionale (massimo 60 caratteri), serve a riconoscere la sessione nell'elenco sul server.

```json
{
  "token": "tm_3q2-…",
  "expires_at": "2027-02-08T09:12:44.031Z",
  "user": { "…": "User" }
}
```

- `token`: opaco, da inviare come `Authorization: Bearer <token>`. È mostrato solo in questa risposta.
- Nome utente sconosciuto, password errata e utente disattivato rispondono allo stesso modo: **`401 invalid_credentials`**. Il backend impiega lo stesso tempo nei tre casi.
- Dopo 5 tentativi falliti in 15 minuti sullo stesso nome utente: **`429 rate_limited`** con header `Retry-After` (secondi), anche se la password successiva è giusta.
- Campi mancanti o di forma sbagliata: `422 validation_failed`.

### 2.2 Sessioni

- Una sessione scade dopo **90 giorni senza richieste**; ogni richiesta autenticata la rinnova. `expires_at` è la scadenza al momento dell'accesso, informativa.
- Sessione scaduta, revocata o di un utente disattivato: `401 unauthorized` su ogni endpoint. L'app torna alla schermata di accesso.
- Più sessioni per utente sono ammesse (più dispositivi).

### 2.3 `PUT /me/password`

```json
{ "current_password": "…", "new_password": "…" }
```

- Password attuale errata: `422 validation_failed` con `details: [{"field": "current_password", "issue": "wrong"}]` (non `401`: la sessione è valida).
- Nuova password fuori dalle regole: `422` con `field: "new_password"`, `issue: "too_short"` o `"too_long"`.
- A buon fine, **tutte le altre sessioni** dell'utente sono revocate; quella della chiamata resta valida.

### 2.4 `PUT /me/llm-key`

```json
{ "api_key": "sk-or-…" }
```

- 1-500 caratteri dopo aver tolto gli spazi ai bordi; altrimenti `422` con `field: "api_key"`.
- Il backend non verifica la chiave presso il provider al salvataggio: un errore emerge al primo uso (§3).
- Provider e modelli restano quelli della configurazione del server; ogni utente porta solo la propria chiave.

## 3. Operazioni che usano l'LLM

Messaggi al Tutor, ricerca delle fonti, generazione e chiusura di una lezione, valutazione delle risposte aperte usano **la chiave dell'utente proprietario dell'argomento**.

| Situazione | Richiesta sincrona | Job o risposta in streaming |
|---|---|---|
| L'utente non ha una chiave | `409 llm_key_missing`, nessun job o messaggio creato | `error_code: "llm_key_missing"` se la chiave è stata tolta dopo l'avvio |
| Il provider rifiuta la chiave (401/403 a monte) | `502 llm_key_rejected` | `error_code: "llm_key_rejected"` |
| Il provider accetta la chiave ma il credito è esaurito (402 a monte, o errore di fatturazione a stream aperto) | `502 llm_credit_exhausted` | `error_code: "llm_credit_exhausted"` |

L'app, per `llm_key_missing` e `llm_key_rejected`, rimanda alle Impostazioni per inserire o correggere la chiave; per `llm_credit_exhausted` no (vedi «Credito esaurito» più sotto).

Precisazioni:

- Il `409 llm_key_missing` vale per le richieste il cui scopo è un'operazione LLM: invio di un messaggio, avvio o aggiornamento della ricerca, generazione di una lezione, chiusura di una lezione, nuovo tentativo di un job o di un messaggio.
- **La conferma della mission riesce sempre**: la mission resta confermata e il job di ricerca che ne nasce fallisce con `error_code: "llm_key_missing"`. Dopo aver salvato la chiave l'utente lo fa ripartire con il retry del job.
- **Risposte aperte**: l'invio dei tentativi non fallisce mai per la chiave. Il tentativo resta in attesa di valutazione come per ogni altro errore del Grader (`lessons.md`); l'app spiega il motivo usando `has_llm_key` e l'ultimo `llm_key_rejected` visto.

## 4. Isolamento

- Ogni risorsa appartiene a un utente (ADR 0008 §1). Tutti gli elenchi restituiscono solo le risorse dell'utente della sessione.
- Una risorsa di un altro utente risponde **`404 not_found`**, identico a una risorsa inesistente. Mai `403`.
- Le unicità documentate negli altri contratti (nome della cartella, preferenze) valgono **per utente**.
- Non esiste alcuna forma di condivisione tra utenti.

## 5. Gestione dal server

Sottocomandi del binario del backend, eseguiti sul server (`docker compose exec`). Non sono API.

| Comando | Effetto |
|---|---|
| `user add <username> [--display-name <nome>]` | crea l'utente; la password si legge da stdin oppure, se assente, se ne genera una temporanea stampata una sola volta |
| `user list` | nome utente, nome, stato, chiave LLM presente o no, ultimo accesso |
| `user passwd <username>` | imposta una nuova password e revoca tutte le sessioni |
| `user rename <username> <nuovo>` | cambia il nome utente |
| `user set-llm-key <username>` | legge la chiave da stdin; non la stampa |
| `user disable <username>` / `user enable <username>` | blocca o riattiva l'accesso; disattivare revoca le sessioni |
| `user delete <username>` | elimina l'utente e tutti i suoi dati, compresi i file caricati; chiede conferma |

## 6. Passaggio dalla versione a utente singolo

- I dati esistenti passano all'utente iniziale `owner`, creato dalla migrazione **senza password**: finché il proprietario non esegue `user passwd owner` nessuno può accedere.
- Al primo avvio, se `owner` non ha una chiave LLM e la configurazione del server ne contiene una, il backend la salva come chiave di `owner`. Dopo, la chiave in configurazione non è più usata dalle API.
- `TEACH_ME_API_TOKEN` non è più usato: se presente viene ignorato con un avviso nei log.
- Nuova variabile obbligatoria **`TEACH_ME_SECRET_KEY`** (32 byte in base64): cifra le chiavi LLM nel database. Senza, il backend non parte.

### Credito esaurito (v1.1)

- `llm_credit_exhausted` **non** riguarda la validità della chiave: la chiave resta quella, `has_llm_key` resta `true` e l'app non la segna come rifiutata né invita a sostituirla. Il `message` segue `Accept-Language` come gli altri errori API (`api-conventions.md` §7); testo italiano: «Il credito del servizio LLM è esaurito: ricaricalo presso il provider e riprova.»
- A differenza di `llm_key_rejected`, **«Riprova» resta disponibile**: la ricarica avviene fuori dall'app e non c'è nulla da salvare prima di riprovare.
- Fino alla v1 un credito esaurito arrivava come `llm_key_rejected`. Un'app che non conosce il codice nuovo mostra il `message` (regola dei codici sconosciuti, `jobs.md` e `api-conventions.md` §7).
- Un 403 del provider resta `llm_key_rejected` (chiave senza permessi, limiti di spesa della chiave, moderazione): il backend non lo distingue.
- Risposte aperte: come per gli altri errori del Grader, il tentativo resta in attesa di valutazione.
