-- Slice-13: add inference columns to graph_edge.
-- NULL on both columns for all existing deterministic edges.
ALTER TABLE graph_edge ADD COLUMN inference_rule_id TEXT;
ALTER TABLE graph_edge ADD COLUMN confidence REAL;

CREATE INDEX IF NOT EXISTS idx_graph_edge_rule
    ON graph_edge(inference_rule_id);
