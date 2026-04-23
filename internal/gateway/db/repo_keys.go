package db

import (
	"context"
	"database/sql"
	"errors"
	"time"

	"github.com/uptrace/bun"
)

type KeyRepository struct{ db *bun.DB }

func NewKeyRepository(db *bun.DB) *KeyRepository { return &KeyRepository{db: db} }

func (r *KeyRepository) FindActiveByHash(ctx context.Context, keyHash string) (*APIKey, error) {
	var key APIKey
	if err := r.db.NewSelect().Model(&key).Where("key_hash = ?", keyHash).Where("revoked_at IS NULL").Limit(1).Scan(ctx); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, nil
		}
		return nil, err
	}
	return &key, nil
}

func (r *KeyRepository) Create(ctx context.Context, key APIKey) error {
	_, err := r.db.NewInsert().Model(&key).Exec(ctx)
	return err
}

func (r *KeyRepository) FindByID(ctx context.Context, id string) (*APIKey, error) {
	var key APIKey
	if err := r.db.NewSelect().Model(&key).Where("id = ?", id).Limit(1).Scan(ctx); err != nil {
		if errors.Is(err, sql.ErrNoRows) {
			return nil, nil
		}
		return nil, err
	}
	return &key, nil
}

func (r *KeyRepository) List(ctx context.Context) ([]APIKey, error) {
	var keys []APIKey
	err := r.db.NewSelect().Model(&keys).OrderExpr("created_at DESC").Scan(ctx)
	return keys, err
}

func (r *KeyRepository) Revoke(ctx context.Context, id string, now time.Time) (bool, error) {
	res, err := r.db.NewUpdate().Model((*APIKey)(nil)).Set("revoked_at = ?", now).Where("id = ?", id).Where("revoked_at IS NULL").Exec(ctx)
	if err != nil {
		return false, err
	}
	affected, _ := res.RowsAffected()
	return affected > 0, nil
}
