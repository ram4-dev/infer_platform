package gateway

import (
	"context"
	"errors"
	"testing"
)

func TestNewAppRequiresDatabaseURL(t *testing.T) {
	_, err := NewApp(context.Background(), Config{InternalKey: "x"})
	if !errors.Is(err, ErrDatabaseRequired) {
		t.Fatalf("expected ErrDatabaseRequired, got %v", err)
	}
}
