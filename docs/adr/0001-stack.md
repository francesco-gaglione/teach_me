# ADR 0001 — Stack tecnologico

- Stato: accettata
- Data: 2026-11-08

## Contesto

teach_me è usato da una sola persona: un backend sull'homelab e un'app Android, collegati via Tailscale. I requisiti sono in `docs/mvp.md`: clean architecture con provider LLM intercambiabili, lezioni come Server-Driven UI resa con componenti nativi, streaming SSE, job asincroni, ripasso FSRS. Il proprietario conosce Rust e Kotlin.

## Decisione

### Backend

| Area | Scelta |
|---|---|
| Linguaggio | Rust stable (edition 2024) |
| Runtime async | tokio |
| HTTP e SSE | axum + tower-http (trace, request id, limite del corpo, timeout) |
| Persistenza | SQLite via sqlx, migrazioni SQL versionate in `backend/migrations/` eseguite all'avvio (`sqlx::migrate!`), modalità WAL |
| Serializzazione | serde + serde_json |
| Errori | `thiserror` in domain, application e infrastructure; `anyhow` solo nel `main` |
| Logging | `tracing` + `tracing-subscriber` (testo in sviluppo, JSON in produzione, filtro con `RUST_LOG`) |
| Config | file TOML opzionale + variabili d'ambiente con prefisso `TEACH_ME_` (le variabili vincono), `toml` + serde con override delle variabili d'ambiente scritto a mano (figment scartato: converte in numero un token di sole cifre) |
| Id e tempo | `uuid` v7, `time` o `chrono` in UTC, dietro i port `IdGenerator` e `Clock` |
| HTTP client (adapter) | reqwest con rustls |
| Test | test unitari nel crate, test di integrazione delle API con `tower::ServiceExt::oneshot` su SQLite in memoria |

### App Android

| Area | Scelta |
|---|---|
| Linguaggio e UI | Kotlin, Jetpack Compose. **Design system custom** (non Material 3 / Material You): tema e componenti propri in `core:designsystem` costruiti su `androidx.compose.foundation`, specificati in `docs/design/` |
| Build | Gradle Kotlin DSL, version catalog (`gradle/libs.versions.toml`), convention plugin in `build-logic/` |
| SDK | `minSdk 29`, `targetSdk 35`, `compileSdk 37` (richiesto dalle versioni stabili di Compose, Lifecycle e OkHttp); JDK 21 per la build |
| Architettura | UDF/MVVM: `ViewModel` + `StateFlow` di stato immutabile, eventi come funzioni |
| DI | Hilt (KSP) |
| Rete | Ktor client (OkHttp engine) per HTTP e SSE, kotlinx.serialization |
| Navigazione | Navigation Compose con route type-safe (`@Serializable`) |
| Persistenza locale | DataStore (impostazioni: URL e token) |
| Test | JUnit, Turbine per i Flow, MockEngine di Ktor, Compose UI test dove serve |

Versioni precise: le ultime stabili al momento dello scaffold, fissate nei file di lock / version catalog.

## Alternative considerate

- **Postgres** invece di SQLite: inutile per un solo utente; il port `*Repository` lascia aperta la migrazione.
- **Koin** invece di Hilt: più semplice, ma errori di DI solo a runtime. Con molti moduli preferiamo la verifica a compile time.
- **Retrofit** invece di Ktor: Ktor gestisce meglio SSE e la serializzazione polimorfica con kotlinx.serialization, che serve al renderer SDUI.
- **Material 3 come linguaggio visivo**: scartato dal proprietario, che vuole un'identità visiva propria. Costo: componenti base da costruire e testare (accessibilità inclusa) invece di averli pronti.
- **WebView per le lezioni**: escluso dai requisiti (UI solo nativa, vedi `docs/mvp.md` §6).

## Conseguenze

- L'app accetta URL `http://`: il traffico passa da Tailscale, che lo cifra già.
- Un solo binario Rust e un file SQLite: deploy e backup semplici.
- Il token dell'app è salvato in DataStore non cifrato: accettabile perché l'accesso è solo via Tailscale e l'utente è uno. Da rivedere se il backend venisse esposto.
- La build Android richiede JDK 21 (`JAVA_HOME` o `org.gradle.java.home`), non il JDK di sistema se più recente.

## Aggiornamento 2026-11-11 — accesso dalla LAN di casa

Decisione del proprietario: il backend si può pubblicare anche sull'indirizzo LAN dell'host, per i dispositivi di casa senza Tailscale (gli utenti sono ormai più d'uno, ADR 0008).

- `backend/deploy/docker-compose.yml` pubblica la porta su `BIND_IP` (Tailscale) e su `LAN_BIND_IP`; se `LAN_BIND_IP` manca vale `127.0.0.1` e nulla di nuovo è esposto.
- **Sulla LAN non c'è cifratura**: password, token di sessione e contenuti viaggiano in HTTP in chiaro e sono leggibili da chi è su quella rete. Rischio accettato per una rete domestica fidata. Le chiavi LLM non escono mai dal backend, quindi non passano sulla LAN se non quando un utente le salva dalle Impostazioni.
- L'app ha un solo indirizzo del server: con quello della LAN funziona solo a casa, con quello Tailscale ovunque ma solo con Tailscale attivo.
- Resta valido il resto: il backend non va esposto a Internet. Se servisse, prima TLS e revisione del token in DataStore.
