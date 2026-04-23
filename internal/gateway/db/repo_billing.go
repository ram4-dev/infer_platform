package db

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"github.com/uptrace/bun"
)

type BillingCustomer struct {
	ID                    string    `bun:",pk"`
	KeyID                 string    `bun:"key_id,notnull"`
	StripeCustomerID      string    `bun:"stripe_customer_id,notnull"`
	StripePaymentMethodID *string   `bun:"stripe_payment_method_id"`
	StripeSubscriptionID  *string   `bun:"stripe_subscription_id"`
	CreatedAt             time.Time `bun:"created_at,notnull"`
	UpdatedAt             time.Time `bun:"updated_at,notnull"`
}

func (BillingCustomer) TableName() string { return "billing_customers" }

type ProviderStripeAccount struct {
	ID                 string    `bun:",pk"`
	NodeID             string    `bun:"node_id,notnull"`
	StripeAccountID    string    `bun:"stripe_account_id,notnull"`
	OnboardingComplete bool      `bun:"onboarding_complete,notnull"`
	CreatedAt          time.Time `bun:"created_at,notnull"`
	UpdatedAt          time.Time `bun:"updated_at,notnull"`
}

func (ProviderStripeAccount) TableName() string { return "provider_stripe_accounts" }

type BillingMeterReport struct {
	ID            int64     `bun:",pk,autoincrement"`
	KeyID         string    `bun:"key_id,notnull"`
	PeriodStart   time.Time `bun:"period_start,notnull"`
	PeriodEnd     time.Time `bun:"period_end,notnull"`
	TokensIn      int64     `bun:"tokens_in,notnull"`
	TokensOut     int64     `bun:"tokens_out,notnull"`
	StripeEventID string    `bun:"stripe_event_id"`
	ReportedAt    time.Time `bun:"reported_at,notnull"`
}

func (BillingMeterReport) TableName() string { return "billing_meter_reports" }

type BillingRepository struct{ db *bun.DB }

func NewBillingRepository(db *bun.DB) *BillingRepository { return &BillingRepository{db: db} }

func (r *BillingRepository) FindCustomerByKeyID(ctx context.Context, keyID string) (*BillingCustomer, error) {
	var customer BillingCustomer
	if err := r.db.NewSelect().Model(&customer).Where("key_id = ?", keyID).Limit(1).Scan(ctx); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, nil
		}
		return nil, err
	}
	return &customer, nil
}

func (r *BillingRepository) CreateCustomer(ctx context.Context, customer BillingCustomer) error {
	_, err := r.db.NewInsert().Model(&customer).Exec(ctx)
	return err
}

func (r *BillingRepository) FindProviderAccountByNodeID(ctx context.Context, nodeID string) (*ProviderStripeAccount, error) {
	var account ProviderStripeAccount
	if err := r.db.NewSelect().Model(&account).Where("node_id = ?", nodeID).Limit(1).Scan(ctx); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, nil
		}
		return nil, err
	}
	return &account, nil
}

func (r *BillingRepository) CreateProviderAccount(ctx context.Context, account ProviderStripeAccount) error {
	_, err := r.db.NewInsert().Model(&account).Exec(ctx)
	return err
}

func (r *BillingRepository) UpdateProviderOnboarding(ctx context.Context, stripeAccountID string, complete bool) error {
	_, err := r.db.NewUpdate().Model((*ProviderStripeAccount)(nil)).Set("onboarding_complete = ?", complete).Set("updated_at = NOW()").Where("stripe_account_id = ?", stripeAccountID).Exec(ctx)
	return err
}

func (r *BillingRepository) TouchCustomerByStripeID(ctx context.Context, stripeCustomerID string) error {
	_, err := r.db.NewUpdate().Model((*BillingCustomer)(nil)).Set("updated_at = NOW()").Where("stripe_customer_id = ?", stripeCustomerID).Exec(ctx)
	return err
}

func (r *BillingRepository) ListCustomers(ctx context.Context) ([]BillingCustomer, error) {
	var customers []BillingCustomer
	err := r.db.NewSelect().Model(&customers).Scan(ctx)
	return customers, err
}

func (r *BillingRepository) MeterReportExists(ctx context.Context, keyID string, periodStart time.Time) (bool, error) {
	count, err := r.db.NewSelect().Model((*BillingMeterReport)(nil)).Where("key_id = ?", keyID).Where("period_start = ?", periodStart).Count(ctx)
	return count > 0, err
}

func (r *BillingRepository) InsertMeterReport(ctx context.Context, report BillingMeterReport) error {
	_, err := r.db.NewInsert().Model(&report).Ignore().Exec(ctx)
	return err
}

func (r *BillingRepository) ListPayoutProviders(ctx context.Context) ([]ProviderStripeAccount, error) {
	var providers []ProviderStripeAccount
	err := r.db.NewSelect().Model(&providers).Where("onboarding_complete = ?", true).Scan(ctx)
	return providers, err
}
