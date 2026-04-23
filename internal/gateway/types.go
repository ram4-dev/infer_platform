package gateway

import (
	"database/sql"
	"net/http"
	"time"

	gwdb "infer_platform/internal/gateway/db"

	"github.com/redis/go-redis/v9"
	"github.com/uptrace/bun"
)

type NodeStatus string

const (
	NodeStatusOnline   NodeStatus = "online"
	NodeStatusOffline  NodeStatus = "offline"
	NodeStatusBusy     NodeStatus = "busy"
	NodeStatusDegraded NodeStatus = "degraded"
)

type NodeInfo struct {
	ID           string     `json:"id"`
	Name         string     `json:"name"`
	Host         string     `json:"host"`
	Port         uint16     `json:"port"`
	AgentPort    uint16     `json:"agent_port"`
	GPUName      string     `json:"gpu_name"`
	VRAMMB       uint64     `json:"vram_mb"`
	Status       NodeStatus `json:"status"`
	Model        *string    `json:"model,omitempty"`
	License      *string    `json:"license,omitempty"`
	RegisteredAt time.Time  `json:"registered_at"`
	LastSeen     time.Time  `json:"last_seen"`
}

type ModelRegistration struct {
	Name    string `json:"name"`
	License string `json:"license"`
}

type RegisterNodeRequest struct {
	Name      string              `json:"name"`
	Host      string              `json:"host"`
	Port      uint16              `json:"port"`
	AgentPort uint16              `json:"agent_port"`
	GPUName   string              `json:"gpu_name"`
	VRAMMB    uint64              `json:"vram_mb"`
	Model     *string             `json:"model,omitempty"`
	License   *string             `json:"license,omitempty"`
	Models    []ModelRegistration `json:"models,omitempty"`
}

type ChatMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

type ChatCompletionRequest struct {
	Model       string        `json:"model"`
	Messages    []ChatMessage `json:"messages"`
	Stream      bool          `json:"stream,omitempty"`
	MaxTokens   *uint32       `json:"max_tokens,omitempty"`
	Temperature *float32      `json:"temperature,omitempty"`
	TopP        *float32      `json:"top_p,omitempty"`
}

type NodeStats struct {
	P50MS    float64 `json:"p50_ms"`
	P95MS    float64 `json:"p95_ms"`
	Uptime7D float64 `json:"uptime_7d"`
}

type ValidatedKey struct {
	KeyID        string
	RateLimitRPM int64
}

type StripeConfig struct {
	SecretKey         string
	WebhookSecret     string
	MeterEventName    string
	PriceID           string
	TokenRateUSDPer1K float64
	CommissionRate    float64
}

type Config struct {
	Port                 string
	InternalKey          string
	OllamaURL            string
	RoutingMode          string
	DatabaseURL          string
	RedisURL             string
	Stripe               *StripeConfig
	ProviderTokenUSDRate float64
	ProviderRevenueShare float64
}

type App struct {
	cfg          Config
	db           *bun.DB
	sqlDB        *sql.DB
	redis        *redis.Client
	httpClient   HTTPDoer
	store        *NodeStore
	router       *ModelRouter
	catalog      *ModelCatalog
	nodeRepo     *gwdb.NodeRepository
	keyRepo      *gwdb.KeyRepository
	usageRepo    *gwdb.UsageRepository
	statsRepo    *gwdb.StatsRepository
	billingRepo  *gwdb.BillingRepository
	approvedLics map[string]struct{}
}

type HTTPDoer interface {
	Do(req *http.Request) (*http.Response, error)
}
