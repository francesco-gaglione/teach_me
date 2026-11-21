#!/usr/bin/env bash
# Deploys the service on THIS host: backup, rebuild, restart, health check, rollback.
# Run by hand on the host:
#
#   TEACH_ME_DEPLOY_DIR=/srv/teach_me ./deploy.sh
#
# Exit status is non-zero when the new version is not healthy (even if the rollback worked).
# Environment: TEACH_ME_DEPLOY_DIR (default: this directory; holds .env and backups/),
# HEALTH_TIMEOUT_SECS (default 90), DOCKER (default docker).
set -euo pipefail
umask 077
cd "$(dirname "$0")"

DEPLOY_DIR="${TEACH_ME_DEPLOY_DIR:-$PWD}"
export TEACH_ME_DEPLOY_DIR="$DEPLOY_DIR"
DOCKER="${DOCKER:-docker}"
HEALTH_TIMEOUT_SECS="${HEALTH_TIMEOUT_SECS:-90}"
COMPOSE="$DOCKER compose -p teach_me --env-file $DEPLOY_DIR/.env"

wait_healthy() {
  local deadline=$((SECONDS + HEALTH_TIMEOUT_SECS)) status
  while [ "$SECONDS" -lt "$deadline" ]; do
    status=$($DOCKER inspect -f '{{.State.Health.Status}}' teach-me 2>/dev/null || echo missing)
    echo "health: $status"
    [ "$status" = healthy ] && return 0
    sleep 2
  done
  return 1
}

echo "== preflight"
[ -f "$DEPLOY_DIR/.env" ] || { echo "missing $DEPLOY_DIR/.env: do the one-time setup in backend/README.md" >&2; exit 1; }
# Without the secret key the new version cannot start (it encrypts the users' LLM keys):
# better to stop here, before the build and with the old version still running.
grep -Eq '^TEACH_ME_SECRET_KEY=.+' "$DEPLOY_DIR/.env" \
  || { echo "TEACH_ME_SECRET_KEY is missing in $DEPLOY_DIR/.env: generate one with 'openssl rand -base64 32' (see backend/README.md, Users)" >&2; exit 1; }
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
  exit 0
fi

$DOCKER logs --tail 80 teach-me || true
echo "Deploy FAILED: service not healthy." >&2
if [ "$have_prev" != 1 ]; then
  echo "No previous image (first deploy): nothing to roll back to." >&2
  exit 1
fi
echo "Rolling back to the previous image (teach-me:prev)..." >&2
export TEACH_ME_IMAGE=teach-me:prev
if [ -n "$backup_file" ]; then
  # The new version may have applied migrations: put the pre-deploy database back.
  ./backup.sh restore "$backup_file" || echo "restore failed" >&2
else
  $COMPOSE up -d --no-build
fi
if wait_healthy; then
  echo "Rolled back: the previous version is running again. Exiting non-zero anyway." >&2
else
  $DOCKER logs --tail 80 teach-me || true
  echo "Rollback FAILED too: intervene by hand." >&2
  [ -n "$backup_file" ] && echo "Backup taken before this deploy: $backup_file" >&2
fi
exit 1
