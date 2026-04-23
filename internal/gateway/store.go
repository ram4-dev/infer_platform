package gateway

import (
	"sync"
)

type NodeStore struct {
	mu        sync.RWMutex
	nodes     []NodeInfo
	nodeStats map[string]NodeStats
}

func NewNodeStore() *NodeStore {
	return &NodeStore{nodeStats: map[string]NodeStats{}}
}

func (s *NodeStore) UpsertNode(node NodeInfo) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for i := range s.nodes {
		if s.nodes[i].Name == node.Name {
			s.nodes[i] = node
			return
		}
	}
	s.nodes = append(s.nodes, node)
}

func (s *NodeStore) ListNodes() []NodeInfo {
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make([]NodeInfo, len(s.nodes))
	copy(out, s.nodes)
	return out
}

func (s *NodeStore) SetNodeStatus(id string, status NodeStatus) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for i := range s.nodes {
		if s.nodes[i].ID == id {
			s.nodes[i].Status = status
			return
		}
	}
}

func (s *NodeStore) SetNodeStats(id string, stats NodeStats) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.nodeStats[id] = stats
}

func (s *NodeStore) GetNodeStats() map[string]NodeStats {
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make(map[string]NodeStats, len(s.nodeStats))
	for k, v := range s.nodeStats {
		out[k] = v
	}
	return out
}
