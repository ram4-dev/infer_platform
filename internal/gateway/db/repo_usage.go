package db

import (
	"context"
	"time"

	"github.com/uptrace/bun"
)

type UsageTotals struct {
	TokensIn  *int64 `bun:"tokens_in"`
	TokensOut *int64 `bun:"tokens_out"`
}

type UsageRepository struct{ db *bun.DB }

func NewUsageRepository(db *bun.DB) *UsageRepository { return &UsageRepository{db: db} }

func (r *UsageRepository) Insert(ctx context.Context, usage UsageLog) error {
	if usage.Timestamp.IsZero() {
		usage.Timestamp = time.Now().UTC()
	}
	_, err := r.db.NewInsert().Model(&usage).Exec(ctx)
	return err
}

func (r *UsageRepository) TotalsForKeyBetween(ctx context.Context, keyID string, from, to time.Time) (UsageTotals, error) {
	row := UsageTotals{}
	err := r.db.NewSelect().TableExpr("usage_logs").
		ColumnExpr("SUM(tokens_in) AS tokens_in").
		ColumnExpr("SUM(tokens_out) AS tokens_out").
		Where("key_id = ?", keyID).
		Where("timestamp >= ?", from).
		Where("timestamp < ?", to).
		Scan(ctx, &row)
	return row, err
}

func (r *UsageRepository) TotalsBetween(ctx context.Context, from, to time.Time) (UsageTotals, error) {
	row := UsageTotals{}
	err := r.db.NewSelect().TableExpr("usage_logs").
		ColumnExpr("SUM(tokens_in) AS tokens_in").
		ColumnExpr("SUM(tokens_out) AS tokens_out").
		Where("timestamp >= ?", from).
		Where("timestamp < ?", to).
		Scan(ctx, &row)
	return row, err
}
