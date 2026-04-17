-- Stripe Customer records, linked to API keys (consumer-side billing)
CREATE TABLE billing_customers (
  id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
  key_id TEXT NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
  stripe_customer_id TEXT NOT NULL UNIQUE,
  stripe_payment_method_id TEXT,
  stripe_subscription_id TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Stripe Connect Express accounts, linked to GPU nodes (provider-side payouts)
CREATE TABLE provider_stripe_accounts (
  id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
  node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  stripe_account_id TEXT NOT NULL UNIQUE,
  onboarding_complete BOOLEAN NOT NULL DEFAULT FALSE,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Tracks which usage periods have already been reported to Stripe Meters API
CREATE TABLE billing_meter_reports (
  id BIGSERIAL PRIMARY KEY,
  key_id TEXT NOT NULL,
  period_start TIMESTAMPTZ NOT NULL,
  period_end TIMESTAMPTZ NOT NULL,
  tokens_in BIGINT NOT NULL DEFAULT 0,
  tokens_out BIGINT NOT NULL DEFAULT 0,
  stripe_event_id TEXT,
  reported_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_billing_customers_key_id ON billing_customers(key_id);
CREATE INDEX idx_billing_customers_stripe ON billing_customers(stripe_customer_id);
CREATE INDEX idx_provider_stripe_node ON provider_stripe_accounts(node_id);
CREATE UNIQUE INDEX idx_billing_meter_reports_key_period ON billing_meter_reports(key_id, period_start);
