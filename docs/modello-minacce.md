# Modello delle minacce

> Documento del tech lead, aggiornamento 2026-11-22 (audit M9). Non è un contratto: descrive cosa proteggiamo, da chi, da dove si entra e quali difese ci sono. Le decisioni di esposizione stanno nelle ADR: `0001-stack.md` (aggiornamenti 2026-11-11 e 2026-11-22), `0020-istanza-singola.md`.

## 1. Asset

- **Account e sessioni**: password con hash Argon2id; token di sessione salvati solo come SHA-256 (`backend/README.md`, «Users»).
- **Chiavi LLM degli utenti**: cifrate nel database con XChaCha20-Poly1305 sotto `TEACH_ME_SECRET_KEY` (`backend/README.md`, «Users»).
- **Contenuti di apprendimento e documenti**: lezioni, chat, note e testo estratto dai PDF, nel database e quindi nei backup.
- **Host del server**: il container, il volume `teach_me_data`, il file `.env`.

## 2. Attori

- Altri dispositivi sulla LAN di casa.
- Persone su internet, se l'host fosse raggiungibile fuori da Tailscale e dalla LAN.
- Altri utenti della stessa istanza (un utente non deve vedere i dati di un altro).
- Un documento o una pagina web malevoli che pilotano gli agenti LLM (prompt injection).
- Una dipendenza compromessa (crate, immagini base, PDFium).

## 3. Punti d'ingresso

- Indirizzo Tailscale del server (`BIND_IP` in `backend/deploy/docker-compose.yml`).
- Indirizzo LAN, opzionale, in HTTP in chiaro: attivo solo con `LAN_BIND_IP` diverso dal loopback e `ALLOW_PLAIN_LAN=1` (`0001-stack.md`, aggiornamento 2026-11-22).
- PDF caricati dall'utente (`documents.md`).
- Pagine web lette da Researcher e Generator (`resources.md`).
- Output dei modelli, reso come SDUI nelle lezioni (`sdui.md`).

## 4. Difese in vigore

- **Autenticazione e isolamento**: `401 unauthorized` senza token valido; una risorsa di un altro utente risponde `404 not_found` (`api-conventions.md` §2).
- **Login**: dopo 5 tentativi falliti in 15 minuti per lo stesso nome utente, `429 rate_limited` con `Retry-After` (`users.md` §2.1).
- **Segreti**: il server non parte senza `TEACH_ME_SECRET_KEY` valida (`backend/README.md`, «Users»).
- **Container**: utente di sistema non root (`USER teachme`); PDFium scaricato a versione e hash fissi (`backend/Dockerfile`, `backend/scripts/fetch-pdfium.sh`).
- **Limiti**: corpo massimo 1 MiB, timeout 30 s, storage per utente 1 GiB, token LLM giornalieri per utente (disattivati di default), stream SSE e job concorrenti (`api-conventions.md` §12).
- **Log**: niente password, token, chiavi, testi di utente o modello; test `backend/crates/api/tests/log_privacy.rs` (`backend/README.md`, «Logging e privacy»).
- **Lezioni generate**: il Validator SDUI rifiuta i blocchi non validi; un blocco sconosciuto mostra solo il fallback (`sdui.md` §1, §6).

## 5. Rischi residui accettati

- **LAN in chiaro**: password, token e contenuti viaggiano senza cifratura e sono leggibili da chi è su quella rete. Accettato per una rete domestica fidata, con opt-in esplicito (`0001-stack.md`, aggiornamento 2026-11-22).
- **Nessun TLS nativo** né CA locale: scartati per l'attrito sul tablet (`0001-stack.md`, aggiornamento 2026-11-22).
- **Istanza singola**: un solo backend per deployment; il limite di login è in memoria e si azzera al riavvio (`0020-istanza-singola.md`, `backend/README.md`).

## 6. Decisioni

### 6.1 Prompt injection (2026-11-22)

- Il contenuto proveniente da documenti, pagine web e risposte dell'utente è dato, non istruzione: i prompt lo delimitano e dichiarano che va trattato come dato.
- Gli agenti non hanno strumenti che agiscano fuori dai dati dell'utente stesso: nessuna email, nessuna shell, nessuna scrittura su altri utenti. Il fetch web è limitato dalle regole dell'ADR 0009 (`0009-web-fetch-generator.md`).
- I segreti (chiavi API) non entrano mai nel contesto del modello.
- L'output degli LLM è validato contro gli schemi SDUI e strutturati prima di essere salvato o mostrato (`sdui.md` §1, §6).
- Rischio residuo accettato: un documento malevolo può orientare la lezione che l'utente genera per sé. Il danno riguarda la qualità, non la riservatezza. L'impatto è limitato dalla fiducia single-user per istanza (`0020-istanza-singola.md`).

### 6.2 Dipendenze (2026-11-22)

- `.github/dependabot.yml`: aggiornamenti settimanali per cargo, gradle, github-actions e docker, con PR raggruppate.
- `Cargo.lock` e le versioni Gradle sono committati.
- PDFium è fissato a versione e sha256 in `backend/scripts/fetch-pdfium.sh`.
- Il modello TTS è fissato alla revisione HF.
- Non adottati per ora: cargo-deny e cargo-audit in CI (aggiungibili in seguito); gli avvisi di sicurezza di Dependabot coprono le vulnerabilità note.

## 7. Aperto

Nessun punto aperto al 2026-11-22.

