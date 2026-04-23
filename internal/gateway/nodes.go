package gateway

import (
	"context"
	"net/http"
	"strings"
	"time"

	gwdb "infer_platform/internal/gateway/db"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"
)

func ExtractSingleModel(req RegisterNodeRequest, catalog *ModelCatalog) (ModelRegistration, error) {
	if req.License != nil {
		return ModelRegistration{}, errValidation("license must not be provided by node registration")
	}
	if len(req.Models) > 0 {
		return ModelRegistration{}, errValidation("models[] is no longer accepted; send only model")
	}
	if req.Model == nil || normalizeModelName(*req.Model) == "" {
		return ModelRegistration{}, errValidation("model is required")
	}
	modelName := normalizeModelName(*req.Model)
	license, ok := catalog.ResolveLicense(modelName)
	if !ok {
		return ModelRegistration{}, errValidation("unknown model")
	}
	return ModelRegistration{Name: modelName, License: license}, nil
}

type validationError struct{ msg string }

func errValidation(msg string) error     { return &validationError{msg: msg} }
func (e *validationError) Error() string { return e.msg }

func (a *App) handleRegisterNode(w http.ResponseWriter, r *http.Request) {
	var req RegisterNodeRequest
	if err := decodeJSON(r, &req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "validation_error", "message": "invalid request body"})
		return
	}
	model, err := ExtractSingleModel(req, a.catalog)
	if err != nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"error": "validation_error", "message": err.Error()})
		return
	}

	now := time.Now().UTC()
	node := NodeInfo{
		ID:           uuid.NewString(),
		Name:         req.Name,
		Host:         req.Host,
		Port:         req.Port,
		AgentPort:    req.AgentPort,
		GPUName:      req.GPUName,
		VRAMMB:       req.VRAMMB,
		Status:       NodeStatusOnline,
		Model:        stringPtr(model.Name),
		License:      stringPtr(model.License),
		RegisteredAt: now,
		LastSeen:     now,
	}

	persisted, err := a.upsertNodeDB(r.Context(), node, model)
	if err != nil {
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": map[string]any{"message": "database unavailable", "type": "service_unavailable"}})
		return
	}
	writeJSON(w, http.StatusCreated, persisted)
}

func (a *App) upsertNodeDB(ctx context.Context, node NodeInfo, model ModelRegistration) (NodeInfo, error) {
	persisted, err := a.nodeRepo.UpsertNodeWithModel(ctx, gwdb.Node{
		ID:           node.ID,
		Name:         node.Name,
		Host:         node.Host,
		Port:         int32(node.Port),
		AgentPort:    int32(node.AgentPort),
		GPUName:      node.GPUName,
		VRAMMB:       int64(node.VRAMMB),
		Status:       string(node.Status),
		RegisteredAt: node.RegisteredAt,
		LastSeen:     node.LastSeen,
	}, gwdb.NodeModel{
		ModelName:    model.Name,
		License:      model.License,
		RegisteredAt: time.Now().UTC(),
	})
	if err != nil {
		return NodeInfo{}, err
	}
	return nodeInfoFromRepo(persisted), nil
}

func (a *App) handleListNodes(w http.ResponseWriter, r *http.Request) {
	nodes, err := a.listNodesDB(r.Context())
	if err != nil {
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": map[string]any{"message": "query failed", "type": "service_unavailable"}})
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"object": "list", "data": nodes, "total": len(nodes)})
}

func (a *App) listNodesDB(ctx context.Context) ([]NodeInfo, error) {
	rows, err := a.nodeRepo.ListNodes(ctx)
	if err != nil {
		return nil, err
	}
	out := make([]NodeInfo, 0, len(rows))
	for _, row := range rows {
		out = append(out, nodeInfoFromRepo(row))
	}
	return out, nil
}

func nodeInfoFromRepo(row gwdb.NodeWithModel) NodeInfo {
	return NodeInfo{
		ID:           row.ID,
		Name:         row.Name,
		Host:         row.Host,
		Port:         uint16(row.Port),
		AgentPort:    uint16(row.AgentPort),
		GPUName:      row.GPUName,
		VRAMMB:       uint64(row.VRAMMB),
		Status:       NodeStatus(row.Status),
		Model:        row.ModelName,
		License:      row.License,
		RegisteredAt: row.RegisteredAt,
		LastSeen:     row.LastSeen,
	}
}

func (a *App) handleCreateKey(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Owner              string `json:"owner"`
		RateLimitRPM       int32  `json:"rate_limit_rpm"`
		DailySpendCapCents *int32 `json:"daily_spend_cap_cents"`
	}
	if err := decodeJSON(r, &req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": map[string]any{"message": "invalid request body", "type": "invalid_request_error"}})
		return
	}
	if req.RateLimitRPM == 0 {
		req.RateLimitRPM = 60
	}
	rawKey := "pk_" + strings.ReplaceAll(uuid.NewString(), "-", "")[:32]
	keyID := uuid.NewString()
	now := time.Now().UTC()
	if err := a.keyRepo.Create(r.Context(), gwdb.APIKey{ID: keyID, KeyHash: hashKey(rawKey), Owner: req.Owner, RateLimitRPM: req.RateLimitRPM, DailySpendCapCents: req.DailySpendCapCents, CreatedAt: now}); err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "failed to create key", "type": "server_error"}})
		return
	}
	writeJSON(w, http.StatusCreated, map[string]any{"id": keyID, "key": rawKey, "owner": req.Owner, "rate_limit_rpm": req.RateLimitRPM, "created_at": now})
}

func (a *App) handleListKeys(w http.ResponseWriter, r *http.Request) {
	keys, err := a.keyRepo.List(r.Context())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "failed to list keys", "type": "server_error"}})
		return
	}
	data := make([]map[string]any, 0, len(keys))
	for _, key := range keys {
		data = append(data, map[string]any{"id": key.ID, "owner": key.Owner, "rate_limit_rpm": key.RateLimitRPM, "created_at": key.CreatedAt, "revoked_at": key.RevokedAt})
	}
	writeJSON(w, http.StatusOK, map[string]any{"object": "list", "data": data, "total": len(data)})
}

func (a *App) handleRevokeKey(w http.ResponseWriter, r *http.Request) {
	id := chi.URLParam(r, "id")
	revoked, err := a.keyRepo.Revoke(r.Context(), id, time.Now().UTC())
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "failed to revoke key", "type": "server_error"}})
		return
	}
	if !revoked {
		writeJSON(w, http.StatusNotFound, map[string]any{"error": map[string]any{"message": "Key not found or already revoked", "type": "invalid_request_error"}})
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (a *App) handleLicenses(w http.ResponseWriter, _ *http.Request) {
	list := sortedStrings(a.approvedLics)
	writeJSON(w, http.StatusOK, map[string]any{"object": "list", "data": list, "total": len(list)})
}
