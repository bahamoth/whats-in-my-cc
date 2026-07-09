# Install npm deps for the webui (idempotent).
webui-install:
    cd webui && npm install

# Dev server with proxy /v1 -> 127.0.0.1:7878.
webui-dev: webui-install
    cd webui && npm run dev

# Production build that rust-embed picks up.
webui-build: webui-install
    cd webui && npm run build

# Frontend unit tests.
webui-test: webui-install
    cd webui && npm test

# Run the backend in dev (assumes ingest already happened).
serve-dev:
    cargo run -- serve --auto-migrate

# Bring up the full dev environment — backend (:7878) + vite (:5173, HMR); Ctrl-C stops both.
dev: webui-install
    #!/usr/bin/env bash
    set -uo pipefail
    # Rust 백엔드 핫리로드: cargo-watch가 있으면 src/Cargo.toml/migrations 변경 시
    # serve를 재시작(vite의 SPA 핫리로드와 대칭). 없으면 1회 실행 + 설치 안내 —
    # 자동 설치는 하지 않는다(CLAUDE.md non-goal: 설치 스크립트 자동 생성 금지).
    if command -v cargo-watch >/dev/null 2>&1; then
        cargo watch -w src -w Cargo.toml -w migrations -x 'run -- serve --auto-migrate' &
    else
        echo "⚠️  cargo-watch 미설치 — 백엔드 핫리로드 없이 1회 실행 (설치: cargo install cargo-watch)" >&2
        cargo run -- serve --auto-migrate &
    fi
    serve_pid=$!
    trap 'kill "$serve_pid" 2>/dev/null || true' EXIT INT TERM
    cd webui && npm run dev

# Release binary including embedded webui/dist.
build-release: webui-build
    cargo build --release
