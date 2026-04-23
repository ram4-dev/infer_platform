package gateway

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"math"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"
)

type ctxKey string

const validatedKeyCtxKey ctxKey = "validated_api_key"

func (a *App) Routes() http.Handler {
	r := chi.NewRouter()

	r.Group(func(r chi.Router) {
		r.Use(a.apiKeyMiddleware)
		r.Post("/v1/chat/completions", a.handleChatCompletions)
		r.Get("/v1/models", a.handleListModels)
		r.Get("/v1/models/{id}", a.handleGetModel)
		r.Post("/v1/billing/setup", a.handleBillingSetup)
	})

	r.Group(func(r chi.Router) {
		r.Use(a.internalKeyMiddleware)
		r.Post("/v1/internal/nodes", a.handleRegisterNode)
		r.Get("/v1/internal/nodes", a.handleListNodes)
		r.Post("/v1/internal/keys", a.handleCreateKey)
		r.Get("/v1/internal/keys", a.handleListKeys)
		r.Delete("/v1/internal/keys/{id}", a.handleRevokeKey)
		r.Get("/v1/internal/usage", a.handleUsageSummary)
		r.Get("/v1/internal/licenses", a.handleLicenses)
		r.Get("/v1/internal/provider/stats", a.handleProviderStats)
		r.Get("/v1/internal/analytics/consumer", a.handleConsumerAnalytics)
		r.Get("/v1/internal/models/stats", a.handleModelsStats)
		r.Post("/v1/internal/billing/connect", a.handleBillingConnect)
	})

	r.Get("/ping", func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte("pong")) })
	r.Get("/health", a.handleHealth)
	r.Post("/v1/webhooks/stripe", a.handleStripeWebhook)

	return r
}

func (a *App) apiKeyMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		key, ok := extractBearer(r)
		if !ok {
			writeJSON(w, http.StatusUnauthorized, map[string]any{"error": map[string]any{"message": "Missing or invalid Authorization header", "type": "invalid_request_error", "code": "invalid_api_key"}})
			return
		}

		validated, err := a.validateAPIKey(r.Context(), key)
		if err != nil {
			Logger().Error("api key validation failed", slog.Any("error", err))
			writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": map[string]any{"message": "api key validation unavailable", "type": "service_unavailable"}})
			return
		}
		if validated == nil {
			writeJSON(w, http.StatusUnauthorized, map[string]any{"error": map[string]any{"message": "Invalid API key", "type": "invalid_request_error", "code": "invalid_api_key"}})
			return
		}

		if a.redis != nil {
			allowed, err := a.checkRateLimit(r.Context(), validated)
			if err != nil {
				Logger().Warn("redis rate limit check failed; failing open", slog.Any("error", err))
			} else if !allowed {
				writeJSON(w, http.StatusTooManyRequests, map[string]any{"error": map[string]any{"message": "Rate limit exceeded", "type": "rate_limit_error", "code": "rate_limit_exceeded"}})
				return
			}
		}

		ctx := context.WithValue(r.Context(), validatedKeyCtxKey, *validated)
		next.ServeHTTP(w, r.WithContext(ctx))
	})
}

func (a *App) internalKeyMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		key, ok := extractBearer(r)
		if !ok || key != a.cfg.InternalKey {
			writeJSON(w, http.StatusUnauthorized, map[string]any{"error": map[string]any{"message": "Invalid internal key", "type": "authentication_error"}})
			return
		}
		next.ServeHTTP(w, r)
	})
}

func extractBearer(r *http.Request) (string, bool) {
	v := r.Header.Get("Authorization")
	if !strings.HasPrefix(v, "Bearer ") {
		return "", false
	}
	return strings.TrimPrefix(v, "Bearer "), true
}

func validatedKeyFromContext(ctx context.Context) (ValidatedKey, bool) {
	v, ok := ctx.Value(validatedKeyCtxKey).(ValidatedKey)
	return v, ok
}

func (a *App) validateAPIKey(ctx context.Context, key string) (*ValidatedKey, error) {
	validated, err := a.keyRepo.FindActiveByHash(ctx, hashKey(key))
	if err != nil {
		return nil, err
	}
	if validated == nil {
		return nil, nil
	}
	return &ValidatedKey{KeyID: validated.ID, RateLimitRPM: int64(validated.RateLimitRPM)}, nil
}

func (a *App) checkRateLimit(ctx context.Context, key *ValidatedKey) (bool, error) {
	minute := time.Now().UTC().Unix() / 60
	redisKey := fmt.Sprintf("rate:%s:%d", key.KeyID, minute)
	count, err := a.redis.Incr(ctx, redisKey).Result()
	if err != nil {
		return false, err
	}
	if count == 1 {
		a.redis.Expire(ctx, redisKey, 120*time.Second)
	}
	return count <= key.RateLimitRPM, nil
}

func (a *App) handleHealth(w http.ResponseWriter, r *http.Request) {
	count, err := a.nodeRepo.CountOnlineNodes(r.Context())
	if err != nil {
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"status": "degraded", "service": "infer-api-gateway", "error": "database unavailable", "timestamp": time.Now().UTC().Format(time.RFC3339)})
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"status": "ok", "service": "infer-api-gateway", "nodes_online": count, "timestamp": time.Now().UTC().Format(time.RFC3339)})
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func decodeJSON(r *http.Request, dst any) error {
	defer r.Body.Close()
	return json.NewDecoder(r.Body).Decode(dst)
}

func sanitizeRequest(req *ChatCompletionRequest) error {
	req.Model = strings.TrimSpace(req.Model)
	if req.Model == "" || strings.Contains(req.Model, "/") || strings.Contains(req.Model, "..") {
		return fmt.Errorf("model name is invalid")
	}
	if len(req.Messages) == 0 {
		return fmt.Errorf("messages must not be empty")
	}
	validRoles := map[string]struct{}{"system": {}, "user": {}, "assistant": {}, "tool": {}, "function": {}}
	for i := range req.Messages {
		req.Messages[i].Content = strings.ReplaceAll(req.Messages[i].Content, "\x00", "")
		if _, ok := validRoles[req.Messages[i].Role]; !ok {
			return fmt.Errorf("invalid role '%s'", req.Messages[i].Role)
		}
		if strings.TrimSpace(req.Messages[i].Content) == "" {
			return fmt.Errorf("messages must not be empty")
		}
	}
	return nil
}

func filterPII(s string) string { return s }

func round2(v float64) float64 { return math.Round(v*100) / 100 }
func round4(v float64) float64 { return math.Round(v*10000) / 10000 }

func validateWebhookSignature(payload []byte, sigHeader, secret string) bool {
	var ts string
	var signatures []string
	for _, part := range strings.Split(sigHeader, ",") {
		if strings.HasPrefix(part, "t=") {
			ts = strings.TrimPrefix(part, "t=")
		}
		if strings.HasPrefix(part, "v1=") {
			signatures = append(signatures, strings.TrimPrefix(part, "v1="))
		}
	}
	if ts == "" || len(signatures) == 0 {
		return false
	}
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(ts))
	mac.Write([]byte("."))
	mac.Write(payload)
	expected := fmt.Sprintf("%x", mac.Sum(nil))
	for _, sig := range signatures {
		if sig == expected {
			return true
		}
	}
	return false
}

func readBody(r *http.Request) ([]byte, error) { return io.ReadAll(r.Body) }

func sortedStrings(values map[string]struct{}) []string {
	out := make([]string, 0, len(values))
	for v := range values {
		out = append(out, v)
	}
	sort.Strings(out)
	return out
}

func requestID() string { return uuid.New().String() }
