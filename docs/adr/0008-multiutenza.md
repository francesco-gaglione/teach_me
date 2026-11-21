# ADR 0008 — Multiutenza: utenti nello stesso database, proprietà su cartelle e argomenti

- Stato: accettata
- Decisioni del proprietario (2026-11-10): D5 utenti creati dal server, con il codice pronto per una registrazione futura; D6 nome utente e password; D7 una chiave LLM per utente; D8 nessuna condivisione, né ora né in futuro.
- Data: 2026-11-10
- Scelta del proprietario: più utenti nello **stesso database** (scartate: un'istanza per utente, un file SQLite per utente).

## Contesto

Oggi il backend ha un solo token statico (`TEACH_ME_API_TOKEN`), nessuna tabella utenti e nessuna query filtrata per proprietario (circa 178 query in 20 file). Due vincoli di unicità sono globali: il nome delle cartelle (`folders_name_key_uq`) e le preferenze (`preferences_dedupe_uq`). Le preferenze `scope = 'global'` valgono per tutti.

Il rischio principale di questa strada è una query dimenticata che mostra i dati di un altro utente. Il design sotto serve a renderlo difficile e a farlo scoprire dai test.

## Scelta

### 1. Chi possiede cosa

- Nuove tabelle `users` (`id`, `username`, `display_name`, `password_hash`, chiave LLM cifrata, `disabled_at`, `created_at`) e `sessions`.
- `user_id` solo su tre tabelle: **`folders`**, **`topics`** e **`preferences`**. Su `topics` è una copia di quello della cartella: l'argomento è la radice di quasi tutto e così il controllo non richiede un join.
- Tutte le altre tabelle arrivano al proprietario attraverso `topic_id` (chat, mission, job, fonti, lezioni, documenti, record, glossario, schede) o attraverso il loro genitore (`attempts` e `lesson_completions` dalla lezione, `review_logs` dalla scheda, pagine e sezioni dal documento). Non ricevono una colonna.
- Unicità per utente: `folders (user_id, name_key)`, `preferences (user_id, COALESCE(topic_id, ''), text_key)`; `position` delle cartelle calcolata per utente.

Scartato: `user_id` su ogni tabella. Sono più colonne da tenere coerenti e non toglie il bisogno di controllare l'accesso.

### 2. Dove si controlla l'accesso

- Il middleware risolve il token di sessione in un `UserId` e lo mette nelle estensioni della richiesta; gli handler lo passano ai casi d'uso. **Nessun caso d'uso chiamato da HTTP esiste senza `UserId`.**
- I repository che cercano per id (`topic`, `folder`, `lesson`, `document`, `card`, `preference`, `job`) ricevono `UserId` e rispondono "non trovato" se la risorsa è di un altro. Verso l'esterno è sempre **`404`**, mai `403`: non si rivela che l'id esiste.
- Le query che attraversano gli argomenti filtrano per utente: elenco cartelle, `GET /reviews/due` e i suoi conteggi, `GET /component-requests`, `GET /preferences`.
- Il flusso SSE `/topics/{id}/events` controlla il proprietario all'apertura.
- Il **job runner** non ha un utente: esegue job di chiunque e ricava il proprietario dall'argomento del job. Il Context Assembler carica le preferenze globali **del proprietario dell'argomento**: è il punto dove un errore finirebbe nel prompt di un altro utente, quindi ha un test dedicato.

### 3. Rete di sicurezza nei test

Una suite di integrazione "isolamento" con due utenti: per **ogni rotta** registrata nel router, l'utente B che usa gli id dell'utente A riceve `404` e gli elenchi di B non contengono nulla di A. La suite legge l'elenco delle rotte dal router: una rotta nuova senza il suo caso fa fallire il test.

### 4. Credenziali (D6): nome utente e password, sessioni opache

- Password salvate con **Argon2id** (parametri raccomandati correnti, da verificare sulla documentazione del crate), dietro un port `PasswordHasher` nel dominio.
- L'accesso crea una **sessione**: token opaco di 256 bit casuali, salvato come SHA-256 (ha abbastanza entropia da non richiedere un hash lento), con scadenza a 90 giorni di inattività. Il middleware cerca la sessione per hash e ne ricava l'utente.
- Protezioni: stessa risposta e stesso tempo per utente sconosciuto, password errata e utente disattivato; blocco temporaneo dopo 5 accessi falliti in 15 minuti per nome utente; il cambio password revoca le altre sessioni.
- Scartati: JWT (non servono token verificabili senza database e la revoca diventa complicata), token personale statico (scelta iniziale del tech lead, scartata dal proprietario).

### 5. Gestione degli utenti (D5): dal server, pronta per la registrazione

- Creare un utente è un caso d'uso dell'applicazione (`CreateUser`: regole su nome utente e password, unicità, hash). Oggi lo chiama solo il sottocomando `user add` del binario; una registrazione futura sarà un handler HTTP che chiama lo stesso caso d'uso, senza toccare dominio e persistenza.
- Nessun ruolo di amministratore e nessun endpoint di amministrazione: chi ha accesso al server gestisce gli utenti. Comandi: `users.md` §5.

### 6. Chiave LLM (D7): una per utente, cifrata

- Ogni utente salva la propria chiave (dall'app o dal server). Provider e modelli restano globali, in configurazione.
- La chiave è cifrata nel database con una chiave del server (`TEACH_ME_SECRET_KEY`, 32 byte) e un cifrario autenticato (AEAD con nonce casuale per valore), dietro un port `SecretCipher`. Motivo: i backup del database contengono le chiavi di altre persone. In memoria viaggia nel tipo `Secret` esistente, che non si stampa.
- La chiave non è mai restituita dalle API né mostrata dai comandi.
- Il port `LlmProvider` oggi riceve la chiave alla costruzione. Diventa per chiamata: i casi d'uso risolvono la chiave del **proprietario dell'argomento** e la passano al provider. Chi non ha una chiave riceve `llm_key_missing`; un 401/403 del provider diventa `llm_key_rejected`.
- Gli eval (`crates/evals`) continuano a usare la chiave dell'ambiente.

### 7. Migrazione dei dati esistenti

La migrazione crea l'utente iniziale `owner` con id fisso, senza password, e gli assegna cartelle, argomenti e preferenze esistenti. Il proprietario imposta la password con `user passwd owner`. Al primo avvio la chiave LLM in configurazione diventa la chiave di `owner`. `TEACH_ME_API_TOKEN` è ignorato con un avviso. Da verificare: SQLite non permette `ADD COLUMN ... NOT NULL REFERENCES` senza default, quindi le tre tabelle vanno ricostruite (come `resources` in `0013`).

## Conseguenze e rischi

- Cambia la firma di quasi tutti i casi d'uso e di molti repository: è il grosso del lavoro e tocca tutti i test del backend.
- `api-conventions.md` §2 cambia (accesso con nome utente e password); le forme delle risorse esistenti non cambiano, quindi resta `/api/v1`. Un'app vecchia riceve `401` ovunque e va aggiornata insieme al backend.
- L'app tiene dati locali legati all'utente (coda dei voti di ripasso non ancora inviati, cache): con un token di un altro utente vanno separati o svuotati, altrimenti i voti di A partirebbero con il token di B.
- Perdere `TEACH_ME_SECRET_KEY` rende illeggibili le chiavi LLM salvate: ogni utente deve reinserire la propria. Va conservata fuori dai backup del database.
- Nuove dipendenze crittografiche (Argon2id, AEAD, generatore casuale): crate della famiglia RustCrypto, da scegliere e verificare.
- Eliminare un utente cancella a cascata i suoi dati e deve rimuovere anche i file dei suoi documenti in `<data_dir>/uploads`.
- Nessuna condivisione tra utenti, per scelta definitiva (D8): nessuna tabella di accessi, nessun campo pensato per introdurla.
