-- Slice-18: add redaction_state and redaction_manifest columns to raw_event.
-- redaction_state replaces the per-row "unredacted" placeholder approach;
-- values are: "redacted" | "not_redacted" | "not_applicable".
-- redaction_manifest is a JSON-serialised RedactionManifest (nullable).
-- Both columns are NULL for pre-slice-18 rows.
ALTER TABLE raw_event ADD COLUMN redaction_state TEXT;
ALTER TABLE raw_event ADD COLUMN redaction_manifest TEXT;
