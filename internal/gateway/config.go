package gateway

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	gwdb "infer_platform/internal/gateway/db"

	"github.com/joho/godotenv"
	"github.com/redis/go-redis/v9"
)

func LoadConfig() Config {
	_ = godotenv.Load()
	cfg := Config{
		Port:                 getenvDefault("PORT", "8080"),
		InternalKey:          getenvDefault("INFER_INTERNAL_KEY", "internal_dev_secret"),
		OllamaURL:            getenvDefault("OLLAMA_URL", "http://localhost:11434"),
		RoutingMode:          getenvDefault("ROUTING_MODE", "single_node_model"),
		DatabaseURL:          os.Getenv("DATABASE_URL"),
		RedisURL:             os.Getenv("REDIS_URL"),
		ProviderTokenUSDRate: getenvFloat("PROVIDER_TOKEN_RATE_USD", 0.000001),
		ProviderRevenueShare: getenvFloat("PROVIDER_REVENUE_SHARE", 0.70),
	}

	if secret := os.Getenv("STRIPE_SECRET_KEY"); secret != "" {
		cfg.Stripe = &StripeConfig{
			SecretKey:         secret,
			WebhookSecret:     os.Getenv("STRIPE_WEBHOOK_SECRET"),
			MeterEventName:    getenvDefault("STRIPE_METER_EVENT_NAME", "tokens_used"),
			PriceID:           os.Getenv("STRIPE_PRICE_ID"),
			TokenRateUSDPer1K: getenvFloat("TOKEN_RATE_USD_PER_1K", 0.002),
			CommissionRate:    getenvFloat("COMMISSION_RATE", 0.20),
		}
	}
	return cfg
}

func NewApp(ctx context.Context, cfg Config) (*App, error) {
	if strings.TrimSpace(cfg.DatabaseURL) == "" {
		return nil, ErrDatabaseRequired
	}

	app := &App{
		cfg:        cfg,
		httpClient: &http.Client{Timeout: 120 * time.Second},
		store:      NewNodeStore(),
		router:     NewModelRouter(),
		catalog:    NewModelCatalog(),
	}
	for _, l := range app.catalog.ApprovedLicenses() {
		if app.approvedLics == nil {
			app.approvedLics = map[string]struct{}{}
		}
		app.approvedLics[l] = struct{}{}
	}

	bunDB, sqlDB, err := gwdb.Open(ctx, cfg.DatabaseURL)
	if err != nil {
		return nil, err
	}
	app.db = bunDB
	app.sqlDB = sqlDB
	app.nodeRepo = gwdb.NewNodeRepository(bunDB)
	app.keyRepo = gwdb.NewKeyRepository(bunDB)
	app.usageRepo = gwdb.NewUsageRepository(bunDB)
	app.statsRepo = gwdb.NewStatsRepository(bunDB)
	app.billingRepo = gwdb.NewBillingRepository(bunDB)
	if err := app.runMigrations(ctx); err != nil {
		return nil, err
	}
	if cfg.RedisURL != "" {
		app.redis = redis.NewClient(&redis.Options{Addr: strings.TrimPrefix(cfg.RedisURL, "redis://")})
	}
	return app, nil
}

func getenvDefault(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}

func getenvFloat(key string, def float64) float64 {
	if v := os.Getenv(key); v != "" {
		if p, err := strconv.ParseFloat(v, 64); err == nil {
			return p
		}
	}
	return def
}

var ErrDatabaseRequired = errors.New("DATABASE_URL is required")

func Logger() *slog.Logger {
	return slog.Default()
}
