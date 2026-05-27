-- Slice-18: add redaction_manifest column to raw_event.
-- The column is NULL for rows inserted before this migration (pre-slice-18).
-- Slice-18 ingest wiring populates it for all new rows.
ALTER TABLE raw_event ADD COLUMN redaction_manifest TEXT;
