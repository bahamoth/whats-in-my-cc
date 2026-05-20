#!/usr/bin/env bash
#
# Run witmcc against a local repo with a *fresh* SQLite database every time.
#
#   ./scripts/watch-fresh.sh [REPO_PATH] [PORT]
#
# Defaults:
#   REPO_PATH = $(pwd)
#   PORT      = 7878
#   DB        = /tmp/witmcc-watch.sqlite   (overridable via $WITMCC_DB)
#
# What it does:
#   1. Removes any existing $WITMCC_DB (so the git poller's `last_seen` starts
#      at the current HEAD and every new commit is captured cleanly).
#   2. Builds the release binary + webui/dist if either is missing.
#   3. Starts `witmcc serve --watch <repo>` in the foreground.
#
# Stop with Ctrl-C — `with_graceful_shutdown` lets background tasks exit.

set -euo pipefail

REPO_PATH="${1:-$PWD}"
PORT="${2:-7878}"
DB_PATH="${WITMCC_DB:-/tmp/witmcc-watch.sqlite}"
POLL_SECS="${WITMCC_GIT_POLL_SECS:-5}"

if [[ ! -d "$REPO_PATH" ]]; then
  echo "ERR: $REPO_PATH does not exist or is not a directory" >&2
  exit 1
fi
REPO_PATH="$(cd "$REPO_PATH" && pwd -P)"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$REPO_ROOT"

# 1. Fresh DB
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm"

# 2. Build if needed
if [[ ! -d webui/dist ]]; then
  echo ">> webui/dist missing; running just webui-build"
  just webui-build
fi
if [[ ! -x target/release/witmcc ]]; then
  echo ">> release binary missing; running cargo build --release"
  cargo build --release
fi

echo
echo "  watch     : $REPO_PATH"
echo "  git poll  : ${POLL_SECS}s$([[ -d "$REPO_PATH/.git" ]] || echo '  (no .git/ — poller disabled)')"
echo "  port      : $PORT"
echo "  db (fresh): $DB_PATH"
echo "  endpoint  : http://127.0.0.1:$PORT"
echo
echo "Tail with:"
echo "  curl -s http://127.0.0.1:$PORT/v1/sessions/filesystem | jq .data.summary"
echo "  curl -s http://127.0.0.1:$PORT/v1/sessions/filesystem/graph | jq '[.data.nodes[].node_kind] | unique'"
echo

exec ./target/release/witmcc \
  --db-path "$DB_PATH" \
  serve \
  --bind 127.0.0.1 \
  --port "$PORT" \
  --watch "$REPO_PATH" \
  --git-poll-secs "$POLL_SECS" \
  --auto-migrate
