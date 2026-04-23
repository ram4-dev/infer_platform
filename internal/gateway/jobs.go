package gateway

import (
	"context"
	"fmt"
	"net/http"
	"time"
)

func (a *App) SpawnBackgroundJobs(ctx context.Context) {
	go a.runStaleSweep(ctx)
	go a.runHealthMonitor(ctx)
	a.spawnBillingJobs(ctx)
}

func (a *App) runStaleSweep(ctx context.Context) {
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			cutoff := time.Now().UTC().Add(-90 * time.Second)
			_ = a.nodeRepo.UpdateStaleNodes(ctx, cutoff)
		}
	}
}

func (a *App) runHealthMonitor(ctx context.Context) {
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()
	client := &http.Client{Timeout: 5 * time.Second}
	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			targets, _ := a.collectProbeTargets(ctx)
			for _, n := range targets {
				start := time.Now()
				resp, err := client.Get(fmt.Sprintf("http://%s:%d/ping", n.Host, n.AgentPort))
				success := err == nil && resp != nil && resp.StatusCode/100 == 2
				latency := int32(time.Since(start).Milliseconds())
				if resp != nil {
					resp.Body.Close()
				}
				_ = a.probeDB(ctx, n.ID, success, latency)
			}
		}
	}
}

func (a *App) collectProbeTargets(ctx context.Context) ([]NodeInfo, error) {
	return a.listNodesDB(ctx)
}

func (a *App) probeDB(ctx context.Context, nodeID string, success bool, latency int32) error {
	var latencyValue any
	if success {
		latencyValue = latency
	}
	if _, err := a.sqlDB.ExecContext(ctx, `INSERT INTO node_health (node_id, latency_ms, success) VALUES ($1,$2,$3)`, nodeID, latencyValue, success); err != nil {
		return err
	}
	rows, err := a.sqlDB.QueryContext(ctx, `SELECT success FROM node_health WHERE node_id = $1 ORDER BY checked_at DESC LIMIT 10`, nodeID)
	if err != nil {
		return err
	}
	defer rows.Close()
	consec := 0
	for rows.Next() {
		var ok bool
		if err := rows.Scan(&ok); err != nil {
			continue
		}
		if !ok {
			consec++
		} else {
			break
		}
	}
	status := "online"
	if consec >= 10 {
		status = "offline"
	} else if consec >= 3 {
		status = "degraded"
	}
	_, _ = a.sqlDB.ExecContext(ctx, `UPDATE nodes SET status = $1 WHERE id = $2`, status, nodeID)
	var p50, p95, uptime *float64
	_ = a.sqlDB.QueryRowContext(ctx, `SELECT percentile_cont(0.50) WITHIN GROUP (ORDER BY latency_ms), percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms) FROM node_health WHERE node_id = $1 AND checked_at > NOW() - INTERVAL '1 hour' AND success = true`, nodeID).Scan(&p50, &p95)
	_ = a.sqlDB.QueryRowContext(ctx, `SELECT COUNT(*) FILTER (WHERE success)::float8 / NULLIF(COUNT(*),0)::float8 FROM node_health WHERE node_id = $1 AND checked_at > NOW() - INTERVAL '7 days'`, nodeID).Scan(&uptime)
	a.store.SetNodeStats(nodeID, NodeStats{P50MS: derefFloat64(p50), P95MS: derefFloat64(p95), Uptime7D: derefFloat64(uptime)})
	return nil
}

func derefFloat64(v *float64) float64 {
	if v == nil {
		return 0
	}
	return *v
}
