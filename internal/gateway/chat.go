package gateway

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	gwdb "infer_platform/internal/gateway/db"

	"github.com/google/uuid"
)

type ollamaChunk struct {
	Message *struct {
		Content string `json:"content"`
	} `json:"message,omitempty"`
	Done            bool   `json:"done"`
	PromptEvalCount uint32 `json:"prompt_eval_count,omitempty"`
	EvalCount       uint32 `json:"eval_count,omitempty"`
}

func (a *App) handleChatCompletions(w http.ResponseWriter, r *http.Request) {
	var req ChatCompletionRequest
	if err := decodeJSON(r, &req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": map[string]any{"message": "invalid request body", "type": "invalid_request_error"}})
		return
	}
	if err := sanitizeRequest(&req); err != nil {
		writeJSON(w, http.StatusUnprocessableEntity, map[string]any{"error": map[string]any{"message": err.Error(), "type": "server_error"}})
		return
	}
	req.Model = normalizeModelName(req.Model)
	a.router.RecordRequest(req.Model)

	candidates, err := a.loadModelCandidates(r.Context(), req.Model)
	if err != nil {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": err.Error(), "type": "server_error"}})
		return
	}
	if len(candidates) == 0 {
		a.router.RecordNoCapacity(req.Model)
		writeJSON(w, http.StatusServiceUnavailable, map[string]any{"error": map[string]any{"message": fmt.Sprintf("model '%s' is not available", req.Model), "type": "server_error"}})
		return
	}
	validated, _ := validatedKeyFromContext(r.Context())

	if req.Stream {
		a.streamChat(w, r, req, validated, candidates[0])
		return
	}

	requestID := requestID()
	var lastErr error
	for i, node := range candidates {
		body, err := a.executeChat(r.Context(), req, node)
		if err == nil {
			resp := buildOpenAIResponse(body, req.Model, requestID)
			a.recordUsage(r.Context(), validated.KeyID, req.Model, int32(resp["usage"].(map[string]any)["prompt_tokens"].(uint32)), int32(resp["usage"].(map[string]any)["completion_tokens"].(uint32)))
			writeJSON(w, http.StatusOK, resp)
			return
		}
		lastErr = err
		if i+1 < len(candidates) {
			a.router.RecordFailover(req.Model)
		}
	}
	writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": lastErr.Error(), "type": "server_error"}})
}

func (a *App) loadModelCandidates(ctx context.Context, model string) ([]NodeInfo, error) {
	nodes, err := a.loadModelNodesDB(ctx, model)
	if err != nil {
		return nil, err
	}
	stats := a.store.GetNodeStats()
	return a.router.BuildCandidates(model, nodes, stats), nil
}

func (a *App) loadModelNodesDB(ctx context.Context, model string) ([]NodeInfo, error) {
	rows, err := a.nodeRepo.LoadOnlineNodesByModel(ctx, model)
	if err != nil {
		return nil, err
	}
	out := make([]NodeInfo, 0, len(rows))
	for _, row := range rows {
		out = append(out, nodeInfoFromRepo(gwdb.NodeWithModel(row)))
	}
	return out, nil
}

func (a *App) executeChat(ctx context.Context, req ChatCompletionRequest, node NodeInfo) (map[string]any, error) {
	payload := map[string]any{"model": req.Model, "messages": req.Messages, "stream": false}
	options := map[string]any{}
	if req.MaxTokens != nil {
		options["num_predict"] = *req.MaxTokens
	}
	if req.Temperature != nil {
		options["temperature"] = *req.Temperature
	}
	if req.TopP != nil {
		options["top_p"] = *req.TopP
	}
	if len(options) > 0 {
		payload["options"] = options
	}
	buf, _ := json.Marshal(payload)
	url := fmt.Sprintf("http://%s:%d/api/chat", node.Host, node.Port)
	hreq, _ := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(buf))
	hreq.Header.Set("Content-Type", "application/json")
	resp, err := a.httpClient.Do(hreq)
	if err != nil {
		return nil, fmt.Errorf("failed to reach node %s at %s: %w", node.ID, url, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		body, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("node %s returned %s: %s", node.ID, resp.Status, strings.TrimSpace(string(body)))
	}
	var decoded map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&decoded); err != nil {
		return nil, fmt.Errorf("failed to parse Ollama response: %w", err)
	}
	return decoded, nil
}

func (a *App) streamChat(w http.ResponseWriter, r *http.Request, req ChatCompletionRequest, validated ValidatedKey, node NodeInfo) {
	payload := map[string]any{"model": req.Model, "messages": req.Messages, "stream": true}
	options := map[string]any{}
	if req.MaxTokens != nil {
		options["num_predict"] = *req.MaxTokens
	}
	if req.Temperature != nil {
		options["temperature"] = *req.Temperature
	}
	if req.TopP != nil {
		options["top_p"] = *req.TopP
	}
	if len(options) > 0 {
		payload["options"] = options
	}
	buf, _ := json.Marshal(payload)
	url := fmt.Sprintf("http://%s:%d/api/chat", node.Host, node.Port)
	hreq, _ := http.NewRequestWithContext(r.Context(), http.MethodPost, url, bytes.NewReader(buf))
	hreq.Header.Set("Content-Type", "application/json")
	resp, err := a.httpClient.Do(hreq)
	if err != nil {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": "inference backend unavailable", "type": "server_error"}})
		return
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": map[string]any{"message": fmt.Sprintf("backend returned %s", resp.Status), "type": "server_error"}})
		return
	}

	flusher, ok := w.(http.Flusher)
	if !ok {
		writeJSON(w, http.StatusInternalServerError, map[string]any{"error": map[string]any{"message": "streaming unsupported", "type": "server_error"}})
		return
	}
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")

	completionID := "chatcmpl-" + strings.ReplaceAll(uuid.New().String(), "-", "")
	created := time.Now().UTC().Unix()
	roleChunk := map[string]any{
		"id": completionID, "object": "chat.completion.chunk", "created": created, "model": req.Model,
		"choices": []any{map[string]any{"index": 0, "delta": map[string]any{"role": "assistant"}, "finish_reason": nil}},
	}
	writeSSE(w, roleChunk)
	flusher.Flush()

	reader := bufio.NewReader(resp.Body)
	var promptTokens, completionTokens uint32
	for {
		lineBytes, err := reader.ReadBytes('\n')
		if len(lineBytes) == 0 && err == io.EOF {
			break
		}
		if err != nil && err != io.EOF {
			break
		}
		line := strings.TrimSpace(string(lineBytes))
		if line == "" {
			continue
		}
		var chunk ollamaChunk
		if err := json.Unmarshal([]byte(line), &chunk); err != nil {
			continue
		}
		promptTokens = chunk.PromptEvalCount
		completionTokens = chunk.EvalCount
		if chunk.Done {
			if validated.KeyID != "" {
				a.recordUsage(r.Context(), validated.KeyID, req.Model, int32(promptTokens), int32(completionTokens))
			}
			finalChunk := map[string]any{
				"id": completionID, "object": "chat.completion.chunk", "created": created, "model": req.Model,
				"choices": []any{map[string]any{"index": 0, "delta": map[string]any{}, "finish_reason": "stop"}},
			}
			writeSSE(w, finalChunk)
			writeRawSSE(w, "[DONE]")
			flusher.Flush()
			return
		}
		if chunk.Message != nil && chunk.Message.Content != "" {
			contentChunk := map[string]any{
				"id": completionID, "object": "chat.completion.chunk", "created": created, "model": req.Model,
				"choices": []any{map[string]any{"index": 0, "delta": map[string]any{"content": filterPII(chunk.Message.Content)}, "finish_reason": nil}},
			}
			writeSSE(w, contentChunk)
			flusher.Flush()
		}
		if err == io.EOF {
			break
		}
	}
	writeRawSSE(w, "[DONE]")
	flusher.Flush()
}

func writeSSE(w http.ResponseWriter, payload any) {
	buf, _ := json.Marshal(payload)
	_, _ = w.Write([]byte("data: "))
	_, _ = w.Write(buf)
	_, _ = w.Write([]byte("\n\n"))
}

func writeRawSSE(w http.ResponseWriter, payload string) {
	_, _ = w.Write([]byte("data: " + payload + "\n\n"))
}

func buildOpenAIResponse(ollamaBody map[string]any, model, requestID string) map[string]any {
	content := ""
	if message, ok := ollamaBody["message"].(map[string]any); ok {
		if s, ok := message["content"].(string); ok {
			content = filterPII(s)
		}
	}
	promptTokens := toUint32(ollamaBody["prompt_eval_count"])
	completionTokens := toUint32(ollamaBody["eval_count"])
	return map[string]any{
		"id":      "chatcmpl-" + requestID,
		"object":  "chat.completion",
		"created": time.Now().UTC().Unix(),
		"model":   model,
		"choices": []any{map[string]any{"index": 0, "message": map[string]any{"role": "assistant", "content": content}, "finish_reason": "stop"}},
		"usage":   map[string]any{"prompt_tokens": promptTokens, "completion_tokens": completionTokens, "total_tokens": promptTokens + completionTokens},
	}
}

func toUint32(v any) uint32 {
	switch t := v.(type) {
	case float64:
		return uint32(t)
	case int:
		return uint32(t)
	case int32:
		return uint32(t)
	case int64:
		return uint32(t)
	case uint32:
		return t
	default:
		return 0
	}
}

func (a *App) recordUsage(ctx context.Context, keyID, model string, tokensIn, tokensOut int32) {
	if keyID == "" {
		return
	}
	go func() {
		_ = a.usageRepo.Insert(context.Background(), gwdb.UsageLog{KeyID: keyID, Model: model, TokensIn: tokensIn, TokensOut: tokensOut})
	}()
}
