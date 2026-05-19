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

# Release binary including embedded webui/dist.
build-release: webui-build
    cargo build --release
