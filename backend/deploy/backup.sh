#!/usr/bin/env bash
# Backup and restore of the SQLite database (hot when the `teach-me` container runs).
#
#   ./backup.sh                 consistent copy -> backups/teach_me-YYYYmmdd-HHMMSS.db.gz, keeps the last KEEP
#   ./backup.sh restore FILE    stops the service, replaces the database with FILE (.db or .db.gz), restarts
#
# Environment: TEACH_ME_DEPLOY_DIR (default: this directory), KEEP (default 14),
# BACKUP_DIR (default $TEACH_ME_DEPLOY_DIR/backups), COMPOSE (default: docker compose -p teach_me --env-file ...).
set -euo pipefail
umask 077
cd "$(dirname "$0")"

# Private files (.env, backups/) live in TEACH_ME_DEPLOY_DIR; default: this directory.
DEPLOY_DIR="${TEACH_ME_DEPLOY_DIR:-$PWD}"
DOCKER="${DOCKER:-docker}"
KEEP="${KEEP:-14}"
BACKUP_DIR="${BACKUP_DIR:-$DEPLOY_DIR/backups}"
COMPOSE="${COMPOSE:-docker compose -p teach_me --env-file $DEPLOY_DIR/.env}"
export TEACH_ME_DEPLOY_DIR="$DEPLOY_DIR"

backup() {
  mkdir -p "$BACKUP_DIR"
  chmod 700 "$BACKUP_DIR"
  local out tmp
  out="$BACKUP_DIR/teach_me-$(date +%Y%m%d-%H%M%S).db.gz"
  tmp="$out.partial"
  # `.backup` uses SQLite's online backup API: consistent while the server writes (WAL).
  # The copy is checked before it is streamed out, so a bad copy never replaces a good one.
  local dump='
    set -e
    f=$(mktemp)
    trap "rm -f $f" EXIT
    sqlite3 /data/teach_me.db ".timeout 10000" ".backup $f"
    [ "$(sqlite3 "$f" "PRAGMA integrity_check")" = ok ] || { echo "integrity check failed" >&2; exit 3; }
    cat "$f"
  '
  if [ -n "$($DOCKER ps -q -f name='^teach-me$')" ]; then
    $COMPOSE exec -T teach-me sh -c "$dump" | gzip > "$tmp"
  elif $DOCKER volume inspect teach_me_data >/dev/null 2>&1; then
    # Service stopped but the data exists: dump through a throwaway container on the volume.
    echo "teach-me is not running: backing up through a temporary container"
    $DOCKER run --rm -i --user 10001 -v teach_me_data:/data --entrypoint sh "${TEACH_ME_IMAGE:-teach-me:local}" -c "$dump" | gzip > "$tmp"
  else
    echo "no teach_me_data volume: first deploy, nothing to back up"
    return 0
  fi
  gzip -t "$tmp"
  mv "$tmp" "$out"
  echo "backup written: $out"
  # Rotation by name (it contains the timestamp, unlike the file mtime): newest first, drop after KEEP.
  ls -1 "$BACKUP_DIR"/teach_me-*.db.gz 2>/dev/null | sort -r | tail -n +"$((KEEP + 1))" | xargs -r rm -f --
}

restore() {
  local src="${1:?usage: backup.sh restore FILE}"
  [ -f "$src" ] || { echo "no such file: $src" >&2; exit 1; }
  echo "Stopping teach-me and replacing the database with $src"
  $COMPOSE stop teach-me
  # The current database is kept aside inside the volume, in case the restore was a mistake.
  case "$src" in
    *.gz) gunzip -c "$src" ;;
    *) cat "$src" ;;
  esac | $COMPOSE run --rm -T --no-deps --entrypoint sh teach-me -c '
    set -e
    cat > /data/restore.tmp
    [ "$(sqlite3 /data/restore.tmp "PRAGMA integrity_check")" = ok ] || { echo "restore file is corrupt" >&2; rm -f /data/restore.tmp; exit 3; }
    if [ -f /data/teach_me.db ]; then mv /data/teach_me.db "/data/teach_me.db.before-restore-$(date +%Y%m%d-%H%M%S)"; fi
    rm -f /data/teach_me.db-wal /data/teach_me.db-shm
    mv /data/restore.tmp /data/teach_me.db
  '
  # up (not start): recreates the container if the image changed, e.g. a rollback to TEACH_ME_IMAGE.
  $COMPOSE up -d --no-build teach-me
  echo "restored."
}

case "${1:-backup}" in
  backup) backup ;;
  restore) shift; restore "$@" ;;
  *) echo "usage: $0 [backup | restore FILE]" >&2; exit 2 ;;
esac
