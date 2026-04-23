package main

import (
	"context"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"infer_platform/internal/gateway"
)

func main() {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	cfg := gateway.LoadConfig()
	app, err := gateway.NewApp(ctx, cfg)
	if err != nil {
		slog.Error("failed to initialize gateway", slog.Any("error", err))
		os.Exit(1)
	}
	app.SpawnBackgroundJobs(ctx)

	srv := &http.Server{
		Addr:              ":" + cfg.Port,
		Handler:           app.Routes(),
		ReadHeaderTimeout: 10 * time.Second,
	}

	go func() {
		slog.Info("infer API gateway listening", slog.String("addr", srv.Addr), slog.String("routing_mode", cfg.RoutingMode))
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			slog.Error("gateway server failed", slog.Any("error", err))
			stop()
		}
	}()

	<-ctx.Done()
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	_ = srv.Shutdown(shutdownCtx)
}
