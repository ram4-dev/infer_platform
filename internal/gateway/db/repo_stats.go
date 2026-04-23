package db

import (
	"context"
	"time"

	"github.com/uptrace/bun"
)

type InferStats struct {
	P50MS    *float64
	P95MS    *float64
	Uptime7D *float64
}

type UsageSummaryRow struct {
	KeyID          *string
	RequestCount   int64
	TotalTokensIn  int64
	TotalTokensOut int64
}

type ConsumerTotals struct {
	TotalRequests int64
	TotalIn       int64
	TotalOut      int64
}

type ConsumerModelRow struct {
	Model        string
	RequestCount int64
	TokensIn     int64
	TokensOut    int64
}

type ConsumerDailyRow struct {
	Date         string
	RequestCount int64
	Tokens       int64
}

type ModelStatsRow struct {
	ModelName  string
	License    *string
	NodeCount  int64
	AvgLatency *float64
	Uptime7D   *float64
}

type ProviderHealthRow struct {
	NodeID       string
	ProbeCount   int64
	SuccessCount int64
	Latency      *float64
}

type ProviderUsageRow struct {
	NodeID       string
	RequestCount int64
	TokensIn     int64
	TokensOut    int64
}

type NodeModelRow struct {
	NodeID    string
	ModelName string
}

type ProviderStripeRow struct {
	NodeID             string
	OnboardingComplete bool
}

type StatsRepository struct{ db *bun.DB }

func NewStatsRepository(db *bun.DB) *StatsRepository { return &StatsRepository{db: db} }

func (r *StatsRepository) InferStats(ctx context.Context) (InferStats, error) {
	row := struct {
		P50MS    *float64 `bun:"p50_ms"`
		P95MS    *float64 `bun:"p95_ms"`
		Uptime7D *float64 `bun:"uptime_7d"`
	}{}
	err := r.db.NewSelect().TableExpr("node_health AS nh").
		ColumnExpr("percentile_cont(0.50) WITHIN GROUP (ORDER BY nh.latency_ms) AS p50_ms").
		ColumnExpr("percentile_cont(0.95) WITHIN GROUP (ORDER BY nh.latency_ms) AS p95_ms").
		ColumnExpr("COUNT(*) FILTER (WHERE nh.success)::float8 / NULLIF(COUNT(*),0)::float8 AS uptime_7d").
		Join("JOIN nodes AS n ON nh.node_id = n.id").
		Where("n.status = 'online'").
		Where("nh.checked_at > NOW() - INTERVAL '7 days'").
		Where("nh.success = true").
		Scan(ctx, &row)
	return InferStats{P50MS: row.P50MS, P95MS: row.P95MS, Uptime7D: row.Uptime7D}, err
}

func (r *StatsRepository) UsageSummary(ctx context.Context) ([]UsageSummaryRow, error) {
	var rows []UsageSummaryRow
	err := r.db.NewSelect().TableExpr("usage_logs").
		ColumnExpr("key_id").
		ColumnExpr("COUNT(*)::bigint AS request_count").
		ColumnExpr("COALESCE(SUM(tokens_in),0)::bigint AS total_tokens_in").
		ColumnExpr("COALESCE(SUM(tokens_out),0)::bigint AS total_tokens_out").
		GroupExpr("key_id").
		OrderExpr("request_count DESC").
		Scan(ctx, &rows)
	return rows, err
}

func (r *StatsRepository) ConsumerTotals(ctx context.Context, keyFilter string, from, to time.Time) (ConsumerTotals, error) {
	row := ConsumerTotals{}
	err := r.db.NewSelect().TableExpr("usage_logs").
		ColumnExpr("COUNT(*)::bigint AS total_requests").
		ColumnExpr("COALESCE(SUM(tokens_in),0)::bigint AS total_in").
		ColumnExpr("COALESCE(SUM(tokens_out),0)::bigint AS total_out").
		Where("(? = '' OR key_id = ?)", keyFilter, keyFilter).
		Where("timestamp >= ?", from).
		Where("timestamp <= ?", to).
		Scan(ctx, &row)
	return row, err
}

func (r *StatsRepository) ConsumerByModel(ctx context.Context, keyFilter string, from, to time.Time) ([]ConsumerModelRow, error) {
	var rows []ConsumerModelRow
	err := r.db.NewSelect().TableExpr("usage_logs").
		ColumnExpr("model").
		ColumnExpr("COUNT(*)::bigint AS request_count").
		ColumnExpr("COALESCE(SUM(tokens_in),0)::bigint AS tokens_in").
		ColumnExpr("COALESCE(SUM(tokens_out),0)::bigint AS tokens_out").
		Where("(? = '' OR key_id = ?)", keyFilter, keyFilter).
		Where("timestamp >= ?", from).
		Where("timestamp <= ?", to).
		GroupExpr("model").
		OrderExpr("SUM(tokens_in + tokens_out) DESC").
		Scan(ctx, &rows)
	return rows, err
}

func (r *StatsRepository) ConsumerDaily(ctx context.Context, keyFilter string, from, to time.Time) ([]ConsumerDailyRow, error) {
	var rows []ConsumerDailyRow
	err := r.db.NewSelect().TableExpr("usage_logs").
		ColumnExpr("TO_CHAR(DATE_TRUNC('day', timestamp), 'YYYY-MM-DD') AS date").
		ColumnExpr("COUNT(*)::bigint AS request_count").
		ColumnExpr("COALESCE(SUM(tokens_in + tokens_out),0)::bigint AS tokens").
		Where("(? = '' OR key_id = ?)", keyFilter, keyFilter).
		Where("timestamp >= ?", from).
		Where("timestamp <= ?", to).
		GroupExpr("DATE_TRUNC('day', timestamp)").
		OrderExpr("DATE_TRUNC('day', timestamp) ASC").
		Scan(ctx, &rows)
	return rows, err
}

func (r *StatsRepository) ModelStats(ctx context.Context) ([]ModelStatsRow, error) {
	var rows []ModelStatsRow
	err := r.db.NewSelect().TableExpr("node_models AS nm").
		ColumnExpr("nm.model_name").
		ColumnExpr("MIN(nm.license) AS license").
		ColumnExpr("COUNT(DISTINCT nm.node_id)::bigint AS node_count").
		ColumnExpr("AVG(CASE WHEN nh.success THEN nh.latency_ms END) AS avg_latency").
		ColumnExpr("SUM(CASE WHEN nh.success THEN 1 ELSE 0 END)::float8 / NULLIF(COUNT(nh.id),0)::float8 AS uptime_7d").
		Join("JOIN nodes AS n ON n.id = nm.node_id AND n.status = 'online'").
		Join("LEFT JOIN node_health AS nh ON nh.node_id = nm.node_id AND nh.checked_at > NOW() - INTERVAL '7 days'").
		GroupExpr("nm.model_name").
		OrderExpr("nm.model_name").
		Scan(ctx, &rows)
	return rows, err
}

func (r *StatsRepository) ProviderHealth(ctx context.Context) ([]ProviderHealthRow, error) {
	var rows []ProviderHealthRow
	err := r.db.NewSelect().TableExpr("node_health").
		ColumnExpr("node_id").
		ColumnExpr("COUNT(*)::bigint AS probe_count").
		ColumnExpr("SUM(CASE WHEN success THEN 1 ELSE 0 END)::bigint AS success_count").
		ColumnExpr("AVG(CASE WHEN success THEN latency_ms END) AS latency").
		Where("checked_at > NOW() - INTERVAL '7 days'").
		GroupExpr("node_id").
		Scan(ctx, &rows)
	return rows, err
}

func (r *StatsRepository) ProviderUsage(ctx context.Context) ([]ProviderUsageRow, error) {
	var rows []ProviderUsageRow
	err := r.db.NewSelect().TableExpr("usage_logs AS ul").
		ColumnExpr("nm.node_id").
		ColumnExpr("COUNT(*)::bigint AS request_count").
		ColumnExpr("COALESCE(SUM(ul.tokens_in),0)::bigint AS tokens_in").
		ColumnExpr("COALESCE(SUM(ul.tokens_out),0)::bigint AS tokens_out").
		Join("JOIN node_models AS nm ON nm.model_name = ul.model").
		Where("ul.timestamp > NOW() - INTERVAL '7 days'").
		GroupExpr("nm.node_id").
		Scan(ctx, &rows)
	return rows, err
}

func (r *StatsRepository) NodeModels(ctx context.Context) ([]NodeModelRow, error) {
	var rows []NodeModelRow
	err := r.db.NewSelect().Model((*NodeModel)(nil)).
		Column("node_id", "model_name").
		OrderExpr("registered_at").
		Scan(ctx, &rows)
	return rows, err
}

func (r *StatsRepository) ProviderStripeStatuses(ctx context.Context) ([]ProviderStripeRow, error) {
	var rows []ProviderStripeRow
	err := r.db.NewSelect().TableExpr("provider_stripe_accounts").
		ColumnExpr("node_id").
		ColumnExpr("onboarding_complete").
		Scan(ctx, &rows)
	return rows, err
}
