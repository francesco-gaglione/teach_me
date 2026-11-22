# Audit tecnico di `teach_me`

**Data audit:** 22 novembre 2026  
**Commit analizzato:** `d8a6607f6c953e9e24a176e9309608fe670ff17a`  
**Perimetro:** repository completo, con attenzione a backend Rust, app Android, deployment Docker, CI, persistenza, autenticazione, job asincroni e pipeline LLM.  
**Modifiche effettuate:** questo file è l’unico artefatto creato dall’audit. Il codice non è stato modificato.

## Executive summary

Il progetto è insolitamente ben documentato per un MVP: separazione a crate, contratti condivisi, migrazioni, test applicativi numerosi, CI per backend e Android, backup SQLite coerente e procedure di recovery esplicite. Non emerge un singolo difetto evidente che renda inutilizzabile l’applicazione.

Le aree che meritano priorità prima di considerarlo un servizio esposto a utenti reali sono però:

1. **Trasporto in LAN non cifrato:** il deployment pubblica esplicitamente HTTP plain sulla LAN quando `LAN_BIND_IP` non è loopback. Questo è coerente con la documentazione, ma espone sessioni, dati didattici e potenzialmente operazioni sulle chiavi LLM a intercettazione.
2. **Confine di sicurezza affidato molto alla configurazione di rete:** l’immagine ascolta su `0.0.0.0:8080` e la protezione TLS è demandata a Tailscale o al perimetro esterno. Un port mapping o firewall errato trasforma una scelta self-hosted in un endpoint non protetto.
3. **Qualità LLM non gate-ata dalla CI:** le eval sono dichiarate manuali e senza chiave reale. Le modifiche ai prompt possono quindi passare la CI anche se peggiorano citazioni, correttezza, lingua o qualità pedagogica.
4. **Test PDF non completamente rappresentativi:** parte della copertura viene saltata se PDFium non è disponibile e il test con un libro reale è `#[ignore]`. Un problema di integrazione PDFium o di documenti reali può restare invisibile.
5. **Deploy e migrazioni richiedono operazioni manuali di recovery:** la procedura è sensata e documentata, ma una migrazione non transazionale ha una finestra di failure nota; il rollback può richiedere il ripristino del DB e non è una transazione unica insieme al cambio immagine.
6. **Scalabilità operativa ancora da definire:** il backend usa SQLite e job locali, mentre l’app prevede upload, generazione LLM, SSE e TTS. Mancano garanzie evidenti di scalabilità orizzontale e sono presenti dipendenze da stato locale e volumi Docker.

Le prime due sono rischi di sicurezza **condizionati al deployment**, non vulnerabilità nascoste nel codice: il repository li dichiara apertamente. Le altre sono principalmente rischi di affidabilità, regressione e operatività.

---

## Evidenze positive

- **Struttura:** workspace Rust diviso in `domain`, `application`, `infrastructure`, `api` ed `evals`; l’app Android è modularizzata per core e feature.
- **Contratti:** `docs/contracts/` contiene convenzioni API, schema SDUI e fixture; il README dichiara il contratto come fonte condivisa fra backend e app.
- **CI:** `.github/workflows/ci.yml:30-32` esegue `cargo fmt`, Clippy con errori bloccanti e tutti i test backend; le righe `45-49` eseguono ktlint, lint, test unitari Android e build debug.
- **Hardening Rust:** `backend/Cargo.toml` vieta `unsafe_code` e applica lint Clippy pedantic a livello workspace.
- **Secret key:** il backend rifiuta l’avvio senza `TEACH_ME_SECRET_KEY`; il deployment verifica la presenza della variabile prima di costruire (`backend/deploy/deploy.sh:31-37`).
- **Backup:** `backend/deploy/backup.sh:27-48` usa SQLite online backup, integrity check, file temporaneo, gzip test e rename finale. La directory dei backup viene protetta con `umask 077` e permessi 700.
- **Recovery job:** `backend/crates/api/src/main.rs:140-168` recupera reply e job interrotti all’avvio, evitando di lasciare silenziosamente lavori in stato running dopo un crash.
- **Limiti HTTP:** `main.rs:192-197` costruisce limiti per body e request timeout, riducendo il rischio di richieste infinite o upload illimitati a livello HTTP.
- **Container:** `backend/Dockerfile:61` esegue come utente non root; PDFium viene scaricato con hash verificato alle righe `29-44`.
- **Stato iniziale:** 206 file Rust, 423 Kotlin e circa 193 file riconducibili a test. L’analisi ha usato anche il grafo esistente (`graphify-out/graph.json`, 21.586 nodi), senza rigenerarlo.

---

## Criticità e rischi prioritari

### C1 - HTTP plain sulla LAN può esporre credenziali e sessioni

**Gravità:** Alta se la porta LAN è esposta a una rete non completamente fidata; Media in una LAN domestica controllata.  
**Tipo:** rischio di deployment documentato, non bug occulto.  
**Evidenza:** `backend/deploy/docker-compose.yml:17-22`; `SECURITY.md:15-18`; `backend/README.md` sezione “Known limits” e deployment.

Il compose usa Tailscale come percorso sicuro, ma consente anche una seconda pubblicazione:

```yaml
- "${LAN_BIND_IP:-127.0.0.1}:${TEACH_ME_PORT:-8080}:8080"
```

Se `LAN_BIND_IP` viene impostato su un indirizzo raggiungibile dalla rete, il traffico è HTTP plain. Il servizio gestisce autenticazione, sessioni, contenuti personali e chiavi API LLM cifrate a riposo. La cifratura a riposo non protegge da un osservatore che intercetti il traffico in transito.

**Scenario:** un dispositivo compromesso o un utente sulla stessa rete osserva login, cookie/token, contenuti delle lezioni, risposte e chiamate di configurazione. L’impatto dipende dal meccanismo di sessione e dalla durata delle credenziali, ma il canale non offre confidenzialità.

**Raccomandazione:** rendere il bind LAN loopback-only il default anche in configurazioni di produzione, oppure aggiungere TLS reverse proxy come percorso ufficiale. Se l’HTTP LAN resta supportato, introdurre un warning di startup molto evidente e una modalità che rifiuti il bind non-loopback senza un’opzione esplicita.

### C2 - La superficie pubblica dipende da configurazione esterna non verificata dal servizio

**Gravità:** Alta in caso di errore operativo; Bassa nel default.  
**Tipo:** hardening/configuration risk.  
**Evidenza:** `backend/Dockerfile:63-70` imposta `TEACH_ME_BIND_ADDR=0.0.0.0:8080`; `docker-compose.yml:17-22` decide il perimetro; `SECURITY.md:17-18` dichiara l’assenza di TLS nativo.

Il processo ascolta su tutte le interfacce del container. La sicurezza viene quindi delegata interamente a Docker port mapping, firewall, Tailscale o reverse proxy. Questo è normale in molti deployment containerizzati, ma non c’è una barriera applicativa che impedisca di esporre direttamente il servizio.

**Raccomandazione:** documentare una matrice di deployment supportati e non supportati, aggiungere un controllo di startup/health che riporti il livello di esposizione, e fornire un compose “safe default” senza port binding LAN. Aggiungere test o smoke test che verifichino che l’endpoint non sia pubblicato oltre il previsto.

### C3 - Eval LLM fuori dalla definizione di done della CI

**Gravità:** Alta per la qualità del prodotto; Media per la sicurezza tecnica.  
**Tipo:** gap di processo e verifica.  
**Evidenza:** `backend/evals/README.md:1-3` dice che non esiste ancora un runner automatico e che non ci sono chiamate reali nei test; `backend/evals/README.md:26-39` descrive il processo manuale e una suite non eseguita.

Il codice può compilare e superare tutti i test mentre i prompt producono lezioni meno corrette, citazioni non supportate, output nella lingua sbagliata, criteri vaghi o regressioni pedagogiche. Il README definisce già un buon formato di suite, baseline e giudice, ma questi controlli non bloccano una PR.

**Raccomandazione:** implementare un runner riproducibile con provider mock per la CI e una pipeline separata per eval reali. Salvare baseline e `prompt_version`, confrontare punteggi e controlli deterministici, e rendere obbligatoria almeno una suite smoke per ogni ruolo prima del merge. Non mettere chiavi reali nei job CI pubblici.

### C4 - Copertura PDF non rappresentativa in CI/local default

**Gravità:** Media.  
**Tipo:** gap di test/integration coverage.  
**Evidenza:** `backend/crates/infrastructure/tests/pdf.rs:8-11` dichiara che senza PDFium i test vengono saltati; `:20-34` restituisce `None` e considera il test “skipped”; `:327-335` contiene un test `#[ignore]` che richiede un PDF reale.

La CI può risultare verde senza aver esercitato il binding della libreria PDFium. Il test su un “real book” è manuale e ignorato. Questo lascia scoperti incompatibilità della shared library, PDF grandi, font/layout reali, memory pressure e differenze fra immagine Docker e ambiente di sviluppo.

**Raccomandazione:** eseguire almeno una job CI containerizzata con PDFium pinned e un fixture PDF reale ma piccolo, distinguendo chiaramente test skipped da pass. Mantenere il test di volume reale in una pipeline nightly o manuale obbligatoria per release.

### C5 - Migrazioni e rollback non sono atomicamente accoppiati al deploy

**Gravità:** Media-Alta per disponibilità e operatività.  
**Tipo:** failure mode documentato.  
**Evidenza:** `backend/README.md:40` documenta la migration `0014` `-- no-transaction` e la finestra fra `COMMIT` e registrazione in `_sqlx_migrations`; `backend/deploy/deploy.sh:51-87` esegue build/start, health check e rollback; `backend/deploy/backup.sh:54-73` ripristina il DB con stop e replace.

Il progetto riconosce il problema, ma una terminazione nel punto indicato può lasciare il DB in uno stato che richiede ripristino manuale. Inoltre il rollback è composto da più operazioni: fermare il servizio, sostituire il DB, riavviare con immagine precedente e verificare health. Se il restore fallisce (`deploy.sh:77`), lo script stampa l’errore e prosegue verso la verifica, quindi l’operatore deve intervenire.

**Raccomandazione:** aggiungere un preflight di compatibilità schema/versione, un registro di deploy, verifica esplicita che il restore sia riuscito prima di considerare il rollback concluso e un runbook di recovery testato periodicamente. Per migrazioni complesse, preferire expand/contract compatibili con entrambe le versioni quando possibile.

### C6 - Stato locale limita la scalabilità e aumenta il rischio di deploy multiistanza

**Gravità:** Media.  
**Tipo:** architettura/operabilità.  
**Evidenza:** `backend/deploy/docker-compose.yml:23-27,70-75` usa volumi locali per DB, audio e modelli; `backend/crates/api/src/main.rs:151-190` avvia runner, audio worker e optimizer nel processo; `backend/README.md:178-184` descrive SQLite e backup locali.

La soluzione è adeguata per self-hosting singolo, ma non per più repliche senza una decisione esplicita: SQLite, coda job, SSE/eventi, cache audio e scheduler sono legati all’istanza/volume. Una seconda replica potrebbe non vedere job, eventi o file audio dell’altra e potrebbe duplicare lavori.

**Raccomandazione:** dichiarare ufficialmente “single instance” come vincolo MVP e farlo rispettare nella documentazione/deploy. Se serve scalare, separare coda e scheduler, centralizzare file/object storage e valutare PostgreSQL, locking distribuito e broker eventi.

### C7 - Deploy da working tree con modifiche non committate è consentito

**Gravità:** Media, soprattutto in produzione.  
**Tipo:** rischio supply/process.  
**Evidenza:** `backend/deploy/push-deploy.sh:9-10,20-28` avverte che le modifiche non committate vengono incluse e usa `rsync --delete`.

La scelta è esplicita e utile in sviluppo, ma rende possibile pubblicare codice non revisionato o diverso da un commit identificabile. Il warning non blocca l’operazione. `--delete` aumenta inoltre l’impatto di un path remoto errato, anche se il target è parametrizzato.

**Raccomandazione:** aggiungere una modalità production che richieda clean tree e commit/tag esplicito; stampare commit sorgente e hash immagine nel deploy. Lasciare la modalità dirty solo dietro flag `--allow-dirty`.

### C8 - Mancano limiti operativi evidenti per risorse non HTTP

**Gravità:** Media, variabile in base all’esposizione.  
**Tipo:** disponibilità/configurazione.  
**Evidenza:** `docker-compose.yml:39-56` imposta `mem_limit` per TTS, ma non per `teach-me`; `Dockerfile:73-74` controlla solo health HTTP. Il body limit e il timeout applicativi coprono la richiesta, non necessariamente costi LLM, concorrenza job, spazio disco SQLite/audio o numero di eventi SSE.

Upload, PDF extraction, generazione LLM e cache audio possono consumare CPU, memoria, spazio e denaro. La presenza di un limite body non equivale a un budget per job o provider.

**Raccomandazione:** introdurre quote per utente e per job, limiti di concorrenza, timeout per provider, budget token/costo, quota disco e metriche/alert su coda, errori, latenza, spazio e memory pressure. Definire cosa succede quando un limite viene superato.

---

## Aspetti migliorabili, non necessariamente critici

### M1 - Health check più informativo

`backend/crates/api/src/health.rs:26-40` espone liveness pubblica e health applicativo con `status: "ok"`, versione e timestamp. È semplice e adatto al compose, ma non distingue liveness, readiness, database disponibile, coda attiva e dipendenze opzionali. Conviene mantenere `/health` leggero e aggiungere `/ready` autenticato o interno, con stato delle dipendenze senza rivelare dettagli sensibili.

### M2 - Osservabilità con privacy esplicita

`backend/crates/api/src/telemetry.rs:37-83` supporta OTLP e JSON logs. È una buona base, ma il report operativo dovrebbe specificare quali campi non devono mai entrare in span/log: prompt completi, risposte, URL privati, token, dati delle lezioni e identificativi eccessivamente granulari. Aggiungere test o redaction centralizzata per i campi sensibili.

### M3 - Test di contratto cross-client più automatici

Le fixture in `docs/contracts/fixtures/` e i test presenti sono un punto forte, ma il repository dovrebbe rendere evidente che ogni fixture viene testata sia dal parser Android sia dall’API/validator Rust. Un test di compatibilità generato dai contratti ridurrebbe il rischio che backend e renderer divergano.

### M4 - Test Android di lifecycle e rete reale simulata

Sono presenti test repository, ViewModel e alcuni `androidTest`, ma le superfici più delicate sono sessione, retry, WorkManager, SSE e rotazione/process death. Aumentare i test di: perdita rete durante upload, logout mentre esiste uno stream, processo terminato con coda locale non sincronizzata, cambio utente sullo stesso device e risposta server incompatibile.

### M5 - Ridurre i moduli molto grandi

Alcuni file sono molto estesi: `backend/crates/application/src/lesson_tests.rs` circa 3552 righe, `infrastructure/src/lessons.rs` circa 2872, `android/.../LessonViewModel.kt` circa 1439 e `LessonScreen.kt` circa 1184. Non è un bug, ma aumenta il costo di revisione e la probabilità di coupling accidentale. Separare per use case, stato, mapping e rendering, mantenendo test vicini al comportamento.

### M6 - PDF skipped deve essere visibile come copertura incompleta

Il comportamento “skip se PDFium non c’è” è utile per sviluppare, ma una pipeline dovrebbe pubblicare il conteggio dei test saltati e fallire o almeno warnare in modo esplicito se la suite di integrazione PDF non è stata eseguita. Il verde attuale non deve essere interpretato come copertura completa.

### M7 - Gestione modello TTS già presente ma migliorabile

`backend/deploy/tts/entrypoint.sh:46-65` usa staging, verifica dimensioni e rename atomico, che è positivo. Tuttavia `:68-76` accetta un modello già presente con revision diversa e si limita a un warning. In produzione questo può generare output non riproducibile dopo un cambio immagine. Meglio rifiutare la partenza o isolare la cache per revision, con una policy esplicita di migrazione.

### M8 - Reproducibilità della release Android

`android/app/build.gradle.kts` avvisa che senza `keystore.properties` l’APK release è unsigned. Questo è corretto per sviluppo, ma la pipeline di release dovrebbe avere un job separato che verifichi firma, manifest, shrinker e provenienza degli artefatti. La CI attuale costruisce debug, non dimostra una release installabile e firmata.

### M9 - Documentare il modello di supporto

`SECURITY.md:11-13` supporta solo l’ultima `main`, mentre il README presenta il prodotto come self-hosted. Sarebbe utile definire esplicitamente compatibilità fra versione DB, versione app e versione server, procedura di downgrade e durata del supporto. Questo è importante perché le migrazioni possono rendere il rollback non banale.

---

## Piano d’azione consigliato

### Prima di un uso multiutente reale

1. Chiudere il percorso di rete: Tailscale-only oppure reverse proxy TLS ufficiale; rendere non esponibile la LAN plain per default.
2. Aggiungere quote e limiti per upload, job, LLM, audio, disco e concorrenza.
3. Automatizzare almeno una suite eval smoke e introdurre baseline di qualità per prompt.
4. Eseguire PDFium in CI con fixture reale e rendere i test skipped visibili come incomplete coverage.
5. Testare deploy fallito, migration interrotta, restore e rollback su una copia del volume.

### Prima di una seconda istanza o uso pubblico

1. Dichiarare e far rispettare il vincolo single-instance, oppure progettare coda, database, storage ed eventi distribuiti.
2. Aggiungere readiness, metriche e alert su job, provider, SSE, spazio e failure recovery.
3. Rendere il deploy production riproducibile da commit/tag, vietando working tree dirty salvo flag esplicito.
4. Definire threat model, retention e redaction per telemetria e log.

### Miglioramenti incrementali

1. Spezzare i moduli più grandi.
2. Aumentare test Android su process death, multiutente e rete intermittente.
3. Creare contract tests cross-backend/Android.
4. Rendere la release Android firmata un artefatto verificato da CI.
5. Rendere incompatibilità del modello TTS un errore esplicito o usare cache versionate.

---

## Verifiche eseguite durante l’audit

- Inventario repository e stato Git iniziale.
- Lettura di README, istruzioni del progetto, CI, Dockerfile, compose, deploy, backup, configurazione TTS, policy sicurezza, eval README e sorgenti health/telemetry/startup.
- Consultazione del grafo esistente `graphify-out/graph.json` senza rigenerazione.
- Verifica sintattica `bash -n` sui cinque script di deployment/backup/TTS: superata.
- Verifica `cargo fmt` tentata dalla root del repository: il comando non è applicabile dalla root perché `Cargo.toml` si trova in `backend/`; non ho rilanciato una build o un test che potesse modificare artefatti locali.
- Non sono state eseguite chiamate LLM reali per le eval e non sono state alterate configurazioni o sorgenti.

## Limiti dell’audit

Questo è un audit statico e orientato al rischio. Non sostituisce penetration test, test di carico, esecuzione end-to-end con provider LLM reale, verifica su dispositivo Android fisico o prova di restore su infrastruttura isolata. Le criticità di rete sono state valutate come rischi di configurazione documentati, non come prova che l’istanza corrente sia effettivamente esposta.
