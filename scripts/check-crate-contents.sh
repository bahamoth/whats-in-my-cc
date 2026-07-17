#!/usr/bin/env bash
# crates.io 패키지에 컴파일 타임 임베드 자산이 동봉됐는지 게이트.
# rust-embed(webui/dist)·include_str!(pricing.json)·sqlx migrate!(migrations)가
# 빠지면 cargo install 사용자 쪽에서 빌드가 깨진다 — 스펙 §2.
set -euo pipefail
cd "$(dirname "$0")/.."
list=$(cargo package --list --allow-dirty)
missing=0
for path in webui/dist/index.html pricing.json migrations LICENSE-MIT LICENSE-APACHE; do
  if ! grep -q "$path" <<<"$list"; then
    echo "MISSING in crate: $path"
    missing=1
  fi
done
[ "$missing" -eq 0 ] && echo "crate contents OK"
exit "$missing"
