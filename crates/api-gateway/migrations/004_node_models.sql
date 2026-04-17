CREATE TABLE IF NOT EXISTS node_models (
    id          BIGSERIAL PRIMARY KEY,
    node_id     TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    model_name  TEXT NOT NULL,
    license     TEXT NOT NULL,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (node_id, model_name)
);

CREATE INDEX IF NOT EXISTS idx_node_models_node_id ON node_models(node_id);
CREATE INDEX IF NOT EXISTS idx_node_models_license ON node_models(license);
