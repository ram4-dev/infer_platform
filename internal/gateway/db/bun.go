package db

import (
	"context"
	"database/sql"

	"github.com/uptrace/bun"
	"github.com/uptrace/bun/dialect/pgdialect"
	"github.com/uptrace/bun/driver/pgdriver"
)

func Open(ctx context.Context, dsn string) (*bun.DB, *sql.DB, error) {
	sqldb := sql.OpenDB(pgdriver.NewConnector(pgdriver.WithDSN(dsn)))
	if err := sqldb.PingContext(ctx); err != nil {
		_ = sqldb.Close()
		return nil, nil, err
	}
	return bun.NewDB(sqldb, pgdialect.New()), sqldb, nil
}
