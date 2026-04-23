package gateway

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"strings"
)

func stringPtr(v string) *string { return &v }

func normalizeModelName(model string) string {
	return strings.ToLower(strings.TrimSpace(model))
}

func normalizeLicenseName(license string) string {
	return strings.ToLower(strings.TrimSpace(license))
}

func hashKey(key string) string {
	h := sha256.Sum256([]byte(key))
	return hex.EncodeToString(h[:])
}

func hmacSHA256Hex(secret string, payload []byte) string {
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write(payload)
	return hex.EncodeToString(mac.Sum(nil))
}
