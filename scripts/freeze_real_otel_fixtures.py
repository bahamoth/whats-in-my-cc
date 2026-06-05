#!/usr/bin/env python3
"""Redact PII and freeze the largest real OTel payload per signal as fixtures.

One-shot helper used during slice-6 capture. Reads .wimcc.sqlite, picks the
largest payload per source_type, replaces PII with stable placeholders, and
writes pretty JSON to tests/fixtures/otel/real/{metrics,logs,traces}_v01.json.

PII redacted (recursive walk of OTLP `attributes` / `resource` arrays):

    user.id           -> "00000000000000000000000000000000000000000000000000000000000000ab"
    user.email        -> "redacted@example.invalid"
    user.account_id   -> "user_REDACTED"
    user.account_uuid -> "11111111-1111-1111-1111-111111111111"
    organization.id   -> "22222222-2222-2222-2222-222222222222"
    session.id        -> "sess-real-A"

Kept verbatim: service.name, service.version, app.version, model, host.arch,
os.type, os.version, terminal.type, gen_ai.*, span.type, instrument names,
metric values, timestamps, hook_event values.
"""
from __future__ import annotations

import json
import pathlib
import sqlite3
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
DB = REPO / ".wimcc.sqlite"
OUT = REPO / "tests" / "fixtures" / "otel" / "real"

REDACT = {
    "user.id":           "00000000000000000000000000000000000000000000000000000000000000ab",
    "user.email":        "redacted@example.invalid",
    "user.account_id":   "user_REDACTED",
    "user.account_uuid": "11111111-1111-1111-1111-111111111111",
    "organization.id":   "22222222-2222-2222-2222-222222222222",
    "session.id":        "sess-real-A",
}

# Signals = (source_type, output filename stem)
SIGNALS = [
    ("otel-metrics", "metrics_v01"),
    ("otel-logs",    "logs_v01"),
    ("otel",         "traces_v01"),
]


def redact_attr(attr: dict) -> dict:
    """Mutate an OTLP attribute object in place if its key needs redacting."""
    key = attr.get("key")
    if key in REDACT and "value" in attr:
        # Only replace stringValue / intValue with stringValue placeholder.
        attr["value"] = {"stringValue": REDACT[key]}
    return attr


def walk(node) -> None:
    """Recursively walk JSON; redact any list element that looks like an OTLP attribute."""
    if isinstance(node, dict):
        # If this dict is itself an attribute object, redact it.
        if "key" in node and "value" in node and isinstance(node.get("value"), dict):
            redact_attr(node)
        for v in node.values():
            walk(v)
    elif isinstance(node, list):
        for item in node:
            walk(item)


def freeze_one(con: sqlite3.Connection, source_type: str, stem: str) -> int:
    row = con.execute(
        "SELECT payload, length(payload) AS n "
        "FROM raw_event WHERE source_type = ? "
        "ORDER BY length(payload) DESC LIMIT 1",
        (source_type,),
    ).fetchone()
    if row is None:
        print(f"  ! no rows for source_type={source_type}", file=sys.stderr)
        return 1
    payload_bytes, n = row
    obj = json.loads(payload_bytes)
    walk(obj)
    out_path = OUT / f"{stem}.json"
    out_path.write_text(json.dumps(obj, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"+ {source_type:<14} {n:>6} B raw -> {out_path.relative_to(REPO)}")
    return 0


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    rc = 0
    with sqlite3.connect(str(DB)) as con:
        for st, stem in SIGNALS:
            rc |= freeze_one(con, st, stem)
    return rc


if __name__ == "__main__":
    sys.exit(main())
