# teach_me backend

Rust workspace (clean architecture): `domain` <- `application` <- `api`, with `infrastructure` implementing the domain ports. See `docs/adr/0002-backend-crate-structure.md`.

## Run in development

```bash
cd backend
export TEACH_ME_SECRET_KEY="$(openssl rand -base64 32)"   # required: encrypts the users' LLM keys
export TEACH_ME_BIND_ADDR=127.0.0.1:8080              # default
export TEACH_ME_DATABASE_URL=sqlite://teach_me.db     # default, created if missing
export RUST_LOG=info,tower_http=debug
cargo run -p api --bin teach-me
```

Reaching the backend from the phone over Tailscale: the default `127.0.0.1` only accepts local connections. Set `TEACH_ME_BIND_ADDR` to the machine's Tailscale IP (e.g. `100.x.y.z:8080`) or to `0.0.0.0:8080`. Only use `0.0.0.0` if the host is not otherwise reachable from untrusted networks.

Full example for the phone:

```bash
TEACH_ME_SECRET_KEY="$(cat ~/.teach_me_secret_key)" \
TEACH_ME_BIND_ADDR="$(tailscale ip -4):8080" \
TEACH_ME_DATABASE_URL=sqlite://$HOME/teach_me/teach_me.db \
TEACH_ME_LOG_FORMAT=text \
cargo run --release -p api --bin teach-me
```

Configuration comes from an optional TOML file (`TEACH_ME_CONFIG`, or `config/teach_me.toml` if present; see `config/teach_me.example.toml`). `TEACH_ME_*` variables win over the file. Other variables: `TEACH_ME_DATA_DIR`, `TEACH_ME_PDFIUM_DIR` (see PDF documents below), `TEACH_ME_LOG_FORMAT` (`text`|`json`), `TEACH_ME_BODY_LIMIT_BYTES`, `TEACH_ME_REQUEST_TIMEOUT_SECS`. The server refuses to start without a valid `TEACH_ME_SECRET_KEY` (32 bytes in base64) and says why; see Users below. `TEACH_ME_API_TOKEN` is no longer used (ignored, with a warning).

```bash
curl -s localhost:8080/health
# Sign in (the user is created first, see Users) and keep the session token:
TOKEN=$(curl -s -H 'Content-Type: application/json' \
  -d '{"username":"owner","password":"..."}' localhost:8080/api/v1/auth/login | jq -r .token)
curl -s -H "Authorization: Bearer $TOKEN" localhost:8080/api/v1/health
```

Migrations in `migrations/` run at startup (never edit an applied one; add a new file).

Migration `0014` is a `-- no-transaction` file (it must switch the foreign keys off to rebuild the parent tables). Its own `BEGIN`/`COMMIT` makes the changes atomic, but sqlx records the migration in `_sqlx_migrations` only afterwards: if the process dies exactly between the `COMMIT` and that record (a window of milliseconds), the next start tries to apply `0014` again and stops with an error such as `table users already exists`. Remedy, with the service stopped: restore the pre-deploy backup (`./backup.sh restore ...`; `deploy.sh` takes one before every deploy) and deploy again. Do not insert the `_sqlx_migrations` row by hand: its checksum must match the file.

### Chat with the Tutor (phase 2)

The Tutor, the Researcher, the Generator and the Grader use the LLM key **of the user who owns the topic**: each user saves theirs from the app (`PUT /me/llm-key`) or the operator does with `teach-me user set-llm-key`. Never put a key in a versioned file. Without one, what needs the model answers `409 llm_key_missing` and creates nothing; a key the provider refuses (401/403) gives `llm_key_rejected`, and an account out of credit (402, or a billing error on an open stream) gives `llm_credit_exhausted`, not retried automatically (contract `docs/contracts/users.md` §3). If `TEACH_ME_ANTHROPIC_API_KEY` (fallback `ANTHROPIC_API_KEY`) or, with OpenRouter, `TEACH_ME_OPENROUTER_API_KEY` is set, at first start it becomes the key of the user `owner` if `owner` has none; afterwards the server does not use it. Models, effort and timeouts per role are configured in the `[llm]` section (see `config/teach_me.example.toml`); the defaults are compiled in. The Generator can read the web sources of a lesson with the provider's web fetch tool: `[llm.roles.generator] max_fetches` (0 = off, the default, until the real test of `docs/adr/0009-web-fetch-generator.md` passes; through OpenRouter it also needs `server_web_fetch = true` under the provider). `ANTHROPIC_BASE_URL` is ignored on purpose: use `TEACH_ME_ANTHROPIC_BASE_URL` for a different endpoint. To use OpenRouter instead (same adapter, its Anthropic-compatible endpoint): `TEACH_ME_LLM_PROVIDER=openrouter` and `TEACH_ME_OPENROUTER_API_KEY`; model ids are mapped to OpenRouter slugs (`claude-haiku-5-5` → `anthropic/claude-haiku-5.5`). The prompts in `prompts/` are embedded at build time, so edit then rebuild. Contract: `docs/contracts/chat.md`.

```bash
T=<topic id>
curl -s -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"text":"Voglio imparare Rust"}' localhost:8080/api/v1/topics/$T/messages
curl -N -H "Authorization: Bearer $TOKEN" localhost:8080/api/v1/topics/$T/events
```

Mission and preferences (contract `docs/contracts/mission.md`): `GET /topics/{id}/mission`, `GET /topics/{id}/mission/versions`, `POST /topics/{id}/mission/confirm` (`{"version": N}`), `GET /preferences[?topic_id=]`, `DELETE /preferences/{id}`. Drafts and preferences are created by the Tutor's tools (loop capped at 4 tool rounds).

API (all under `/api/v1`, contract in `docs/contracts/folders-topics.md`): `GET|POST /folders`, `GET|PATCH|DELETE /folders/{id}`, `GET|POST /folders/{id}/topics`, `GET|PATCH|DELETE /topics/{id}`. Deleting a folder deletes its topics (SQLite `ON DELETE CASCADE`; `PRAGMA foreign_keys` is enabled on every pooled connection). List endpoints use opaque cursors (`?limit=&cursor=`).

```bash
curl -s -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"Lavoro"}' localhost:8080/api/v1/folders
```

### PDF documents (phase 10)

The user can upload up to 5 PDFs per topic (`POST /api/v1/topics/{id}/documents`, multipart, one `file` field). Lessons then follow the chosen sections and cite their pages, next to the web sources. Contract: `docs/contracts/documents.md`.

- **Limits.** 50 MB per file, on this route only (every other route keeps `TEACH_ME_BODY_LIMIT_BYTES`, and the upload has no request timeout). More than 1,500 pages, password-protected files and scans without selectable text (fewer than 200 characters per page on average, or more than half of the pages empty) are refused by the import job with `too_many_pages`, `encrypted_pdf` and `no_text`. There is no OCR.
- **The original file is not kept.** The upload is streamed to `<data dir>/uploads/<id>.pdf` (never held in memory), read once by the `document_import` job, and deleted as soon as the import succeeds, or when the document is deleted. A failed import keeps the file so that a retry can run; a partial upload left by a crash is removed at startup. What is stored (and backed up with the database) is the text of each page and the sections built from the PDF's bookmarks (or blocks of 10 pages).
- **PDF engine.** Text is extracted with PDFium (`pdfium-render`), a shared library loaded at run time: the Docker image contains it (`/usr/local/lib/pdfium`, pinned and checked by hash). In development download `pdfium-linux-x64.tgz` from <https://github.com/bblanchon/pdfium-binaries/releases>, unpack it and set `TEACH_ME_PDFIUM_DIR` to its `lib/` directory (or install `libpdfium.so` in the library path). Without it the backend still starts, with a warning, and imports fail with `internal`; the PDF tests of `infrastructure` are skipped. Why PDFium: `docs/adr/0007-pdf-extraction.md`.
- **Settings.** `TEACH_ME_DATA_DIR` (default: the directory of the database file) and `TEACH_ME_PDFIUM_DIR`.

```bash
curl -s -H "Authorization: Bearer $TOKEN" -F file=@book.pdf localhost:8080/api/v1/topics/$T/documents
```

To try the extractor on a real book: `TEACH_ME_PDFIUM_DIR=… TEACH_ME_TEST_PDF=book.pdf cargo test -p infrastructure --test pdf -- --ignored --nocapture`.

### Read aloud (issue #8, stages 1-2b)

ADR 0013 / `docs/contracts/audio.md`.

- `domain::readable_blocks` gives the readable blocks of a lesson document with the text the voice speaks (heading, paragraph, callout, diagram `alt`; markdown cleaned, `[[key]]` read as the glossary term).
- `domain::SpeechSynthesizer` is the engine port (opaque voice ids, engine/model/params identity for the cache key). The adapter `infrastructure::Qwen3TtsSynthesizer` calls `POST {TEACH_ME_TTS_URL}/v1/tts` with a fixed seed and encodes the 24 kHz WAV to Ogg Opus mono 32 kbit/s (`opus` crate with libopus built from source and linked statically: the build needs `cmake`, the runtime image nothing).
- `application::ClipCache` makes clips on demand and stores them: metadata in `audio_clips` (migration 0017), files in `TEACH_ME_AUDIO_DIR` (`<clip id>.ogg`, written through a temporary file in the same directory), keyed by a hash of spoken text, engine, model, voice and parameters.
- Settings, all optional: `TEACH_ME_TTS_URL` (unset = feature off), `TEACH_ME_TTS_VOICES` (`id:Label,...`, default `vivian:Vivian,serena:Serena,ryan:Ryan,aiden:Aiden,eric:Eric,dylan:Dylan`), `TEACH_ME_TTS_DEFAULT_VOICE` (default `vivian`, must be in the list), `TEACH_ME_TTS_MODEL` (default `1.7b-customvoice`, part of the cache key), `TEACH_ME_AUDIO_DIR` (default `/audio`). A wrong value stops the start. See `deploy/.env.example`.
- The audio queue is internal (not a `Job`): table `audio_queue` (migration 0018), port `domain::AudioQueue`, adapter `SqliteAudioQueue`. One worker (`application::AudioWorker`, spawned in `main` only when `TEACH_ME_TTS_URL` is set) makes one clip at a time: highest `bump` first (the lesson opened last), then oldest request. An entry exists only while its clip is missing, so a restart just resumes (`running` entries are queued again). A block that fails 3 times is `failed` until the lesson is opened again; an engine that is down (`SpeechError::Unavailable`) uses no attempt, the worker backs off 60 s.
- Triggers (`application::AudioPreparer`, voice = the user's effective voice, see `AudioPreparer::voice_of`): after a `lesson_generation` saves its lesson, and on every `GET /lessons/{id}`, which also queues `failed` blocks again and moves the lesson to the front. Queue errors are logged, never returned. `AudioPreparer::states` tells queued/running/failed per block, for the manifest.
- User voice: `users.tts_voice` (migration 0019) keeps the saved choice; the effective voice is worked out in `application` (`AudioPreparer::effective_voice`: the saved one while the server lists it, else the default) and is what `User.tts_voice` shows (`null` with the feature off). `PUT /me/tts-voice`: `422 unknown_voice`, `409` when off. When a lesson is prepared for voice V, `SqliteAudioQueue::enqueue` deletes that lesson's still-`queued` entries of other voices (a running one finishes).
- Endpoints (`api::audio`, use cases in `application::AudioService`): `GET /lessons/{id}/audio` (manifest: derived from the document, the clip cache and the queue; never queues; lesson checked first, so another user's lesson is `404` even with the feature off), `GET /tts/voices`, `GET /audio/clips/{id}`. A clip is readable if it is a voice sample (recognised by its cache key) or `audio_clip_grants` has (user, clip), written when the manifest names it (also as the other-voice fallback). The file is streamed in 64 KiB chunks through `ClipStore::size`/`read_range`; one `Range` is served as `206`, past the end is `416` with `Content-Range: bytes */size`, several ranges or a malformed header are ignored.
- Voice samples are not in the persistent queue: `AudioPreparer::prepare_samples` (called in `main` at startup) puts the missing ones in memory and the worker makes one only when the queue is empty, so lessons always go first; a sample that fails 3 times is dropped until the next start.
- The engine can take minutes per paragraph: the adapter allows 15 minutes per request. The engine itself refuses texts over its own character limit (HTTP 400, reported as a failed synthesis).
- Orphan clip files: after a database restore (`backup.sh restore`) the volume may hold `<clip id>.ogg` files that no `audio_clips` row mentions. They are harmless (never served, the clips are made again on demand), only disk space. To clear them, stop the service and empty the volume: `docker compose -p teach_me stop teach-me && docker run --rm -v teach_me_audio:/audio alpine sh -c 'rm -f /audio/*.ogg'`, then start it again (clips regenerate as lessons are opened).

## Tracing (OpenTelemetry, Jaeger)

Off by default. Setting `OTEL_EXPORTER_OTLP_ENDPOINT` (or `[telemetry] otlp_endpoint` in the toml; the variable wins) turns on span export over OTLP/HTTP protobuf (`<endpoint>/v1/traces`), with `service.name=teach-me`. Spans: HTTP requests (method, matched route, status), jobs (kind, id, outcome), LLM calls (role, model, token counts) and TTS calls (voice, text length). No bodies, headers, keys or lesson/chat text are recorded. Spans are flushed at shutdown.

- **Activate (compose):** `docker compose up -d` already starts `jaeger` (pinned `jaegertracing/jaeger`, in-memory, last 5000 traces, 512 MB) and points `OTEL_EXPORTER_OTLP_ENDPOINT` at `http://jaeger:4318`.
- **Open the UI:** http://127.0.0.1:16686 on the host (service `teach-me`). Set `JAEGER_UI_BIND_IP` in `.env` to publish it on another address (it has no login: use the Tailscale IP, never a public one).
- **Deactivate:** `OTEL_EXPORTER_OTLP_ENDPOINT=` (empty) in `.env`, and `docker compose stop jaeger` if you do not want the container.

## Verify

```bash
./scripts/check.sh   # fmt --check, clippy -D warnings, tests
cargo tree -p domain # must not list tokio/axum/sqlx/reqwest
```

## Users (phase 11)

Several people can use the same backend, each with a user name and password, their own LLM key and their own folders, lessons and reviews: nobody sees anything of the others (a resource of another user is `404`, as if it did not exist). Design: `docs/adr/0008-multiutenza.md`; contract: `docs/contracts/users.md`.

There is no registration and no admin API: the operator manages users **on the server**, with subcommands of the same binary. In the container: `docker compose exec teach-me teach-me user ...` (add `-T` when piping input in). Passwords and keys are read from standard input, one line, never from arguments (so they stay out of the shell history and the process list); a terminal counts as "no input".

| Command | Effect |
|---|---|
| `user add <username> [--display-name <name>]` | creates the user; password from stdin, or a temporary one printed **once** |
| `user list` | user name, name, state, whether an LLM key is set, last sign-in |
| `user passwd <username>` | new password (stdin, or generated and printed once); every session of the user is revoked |
| `user rename <username> <new>` | changes the user name |
| `user set-llm-key <username>` | reads the key from stdin; never prints it |
| `user disable <username>` / `user enable <username>` | blocks / allows the sign-in; disabling revokes the sessions |
| `user delete <username> [--yes]` | deletes the user, all their data and their uploaded files; asks to type the name |

```bash
printf '%s' 'a long password' | docker compose exec -T teach-me teach-me user passwd owner
docker compose exec teach-me teach-me user add valentina --display-name Valentina   # prints a temporary password
printf '%s' "$KEY" | docker compose exec -T teach-me teach-me user set-llm-key valentina
```

User names are 3-32 characters of `a-z 0-9 . _ -` (compared in lowercase); passwords 10-128 characters. Passwords are stored with Argon2id (19 MiB, 2 iterations, 1 lane: the OWASP minimum, written in `crates/infrastructure/src/crypto.rs`). A sign-in gives an opaque token (256 random bits, stored only as its SHA-256), valid 90 days from the last request. After 5 failed sign-ins for the same name in 15 minutes the answer is `429` with `Retry-After` (counted in memory: a restart resets it). The LLM keys are in the database encrypted with XChaCha20-Poly1305 under `TEACH_ME_SECRET_KEY`.

Known limits, accepted: (a) five wrong passwords for a user name block that name for 15 minutes **even if a stranger sent them**, and the real user waits (`429` with `Retry-After`); (b) the count is in memory: a restart resets it, and it is shared by all names longer than 32 characters (which cannot exist); (c) there is no attempt limit on `PUT /me/password` and `PUT /me/llm-key` (they need a valid session); (d) secrets are not zeroed in memory; (e) at most 3 password verifications run at once (19 MiB each); (f) an open event stream checks its session at every ping (15 s), so it ends within that time after a logout, a password change, or a user disabled or deleted (also from the command line, another process).

### Upgrading from the single-user version (once)

1. **Generate the secret key and keep it safe**, outside the database backups: `openssl rand -base64 32`. Add it to `.env` as `TEACH_ME_SECRET_KEY=...` (`/srv/teach_me/.env` with the pipeline). Without it the backend does not start (and `deploy.sh` stops before building). Do not change it later: the stored LLM keys could not be read any more.
2. Deploy as usual (`git push`, or `deploy.sh`). Migration `0014` creates the user `owner`, **without a password**, and gives it every folder, topic and preference that exist; at first start the OpenRouter/Anthropic key still in `.env` becomes the key of `owner`. Until the next step nobody can sign in. The old `TEACH_ME_API_TOKEN` is ignored (a warning in the log): remove it from `.env`.
3. **Set the password of `owner`:** `printf '%s' '<password>' | docker compose exec -T teach-me teach-me user passwd owner` (or without input to get a generated one).
4. Update the app (the old one gets `401` everywhere) and sign in as `owner`: all the data is there and lessons are generated with the key already saved, with nothing to enter again.
5. Other people: `docker compose exec teach-me teach-me user add <name>`; they sign in, enter their own key in Settings, and see an empty app.
6. **Remove the LLM key from `.env`** (`TEACH_ME_OPENROUTER_API_KEY` / `TEACH_ME_ANTHROPIC_API_KEY`) after the first start. As long as it stays in the configuration, `owner` adopts it again at every start whenever `owner` has no key (for example after `owner` removed it from Settings), which is rarely what you want.
7. The secret key is now part of what you must not lose: copy it somewhere safe. A lost key does not lose the database, only the saved LLM keys.

## Deploy on a home server

Docker Compose on a Linux x86_64 host. The backend is published on the host's Tailscale IP (ADR 0001: no TLS, the tunnel is encrypted) and, only if `.env` sets `LAN_BIND_IP`, also on the home LAN IP, where traffic is plain HTTP (ADR 0001, update of 11 November 2026). Files are in `backend/deploy/`; the image is built from the `Dockerfile` (prompts and migrations are embedded in the binary).

1. The host needs Docker with the Compose plugin and Tailscale running. Copy the `backend/` folder (or clone the repo) into a stable directory, e.g. `~/teach_me/backend`.
2. Prepare the configuration:
   ```bash
   cd ~/teach_me/backend/deploy
   cp .env.example .env && chmod 600 .env
   openssl rand -base64 32       # paste into TEACH_ME_SECRET_KEY and keep a copy outside the backups
   tailscale ip -4               # paste into BIND_IP
   $EDITOR .env                  # LLM key for the first start only: it becomes the key of `owner`
   ```
   Models are set in `deploy/teach_me.toml` (embedded in the image: rebuild with `--build` after a change). The default is Anthropic directly: Sonnet 5.5 for every role, Haiku 5.5 for the Grader, at most 4 searches for the Researcher. The provider applies to the whole server: every user needs an Anthropic key saved (app Settings, or `teach-me user set-llm-key`); a leftover OpenRouter key is rejected (`llm_key_rejected`). To switch back to OpenRouter: `default_provider = "openrouter"` and OpenRouter keys again.
3. Build and start: `docker compose up -d --build` (the first build compiles Rust and takes a few minutes).
4. Check from the Tailscale IP (the container becomes `healthy` within ~20 s: `docker compose ps`):
   ```bash
   curl -s http://<tailscale-ip>:8080/health
   ```
5. Users: set the password of `owner` and create the others (*Users* section above), then enter `http://<tailscale-ip>:8080` in the app and sign in with user name and password. From the LAN, with `LAN_BIND_IP` set, the URL is `http://<lan-ip>:8080`: the app keeps a single address, so the LAN one only works at home.

The SQLite database lives in the Docker volume `teach_me_data` (`/data/teach_me.db`) and survives rebuilds and restarts.

### Read aloud (`tts` service, ADR 0013)

`docker compose up` also starts `tts` (container `teach-me-tts`): the Qwen3-TTS 1.7B engine written in C (`deploy/tts/Dockerfile`, engine commit pinned by `ENGINE_COMMIT`), CPU only, with 8 threads, low priority (`nice 19`) and a 5 GB memory limit (measured peak about 2.8 GB). It publishes no ports: only the backend reaches it, as `http://tts:8080` (`TEACH_ME_TTS_URL`, already set by compose; leave it commented in `.env`, or set `TEACH_ME_TTS_URL=` empty to switch the feature off).

- **First start:** `entrypoint.sh` downloads the model (about 4.5 GB) into the volume `teach_me_tts_models`. The download resumes after an interruption and the model folder appears only when it is complete. `deploy.sh` does not wait for it: the backend starts anyway, and listening works once `docker ps` shows `teach-me-tts` as `healthy` (`docker logs -f teach-me-tts` shows the progress).
- **Long requests:** the engine runs with `--max-request-seconds 300` (the default, 60 s, would silently cut long paragraphs). Container variables, if needed: `TTS_THREADS`, `TTS_MAX_REQUEST_SECONDS`, `TTS_NICE`.
- **Image tied to the CPU:** the engine is compiled with the SIMD instructions of the machine that builds the image; build and run it on the same host (as compose does).
- **Audio:** clips are kept in the volume `teach_me_audio` (`/audio` in the backend). It is a rebuildable cache: `backup.sh` leaves it out and losing it only costs regenerating the clips.
- **Changing the engine commit:** edit `ENGINE_COMMIT`, check the flags again with `qwen_tts -h` and rebuild with `docker compose build tts`. If the model changes, change `TEACH_ME_TTS_MODEL` too (it is part of the cache key).
- **Previous model in the volume:** since the switch to 1.7B (2026-11-16) the folder `qwen3-tts-0.6b` (2.4 GB) is no longer used. Remove it, with the service running and `healthy`, with `docker run --rm -v teach_me_tts_models:/m alpine rm -rf /m/qwen3-tts-0.6b`.

### Operations

- **Logs:** `docker compose logs -f teach-me` (JSON format).
- **New version:** update the code (`git pull`), then `docker compose up -d --build` from the `deploy/` folder, or `./deploy.sh` (below). Migrations run at startup. Before an update with new migrations take a backup (`./backup.sh`). To remove old images: `docker image prune`.
- **Change to `.env` or `teach_me.toml`:** `docker compose up -d` (recreates the container if needed), or `docker compose restart teach-me` for the toml only.
- **Stop:** `docker compose down` (the volume stays; `-v` would delete it, do not use it).

### Backup and restore

`deploy/backup.sh` takes a consistent hot copy (SQLite online backup API, safe while the server writes), checks its integrity, compresses it into `deploy/backups/teach_me-YYYYMMDD-HHMMSS.db.gz` and keeps the latest 14 (`KEEP=30 ./backup.sh` to change it). The container must be running. Copy the `backups/` folder off the host from time to time.

Scheduling, either:

```cron
# crontab -e   (the user must be able to run docker)
30 3 * * * /home/<user>/teach_me/backend/deploy/backup.sh >> /home/<user>/teach_me/backup.log 2>&1
```

or systemd: copy `deploy/systemd/teach-me-backup.{service,timer}` to `/etc/systemd/system/`, fix `User` and the paths, then `sudo systemctl daemon-reload && sudo systemctl enable --now teach-me-backup.timer`.

Restore (stops the service, replaces the DB, starts it again; the current DB stays in the volume as `teach_me.db.before-restore-*`):

```bash
cd ~/teach_me/backend/deploy
./backup.sh restore backups/teach_me-20260101-033000.db.gz
```

### If migrations fail

The backend exits at startup with the error in the logs and the container keeps restarting (`docker compose logs teach-me`). Do not edit the DB or an applied migration by hand (if the message says an applied migration has changed, that version of the code is not compatible with the DB).
1. `docker compose stop teach-me`
2. Go back to the previous version of the code, or restore the latest backup (`./backup.sh restore ...`) if the DB was left half-migrated.
3. `docker compose up -d --build` and check `/health`.
Then report the problem: the migration must be fixed with a new file, never by rewriting the applied one.

### Updating with `deploy.sh`

`deploy/deploy.sh` updates the service on the host in one command: preflight, DB backup, tag `teach-me:prev` on the current image, `docker compose -p teach_me up -d --build`, wait for `teach-me` to become `healthy` (90 s, `HEALTH_TIMEOUT_SECS`; the `tts` service, which downloads its model at first start, is not waited for). If the new version does not become `healthy` it restores the backup and restarts with `teach-me:prev` (variable `TEACH_ME_IMAGE`), still exiting with an error. On the first deploy (no previous image) it only prints the logs. The compose project is always `teach_me` and the volume `teach_me_data` has a fixed name: `deploy.sh` and manual commands use the same container and the same DB.

On the host, after updating the code (`git pull`):

```bash
cd ~/teach_me/backend/deploy
./deploy.sh                                       # .env and backups/ in this folder
TEACH_ME_DEPLOY_DIR=/srv/teach_me ./deploy.sh     # or .env and backups/ in another folder
```

With `TEACH_ME_DEPLOY_DIR` different from the code folder, pass the same file to manual compose commands too: `docker compose --env-file "$TEACH_ME_DEPLOY_DIR/.env" ...`. Backups are in `backups/` with mode 700, rotated by name. To go back after a deploy that succeeded but is wrong: check out the previous code, run `./deploy.sh` again and, if you need the previous DB, `./backup.sh restore backups/<file>.db.gz`.
