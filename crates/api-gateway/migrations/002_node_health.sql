CREATE TABLE IF NOT EXISTS node_health (
    id          BIGSERIAL PRIMARY KEY,
    node_id     TEXT        NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    latency_ms  INTEGER,
    success     BOOLEAN     NOT NULL,
    checked_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_node_health_node_checked
    ON node_health(node_id, checked_at DESC);
