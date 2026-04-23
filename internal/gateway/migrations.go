package gateway

import "context"

var migrationStatements = []string{
	`CREATE TABLE IF NOT EXISTS nodes (
	    id TEXT PRIMARY KEY,
	    name TEXT UNIQUE NOT NULL,
	    host TEXT NOT NULL,
	    port INTEGER NOT NULL,
	    agent_port INTEGER NOT NULL DEFAULT 8181,
	    gpu_name TEXT NOT NULL,
	    vram_mb BIGINT NOT NULL,
	    status TEXT NOT NULL DEFAULT 'online',
	    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	    last_seen TIMESTAMPTZ NOT NULL DEFAULT NOW()
	);`,
	`CREATE TABLE IF NOT EXISTS api_keys (
	    id TEXT PRIMARY KEY,
	    key_hash TEXT UNIQUE NOT NULL,
	    owner TEXT NOT NULL,
	    rate_limit_rpm INTEGER NOT NULL DEFAULT 60,
	    daily_spend_cap_cents INTEGER DEFAULT NULL,
	    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	    revoked_at TIMESTAMPTZ DEFAULT NULL
	);`,
	`CREATE TABLE IF NOT EXISTS usage_logs (
	    id BIGSERIAL PRIMARY KEY,
	    key_id TEXT REFERENCES api_keys(id),
	    model TEXT NOT NULL,
	    tokens_in INTEGER NOT NULL DEFAULT 0,
	    tokens_out INTEGER NOT NULL DEFAULT 0,
	    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
	);`,
	`CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);`,
	`CREATE INDEX IF NOT EXISTS idx_usage_logs_key_id ON usage_logs(key_id);`,
	`CREATE INDEX IF NOT EXISTS idx_usage_logs_timestamp ON usage_logs(timestamp);`,
	`CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);`,
	`CREATE TABLE IF NOT EXISTS node_health (
	    id BIGSERIAL PRIMARY KEY,
	    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
	    latency_ms INTEGER,
	    success BOOLEAN NOT NULL,
	    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
	);`,
	`CREATE INDEX IF NOT EXISTS idx_node_health_node_checked ON node_health(node_id, checked_at DESC);`,
	`CREATE TABLE IF NOT EXISTS billing_customers (
	    id TEXT PRIMARY KEY,
	    key_id TEXT NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
	    stripe_customer_id TEXT NOT NULL UNIQUE,
	    stripe_payment_method_id TEXT,
	    stripe_subscription_id TEXT,
	    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
	);`,
	`CREATE TABLE IF NOT EXISTS provider_stripe_accounts (
	    id TEXT PRIMARY KEY,
	    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
	    stripe_account_id TEXT NOT NULL UNIQUE,
	    onboarding_complete BOOLEAN NOT NULL DEFAULT FALSE,
	    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
	);`,
	`CREATE TABLE IF NOT EXISTS billing_meter_reports (
	    id BIGSERIAL PRIMARY KEY,
	    key_id TEXT NOT NULL,
	    period_start TIMESTAMPTZ NOT NULL,
	    period_end TIMESTAMPTZ NOT NULL,
	    tokens_in BIGINT NOT NULL DEFAULT 0,
	    tokens_out BIGINT NOT NULL DEFAULT 0,
	    stripe_event_id TEXT,
	    reported_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
	);`,
	`CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_customers_key_id ON billing_customers(key_id);`,
	`CREATE INDEX IF NOT EXISTS idx_billing_customers_stripe ON billing_customers(stripe_customer_id);`,
	`CREATE INDEX IF NOT EXISTS idx_provider_stripe_node ON provider_stripe_accounts(node_id);`,
	`CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_meter_reports_key_period ON billing_meter_reports(key_id, period_start);`,
	`CREATE TABLE IF NOT EXISTS node_models (
	    id BIGSERIAL PRIMARY KEY,
	    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
	    model_name TEXT NOT NULL,
	    license TEXT NOT NULL,
	    registered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
	    UNIQUE (node_id, model_name)
	);`,
	`CREATE INDEX IF NOT EXISTS idx_node_models_node_id ON node_models(node_id);`,
	`CREATE INDEX IF NOT EXISTS idx_node_models_license ON node_models(license);`,
}

func (a *App) runMigrations(ctx context.Context) error {
	if a.db == nil {
		return nil
	}
	for _, stmt := range migrationStatements {
		if _, err := a.sqlDB.ExecContext(ctx, stmt); err != nil {
			return err
		}
	}
	return nil
}
