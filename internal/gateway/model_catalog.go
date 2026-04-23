package gateway

type ModelCatalog struct {
	licenses map[string]string
}

func NewModelCatalog() *ModelCatalog {
	entries := map[string]string{
		"llama3.1:8b":    "llama-3.1",
		"llama3:8b":      "llama-3",
		"qwen2.5:0.5b":  "apache-2.0",
		"qwen2.5:7b":     "apache-2.0",
		"mistral:7b":     "apache-2.0",
		"phi4:14b":       "mit",
		"deepseek-r1:8b": "mit",
		"stream-model":   "apache-2.0",
		"failover-model": "apache-2.0",
	}
	licenses := make(map[string]string, len(entries))
	for model, license := range entries {
		licenses[normalizeModelName(model)] = normalizeLicenseName(license)
	}
	return &ModelCatalog{licenses: licenses}
}

func (c *ModelCatalog) ResolveLicense(model string) (string, bool) {
	if c == nil {
		return "", false
	}
	license, ok := c.licenses[normalizeModelName(model)]
	return license, ok
}

func (c *ModelCatalog) ApprovedLicenses() []string {
	seen := map[string]struct{}{}
	for _, license := range c.licenses {
		seen[license] = struct{}{}
	}
	out := make([]string, 0, len(seen))
	for license := range seen {
		out = append(out, license)
	}
	return sortedStrings(seen)
}

func (c *ModelCatalog) HasModel(model string) bool {
	_, ok := c.ResolveLicense(model)
	return ok
}
