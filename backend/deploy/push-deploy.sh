#!/usr/bin/env bash
# Deploys from a development machine: copies backend/ to the host over SSH, then runs
# deploy.sh there (backup, rebuild, health check, rollback).
#
#   backend/deploy/push-deploy.sh <ssh-host> [remote-dir]
#
# <ssh-host>: an SSH destination (an alias from ~/.ssh/config works).
# [remote-dir]: the folder with .env and backups/ on the host (default /srv/teach_me);
# the code goes to <remote-dir>/src/backend. What is copied is the working tree as it is,
# uncommitted changes included: the script warns about them.
set -euo pipefail

host="${1:?usage: push-deploy.sh <ssh-host> [remote-dir]}"
remote_dir="${2:-/srv/teach_me}"
backend_dir="$(cd "$(dirname "$0")/.." && pwd)"

# What git ignores (target/, local databases, eval results) stays here.
ignored=$(git -C "$backend_dir" ls-files --others --ignored --exclude-standard --directory | sed 's|^|/|')

if [ -n "$(git -C "$backend_dir" status --porcelain -- .)" ]; then
  echo "WARNING: backend/ has uncommitted changes; they are deployed too." >&2
fi

echo "== copy backend/ to $host:$remote_dir/src/backend"
rsync -az --delete \
  --exclude 'target/' --exclude 'deploy/backups/' --exclude 'deploy/.env' \
  --exclude-from=<(printf '%s\n' "$ignored") \
  "$backend_dir/" "$host:$remote_dir/src/backend/"

echo "== deploy on $host"
ssh "$host" "TEACH_ME_DEPLOY_DIR=$(printf %q "$remote_dir") $(printf %q "$remote_dir/src/backend/deploy/deploy.sh")"
