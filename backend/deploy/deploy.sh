#!/usr/bin/env bash
# Deploys the service on THIS host: backup, rebuild, restart, health check, rollback.
# Run by hand on the host:
#
#   TEACH_ME_DEPLOY_DIR=/srv/teach_me ./deploy.sh
#
# Exit status is non-zero when the new version is not healthy (even if the rollback worked).
# Environment: TEACH_ME_DEPLOY_DIR (default: this directory; holds .env and backups/),
# HEALTH_TIMEOUT_SECS (default 90), DOCKER (default docker), TEACH_ME_SOURCE_COMMIT (set by
# push-deploy.sh: the commit being deployed, "-dirty" suffix when uncommitted; "unknown" if unset).
# Every deploy appends a line to $TEACH_ME_DEPLOY_DIR/deploys.log.
# Outcome "failed" is logged for any other failure. Exit status 2: the rollback could not restore the database (state uncertain).
# LAN_BIND_IP in .env (non-loopback) needs ALLOW_PLAIN_LAN=1 there too: plain HTTP on the LAN.
set -euo pipefail
umask 077
cd "$(dirname "$0")"

DEPLOY_DIR="${TEACH_ME_DEPLOY_DIR:-$PWD}"
export TEACH_ME_DEPLOY_DIR="$DEPLOY_DIR"
DOCKER="${DOCKER:-docker}"
HEALTH_TIMEOUT_SECS="${HEALTH_TIMEOUT_SECS:-90}"
COMPOSE="$DOCKER compose -p teach_me --env-file $DEPLOY_DIR/.env"
SOURCE_COMMIT="${TEACH_ME_SOURCE_COMMIT:-unknown}"

# One line per deploy: UTC timestamp, source commit, outcome.
log_outcome() {
  OUTCOME_LOGGED=1
  printf '%s\t%s\t%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$SOURCE_COMMIT" "$1" >> "$DEPLOY_DIR/deploys.log" \
    || echo "WARNING: could not write $DEPLOY_DIR/deploys.log" >&2
}

# Value of KEY as compose sees it: the shell environment wins over .env. In .env the last
# assignment wins; an "export " prefix, quotes and an inline " # comment" are handled.
env_get() {
  if [ -n "${!1+x}" ]; then printf '%s' "${!1}"; return; fi
  local v
  v=$(grep -E "^[[:space:]]*(export[[:space:]]+)?$1=" "$DEPLOY_DIR/.env" | tail -n 1 \
    | sed -E "s/^[^=]*=//; s/^[[:space:]]+//; /^\"/{s/^\"([^\"]*)\".*\$/\1/;b};/^'/{s/^'([^']*)'.*\$/\1/;b};s/[[:space:]]+#.*\$//;s/[[:space:]]+\$//") || true
  printf '%s' "$v"
}

# Any run that ends without an outcome line (preflight refusal, backup or build failure) is
# recorded as "failed". Exit status is kept.
OUTCOME_LOGGED=0
on_exit() { [ "$OUTCOME_LOGGED" = 1 ] || log_outcome failed; }
trap on_exit EXIT

# Readiness (GET /ready: database and job runner), asked inside the container: faster than
# waiting for the Docker health check interval, which uses the same endpoint.
# "wait_healthy rollback" also accepts GET /health: an image built before /ready existed
# answers 404 there while being perfectly healthy.
wait_healthy() {
  local mode="${1:-}"
  local deadline=$((SECONDS + HEALTH_TIMEOUT_SECS)) status
  while [ "$SECONDS" -lt "$deadline" ]; do
    if $DOCKER exec teach-me curl -fsS -o /dev/null http://127.0.0.1:8080/ready 2>/dev/null \
      || { [ "$mode" = rollback ] \
        && $DOCKER exec teach-me curl -fsS -o /dev/null http://127.0.0.1:8080/health 2>/dev/null; }; then
      status=ready
    else
      status=$($DOCKER inspect -f '{{.State.Status}}' teach-me 2>/dev/null || echo missing)
      status="not ready ($status)"
    fi
    echo "ready: $status"
    [ "$status" = ready ] && return 0
    sleep 2
  done
  return 1
}

echo "== source commit: $SOURCE_COMMIT"
echo "== preflight"
[ -f "$DEPLOY_DIR/.env" ] || { echo "missing $DEPLOY_DIR/.env: do the one-time setup in backend/README.md" >&2; exit 1; }
# Without the secret key the new version cannot start (it encrypts the users' LLM keys):
# better to stop here, before the build and with the old version still running.
grep -Eq '^TEACH_ME_SECRET_KEY=.+' "$DEPLOY_DIR/.env" \
  || { echo "TEACH_ME_SECRET_KEY is missing in $DEPLOY_DIR/.env: generate one with 'openssl rand -base64 32' (see backend/README.md, Users)" >&2; exit 1; }
# Plain HTTP on the LAN (ADR 0001) is opt-in: passwords and session tokens would travel in clear.
lan_ip=$(env_get LAN_BIND_IP)
case "$lan_ip" in
  ''|127.*|::1|\[::1\]|localhost) ;;
  *)
    if [ "$(env_get ALLOW_PLAIN_LAN)" != 1 ]; then
      {
        echo "LAN_BIND_IP=$lan_ip in $DEPLOY_DIR/.env publishes the backend on the LAN over plain HTTP:"
        echo "passwords and session tokens would be readable by anyone on that network."
        echo "To accept the risk add ALLOW_PLAIN_LAN=1 (exactly 1, nothing else counts) to the same .env; otherwise unset LAN_BIND_IP."
      } >&2
      exit 1
    fi
    echo "WARNING: LAN_BIND_IP=$lan_ip: plain HTTP on the LAN (ALLOW_PLAIN_LAN=1)." >&2
    ;;
esac
$DOCKER compose version >/dev/null
$DOCKER info >/dev/null

echo "== backup"
# backup.sh covers a running service, a stopped one with an existing volume, and the first deploy.
backup_file=$(./backup.sh | tee /dev/stderr | sed -n 's/^backup written: //p')

echo "== keep the previous image"
have_prev=0
if $DOCKER image inspect teach-me:local >/dev/null 2>&1; then
  $DOCKER tag teach-me:local teach-me:prev
  have_prev=1
fi

echo "== build and start"
# teach-me first, alone: a failing build of the speech engine must not block (or roll back) the
# backend, which works without it. --remove-orphans here cleans up once, for the whole project.
$COMPOSE up -d --build --remove-orphans teach-me
# Then the engine. Only teach-me is waited for (below): the first start of `tts` downloads ~4.5 GB
# and can take many minutes. A failure here leaves read-aloud off, nothing else.
$COMPOSE up -d --build tts \
  || echo "WARNING: the tts image failed to build or start: read-aloud is unavailable until the next deploy fixes it. The backend is deployed." >&2

echo "== wait for health"
if wait_healthy; then
  echo "tts: $($DOCKER inspect -f '{{.State.Health.Status}}' teach-me-tts 2>/dev/null || echo not-running) (not waited for: it may still be downloading the model)"
  echo "deploy ok"
  log_outcome ok
  exit 0
fi

$DOCKER logs --tail 80 teach-me || true
echo "Deploy FAILED: service not healthy." >&2
if [ "$have_prev" != 1 ]; then
  echo "No previous image (first deploy): nothing to roll back to." >&2
  log_outcome failed-first-deploy
  exit 1
fi
echo "Rolling back to the previous image (teach-me:prev)..." >&2
export TEACH_ME_IMAGE=teach-me:prev
if [ -n "$backup_file" ]; then
  # The new version may have applied migrations: put the pre-deploy database back.
  if ! ./backup.sh restore "$backup_file"; then
    # Do not wait for health on a database that may be half replaced.
    {
      echo "RESTORE FAILED: the state of the database is UNCERTAIN and the teach-me service is STOPPED. Not going on."
      echo "Backup taken before this deploy: $backup_file"
      echo "Restore by hand (from $PWD): TEACH_ME_DEPLOY_DIR=$DEPLOY_DIR TEACH_ME_IMAGE=teach-me:prev ./backup.sh restore $backup_file"
      echo "Copies of the previous database may be in the teach_me_data volume as teach_me.db.before-restore-*."
    } >&2
    log_outcome restore-failed
    exit 2
  fi
else
  $COMPOSE up -d --no-build
fi
if wait_healthy rollback; then
  echo "Rolled back: the previous version is running again. Exiting non-zero anyway." >&2
  log_outcome rolled-back
else
  $DOCKER logs --tail 80 teach-me || true
  echo "Rollback FAILED too: intervene by hand." >&2
  [ -n "$backup_file" ] && echo "Backup taken before this deploy: $backup_file" >&2
  log_outcome rollback-failed
fi
exit 1
