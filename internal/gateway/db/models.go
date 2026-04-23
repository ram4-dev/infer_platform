package db

import "time"

type Node struct {
	ID           string    `bun:",pk"`
	Name         string    `bun:",unique,notnull"`
	Host         string    `bun:",notnull"`
	Port         int32     `bun:",notnull"`
	AgentPort    int32     `bun:"agent_port,notnull"`
	GPUName      string    `bun:"gpu_name,notnull"`
	VRAMMB       int64     `bun:"vram_mb,notnull"`
	Status       string    `bun:",notnull"`
	RegisteredAt time.Time `bun:"registered_at,notnull"`
	LastSeen     time.Time `bun:"last_seen,notnull"`
}

func (Node) TableName() string { return "nodes" }

type NodeModel struct {
	ID           int64     `bun:",pk,autoincrement"`
	NodeID       string    `bun:"node_id,notnull"`
	ModelName    string    `bun:"model_name,notnull"`
	License      string    `bun:",notnull"`
	RegisteredAt time.Time `bun:"registered_at,notnull"`
}

func (NodeModel) TableName() string { return "node_models" }

type APIKey struct {
	ID                 string     `bun:",pk"`
	KeyHash            string     `bun:"key_hash,notnull"`
	Owner              string     `bun:",notnull"`
	RateLimitRPM       int32      `bun:"rate_limit_rpm,notnull"`
	DailySpendCapCents *int32     `bun:"daily_spend_cap_cents"`
	CreatedAt          time.Time  `bun:"created_at,notnull"`
	RevokedAt          *time.Time `bun:"revoked_at"`
}

func (APIKey) TableName() string { return "api_keys" }

type UsageLog struct {
	ID        int64     `bun:",pk,autoincrement"`
	KeyID     string    `bun:"key_id"`
	Model     string    `bun:",notnull"`
	TokensIn  int32     `bun:"tokens_in,notnull"`
	TokensOut int32     `bun:"tokens_out,notnull"`
	Timestamp time.Time `bun:",notnull"`
}

func (UsageLog) TableName() string { return "usage_logs" }
