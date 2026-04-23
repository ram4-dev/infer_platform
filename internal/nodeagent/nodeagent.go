package nodeagent

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"runtime"
	"strconv"
	"strings"
	"sync/atomic"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/joho/godotenv"
)

type HardwareInfo struct {
	GPUName    string `json:"gpu_name"`
	VRAMMB     uint64 `json:"vram_mb"`
	CPUCount   int    `json:"cpu_count"`
	TotalRAMMB uint64 `json:"total_ram_mb"`
}

type Config struct {
	NodeName       string
	NodeHost       string
	NodePort       uint16
	AgentPort      uint16
	CoordinatorURL string
	InternalKey    string
	NodeModel      string
}

type Agent struct {
	cfg          Config
	hardware     HardwareInfo
	startedAt    time.Time
	registeredOK atomic.Bool
	httpClient   *http.Client
}

func LoadConfig() Config {
	_ = godotenv.Load()
	return Config{
		NodeName:       getenvDefault("NODE_NAME", hostname()),
		NodeHost:       getenvDefault("NODE_HOST", "127.0.0.1"),
		NodePort:       uint16(getenvInt("NODE_PORT", 11434)),
		AgentPort:      uint16(getenvInt("AGENT_PORT", 8181)),
		CoordinatorURL: getenvDefault("COORDINATOR_URL", "http://localhost:8080"),
		InternalKey:    getenvDefault("INFER_INTERNAL_KEY", "internal_dev_secret"),
		NodeModel:      mustGetenv("NODE_MODEL"),
	}
}

func New(cfg Config) *Agent {
	return &Agent{
		cfg:        cfg,
		hardware:   collectHardware(),
		startedAt:  time.Now().UTC(),
		httpClient: &http.Client{Timeout: 120 * time.Second},
	}
}

func (a *Agent) Routes() http.Handler {
	r := chi.NewRouter()
	r.Get("/health", a.handleHealth)
	r.Get("/info", a.handleInfo)
	r.Get("/ping", func(w http.ResponseWriter, _ *http.Request) { _, _ = w.Write([]byte("pong")) })
	r.Post("/infer/shard", a.handleInferShard)
	return r
}

func (a *Agent) RegisterLoop(ctx context.Context) {
	backoff := 2 * time.Second
	for {
		if err := a.registerOnce(ctx); err != nil {
			slog.Warn("registration failed", slog.Any("error", err))
			a.registeredOK.Store(false)
			select {
			case <-ctx.Done():
				return
			case <-time.After(backoff):
			}
			if backoff < 120*time.Second {
				backoff *= 2
			}
			continue
		}
		a.registeredOK.Store(true)
		backoff = 2 * time.Second
		select {
		case <-ctx.Done():
			return
		case <-time.After(30 * time.Second):
		}
	}
}

func (a *Agent) registerOnce(ctx context.Context) error {
	payload := map[string]any{
		"name":       a.cfg.NodeName,
		"host":       a.cfg.NodeHost,
		"port":       a.cfg.NodePort,
		"agent_port": a.cfg.AgentPort,
		"gpu_name":   a.hardware.GPUName,
		"vram_mb":    a.hardware.VRAMMB,
		"model":      strings.ToLower(strings.TrimSpace(a.cfg.NodeModel)),
	}
	buf, _ := json.Marshal(payload)
	req, _ := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimRight(a.cfg.CoordinatorURL, "/")+"/v1/internal/nodes", bytes.NewReader(buf))
	req.Header.Set("Authorization", "Bearer "+a.cfg.InternalKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := a.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		body, _ := io.ReadAll(resp.Body)
		return fmt.Errorf("registration rejected: HTTP %s — %s", resp.Status, strings.TrimSpace(string(body)))
	}
	return nil
}

func (a *Agent) handleHealth(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{"status": "ok", "node_name": a.cfg.NodeName, "registered": a.registeredOK.Load(), "uptime_secs": int64(time.Since(a.startedAt).Seconds()), "hardware": a.hardware})
}

func (a *Agent) handleInfo(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{"name": a.cfg.NodeName, "coordinator_url": a.cfg.CoordinatorURL, "ollama_port": a.cfg.NodePort, "model": a.cfg.NodeModel, "hardware": a.hardware})
}

type chatMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}
type shardAssignment struct {
	NodeID, Host          string `json:"node_id","host"`
	OllamaPort, AgentPort uint16 `json:"ollama_port","agent_port"`
}
type shardPlan struct {
	Assignments []shardAssignment `json:"assignments"`
}
type shardForwardRequest struct {
	RequestID, Model string        `json:"request_id","model"`
	Messages         []chatMessage `json:"messages"`
	Stream           bool          `json:"stream"`
	MaxTokens        *uint32       `json:"max_tokens,omitempty"`
	Temperature      *float32      `json:"temperature,omitempty"`
	TopP             *float32      `json:"top_p,omitempty"`
	Plan             shardPlan     `json:"plan"`
}

func (a *Agent) handleInferShard(w http.ResponseWriter, r *http.Request) {
	var req shardForwardRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeJSON(w, http.StatusBadRequest, map[string]any{"error": "invalid request body"})
		return
	}
	body, err := a.runOllamaInference(r.Context(), req)
	if err != nil {
		writeJSON(w, http.StatusBadGateway, map[string]any{"error": err.Error()})
		return
	}
	assignments := req.Plan.Assignments
	index := 0
	for i, asg := range assignments {
		if asg.AgentPort == a.cfg.AgentPort || asg.Host == a.cfg.NodeHost {
			index = i
			break
		}
	}
	if index+1 < len(assignments) {
		next := assignments[index+1]
		assistant := ""
		if msg, ok := body["message"].(map[string]any); ok {
			assistant, _ = msg["content"].(string)
		}
		req.Messages = append(req.Messages, chatMessage{Role: "assistant", Content: assistant})
		req.Stream = false
		buf, _ := json.Marshal(req)
		hreq, _ := http.NewRequestWithContext(r.Context(), http.MethodPost, fmt.Sprintf("http://%s:%d/infer/shard", next.Host, next.AgentPort), bytes.NewReader(buf))
		hreq.Header.Set("Content-Type", "application/json")
		resp, err := a.httpClient.Do(hreq)
		if err != nil {
			writeJSON(w, http.StatusBadGateway, map[string]any{"error": fmt.Sprintf("next shard unreachable: %v", err)})
			return
		}
		defer resp.Body.Close()
		w.WriteHeader(resp.StatusCode)
		_, _ = io.Copy(w, resp.Body)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"request_id": req.RequestID, "model": req.Model, "message": body["message"], "prompt_eval_count": body["prompt_eval_count"], "eval_count": body["eval_count"]})
}

func (a *Agent) runOllamaInference(ctx context.Context, req shardForwardRequest) (map[string]any, error) {
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
	reqHTTP, _ := http.NewRequestWithContext(ctx, http.MethodPost, fmt.Sprintf("http://localhost:%d/api/chat", a.cfg.NodePort), bytes.NewReader(buf))
	reqHTTP.Header.Set("Content-Type", "application/json")
	resp, err := a.httpClient.Do(reqHTTP)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode/100 != 2 {
		return nil, fmt.Errorf("Ollama returned %s", resp.Status)
	}
	var decoded map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&decoded); err != nil {
		return nil, err
	}
	return decoded, nil
}

func collectHardware() HardwareInfo {
	gpuName, vram := detectGPU()
	var totalRAMMB uint64
	if runtime.GOOS == "darwin" {
		totalRAMMB = vram * 2
	}
	return HardwareInfo{GPUName: gpuName, VRAMMB: vram, CPUCount: runtime.NumCPU(), TotalRAMMB: totalRAMMB}
}

func detectGPU() (string, uint64) {
	if name, ok := os.LookupEnv("GPU_NAME"); ok {
		if vramStr, ok := os.LookupEnv("GPU_VRAM_MB"); ok {
			if vram, err := strconv.ParseUint(vramStr, 10, 64); err == nil {
				return name, vram
			}
		}
	}
	if runtime.GOOS == "linux" {
		entries, _ := os.ReadDir("/proc/driver/nvidia/gpus")
		for _, entry := range entries {
			content, err := os.ReadFile("/proc/driver/nvidia/gpus/" + entry.Name() + "/information")
			if err != nil {
				continue
			}
			var name string
			var vram uint64
			for _, line := range strings.Split(string(content), "\n") {
				if strings.HasPrefix(line, "Model:") {
					name = strings.TrimSpace(strings.TrimPrefix(line, "Model:"))
				}
				if strings.HasPrefix(line, "Video Memory:") {
					fmt.Sscanf(strings.TrimSpace(strings.TrimPrefix(line, "Video Memory:")), "%d", &vram)
				}
			}
			if name != "" {
				return name, vram
			}
		}
	}
	if runtime.GOOS == "darwin" {
		return "Apple Silicon (unified)", 0
	}
	return "Unknown GPU", 0
}

func hostname() string {
	if v := os.Getenv("HOSTNAME"); v != "" {
		return v
	}
	if b, err := os.ReadFile("/etc/hostname"); err == nil {
		return strings.TrimSpace(string(b))
	}
	return "unknown-node"
}

func getenvDefault(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}
func getenvInt(key string, def int) int {
	if v := os.Getenv(key); v != "" {
		if n, err := strconv.Atoi(v); err == nil {
			return n
		}
	}
	return def
}
func mustGetenv(key string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	panic(key + " is required")
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}
