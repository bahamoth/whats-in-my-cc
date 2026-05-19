# Slice-2 Read-only Session Replay UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only browser UI for replaying a single Claude Code session. The same `witmcc` binary serves the existing Pull API plus a React SPA from a single `127.0.0.1` origin. A new endpoint exposes the raw transcript record for a clicked timeline node so users can verify evidence end-to-end.

**Architecture:** axum gains one new endpoint (`GET /v1/events/:event_id/raw`) plus a catch-all static fallback that serves the Vite build output of a React SPA embedded via `rust-embed`. The SPA has two pages — Session List and Session Detail (six-lane timeline + right-hand SourcePanel) — and calls the existing four endpoints plus the new raw endpoint. No graph or schema changes; no new ingest sources.

**Tech Stack:** Backend additions: `rust-embed 8`, `mime_guess 2` on top of the slice-1 axum/sqlx stack. Frontend: React 18, react-router-dom 6, TypeScript 5, Vite 5, Radix UI Dialog/ScrollArea, `react-json-view-lite`, vanilla CSS modules. Dev tooling: vitest + @testing-library/react for FE; `just` for orchestration.

**Reference spec:** `docs/superpowers/specs/2026-05-19-witmcc-slice2-replay-ui-design.md` — read once before starting and refer back when a task references a section.

---

## File Structure (locked at plan-time)

| Path | Responsibility |
|---|---|
| `Cargo.toml` | Add `rust-embed`, `mime_guess` |
| `src/db/repo_raw.rs` | Add `get_for_event_id(pool, event_id)` joining `observed_event` ↔ `raw_event` |
| `src/api/dto.rs` | Add `RawEventResponse` + `RawSource` DTOs |
| `src/api/routes.rs` | Add `event_raw` handler |
| `src/api/static_assets.rs` (NEW) | `rust-embed` asset struct + axum fallback handler |
| `src/api/mod.rs` | Wire `/v1/events/:event_id/raw` + catch-all fallback |
| `webui/` (NEW) | React+Vite SPA workspace |
| `webui/package.json` | npm deps + scripts |
| `webui/tsconfig.json` | TypeScript strict |
| `webui/vite.config.ts` | Vite + `/v1` proxy in dev |
| `webui/index.html` | SPA entry |
| `webui/.nvmrc` | `20` |
| `webui/.gitignore` | `node_modules`, `dist` |
| `webui/src/main.tsx` | React root + router |
| `webui/src/App.tsx` | route table + shell |
| `webui/src/api/client.ts` | typed fetch wrappers |
| `webui/src/api/types.ts` | Pull API response types (mirrors `src/api/dto.rs`) |
| `webui/src/api/laneMapping.ts` | `node_kind` → lane constant |
| `webui/src/routes/SessionListPage.tsx` | `/sessions` |
| `webui/src/routes/SessionDetailPage.tsx` | `/sessions/:sessionId` |
| `webui/src/components/MetaStrip.tsx` | counts strip |
| `webui/src/components/Timeline.tsx` | SVG lanes + nodes + edges |
| `webui/src/components/SourcePanel.tsx` | lazy raw fetch + JsonView |
| `webui/src/components/JsonView.tsx` | thin wrapper over `react-json-view-lite` |
| `webui/src/styles/*.module.css` | per-component scoped styles |
| `webui/vitest.config.ts` | vitest + jsdom |
| `webui/src/**/__tests__/*.test.tsx` | RTL tests |
| `tests/api.rs` | Extend with `event_raw` cases |
| `tests/static_serve.rs` (NEW) | axum static fallback cases |
| `justfile` (NEW) | `webui-install`, `webui-dev`, `webui-build`, `serve-dev`, `build-release` |
| `.gitignore` | Ensure `webui/node_modules`, `webui/dist` ignored |
| `README.md` | Document UI dev/build workflow |
| `docs/implementation-notes.html` | Append slice-2 deviations section |

---

## Task 1: Backend deps + minimal placeholder dist

**Why first:** `rust-embed` macro panics at compile-time if the directory it points to does not exist. We create an empty `webui/dist/` placeholder so `cargo build` works *before* the Vite scaffold lands, letting the rest of the backend tasks (raw endpoint, static fallback tests) proceed independently of the FE.

**Files:**
- Modify: `Cargo.toml`
- Create: `webui/dist/.gitkeep`
- Create: `webui/.gitignore`
- Modify: `.gitignore` (root)

- [ ] **Step 1: Add new Cargo dependencies**

Edit `Cargo.toml` `[dependencies]` block — append:

```toml
rust-embed         = { version = "8", features = ["interpolate-folder-path"] }
mime_guess         = "2"
```

- [ ] **Step 2: Create placeholder dist directory**

```bash
mkdir -p webui/dist
printf '' > webui/dist/.gitkeep
```

- [ ] **Step 3: Create `webui/.gitignore`**

```
node_modules/
dist/
.vite/
*.log
```

Note: `dist/` is intentionally git-ignored *and* `.gitkeep` is committed to keep the folder shape. After `webui-build` runs locally the real `dist/` content is generated but never committed.

- [ ] **Step 4: Update root `.gitignore`**

Append (if not already present):

```
webui/node_modules/
webui/dist/
!webui/dist/.gitkeep
```

- [ ] **Step 5: Verify build still passes**

Run: `cargo build`
Expected: PASS (compiles; `rust-embed` is not yet used so the empty dir is fine).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock webui/dist/.gitkeep webui/.gitignore .gitignore
git commit -m "chore(slice-2): add rust-embed + mime_guess deps, webui/dist placeholder"
```

---

## Task 2: DB helper — `get_for_event_id`

**Files:**
- Modify: `src/db/repo_raw.rs`
- Test: `tests/repo_raw.rs` (extend)

- [ ] **Step 1: Write the failing test**

Append to `tests/repo_raw.rs`:

```rust
#[tokio::test]
async fn get_for_event_id_returns_joined_row() {
    let (pool, _tmp) = test_pool().await;
    // seed minimal ingest_run + raw_event + observed_event
    let run_id = "run_test_x";
    sqlx::query(
        "INSERT INTO ingest_run(run_id, started_at, status) \
         VALUES (?, ?, 'ok')",
    )
    .bind(run_id)
    .bind("2026-05-19T00:00:00Z")
    .execute(&pool).await.unwrap();

    let raw = witmcc::db::repo_raw::NewRaw {
        raw_event_id: "raw_x_001".into(),
        ingest_run_id: run_id.into(),
        source_type: "transcript".into(),
        source_uri: "/tmp/sample.jsonl".into(),
        source_line_no: 7,
        source_byte_offset: 0,
        payload_sha256: "deadbeef".into(),
        payload: br#"{"type":"user","content":"hi"}"#.to_vec(),
        parse_error: None,
        captured_at: chrono::Utc::now(),
    };
    witmcc::db::repo_raw::insert_dedup(&pool, &raw).await.unwrap();

    // synthesize an observed_event referencing the raw row
    sqlx::query(
        "INSERT INTO observed_event(\
            event_id, raw_event_id, schema_version, session_id, event_uuid, \
            observed_at, actor, kind, is_sidechain, is_meta, payload, parser_version)\
         VALUES ('ev_x_001','raw_x_001','1.0','sess_x','uuid-1',\
                 '2026-05-19T00:00:00Z','user','user_message',0,0,'{}','0.1')",
    )
    .execute(&pool).await.unwrap();

    let row = witmcc::db::repo_raw::get_for_event_id(&pool, "ev_x_001")
        .await.unwrap().expect("row");
    assert_eq!(row.event_id, "ev_x_001");
    assert_eq!(row.session_id, "sess_x");
    assert_eq!(row.source_uri, "/tmp/sample.jsonl");
    assert_eq!(row.source_line_no, 7);
    assert_eq!(row.source_type, "transcript");
    assert_eq!(row.kind, "user_message");
    assert!(row.payload.starts_with(b"{"));
}

#[tokio::test]
async fn get_for_event_id_returns_none_when_missing() {
    let (pool, _tmp) = test_pool().await;
    let row = witmcc::db::repo_raw::get_for_event_id(&pool, "no_such_event")
        .await.unwrap();
    assert!(row.is_none());
}
```

If `test_pool()` is not yet a shared helper, copy the existing helper from `tests/repo_raw.rs` head (slice-1 already has one). Re-use it without duplicating.

- [ ] **Step 2: Run tests; expect failure**

Run: `cargo test --test repo_raw -- get_for_event_id`
Expected: FAIL — `repo_raw::get_for_event_id` not found.

- [ ] **Step 3: Implement `get_for_event_id`**

Append to `src/db/repo_raw.rs`:

```rust
pub struct RawForEventRow {
    pub event_id: String,
    pub session_id: String,
    pub kind: String,
    pub raw_event_id: String,
    pub source_type: String,
    pub source_uri: String,
    pub source_line_no: i64,
    pub captured_at: String, // RFC3339 string straight from sqlite
    pub payload: Vec<u8>,
}

pub async fn get_for_event_id(
    pool: &SqlitePool,
    event_id: &str,
) -> Result<Option<RawForEventRow>> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT o.event_id        AS event_id, \
                o.session_id      AS session_id, \
                o.kind            AS kind, \
                r.raw_event_id    AS raw_event_id, \
                r.source_type     AS source_type, \
                r.source_uri      AS source_uri, \
                r.source_line_no  AS source_line_no, \
                r.captured_at     AS captured_at, \
                r.payload         AS payload \
         FROM observed_event o \
         JOIN raw_event r ON r.raw_event_id = o.raw_event_id \
         WHERE o.event_id = ?",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| RawForEventRow {
        event_id: r.get("event_id"),
        session_id: r.get("session_id"),
        kind: r.get("kind"),
        raw_event_id: r.get("raw_event_id"),
        source_type: r.get("source_type"),
        source_uri: r.get("source_uri"),
        source_line_no: r.get("source_line_no"),
        captured_at: r.get("captured_at"),
        payload: r.get("payload"),
    }))
}
```

- [ ] **Step 4: Run tests; expect pass**

Run: `cargo test --test repo_raw -- get_for_event_id`
Expected: PASS (both new cases).

- [ ] **Step 5: Run the full test suite to confirm no regressions**

Run: `cargo test`
Expected: PASS (existing 22 + 2 new = 24 tests).

- [ ] **Step 6: Commit**

```bash
git add src/db/repo_raw.rs tests/repo_raw.rs
git commit -m "feat(db): repo_raw::get_for_event_id (observed↔raw join for raw endpoint)"
```

---

## Task 3: DTO + handler for `GET /v1/events/:event_id/raw`

**Files:**
- Modify: `src/api/dto.rs`
- Modify: `src/api/routes.rs`
- Modify: `src/api/mod.rs`
- Test: `tests/api.rs` (extend)

- [ ] **Step 1: Write the failing test**

Append to `tests/api.rs` (reuse the existing helper that seeds a session + spins up an in-process axum app — slice-1 already has `seed_demo_session(pool)` or equivalent; if not, copy from the existing `session_detail_returns_events` test in the same file):

```rust
#[tokio::test]
async fn raw_endpoint_returns_record() {
    let (pool, _tmp) = setup_with_seeded_session().await;
    // pick the first event_id from the seeded session
    let event_id: String = sqlx::query_scalar(
        "SELECT event_id FROM observed_event LIMIT 1",
    )
    .fetch_one(&pool).await.unwrap();

    let server = axum_test::TestServer::new(witmcc::api::router(pool)).unwrap();
    let resp = server
        .get(&format!("/v1/events/{event_id}/raw"))
        .add_header("host", "127.0.0.1".parse().unwrap())
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["data"]["event_id"], event_id);
    assert!(body["data"]["source"]["file_path"].is_string());
    assert_eq!(body["data"]["source"]["kind"], "transcript");
    assert!(body["data"]["record"].is_object());
    assert!(body["data"]["record_type"].is_string());
    assert_eq!(body["data"]["redaction_state"], "none");
}

#[tokio::test]
async fn raw_endpoint_404_for_unknown_event() {
    let (pool, _tmp) = setup_with_seeded_session().await;
    let server = axum_test::TestServer::new(witmcc::api::router(pool)).unwrap();
    let resp = server
        .get("/v1/events/no_such_event/raw")
        .add_header("host", "127.0.0.1".parse().unwrap())
        .await;
    resp.assert_status_not_found();
}
```

If `setup_with_seeded_session` does not exist verbatim, replace with whichever helper the existing `session_detail_*` tests use — do not invent a new one.

- [ ] **Step 2: Run tests; expect failure**

Run: `cargo test --test api -- raw_endpoint`
Expected: FAIL — route not registered (404 for the happy path will surface as the assertion on `body["data"]` failing).

- [ ] **Step 3: Add DTOs**

Append to `src/api/dto.rs`:

```rust
#[derive(serde::Serialize)]
pub struct RawSource {
    pub kind: String,
    pub file_path: String,
    pub line_no: i64,
    pub ingested_at: String,
}

#[derive(serde::Serialize)]
pub struct RawEventResponse {
    pub schema_version: String,
    pub event_id: String,
    pub session_id: String,
    pub source: RawSource,
    pub record: serde_json::Value,
    pub record_type: String,
    pub redaction_state: String,
}
```

- [ ] **Step 4: Add `event_raw` handler**

Append to `src/api/routes.rs`:

```rust
pub async fn event_raw(
    State(pool): State<SqlitePool>,
    Path(event_id): Path<String>,
) -> Result<Json<Envelope<RawEventResponse>>, (StatusCode, Json<serde_json::Value>)> {
    let row = crate::db::repo_raw::get_for_event_id(&pool, &event_id)
        .await
        .expect("db");
    let row = match row {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({
                    "type": "about:blank",
                    "title": "RESOURCE_NOT_FOUND",
                    "detail": format!("event {event_id} not found")
                })),
            ));
        }
    };

    let record = match std::str::from_utf8(&row.payload)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    {
        Some(v) => v,
        None => serde_json::Value::Null,
    };

    Ok(Json(Envelope {
        meta: crate::model::meta::ResponseMeta::now(),
        data: RawEventResponse {
            schema_version: crate::model::meta::SCHEMA_VERSION.into(),
            event_id: row.event_id,
            session_id: row.session_id,
            source: RawSource {
                kind: row.source_type,
                file_path: row.source_uri,
                line_no: row.source_line_no,
                ingested_at: row.captured_at,
            },
            record,
            record_type: row.kind,
            redaction_state: "none".into(),
        },
    }))
}
```

- [ ] **Step 5: Wire the route**

Edit `src/api/mod.rs` — extend the router:

```rust
pub fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/sessions", get(routes::list_sessions))
        .route("/v1/sessions/:id", get(routes::session_detail))
        .route("/v1/sessions/:id/graph", get(routes::session_graph))
        .route("/v1/events/:event_id/raw", get(routes::event_raw))
        .layer(axum_mw::from_fn(middleware::host_allowlist))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(pool)
}
```

- [ ] **Step 6: Run tests; expect pass**

Run: `cargo test --test api -- raw_endpoint`
Expected: PASS (both cases).

- [ ] **Step 7: Run the full test suite**

Run: `cargo test`
Expected: PASS (no regressions).

- [ ] **Step 8: Commit**

```bash
git add src/api/dto.rs src/api/routes.rs src/api/mod.rs tests/api.rs
git commit -m "feat(api): GET /v1/events/:event_id/raw — raw transcript record for evidence panel"
```

---

## Task 4: Static asset fallback (axum + rust-embed)

**Files:**
- Create: `src/api/static_assets.rs`
- Modify: `src/api/mod.rs`
- Test: `tests/static_serve.rs` (NEW)
- Modify: `webui/dist/.gitkeep` → replace with a minimal `index.html` for placeholder serving

We embed `webui/dist/` and fall back to `index.html` for unknown paths so deep links work. Because `webui/dist/` is still mostly empty, we drop a placeholder `index.html` so the fallback test has something to assert against. The placeholder is overwritten by the real Vite build later.

- [ ] **Step 1: Replace the dist placeholder with a real `index.html`**

```bash
rm webui/dist/.gitkeep
```

Create `webui/dist/index.html`:

```html
<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>witmcc</title></head>
<body>
  <div id="root">witmcc spa placeholder</div>
</body>
</html>
```

Update root `.gitignore` so the dist directory is ignored except for the placeholder during early development:

```
webui/node_modules/
webui/dist/*
!webui/dist/index.html
```

Rationale: this is short-lived; after Task 6 the real Vite build will be invoked locally and the placeholder line will be removed in Task 11. We keep one committed file so CI / fresh clones can `cargo build` immediately.

- [ ] **Step 2: Write the failing tests**

Create `tests/static_serve.rs`:

```rust
use witmcc::api::router;

async fn setup_empty_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    (pool, tmp)
}

#[tokio::test]
async fn serves_index_html_at_root() {
    let (pool, _tmp) = setup_empty_pool().await;
    let server = axum_test::TestServer::new(router(pool)).unwrap();
    let resp = server.get("/")
        .add_header("host", "127.0.0.1".parse().unwrap())
        .await;
    resp.assert_status_ok();
    assert!(resp.headers()
        .get("content-type").unwrap()
        .to_str().unwrap()
        .starts_with("text/html"));
    let body = resp.text();
    assert!(body.contains("<div id=\"root\""));
}

#[tokio::test]
async fn serves_spa_fallback_for_unknown_route() {
    let (pool, _tmp) = setup_empty_pool().await;
    let server = axum_test::TestServer::new(router(pool)).unwrap();
    let resp = server.get("/sessions/anything")
        .add_header("host", "127.0.0.1".parse().unwrap())
        .await;
    resp.assert_status_ok();
    assert!(resp.text().contains("<div id=\"root\""));
}

#[tokio::test]
async fn v1_routes_are_not_swallowed_by_fallback() {
    let (pool, _tmp) = setup_empty_pool().await;
    let server = axum_test::TestServer::new(router(pool)).unwrap();
    let resp = server.get("/v1/health")
        .add_header("host", "127.0.0.1".parse().unwrap())
        .await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["status"], "ok");
}
```

- [ ] **Step 3: Run tests; expect failure**

Run: `cargo test --test static_serve`
Expected: FAIL — `/` returns 404 (no route yet).

- [ ] **Step 4: Implement the static handler**

Create `src/api/static_assets.rs`:

```rust
use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/webui/dist"]
struct Assets;

pub async fn spa_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty() {
        if let Some(content) = Assets::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.into_owned()))
                .unwrap();
        }
    }
    match Assets::get("index.html") {
        Some(content) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(content.data.into_owned()))
            .unwrap(),
        None => (StatusCode::NOT_FOUND, "index.html missing from embed").into_response(),
    }
}
```

- [ ] **Step 5: Wire the fallback**

Edit `src/api/mod.rs`:

```rust
pub mod dto;
pub mod middleware;
pub mod routes;
pub mod static_assets;

use axum::{middleware as axum_mw, routing::get, Router};
use sqlx::SqlitePool;

pub fn router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/v1/health", get(routes::health))
        .route("/v1/sessions", get(routes::list_sessions))
        .route("/v1/sessions/:id", get(routes::session_detail))
        .route("/v1/sessions/:id/graph", get(routes::session_graph))
        .route("/v1/events/:event_id/raw", get(routes::event_raw))
        .fallback(static_assets::spa_handler)
        .layer(axum_mw::from_fn(middleware::host_allowlist))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(pool)
}
```

- [ ] **Step 6: Run tests; expect pass**

Run: `cargo test --test static_serve`
Expected: PASS (all three).

- [ ] **Step 7: Full suite**

Run: `cargo test`
Expected: PASS (api + static + repo + others; no regressions).

- [ ] **Step 8: Commit**

```bash
git add src/api/static_assets.rs src/api/mod.rs tests/static_serve.rs \
        webui/dist/index.html .gitignore
git rm webui/dist/.gitkeep
git commit -m "feat(api): rust-embed SPA fallback handler + placeholder dist/index.html"
```

---

## Task 5: Vite + React + TS scaffold for `webui/`

**Files:**
- Create: `webui/package.json`
- Create: `webui/tsconfig.json`
- Create: `webui/tsconfig.node.json`
- Create: `webui/vite.config.ts`
- Create: `webui/.nvmrc`
- Create: `webui/index.html`
- Create: `webui/src/main.tsx`
- Create: `webui/src/App.tsx`
- Create: `webui/src/styles/global.css`

- [ ] **Step 1: Pin Node version**

Create `webui/.nvmrc`:

```
20
```

- [ ] **Step 2: Write `webui/package.json`**

```json
{
  "name": "witmcc-webui",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-router-dom": "^6.26.2",
    "@radix-ui/react-dialog": "^1.1.2",
    "@radix-ui/react-scroll-area": "^1.2.0",
    "react-json-view-lite": "^1.5.0"
  },
  "devDependencies": {
    "@testing-library/jest-dom": "^6.5.0",
    "@testing-library/react": "^16.0.1",
    "@testing-library/user-event": "^14.5.2",
    "@types/react": "^18.3.10",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "jsdom": "^25.0.0",
    "typescript": "^5.6.2",
    "vite": "^5.4.6",
    "vitest": "^2.1.1"
  }
}
```

- [ ] **Step 3: TypeScript config**

`webui/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

`webui/tsconfig.node.json`:

```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "strict": true
  },
  "include": ["vite.config.ts"]
}
```

- [ ] **Step 4: Vite config with dev proxy**

`webui/vite.config.ts`:

```ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/v1': {
        target: 'http://127.0.0.1:7878',
        changeOrigin: false,
      },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
```

- [ ] **Step 5: SPA entry**

`webui/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width,initial-scale=1" />
    <title>witmcc</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Global stylesheet**

`webui/src/styles/global.css`:

```css
:root {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
  color: #1a1d23;
  background: #f7f8fa;
}
* { box-sizing: border-box; }
body, html, #root { height: 100%; margin: 0; }
a { color: #2c5cc5; text-decoration: none; }
a:hover { text-decoration: underline; }
```

- [ ] **Step 7: React root**

`webui/src/main.tsx`:

```tsx
import React from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import './styles/global.css';

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>,
);
```

- [ ] **Step 8: App shell with empty routes**

`webui/src/App.tsx`:

```tsx
import { Navigate, Route, Routes } from 'react-router-dom';

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/sessions" replace />} />
      <Route path="/sessions" element={<div>session list (todo)</div>} />
      <Route path="/sessions/:sessionId" element={<div>session detail (todo)</div>} />
      <Route path="*" element={<div>not found</div>} />
    </Routes>
  );
}
```

(The two `(todo)` placeholders are deliberate plan scaffolds — they are *replaced* in Tasks 8 and 11. They will not survive into the final state.)

- [ ] **Step 9: Install + smoke-build**

```bash
cd webui && npm install && npm run build
```

Expected: `webui/dist/index.html` regenerated; assets folder populated. No type errors.

- [ ] **Step 10: Verify Rust still builds against the new dist**

Run: `cargo build`
Expected: PASS — rust-embed now picks up the freshly built assets.

- [ ] **Step 11: Verify static tests still pass**

Run: `cargo test --test static_serve`
Expected: PASS (asserts `<div id="root"` which the Vite build still emits).

- [ ] **Step 12: Commit**

```bash
git add webui/package.json webui/tsconfig.json webui/tsconfig.node.json \
        webui/vite.config.ts webui/.nvmrc webui/index.html webui/src
git commit -m "feat(webui): Vite + React + TS scaffold with router skeleton"
```

Do **not** commit `webui/package-lock.json` or `webui/dist/*` here — they're ignored. Lockfile is committed in Task 6 once dependencies stabilize.

---

## Task 6: Commit lockfile + add justfile

**Files:**
- Create: `webui/package-lock.json` (commit it)
- Modify: `webui/.gitignore` (lockfile is not ignored — keep it tracked)
- Create: `justfile`

- [ ] **Step 1: Stop ignoring package-lock**

Ensure `webui/.gitignore` does **not** list `package-lock.json`. It already shouldn't — confirm.

- [ ] **Step 2: Write `justfile`**

Create `justfile` at repo root:

```just
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
```

- [ ] **Step 3: Commit**

```bash
git add webui/package-lock.json justfile
git commit -m "chore(slice-2): commit webui lockfile + justfile for build/dev orchestration"
```

---

## Task 7: API client + types (frontend)

**Files:**
- Create: `webui/src/api/types.ts`
- Create: `webui/src/api/client.ts`
- Create: `webui/src/api/laneMapping.ts`
- Test: `webui/src/api/__tests__/client.test.ts`
- Modify: `webui/vitest.config.ts` (create)

- [ ] **Step 1: Types mirroring `src/api/dto.rs`**

`webui/src/api/types.ts`:

```ts
export type Envelope<T> = { meta: { generated_at: string }; data: T };

export type SessionListItem = {
  session_id: string;
  first_observed_at: string;
  last_observed_at: string;
  event_count: number;
  source_uris: string[];
};

export type ObservedEventDto = {
  event_id: string;
  raw_event_id: string;
  session_id: string;
  event_uuid: string | null;
  parent_uuid: string | null;
  observed_at: string;
  actor: string;
  kind: string;
  subkind: string | null;
  tool_use_id: string | null;
  tool_name: string | null;
  turn_id: string | null;
  is_sidechain: boolean | number;
  is_meta: boolean | number;
  payload: unknown;
};

export type SessionDetail = {
  session_id: string;
  summary: {
    event_count: number;
    by_kind: Record<string, number>;
    first_observed_at: string;
    last_observed_at: string;
  };
  events: ObservedEventDto[];
};

export type GraphNodeDto = {
  node_id: string;
  schema_version: string;
  session_id: string;
  node_kind: string;
  started_at: string;
  ended_at: string | null;
  merge_keys: Record<string, unknown>;
  source_event_ids: string[];
  source_uris: string[];
  payload: unknown;
};

export type GraphEdgeDto = {
  edge_id: string;
  schema_version: string;
  session_id: string;
  from_node_id: string;
  to_node_id: string;
  edge_kind: 'message_reply' | 'tool_call_to_result' | string;
  origin: 'deterministic' | 'inferred' | string;
  attributes: Record<string, unknown>;
};

export type GraphPayload = { nodes: GraphNodeDto[]; edges: GraphEdgeDto[] };

export type RawEventResponse = {
  schema_version: string;
  event_id: string;
  session_id: string;
  source: {
    kind: string;
    file_path: string;
    line_no: number;
    ingested_at: string;
  };
  record: unknown;
  record_type: string;
  redaction_state: 'none' | 'partial' | 'redacted' | string;
};
```

- [ ] **Step 2: Lane mapping constant**

`webui/src/api/laneMapping.ts`:

```ts
export const LANES = [
  'Intent',
  'Context',
  'Action',
  'State',
  'OTel',
  'Quality',
] as const;
export type Lane = (typeof LANES)[number];

export function laneForNodeKind(kind: string): Lane | null {
  switch (kind) {
    case 'user_message':            return 'Intent';
    case 'assistant_message':       return 'Context';
    case 'tool_call':               return 'Action';
    case 'tool_result':             return 'Action'; // merged into tool_call, but defensive
    case 'file_history_snapshot':   return 'State';
    default:                        return null;
  }
}
```

- [ ] **Step 3: Failing test for `client.ts`**

`webui/src/api/__tests__/client.test.ts`:

```ts
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { listSessions, getSession, getGraph, getEventRaw, ApiError } from '../client';

describe('api client', () => {
  beforeEach(() => {
    vi.stubGlobal('fetch', vi.fn());
  });
  afterEach(() => { vi.unstubAllGlobals(); });

  function ok(body: unknown) {
    return new Response(JSON.stringify(body), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('listSessions unwraps envelope', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      ok({ meta: { generated_at: 'now' }, data: [{ session_id: 's1', first_observed_at: 'a', last_observed_at: 'b', event_count: 3, source_uris: [] }] })
    );
    const out = await listSessions();
    expect(out[0].session_id).toBe('s1');
  });

  it('getSession returns SessionDetail', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      ok({ meta: { generated_at: 'now' }, data: { session_id: 's1', summary: { event_count: 0, by_kind: {}, first_observed_at: 'a', last_observed_at: 'b' }, events: [] } })
    );
    const out = await getSession('s1');
    expect(out.session_id).toBe('s1');
  });

  it('getEventRaw throws ApiError on 404', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      new Response('{"detail":"event x not found"}', { status: 404 })
    );
    await expect(getEventRaw('x')).rejects.toBeInstanceOf(ApiError);
  });

  it('getGraph uses /v1/sessions/:id/graph', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(ok({ meta: {generated_at:'n'}, data: { nodes: [], edges: [] } }));
    await getGraph('abc');
    expect(f).toHaveBeenCalledWith('/v1/sessions/abc/graph', expect.any(Object));
  });
});
```

- [ ] **Step 4: vitest config**

`webui/vitest.config.ts`:

```ts
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: false,
  },
});
```

- [ ] **Step 5: Run tests; expect failure**

```bash
cd webui && npx vitest run
```

Expected: FAIL — `../client` module not found.

- [ ] **Step 6: Implement `client.ts`**

`webui/src/api/client.ts`:

```ts
import type {
  Envelope,
  SessionListItem,
  SessionDetail,
  GraphPayload,
  RawEventResponse,
} from './types';

export class ApiError extends Error {
  constructor(public status: number, public detail: string) {
    super(detail);
  }
}

async function jsonGet<T>(path: string): Promise<T> {
  const resp = await fetch(path, { headers: { accept: 'application/json' } });
  if (!resp.ok) {
    let detail = resp.statusText;
    try {
      const body = await resp.json();
      detail = body.detail ?? detail;
    } catch { /* ignore */ }
    throw new ApiError(resp.status, detail);
  }
  const env = (await resp.json()) as Envelope<T>;
  return env.data;
}

export const listSessions = () => jsonGet<SessionListItem[]>('/v1/sessions');
export const getSession   = (id: string) => jsonGet<SessionDetail>(`/v1/sessions/${encodeURIComponent(id)}`);
export const getGraph     = (id: string) => jsonGet<GraphPayload>(`/v1/sessions/${encodeURIComponent(id)}/graph`);
export const getEventRaw  = (eventId: string) =>
  jsonGet<RawEventResponse>(`/v1/events/${encodeURIComponent(eventId)}/raw`);
```

- [ ] **Step 7: Run tests; expect pass**

```bash
cd webui && npx vitest run
```

Expected: PASS (4 tests).

- [ ] **Step 8: Commit**

```bash
git add webui/src/api webui/vitest.config.ts
git commit -m "feat(webui): typed API client + lane mapping + unit tests"
```

---

## Task 8: SessionListPage

**Files:**
- Create: `webui/src/routes/SessionListPage.tsx`
- Create: `webui/src/routes/SessionListPage.module.css`
- Modify: `webui/src/App.tsx`
- Test: `webui/src/routes/__tests__/SessionListPage.test.tsx`

- [ ] **Step 1: Failing test**

`webui/src/routes/__tests__/SessionListPage.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import SessionListPage from '../SessionListPage';

function withRouter(node: React.ReactNode) {
  return <MemoryRouter>{node}</MemoryRouter>;
}

describe('SessionListPage', () => {
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { vi.unstubAllGlobals(); });

  function envelope(data: unknown) {
    return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('renders empty state with CLI hint', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([]));
    render(withRouter(<SessionListPage />));
    await waitFor(() => expect(screen.getByText(/no sessions yet/i)).toBeInTheDocument());
    expect(screen.getByText(/witmcc ingest --all/)).toBeInTheDocument();
  });

  it('renders rows sorted by last_observed_at desc', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope([
      { session_id: 'older', first_observed_at: '2026-05-19T08:00:00Z', last_observed_at: '2026-05-19T09:00:00Z', event_count: 5, source_uris: [] },
      { session_id: 'newer', first_observed_at: '2026-05-19T10:00:00Z', last_observed_at: '2026-05-19T11:00:00Z', event_count: 7, source_uris: [] },
    ]));
    render(withRouter(<SessionListPage />));
    const rows = await screen.findAllByRole('row');
    // [header, newer, older]
    expect(rows[1]).toHaveTextContent('newer');
    expect(rows[2]).toHaveTextContent('older');
  });

  it('renders error state with retry', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      new Response('{"detail":"db gone"}', { status: 500 })
    );
    render(withRouter(<SessionListPage />));
    await waitFor(() => expect(screen.getByText(/db gone/)).toBeInTheDocument());
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run; expect failure**

```bash
cd webui && npx vitest run routes
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement `SessionListPage`**

`webui/src/routes/SessionListPage.tsx`:

```tsx
import { useEffect, useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { listSessions, ApiError } from '../api/client';
import type { SessionListItem } from '../api/types';
import styles from './SessionListPage.module.css';

type State =
  | { kind: 'loading' }
  | { kind: 'ok'; rows: SessionListItem[] }
  | { kind: 'error'; message: string };

export default function SessionListPage() {
  const [state, setState] = useState<State>({ kind: 'loading' });

  const load = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const rows = await listSessions();
      rows.sort((a, b) => b.last_observed_at.localeCompare(a.last_observed_at));
      setState({ kind: 'ok', rows });
    } catch (e) {
      const message = e instanceof ApiError ? e.detail : String(e);
      setState({ kind: 'error', message });
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1>witmcc · Sessions</h1>
        <button type="button" onClick={() => void load()}>refresh</button>
      </header>
      {state.kind === 'loading' && <p>Loading…</p>}
      {state.kind === 'error' && (
        <div role="alert">
          <p>{state.message}</p>
          <button type="button" onClick={() => void load()}>Retry</button>
        </div>
      )}
      {state.kind === 'ok' && state.rows.length === 0 && (
        <div className={styles.empty}>
          <p>No sessions yet.</p>
          <p>Run <code>witmcc ingest --all</code> to start.</p>
        </div>
      )}
      {state.kind === 'ok' && state.rows.length > 0 && (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>session_id</th>
              <th>first_observed_at</th>
              <th>last_observed_at</th>
              <th>events</th>
            </tr>
          </thead>
          <tbody>
            {state.rows.map((r) => (
              <tr key={r.session_id}>
                <td><Link to={`/sessions/${r.session_id}`}>{r.session_id}</Link></td>
                <td>{r.first_observed_at}</td>
                <td>{r.last_observed_at}</td>
                <td>{r.event_count}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
```

`webui/src/routes/SessionListPage.module.css`:

```css
.page { padding: 24px; }
.header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
.table { width: 100%; border-collapse: collapse; }
.table th, .table td { padding: 8px; border-bottom: 1px solid #e3e6eb; text-align: left; }
.empty { padding: 32px; background: #fff; border: 1px dashed #b9bfca; border-radius: 8px; }
```

- [ ] **Step 4: Wire route**

Edit `webui/src/App.tsx`:

```tsx
import { Navigate, Route, Routes } from 'react-router-dom';
import SessionListPage from './routes/SessionListPage';

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/sessions" replace />} />
      <Route path="/sessions" element={<SessionListPage />} />
      <Route path="/sessions/:sessionId" element={<div>session detail (todo)</div>} />
      <Route path="*" element={<div>not found</div>} />
    </Routes>
  );
}
```

- [ ] **Step 5: Run tests; expect pass**

```bash
cd webui && npx vitest run
```

Expected: PASS (all FE tests).

- [ ] **Step 6: Commit**

```bash
git add webui/src/routes/SessionListPage.tsx \
        webui/src/routes/SessionListPage.module.css \
        webui/src/routes/__tests__/SessionListPage.test.tsx \
        webui/src/App.tsx
git commit -m "feat(webui): SessionListPage with empty/error/loading states + sort by last_observed_at"
```

---

## Task 9: Timeline component

**Files:**
- Create: `webui/src/components/Timeline.tsx`
- Create: `webui/src/components/Timeline.module.css`
- Test: `webui/src/components/__tests__/Timeline.test.tsx`

The timeline takes `graph` (nodes + edges) and a `selectedNodeId` plus an `onSelect(nodeId)` callback. It does *not* fetch — it's a pure component. The `events` array from `/v1/sessions/:id` is *not* used by Timeline (events without `node_id` cannot be drawn on the lane); detail page passes only `graph`.

- [ ] **Step 1: Failing tests**

`webui/src/components/__tests__/Timeline.test.tsx`:

```tsx
import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { Timeline } from '../Timeline';
import type { GraphPayload } from '../../api/types';

const fixture: GraphPayload = {
  nodes: [
    { node_id: 'n1', schema_version: '1.0', session_id: 's', node_kind: 'user_message',
      started_at: '2026-05-19T10:00:00Z', ended_at: null,
      merge_keys: {}, source_event_ids: ['ev1'], source_uris: [], payload: {} },
    { node_id: 'n2', schema_version: '1.0', session_id: 's', node_kind: 'assistant_message',
      started_at: '2026-05-19T10:00:05Z', ended_at: null,
      merge_keys: {}, source_event_ids: ['ev2'], source_uris: [], payload: {} },
    { node_id: 'n3', schema_version: '1.0', session_id: 's', node_kind: 'tool_call',
      started_at: '2026-05-19T10:00:10Z', ended_at: null,
      merge_keys: {}, source_event_ids: ['ev3'], source_uris: [], payload: {} },
  ],
  edges: [
    { edge_id: 'e1', schema_version: '1.0', session_id: 's',
      from_node_id: 'n1', to_node_id: 'n2', edge_kind: 'message_reply',
      origin: 'deterministic', attributes: {} },
  ],
};

describe('Timeline', () => {
  it('renders all six lanes', () => {
    render(<Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />);
    for (const lane of ['Intent','Context','Action','State','OTel','Quality']) {
      expect(screen.getByText(lane)).toBeInTheDocument();
    }
  });

  it('draws one marker per drawable node', () => {
    const { container } = render(
      <Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />
    );
    // each node marker has data-testid="node-marker"
    expect(container.querySelectorAll('[data-testid="node-marker"]').length).toBe(3);
  });

  it('shows placeholder text in empty lanes', () => {
    render(<Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />);
    expect(screen.getByText(/no OTel observed/i)).toBeInTheDocument();
    expect(screen.getByText(/no findings yet/i)).toBeInTheDocument();
  });

  it('calls onSelect with node_id when a marker is clicked', () => {
    const onSelect = vi.fn();
    const { container } = render(
      <Timeline graph={fixture} selectedNodeId={null} onSelect={onSelect} />
    );
    const marker = container.querySelector('[data-node-id="n3"]');
    expect(marker).not.toBeNull();
    fireEvent.click(marker!);
    expect(onSelect).toHaveBeenCalledWith('n3');
  });

  it('renders one path per edge', () => {
    const { container } = render(
      <Timeline graph={fixture} selectedNodeId={null} onSelect={vi.fn()} />
    );
    expect(container.querySelectorAll('[data-testid="edge-path"]').length).toBe(1);
  });
});
```

- [ ] **Step 2: Run; expect failure**

```bash
cd webui && npx vitest run Timeline
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement `Timeline.tsx`**

`webui/src/components/Timeline.tsx`:

```tsx
import { useMemo } from 'react';
import type { GraphPayload, GraphNodeDto } from '../api/types';
import { LANES, laneForNodeKind } from '../api/laneMapping';
import styles from './Timeline.module.css';

type Props = {
  graph: GraphPayload;
  selectedNodeId: string | null;
  onSelect: (nodeId: string) => void;
};

const ROW_HEIGHT = 56;
const HEADER_WIDTH = 96;
const NODE_RADIUS = 6;

const PLACEHOLDERS: Partial<Record<(typeof LANES)[number], string>> = {
  OTel: 'no OTel observed in this session',
  Quality: 'no findings yet',
};

export function Timeline({ graph, selectedNodeId, onSelect }: Props) {
  const { layout, width, height } = useMemo(() => buildLayout(graph), [graph]);
  return (
    <div className={styles.wrap}>
      <svg width={width} height={height} role="img" aria-label="session timeline">
        {/* lane backgrounds + labels */}
        {LANES.map((lane, idx) => {
          const y = idx * ROW_HEIGHT;
          return (
            <g key={lane}>
              <rect x={0} y={y} width={width} height={ROW_HEIGHT}
                    className={idx % 2 ? styles.laneAlt : styles.lane} />
              <text x={8} y={y + ROW_HEIGHT / 2 + 4} className={styles.laneLabel}>
                {lane}
              </text>
              {PLACEHOLDERS[lane] && layout.byLane[lane].length === 0 && (
                <text x={HEADER_WIDTH + 16} y={y + ROW_HEIGHT / 2 + 4}
                      className={styles.placeholder}>
                  {PLACEHOLDERS[lane]}
                </text>
              )}
            </g>
          );
        })}
        {/* edges first so nodes sit on top */}
        {graph.edges.map((e) => {
          const a = layout.posByNodeId[e.from_node_id];
          const b = layout.posByNodeId[e.to_node_id];
          if (!a || !b) return null;
          const dashed = e.origin !== 'deterministic';
          const merged = (e.attributes as Record<string, unknown>)?.merged === true;
          let d: string;
          if (e.from_node_id === e.to_node_id) {
            // merged self-loop: small arc above the node
            d = `M ${a.x},${a.y} C ${a.x - 12},${a.y - 18} ${a.x + 12},${a.y - 18} ${a.x},${a.y}`;
          } else if (a.lane === b.lane) {
            d = `M ${a.x},${a.y} L ${b.x},${b.y}`;
          } else {
            const mx = (a.x + b.x) / 2;
            d = `M ${a.x},${a.y} L ${mx},${a.y} L ${mx},${b.y} L ${b.x},${b.y}`;
          }
          return (
            <path
              key={e.edge_id}
              d={d}
              data-testid="edge-path"
              className={merged ? styles.edgeMerged : dashed ? styles.edgeInferred : styles.edge}
              fill="none"
            />
          );
        })}
        {/* nodes */}
        {graph.nodes.map((n) => {
          const p = layout.posByNodeId[n.node_id];
          if (!p) return null;
          const selected = selectedNodeId === n.node_id;
          return (
            <circle
              key={n.node_id}
              cx={p.x}
              cy={p.y}
              r={selected ? NODE_RADIUS + 2 : NODE_RADIUS}
              data-testid="node-marker"
              data-node-id={n.node_id}
              className={`${styles.node} ${styles[`node_${n.node_kind}`] ?? ''} ${selected ? styles.selected : ''}`}
              onClick={() => onSelect(n.node_id)}
            >
              <title>{`${n.node_kind} · ${n.started_at}`}</title>
            </circle>
          );
        })}
      </svg>
    </div>
  );
}

type Layout = {
  byLane: Record<(typeof LANES)[number], GraphNodeDto[]>;
  posByNodeId: Record<string, { x: number; y: number; lane: (typeof LANES)[number] }>;
};

function buildLayout(graph: GraphPayload): { layout: Layout; width: number; height: number } {
  const byLane = {
    Intent: [], Context: [], Action: [], State: [], OTel: [], Quality: [],
  } as Layout['byLane'];
  for (const n of graph.nodes) {
    const lane = laneForNodeKind(n.node_kind);
    if (lane) byLane[lane].push(n);
  }
  const allTimes = graph.nodes.map((n) => Date.parse(n.started_at));
  const minT = allTimes.length ? Math.min(...allTimes) : 0;
  const maxT = allTimes.length ? Math.max(...allTimes) : minT + 1;
  const span = Math.max(maxT - minT, 1);
  const innerWidth = 720;
  const width = HEADER_WIDTH + innerWidth + 32;
  const height = LANES.length * ROW_HEIGHT;
  const posByNodeId: Layout['posByNodeId'] = {};
  for (const lane of LANES) {
    const idx = LANES.indexOf(lane);
    const y = idx * ROW_HEIGHT + ROW_HEIGHT / 2;
    for (const n of byLane[lane]) {
      const t = Date.parse(n.started_at);
      const x = HEADER_WIDTH + 16 + ((t - minT) / span) * (innerWidth - 32);
      posByNodeId[n.node_id] = { x, y, lane };
    }
  }
  return { layout: { byLane, posByNodeId }, width, height };
}
```

`webui/src/components/Timeline.module.css`:

```css
.wrap { overflow-x: auto; background: #fff; border: 1px solid #e3e6eb; border-radius: 8px; }
.lane { fill: #fafbfc; }
.laneAlt { fill: #f3f5f8; }
.laneLabel { font: 12px ui-monospace, SFMono-Regular, monospace; fill: #4b5563; }
.placeholder { font: 11px sans-serif; fill: #9aa1ad; font-style: italic; }
.node { cursor: pointer; fill: #2c5cc5; }
.node:hover { fill: #1e4099; }
.selected { stroke: #111; stroke-width: 2; }
.edge { stroke: #6b7280; stroke-width: 1.25; }
.edgeMerged { stroke: #2c5cc5; stroke-width: 1.25; }
.edgeInferred { stroke: #6b7280; stroke-width: 1.25; stroke-dasharray: 4 3; }
.node_user_message { fill: #d7813a; }
.node_assistant_message { fill: #2c5cc5; }
.node_tool_call { fill: #1aa37a; }
.node_file_history_snapshot { fill: #8a4fbd; }
```

- [ ] **Step 4: Run tests; expect pass**

```bash
cd webui && npx vitest run Timeline
```

Expected: PASS (5 cases).

- [ ] **Step 5: Commit**

```bash
git add webui/src/components/Timeline.tsx \
        webui/src/components/Timeline.module.css \
        webui/src/components/__tests__/Timeline.test.tsx
git commit -m "feat(webui): Timeline component — six lanes, node markers, edges, click handler"
```

---

## Task 10: SourcePanel + JsonView

**Files:**
- Create: `webui/src/components/JsonView.tsx`
- Create: `webui/src/components/SourcePanel.tsx`
- Create: `webui/src/components/SourcePanel.module.css`
- Test: `webui/src/components/__tests__/SourcePanel.test.tsx`

- [ ] **Step 1: Failing test**

`webui/src/components/__tests__/SourcePanel.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import '@testing-library/jest-dom/vitest';
import { SourcePanel } from '../SourcePanel';

describe('SourcePanel', () => {
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { vi.unstubAllGlobals(); });

  function envelope(data: unknown) {
    return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
      status: 200, headers: { 'content-type': 'application/json' },
    });
  }

  it('shows empty hint when no event_id is selected', () => {
    render(<SourcePanel eventId={null} />);
    expect(screen.getByText(/click a node/i)).toBeInTheDocument();
  });

  it('fetches and renders raw record', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(envelope({
      schema_version: '1.0',
      event_id: 'ev_x',
      session_id: 's1',
      source: { kind: 'transcript', file_path: '/tmp/a.jsonl', line_no: 42, ingested_at: 'now' },
      record: { type: 'user', content: 'hi' },
      record_type: 'user_message',
      redaction_state: 'none',
    }));
    render(<SourcePanel eventId="ev_x" />);
    await waitFor(() => expect(screen.getByText('user_message')).toBeInTheDocument());
    expect(screen.getByText('/tmp/a.jsonl')).toBeInTheDocument();
    expect(screen.getByText(/:42/)).toBeInTheDocument();
  });

  it('renders 404 message', async () => {
    (fetch as unknown as ReturnType<typeof vi.fn>).mockResolvedValueOnce(
      new Response('{"detail":"event nope not found"}', { status: 404 })
    );
    render(<SourcePanel eventId="nope" />);
    await waitFor(() => expect(screen.getByText(/raw record not available/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run; expect failure**

```bash
cd webui && npx vitest run SourcePanel
```

Expected: FAIL — module not found.

- [ ] **Step 3: `JsonView` thin wrapper**

`webui/src/components/JsonView.tsx`:

```tsx
import { JsonView as JsonViewLite, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

export function JsonView({ data }: { data: unknown }) {
  return <JsonViewLite data={data as object} style={defaultStyles} shouldExpandNode={(level) => level < 1} />;
}
```

- [ ] **Step 4: `SourcePanel.tsx`**

`webui/src/components/SourcePanel.tsx`:

```tsx
import { useEffect, useState } from 'react';
import { ApiError, getEventRaw } from '../api/client';
import type { RawEventResponse } from '../api/types';
import { JsonView } from './JsonView';
import styles from './SourcePanel.module.css';

type Props = { eventId: string | null };

type State =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'ok'; data: RawEventResponse }
  | { kind: 'error'; status: number; message: string };

export function SourcePanel({ eventId }: Props) {
  const [state, setState] = useState<State>(eventId ? { kind: 'loading' } : { kind: 'idle' });

  useEffect(() => {
    if (!eventId) { setState({ kind: 'idle' }); return; }
    let cancelled = false;
    setState({ kind: 'loading' });
    getEventRaw(eventId)
      .then((data) => { if (!cancelled) setState({ kind: 'ok', data }); })
      .catch((e: unknown) => {
        if (cancelled) return;
        if (e instanceof ApiError) setState({ kind: 'error', status: e.status, message: e.detail });
        else setState({ kind: 'error', status: 0, message: String(e) });
      });
    return () => { cancelled = true; };
  }, [eventId]);

  return (
    <aside className={styles.panel}>
      {state.kind === 'idle' && <p className={styles.hint}>Click a node to see its source record.</p>}
      {state.kind === 'loading' && <p>Loading raw record…</p>}
      {state.kind === 'error' && state.status === 404 && (
        <p className={styles.hint}>raw record not available for this event</p>
      )}
      {state.kind === 'error' && state.status === 410 && (
        <p className={styles.hint}>raw record pruned by retention</p>
      )}
      {state.kind === 'error' && state.status !== 404 && state.status !== 410 && (
        <p role="alert">Error: {state.message}</p>
      )}
      {state.kind === 'ok' && (
        <>
          <header className={styles.header}>
            <span className={styles.type}>{state.data.record_type}</span>
            <span className={styles.source}>
              {state.data.source.file_path}:{state.data.source.line_no}
            </span>
          </header>
          <div className={styles.body}>
            <JsonView data={state.data.record} />
          </div>
        </>
      )}
    </aside>
  );
}
```

`webui/src/components/SourcePanel.module.css`:

```css
.panel { background: #fff; border: 1px solid #e3e6eb; border-radius: 8px; padding: 16px; min-height: 480px; overflow: auto; }
.hint { color: #6b7280; font-style: italic; }
.header { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 12px; }
.type { font-weight: 600; }
.source { font: 12px ui-monospace, SFMono-Regular, monospace; color: #6b7280; }
.body { font: 12px ui-monospace, SFMono-Regular, monospace; }
```

- [ ] **Step 5: Run tests; expect pass**

```bash
cd webui && npx vitest run SourcePanel
```

Expected: PASS (3 cases).

- [ ] **Step 6: Commit**

```bash
git add webui/src/components/JsonView.tsx \
        webui/src/components/SourcePanel.tsx \
        webui/src/components/SourcePanel.module.css \
        webui/src/components/__tests__/SourcePanel.test.tsx
git commit -m "feat(webui): SourcePanel — lazy raw fetch + JsonView + 404/410 states"
```

---

## Task 11: MetaStrip + SessionDetailPage integration

**Files:**
- Create: `webui/src/components/MetaStrip.tsx`
- Create: `webui/src/routes/SessionDetailPage.tsx`
- Create: `webui/src/routes/SessionDetailPage.module.css`
- Modify: `webui/src/App.tsx`
- Test: `webui/src/routes/__tests__/SessionDetailPage.test.tsx`

- [ ] **Step 1: Failing test**

`webui/src/routes/__tests__/SessionDetailPage.test.tsx`:

```tsx
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import '@testing-library/jest-dom/vitest';
import SessionDetailPage from '../SessionDetailPage';

function rendered(sessionId: string) {
  return render(
    <MemoryRouter initialEntries={[`/sessions/${sessionId}`]}>
      <Routes>
        <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
      </Routes>
    </MemoryRouter>,
  );
}

const sessionDetail = {
  session_id: 's1',
  summary: {
    event_count: 2,
    by_kind: { user_message: 1, assistant_message: 1 },
    first_observed_at: '2026-05-19T10:00:00Z',
    last_observed_at: '2026-05-19T10:00:05Z',
  },
  events: [],
};

const graph = {
  nodes: [
    { node_id: 'n1', schema_version: '1.0', session_id: 's1', node_kind: 'user_message',
      started_at: '2026-05-19T10:00:00Z', ended_at: null, merge_keys: {},
      source_event_ids: ['ev1'], source_uris: [], payload: {} },
    { node_id: 'n2', schema_version: '1.0', session_id: 's1', node_kind: 'assistant_message',
      started_at: '2026-05-19T10:00:05Z', ended_at: null, merge_keys: {},
      source_event_ids: ['ev2'], source_uris: [], payload: {} },
  ],
  edges: [
    { edge_id: 'e1', schema_version: '1.0', session_id: 's1',
      from_node_id: 'n1', to_node_id: 'n2', edge_kind: 'message_reply',
      origin: 'deterministic', attributes: {} },
  ],
};

const raw = {
  schema_version: '1.0', event_id: 'ev1', session_id: 's1',
  source: { kind: 'transcript', file_path: '/tmp/a.jsonl', line_no: 1, ingested_at: 'n' },
  record: { hello: 'world' }, record_type: 'user_message', redaction_state: 'none',
};

function env(data: unknown) {
  return new Response(JSON.stringify({ meta: { generated_at: 'n' }, data }), {
    status: 200, headers: { 'content-type': 'application/json' },
  });
}

describe('SessionDetailPage', () => {
  beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });
  afterEach(() => { vi.unstubAllGlobals(); });

  it('renders meta strip + timeline + empty SourcePanel hint', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(env(sessionDetail));
    f.mockResolvedValueOnce(env(graph));
    rendered('s1');
    await waitFor(() => expect(screen.getByText(/2 events/)).toBeInTheDocument());
    expect(screen.getByText(/Click a node/i)).toBeInTheDocument();
  });

  it('clicking a node fetches raw and renders SourcePanel content', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(env(sessionDetail));
    f.mockResolvedValueOnce(env(graph));
    rendered('s1');
    const marker = await waitFor(() =>
      document.querySelector('[data-node-id="n1"]')!,
    );
    f.mockResolvedValueOnce(env(raw));
    fireEvent.click(marker);
    await waitFor(() => expect(screen.getByText('/tmp/a.jsonl')).toBeInTheDocument());
  });

  it('shows 404 when session detail missing', async () => {
    const f = fetch as unknown as ReturnType<typeof vi.fn>;
    f.mockResolvedValueOnce(new Response('{"detail":"session nope not found"}', { status: 404 }));
    f.mockResolvedValueOnce(new Response('{"detail":"no graph"}', { status: 404 }));
    rendered('nope');
    await waitFor(() => expect(screen.getByText(/session not found/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run; expect failure**

```bash
cd webui && npx vitest run SessionDetailPage
```

Expected: FAIL — module not found.

- [ ] **Step 3: `MetaStrip.tsx`**

```tsx
import type { SessionDetail } from '../api/types';

export function MetaStrip({ session }: { session: SessionDetail }) {
  const turns = new Set(
    session.events
      .map((e) => e.turn_id)
      .filter((t): t is string => Boolean(t)),
  ).size;
  return (
    <div>
      <strong>{session.summary.event_count} events</strong>
      {turns > 0 && <> · {turns} turns</>}
      {' · '}
      {session.summary.first_observed_at} → {session.summary.last_observed_at}
    </div>
  );
}
```

- [ ] **Step 4: `SessionDetailPage.tsx`**

```tsx
import { useEffect, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { ApiError, getGraph, getSession } from '../api/client';
import type { GraphPayload, SessionDetail } from '../api/types';
import { MetaStrip } from '../components/MetaStrip';
import { SourcePanel } from '../components/SourcePanel';
import { Timeline } from '../components/Timeline';
import styles from './SessionDetailPage.module.css';

type Loaded = { session: SessionDetail; graph: GraphPayload };
type State =
  | { kind: 'loading' }
  | { kind: 'ok'; data: Loaded }
  | { kind: 'not_found' }
  | { kind: 'error'; message: string };

export default function SessionDetailPage() {
  const { sessionId = '' } = useParams();
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setState({ kind: 'loading' });
    Promise.all([getSession(sessionId), getGraph(sessionId)])
      .then(([session, graph]) => {
        if (!cancelled) setState({ kind: 'ok', data: { session, graph } });
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        if (e instanceof ApiError && e.status === 404) setState({ kind: 'not_found' });
        else setState({ kind: 'error', message: e instanceof Error ? e.message : String(e) });
      });
    return () => { cancelled = true; };
  }, [sessionId]);

  const selectedEventId =
    state.kind === 'ok' && selectedNodeId
      ? state.data.graph.nodes.find((n) => n.node_id === selectedNodeId)
          ?.source_event_ids[0] ?? null
      : null;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to="/sessions">← Sessions</Link>
        <code>{sessionId}</code>
      </header>
      {state.kind === 'loading' && <p>Loading…</p>}
      {state.kind === 'not_found' && (
        <p>Session not found. <Link to="/sessions">Back to list</Link></p>
      )}
      {state.kind === 'error' && <p role="alert">{state.message}</p>}
      {state.kind === 'ok' && (
        <>
          <MetaStrip session={state.data.session} />
          <div className={styles.split}>
            <Timeline
              graph={state.data.graph}
              selectedNodeId={selectedNodeId}
              onSelect={setSelectedNodeId}
            />
            <SourcePanel eventId={selectedEventId} />
          </div>
        </>
      )}
    </div>
  );
}
```

`webui/src/routes/SessionDetailPage.module.css`:

```css
.page { padding: 24px; }
.header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
.split { display: grid; grid-template-columns: 3fr 2fr; gap: 16px; margin-top: 16px; }
```

- [ ] **Step 5: Wire route**

Edit `webui/src/App.tsx`:

```tsx
import { Navigate, Route, Routes } from 'react-router-dom';
import SessionListPage from './routes/SessionListPage';
import SessionDetailPage from './routes/SessionDetailPage';

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/sessions" replace />} />
      <Route path="/sessions" element={<SessionListPage />} />
      <Route path="/sessions/:sessionId" element={<SessionDetailPage />} />
      <Route path="*" element={<div style={{ padding: 24 }}>not found</div>} />
    </Routes>
  );
}
```

- [ ] **Step 6: Run tests; expect pass**

```bash
cd webui && npx vitest run
```

Expected: PASS (all FE tests across `api`, `routes`, `components`).

- [ ] **Step 7: Smoke build**

```bash
cd webui && npm run build
```

Expected: PASS. `webui/dist/index.html` and `webui/dist/assets/*` regenerated.

- [ ] **Step 8: Commit**

```bash
git add webui/src/components/MetaStrip.tsx \
        webui/src/routes/SessionDetailPage.tsx \
        webui/src/routes/SessionDetailPage.module.css \
        webui/src/routes/__tests__/SessionDetailPage.test.tsx \
        webui/src/App.tsx
git commit -m "feat(webui): SessionDetailPage — parallel fetch + timeline + SourcePanel wiring"
```

---

## Task 12: Documentation + finalize dist gitignore

**Files:**
- Modify: `.gitignore`
- Modify: `README.md`
- Modify: `docs/implementation-notes.html`

- [ ] **Step 1: Remove the placeholder exception from `.gitignore`**

The placeholder `webui/dist/index.html` is no longer needed — `webui-build` is now part of the documented dev flow and `webui/dist/` should be a build artifact again. Update `.gitignore`:

```
webui/node_modules/
webui/dist/
```

(Remove the `!webui/dist/index.html` line.)

Delete the committed placeholder file:

```bash
git rm webui/dist/index.html
```

Note: After this commit, `cargo build` requires `webui/dist/` to exist locally (`just webui-build` creates it). This is the steady state — documented in the README in the next step.

- [ ] **Step 2: README section on UI**

Append to `README.md`:

```markdown
## Web UI (slice-2)

The `witmcc` binary embeds a small React SPA at runtime. Build it once before
`cargo build`:

```
just webui-build      # cd webui && npm install && npm run build
just build-release    # cargo build --release
./target/release/witmcc serve --auto-migrate
# then open http://127.0.0.1:7878/
```

For frontend-only iteration:

```
just serve-dev        # axum on 127.0.0.1:7878
just webui-dev        # vite on 127.0.0.1:5173, proxies /v1 → 7878
```

The SPA has two pages:

- `/sessions` — session list
- `/sessions/:id` — six-lane timeline + raw source panel

Node 20 is required; see `webui/.nvmrc`.
```

- [ ] **Step 3: Implementation notes**

Open `docs/implementation-notes.html` and append a new `<section id="slice-2">…</section>` block following the same structure as the slice-1 deviations section. Capture:

- **DEV-S2-01: `event_id` lookup key instead of `event_uuid`** — design spec originally said `event_uuid`. observed_event PK `event_id` (ULID) is used because some record types (`file-history-snapshot`) carry no `event_uuid`.
- **DEV-S2-02: SPA fallback uses 200 for unknown routes** — axum `.fallback(spa_handler)` returns `index.html` with status 200 even for non-asset paths. SPA's `*` route shows "not found" client-side. Trade-off: deep-link refresh works, but `/random-path` returns 200; this is intentional.
- **DEV-S2-03: `webui/dist/` is build-required, not committed** — README and justfile call out `just webui-build` before `cargo build`. No `build.rs` automation to avoid forcing Node toolchain on cargo-only consumers.
- **DEV-S2-04: lane mapping table is client-side** — node_kind → lane is a TypeScript constant, not server-supplied. Trade-off: any new node_kind requires a webui change, but keeps server response stable.

Add to the commit-reference table once commits land.

- [ ] **Step 4: Verify backend tests still pass after dist cleanup**

After Step 1 deletes `webui/dist/index.html`, the static tests assert `<div id="root"` is present in the served HTML. The real Vite build (`just webui-build`) writes a `dist/index.html` with that root div, so the tests pass *as long as* `webui/dist/` exists.

Run:

```bash
just webui-build && cargo test
```

Expected: PASS — both Rust and TS tests green.

- [ ] **Step 5: Commit**

```bash
git add .gitignore README.md docs/implementation-notes.html
git commit -m "docs(slice-2): README + implementation notes for slice-2; drop dist placeholder"
```

---

## Task 13: Final verification (manual smoke + checklist)

**Files:** none modified — this task is a checklist.

- [ ] **Step 1: Clean rebuild**

```bash
just webui-build
cargo build --release
```

Expected: PASS. `target/release/witmcc` produced.

- [ ] **Step 2: Run a real ingest + serve session**

```bash
./target/release/witmcc init-db
./target/release/witmcc ingest --all
./target/release/witmcc serve --auto-migrate
```

- [ ] **Step 3: Browser smoke checklist**

Open `http://127.0.0.1:7878/` and verify:

| # | Check | Pass criterion |
|---|---|---|
| 1 | `/` redirects to `/sessions` | URL becomes `/sessions` |
| 2 | Session table populated | ≥1 row when ingest ran |
| 3 | Sort order | newer `last_observed_at` first |
| 4 | Click row | navigates to `/sessions/<id>` |
| 5 | Meta strip | shows `N events · M turns · …` |
| 6 | Timeline | 6 lanes rendered, ≥1 lane has nodes |
| 7 | Empty lanes | OTel + Quality show italic placeholder |
| 8 | Click a node | SourcePanel populates within ~1s |
| 9 | Raw record content | matches the original JSONL line |
| 10 | Deep-link refresh | `Cmd-R` on `/sessions/<id>` keeps the page |
| 11 | Edges | message_reply path visible between Intent/Context nodes |
| 12 | tool_call merged result | tool_call node has self-loop curve (if data present) |
| 13 | host allowlist | `curl -H 'Host: evil' http://127.0.0.1:7878/v1/health` → 400 |

- [ ] **Step 4: Run the whole test suite one more time**

```bash
cargo test && (cd webui && npm test)
```

Expected: PASS on both sides. No regressions to slice-1 tests.

- [ ] **Step 5: Commit any docs adjustments if checks revealed surprises**

If anything in Step 3 surprised you, jot it down in `docs/implementation-notes.html` and commit:

```bash
git add docs/implementation-notes.html
git commit -m "docs(slice-2): record smoke-test deviation/observation"
```

(If nothing surprised you, skip this step.)

---

## Spec Coverage Map (self-review)

| Spec section | Plan tasks |
|---|---|
| §2 Architecture (axum + rust-embed + SPA) | Tasks 1, 4, 5 |
| §3 New endpoint `GET /v1/events/:event_id/raw` | Tasks 2, 3 |
| §3 Existing endpoints unchanged | Verified in Task 4 (`v1_routes_are_not_swallowed_by_fallback`) |
| §4 Client routing | Tasks 5, 8, 11 |
| §4 Fetch sequence | Tasks 7, 8, 10, 11 |
| §4 Lane mapping | Task 7 (`laneMapping.ts`), Task 9 (Timeline) |
| §4 Edge visualization (deterministic, inferred, merged self-loop) | Task 9 |
| §5 Layout, components, libraries | Tasks 8, 9, 10, 11 |
| §6 Error / empty / loading | Tasks 8, 10, 11 |
| §7 Build & dev workflow | Tasks 5, 6, 12 |
| §8 Testing (Rust + FE + manual) | Tasks 2, 3, 4, 7, 8, 9, 10, 11, 13 |
| §9 Acceptance criteria | Task 13 |
| §11 Risks | Mitigated by Task 4 placeholder + Task 12 README |

If any row says "unmapped" — fix the plan before starting.

---

## Notes for the executing agent

1. **Run tests after every step that has them.** Slice-1 has a strict TDD discipline; keep it.
2. **Do not skip the placeholder `webui/dist/index.html` (Task 4) and its removal (Task 12).** The placeholder is what lets the Rust tests run before the FE scaffold exists. Removing it is what makes the steady-state workflow honest.
3. **Frontend tests run from `webui/`**, not the repo root. Either `cd webui` or use the justfile aliases.
4. **No backend schema change**, no new migration file, no graph algorithm change.
5. **No backwards-compat shims.** If a deviation matters, document it in `docs/implementation-notes.html` per CLAUDE.md.
6. **CLAUDE.md mandates browser smoke** for UI changes — Task 13 is not optional.
