package gateway

import "testing"

func TestExtractSingleModelAcceptsModelOnlyContract(t *testing.T) {
	catalog := NewModelCatalog()
	req := RegisterNodeRequest{
		Name:      "n1",
		Host:      "127.0.0.1",
		Port:      11434,
		AgentPort: 8181,
		GPUName:   "gpu",
		VRAMMB:    8192,
		Model:     stringPtr(" Llama3.1:8B "),
	}

	m, err := ExtractSingleModel(req, catalog)
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if m.Name != "llama3.1:8b" {
		t.Fatalf("unexpected normalized model: %s", m.Name)
	}
	if m.License != "llama-3.1" {
		t.Fatalf("unexpected resolved license: %s", m.License)
	}
}

func TestExtractSingleModelRejectsExternalLicense(t *testing.T) {
	catalog := NewModelCatalog()
	req := RegisterNodeRequest{
		Model:   stringPtr("qwen2.5:7b"),
		License: stringPtr("apache-2.0"),
	}

	if _, err := ExtractSingleModel(req, catalog); err == nil {
		t.Fatal("expected validation error")
	}
}

func TestExtractSingleModelRejectsUnknownModel(t *testing.T) {
	catalog := NewModelCatalog()
	req := RegisterNodeRequest{Model: stringPtr("unknown-model")}

	if _, err := ExtractSingleModel(req, catalog); err == nil {
		t.Fatal("expected validation error")
	}
}
