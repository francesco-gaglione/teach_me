#!/usr/bin/env bash
# Deploys from a development machine: copies backend/ to the host over SSH, then runs
# deploy.sh there (backup, rebuild, health check, rollback).
#
#   backend/deploy/push-deploy.sh [--allow-dirty] <ssh-host> [remote-dir]
#
# <ssh-host>: an SSH destination (an alias from ~/.ssh/config works).
# [remote-dir]: the folder with .env and backups/ on the host (default /srv/teach_me);
# the code goes to <remote-dir>/src/backend. What is copied is the working tree as it is.
# With uncommitted changes in backend/ it refuses, unless --allow-dirty is given (then it warns
# and the recorded commit gets a "-dirty" suffix). The commit reaches deploy.sh as
# TEACH_ME_SOURCE_COMMIT and ends up in deploys.log on the host.
set -euo pipefail

allow_dirty=0
if [ "${1:-}" = "--allow-dirty" ]; then allow_dirty=1; shift; fi
host="${1:?usage: push-deploy.sh [--allow-dirty] <ssh-host> [remote-dir]}"
remote_dir="${2:-/srv/teach_me}"
backend_dir="$(cd "$(dirname "$0")/.." && pwd)"

# What git ignores (target/, local databases, eval results) stays here. The host's own
# docker-compose.override.yml is excluded too, so --delete never removes it there.
ignored=$(git -C "$backend_dir" ls-files --others --ignored --exclude-standard --directory | sed 's|^|/|')

source_commit=$(git -C "$backend_dir" rev-parse --short HEAD)
if [ -n "$(git -C "$backend_dir" status --porcelain -- .)" ]; then
  if [ "$allow_dirty" != 1 ]; then
    echo "backend/ has uncommitted changes: commit them, or pass --allow-dirty to deploy them anyway." >&2
    exit 1
  fi
  echo "WARNING: backend/ has uncommitted changes; they are deployed too." >&2
  source_commit="$source_commit-dirty"
fi

echo "== copy backend/ to $host:$remote_dir/src/backend"
rsync -az --delete \
  --exclude 'target/' --exclude 'deploy/backups/' --exclude 'deploy/.env' \
  --exclude 'deploy/docker-compose.override.yml' \
  --exclude-from=<(printf '%s\n' "$ignored") \
  "$backend_dir/" "$host:$remote_dir/src/backend/"

echo "== deploy on $host"
ssh "$host" "TEACH_ME_SOURCE_COMMIT=$(printf %q "$source_commit") TEACH_ME_DEPLOY_DIR=$(printf %q "$remote_dir") $(printf %q "$remote_dir/src/backend/deploy/deploy.sh")"
