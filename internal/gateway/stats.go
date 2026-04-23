package gateway

import (
	"context"
	"net/http"
	"sort"
	"time"

	"github.com/go-chi/chi/v5"
)

func (a *App) handleListModels(w http.ResponseWriter, r *http.Request) {
	models, err := a.listOnlineModels(r.Context())
	if err != nil {
		writeJSON(w, http.StatusOK, map[string]any{"object": "list", "data": []any{}})
		return
	}
	data := make([]map[string]any, 0, len(models))
	now := time.Now().UTC().Unix()
	for _, model := range models {
		data = append(data, map[string]any{"id": model, "object": "model", "created": now, "owned_by": "infer-platform"})
	}
	writeJSON(w, http.StatusOK, map[string]any{"object": "list", "data": data})
}

func (a *App) listOnlineModels(ctx context.Context) ([]string, error) {
	return a.nodeRepo.ListOnlineModels(ctx)
}

func (a *App) handleGetModel(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{
		"id":          chi.URLParam(r, "id"),
		"object":      "model",
		"created":     time.Now().UTC().Unix(),
		"owned_by":    "infer-platform",
		"infer_stats": a.buildInferStats(r.Context()),
	})
}

func (a *App) buildInferStats(ctx context.Context) map[string]any {
	statsSnapshot := a.store.GetNodeStats()
	availableNodes, _ := a.nodeRepo.CountOnlineNodes(ctx)
	stats, _ := a.statsRepo.InferStats(ctx)
	if stats.P50MS != nil || stats.P95MS != nil || stats.Uptime7D != nil {
		return map[string]any{"available_nodes": availableNodes, "latency_p50_ms": stats.P50MS, "latency_p95_ms": stats.P95MS, "uptime_7d": stats.Uptime7D}
	}
	var samples []NodeStats
	for _, s := range statsSnapshot {
		samples = append(samples, s)
	}
	if len(samples) == 0 {
		return map[string]any{"available_nodes": availableNodes, "latency_p50_ms": nil, "latency_p95_ms": nil, "uptime_7d": nil}
	}
	var p50v, p95v, uptimev float64
	for _, s := range samples {
		p50v += s.P50MS
		p95v += s.P95MS
		uptimev += s.Uptime7D
	}
	return map[string]any{"available_nodes": availableNodes, "latency_p50_ms": p50v / float64(len(samples)), "latency_p95_ms": p95v / float64(len(samples)), "uptime_7d": uptimev / float64(len(samples))}
}

func (a *App) handleUsageSummary(w http.ResponseWriter, r *http.Request) {
	rows, err := a.statsRepo.UsageSummary(r.Context())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	byKey := []map[string]any{}
	var totalsReq, totalsIn, totalsOut int64
	for _, row := range rows {
		byKey = append(byKey, map[string]any{"key_id": derefString(row.KeyID), "request_count": row.RequestCount, "total_tokens_in": row.TotalTokensIn, "total_tokens_out": row.TotalTokensOut, "total_tokens": row.TotalTokensIn + row.TotalTokensOut})
		totalsReq += row.RequestCount
		totalsIn += row.TotalTokensIn
		totalsOut += row.TotalTokensOut
	}
	writeJSON(w, http.StatusOK, map[string]any{"by_key": byKey, "totals": map[string]any{"request_count": totalsReq, "total_tokens_in": totalsIn, "total_tokens_out": totalsOut, "total_tokens": totalsIn + totalsOut}})
}

func (a *App) handleConsumerAnalytics(w http.ResponseWriter, r *http.Request) {
	keyFilter := r.URL.Query().Get("api_key_id")
	from, to := parseDateRange(r.URL.Query().Get("from"), r.URL.Query().Get("to"))
	tokenRate := a.cfg.ProviderTokenUSDRate

	totals, err := a.statsRepo.ConsumerTotals(r.Context(), keyFilter, from, to)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}

	modelRows, err := a.statsRepo.ConsumerByModel(r.Context(), keyFilter, from, to)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	models := make([]map[string]any, 0, len(modelRows))
	for _, row := range modelRows {
		total := row.TokensIn + row.TokensOut
		models = append(models, map[string]any{"model": row.Model, "requests": row.RequestCount, "tokens_in": row.TokensIn, "tokens_out": row.TokensOut, "tokens_total": total, "spend_usd": round4(float64(total) * tokenRate)})
	}

	dayRows, err := a.statsRepo.ConsumerDaily(r.Context(), keyFilter, from, to)
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	daily := make([]map[string]any, 0, len(dayRows))
	for _, row := range dayRows {
		daily = append(daily, map[string]any{"date": row.Date, "requests": row.RequestCount, "tokens": row.Tokens, "spend_usd": round4(float64(row.Tokens) * tokenRate)})
	}

	writeJSON(w, http.StatusOK, map[string]any{"total_requests": totals.TotalRequests, "total_tokens_in": totals.TotalIn, "total_tokens_out": totals.TotalOut, "total_tokens": totals.TotalIn + totals.TotalOut, "total_spend_usd": round4(float64(totals.TotalIn+totals.TotalOut) * tokenRate), "tokens_by_model": models, "daily_spend": daily})
}

func (a *App) handleModelsStats(w http.ResponseWriter, r *http.Request) {
	rows, err := a.statsRepo.ModelStats(r.Context())
	pricePerM := round4(a.cfg.ProviderTokenUSDRate * 1000000)
	if err != nil {
		writeJSON(w, http.StatusOK, map[string]any{"models": []any{}, "price_per_m_tokens": pricePerM})
		return
	}
	models := []map[string]any{}
	for _, row := range rows {
		models = append(models, map[string]any{"name": row.ModelName, "license": derefString(row.License), "node_count": row.NodeCount, "avg_latency_ms": row.AvgLatency, "uptime_7d": row.Uptime7D, "price_per_m_tokens": pricePerM})
	}
	writeJSON(w, http.StatusOK, map[string]any{"models": models, "price_per_m_tokens": pricePerM})
}

func (a *App) handleProviderStats(w http.ResponseWriter, r *http.Request) {
	nodes, err := a.listNodesDB(r.Context())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	tokenRate := a.cfg.ProviderTokenUSDRate
	revenueShare := a.cfg.ProviderRevenueShare

	healthRows, err := a.statsRepo.ProviderHealth(r.Context())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	healthMap := map[string]struct {
		probe, success int64
		latency        *float64
	}{}
	for _, row := range healthRows {
		healthMap[row.NodeID] = struct {
			probe, success int64
			latency        *float64
		}{row.ProbeCount, row.SuccessCount, row.Latency}
	}

	usageRows, err := a.statsRepo.ProviderUsage(r.Context())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	usageMap := map[string]struct{ req, ti, to int64 }{}
	for _, row := range usageRows {
		usageMap[row.NodeID] = struct{ req, ti, to int64 }{row.RequestCount, row.TokensIn, row.TokensOut}
	}

	modelRows, err := a.statsRepo.NodeModels(r.Context())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	modelsMap := map[string][]string{}
	for _, row := range modelRows {
		modelsMap[row.NodeID] = append(modelsMap[row.NodeID], row.ModelName)
	}

	stripeRows, err := a.statsRepo.ProviderStripeStatuses(r.Context())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "query failed", "type": "server_error"}})
		return
	}
	stripeMap := map[string]bool{}
	for _, row := range stripeRows {
		stripeMap[row.NodeID] = row.OnboardingComplete
	}

	data := []map[string]any{}
	var totalReq, totalTokens int64
	var totalEarnings float64
	for _, n := range nodes {
		h := healthMap[n.ID]
		u := usageMap[n.ID]
		tokens := u.ti + u.to
		earnings := round4(float64(tokens) * tokenRate * revenueShare)
		uptime := 0.0
		if h.probe > 0 {
			uptime = round2((float64(h.success) / float64(h.probe)) * 100)
		}
		data = append(data, map[string]any{
			"node_id": n.ID, "node_name": n.Name, "gpu_name": n.GPUName, "vram_mb": n.VRAMMB, "status": n.Status,
			"uptime_pct_7d": uptime, "avg_latency_ms_7d": h.latency, "probe_count_7d": h.probe,
			"request_count_7d": u.req, "tokens_in_7d": u.ti, "tokens_out_7d": u.to, "tokens_served_7d": tokens,
			"estimated_earnings_usd_7d": earnings, "stripe_onboarding_complete": stripeMap[n.ID], "models": modelsMap[n.ID],
		})
		totalReq += u.req
		totalTokens += tokens
		totalEarnings += earnings
	}
	sort.SliceStable(data, func(i, j int) bool { return data[i]["node_name"].(string) < data[j]["node_name"].(string) })
	writeJSON(w, http.StatusOK, map[string]any{"nodes": data, "totals": map[string]any{"node_count": len(data), "request_count_7d": totalReq, "tokens_served_7d": totalTokens, "estimated_earnings_usd_7d": round4(totalEarnings)}})
}

func parseDateRange(fromStr, toStr string) (time.Time, time.Time) {
	from := time.Now().UTC().Add(-30 * 24 * time.Hour)
	to := time.Now().UTC()
	if fromStr != "" {
		if t, err := time.Parse(time.RFC3339, fromStr); err == nil {
			from = t.UTC()
		} else if t, err := time.Parse("2006-01-02", fromStr); err == nil {
			from = t.UTC()
		}
	}
	if toStr != "" {
		if t, err := time.Parse(time.RFC3339, toStr); err == nil {
			to = t.UTC()
		} else if t, err := time.Parse("2006-01-02", toStr); err == nil {
			to = t.UTC()
		}
	}
	return from, to
}

func derefString(v *string) string {
	if v == nil {
		return ""
	}
	return *v
}
