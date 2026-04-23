package gateway

import (
	"testing"
	"time"
)

func testNode(id, model string, status NodeStatus, vram uint64) NodeInfo {
	return NodeInfo{
		ID:           id,
		Name:         id,
		Host:         "127.0.0.1",
		Port:         11434,
		AgentPort:    8181,
		GPUName:      "gpu",
		VRAMMB:       vram,
		Status:       status,
		Model:        stringPtr(model),
		License:      stringPtr("apache-2.0"),
		RegisteredAt: time.Now().UTC(),
		LastSeen:     time.Now().UTC(),
	}
}

func TestModelRouterFiltersByModelAndStatus(t *testing.T) {
	r := NewModelRouter()
	nodes := []NodeInfo{
		testNode("n1", "llama3.1:8b", NodeStatusOnline, 8192),
		testNode("n2", "llama3.1:8b", NodeStatusOffline, 8192),
		testNode("n3", "qwen2.5:7b", NodeStatusOnline, 8192),
	}

	candidates := r.BuildCandidates("llama3.1:8b", nodes, map[string]NodeStats{})
	if len(candidates) != 1 {
		t.Fatalf("expected 1 candidate, got %d", len(candidates))
	}
	if candidates[0].ID != "n1" {
		t.Fatalf("expected n1, got %s", candidates[0].ID)
	}
}

func TestModelRouterRoundRobinPerModel(t *testing.T) {
	r := NewModelRouter()
	nodes := []NodeInfo{
		testNode("fast", "llama3.1:8b", NodeStatusOnline, 8192),
		testNode("slow", "llama3.1:8b", NodeStatusOnline, 8192),
	}
	stats := map[string]NodeStats{
		"fast": {P50MS: 10},
		"slow": {P50MS: 50},
	}

	first := r.BuildCandidates("llama3.1:8b", nodes, stats)
	second := r.BuildCandidates("llama3.1:8b", nodes, stats)

	if first[0].ID != "fast" {
		t.Fatalf("expected first selection fast, got %s", first[0].ID)
	}
	if second[0].ID != "slow" {
		t.Fatalf("expected second selection slow, got %s", second[0].ID)
	}
}
