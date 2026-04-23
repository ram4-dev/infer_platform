package gateway

import (
	"sort"
	"sync"
)

type RoutingCounters struct {
	Requests   uint64 `json:"requests"`
	Failovers  uint64 `json:"failovers"`
	NoCapacity uint64 `json:"no_capacity"`
}

type ModelRouter struct {
	mu        sync.Mutex
	rrOffsets map[string]uint64
	counters  map[string]RoutingCounters
}

func NewModelRouter() *ModelRouter {
	return &ModelRouter{
		rrOffsets: map[string]uint64{},
		counters:  map[string]RoutingCounters{},
	}
}

func (r *ModelRouter) BuildCandidates(model string, nodes []NodeInfo, stats map[string]NodeStats) []NodeInfo {
	candidates := make([]NodeInfo, 0)
	for _, n := range nodes {
		if n.Status != NodeStatusOnline {
			continue
		}
		if n.Model == nil || *n.Model != model {
			continue
		}
		candidates = append(candidates, n)
	}

	sort.SliceStable(candidates, func(i, j int) bool {
		pi := stats[candidates[i].ID].P50MS
		pj := stats[candidates[j].ID].P50MS
		if pi == 0 {
			pi = 1e18
		}
		if pj == 0 {
			pj = 1e18
		}
		if pi == pj {
			return candidates[i].VRAMMB > candidates[j].VRAMMB
		}
		return pi < pj
	})

	if len(candidates) > 1 {
		r.mu.Lock()
		offset := int(r.rrOffsets[model] % uint64(len(candidates)))
		r.rrOffsets[model]++
		r.mu.Unlock()
		rotated := append(candidates[offset:], candidates[:offset]...)
		return rotated
	}

	return candidates
}

func (r *ModelRouter) RecordRequest(model string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	c := r.counters[model]
	c.Requests++
	r.counters[model] = c
}

func (r *ModelRouter) RecordFailover(model string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	c := r.counters[model]
	c.Failovers++
	r.counters[model] = c
}

func (r *ModelRouter) RecordNoCapacity(model string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	c := r.counters[model]
	c.NoCapacity++
	r.counters[model] = c
}
