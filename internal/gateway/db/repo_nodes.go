package db

import (
	"context"
	"database/sql"
	"time"

	"github.com/uptrace/bun"
)

type NodeWithModel struct {
	ID           string    `bun:"id"`
	Name         string    `bun:"name"`
	Host         string    `bun:"host"`
	Port         int32     `bun:"port"`
	AgentPort    int32     `bun:"agent_port"`
	GPUName      string    `bun:"gpu_name"`
	VRAMMB       int64     `bun:"vram_mb"`
	Status       string    `bun:"status"`
	ModelName    *string   `bun:"model_name"`
	License      *string   `bun:"license"`
	RegisteredAt time.Time `bun:"registered_at"`
	LastSeen     time.Time `bun:"last_seen"`
}

type NodeRepository struct{ db *bun.DB }

func NewNodeRepository(db *bun.DB) *NodeRepository { return &NodeRepository{db: db} }

func (r *NodeRepository) UpsertNodeWithModel(ctx context.Context, node Node, model NodeModel) (NodeWithModel, error) {
	var persisted Node
	err := r.db.RunInTx(ctx, &sql.TxOptions{}, func(ctx context.Context, tx bun.Tx) error {
		_, err := tx.NewInsert().Model(&node).
			On("CONFLICT (name) DO UPDATE").
			Set("host = EXCLUDED.host").
			Set("port = EXCLUDED.port").
			Set("agent_port = EXCLUDED.agent_port").
			Set("gpu_name = EXCLUDED.gpu_name").
			Set("vram_mb = EXCLUDED.vram_mb").
			Set("status = 'online'").
			Set("last_seen = EXCLUDED.last_seen").
			Returning("id, name, host, port, agent_port, gpu_name, vram_mb, status, registered_at, last_seen").
			Exec(ctx, &persisted)
		if err != nil {
			return err
		}
		if _, err := tx.NewDelete().Model((*NodeModel)(nil)).Where("node_id = ?", persisted.ID).Exec(ctx); err != nil {
			return err
		}
		model.NodeID = persisted.ID
		if _, err := tx.NewInsert().Model(&model).Exec(ctx); err != nil {
			return err
		}
		return nil
	})
	if err != nil {
		return NodeWithModel{}, err
	}
	return NodeWithModel{
		ID: persisted.ID, Name: persisted.Name, Host: persisted.Host, Port: persisted.Port, AgentPort: persisted.AgentPort,
		GPUName: persisted.GPUName, VRAMMB: persisted.VRAMMB, Status: persisted.Status, ModelName: &model.ModelName,
		License: &model.License, RegisteredAt: persisted.RegisteredAt, LastSeen: persisted.LastSeen,
	}, nil
}

func (r *NodeRepository) ListNodes(ctx context.Context) ([]NodeWithModel, error) {
	var rows []NodeWithModel
	err := r.db.NewSelect().TableExpr("nodes AS n").
		ColumnExpr("n.id, n.name, n.host, n.port, n.agent_port, n.gpu_name, n.vram_mb, n.status, nm.model_name, nm.license, n.registered_at, n.last_seen").
		Join("LEFT JOIN LATERAL (SELECT model_name, license FROM node_models WHERE node_id = n.id ORDER BY registered_at DESC LIMIT 1) AS nm ON true").
		OrderExpr("n.registered_at DESC").
		Scan(ctx, &rows)
	return rows, err
}

func (r *NodeRepository) LoadOnlineNodesByModel(ctx context.Context, model string) ([]NodeWithModel, error) {
	var rows []NodeWithModel
	err := r.db.NewSelect().TableExpr("nodes AS n").
		ColumnExpr("n.id, n.name, n.host, n.port, n.agent_port, n.gpu_name, n.vram_mb, n.status, nm.model_name, nm.license, n.registered_at, n.last_seen").
		Join("JOIN node_models AS nm ON nm.node_id = n.id").
		Where("n.status = 'online'").
		Where("nm.model_name = ?", model).
		OrderExpr("n.registered_at DESC").
		Scan(ctx, &rows)
	return rows, err
}

func (r *NodeRepository) ListOnlineModels(ctx context.Context) ([]string, error) {
	var models []string
	err := r.db.NewSelect().TableExpr("node_models AS nm").
		ColumnExpr("DISTINCT nm.model_name").
		Join("JOIN nodes AS n ON n.id = nm.node_id").
		Where("n.status = 'online'").
		OrderExpr("nm.model_name").
		Scan(ctx, &models)
	return models, err
}

func (r *NodeRepository) CountOnlineNodes(ctx context.Context) (int, error) {
	count, err := r.db.NewSelect().Table("nodes").Where("status = 'online'").Count(ctx)
	return count, err
}

func (r *NodeRepository) UpdateStaleNodes(ctx context.Context, cutoff time.Time) error {
	_, err := r.db.NewUpdate().Table("nodes").Set("status = 'offline'").Where("last_seen < ?", cutoff).Where("status != 'offline'").Exec(ctx)
	return err
}

func (r *NodeRepository) ListProbeTargets(ctx context.Context) ([]Node, error) {
	var nodes []Node
	err := r.db.NewSelect().Model(&nodes).OrderExpr("registered_at DESC").Scan(ctx)
	return nodes, err
}

func (r *NodeRepository) InsertHealthProbe(ctx context.Context, nodeID string, success bool, latency *int32) error {
	_, err := r.db.ExecContext(ctx, `INSERT INTO node_health (node_id, latency_ms, success) VALUES (?, ?, ?)`, nodeID, latency, success)
	return err
}

func (r *NodeRepository) RecentProbeSuccesses(ctx context.Context, nodeID string, limit int) ([]bool, error) {
	var values []bool
	err := r.db.NewSelect().Table("node_health").Column("success").Where("node_id = ?", nodeID).OrderExpr("checked_at DESC").Limit(limit).Scan(ctx, &values)
	return values, err
}

func (r *NodeRepository) UpdateNodeStatus(ctx context.Context, nodeID, status string) error {
	_, err := r.db.NewUpdate().Table("nodes").Set("status = ?", status).Where("id = ?", nodeID).Exec(ctx)
	return err
}
