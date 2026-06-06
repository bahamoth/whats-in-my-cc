-- Judge subsystem removed: extractors now promote deterministically at L1.
DROP TABLE IF EXISTS judge_verdict_cache;
DROP TABLE IF EXISTS findings_pending_judge;
