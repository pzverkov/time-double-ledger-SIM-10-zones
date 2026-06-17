package app

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"os"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/nats-io/nats.go"
	"github.com/prometheus/client_golang/prometheus/promhttp"

	"time-ledger-sim/go/internal/ledger"
	"time-ledger-sim/go/internal/messaging"
	"time-ledger-sim/go/internal/web"
)

type App struct {
	cfg Config
	log *slog.Logger
	db  *pgxpool.Pool
	nc  *nats.Conn
	js  nats.JetStreamContext

	shutdownTracer func(context.Context) error

	router http.Handler
	done   chan struct{}
}

func New(ctx context.Context, cfg Config) (*App, error) {
	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))
	shutdown, err := initTracer(ctx, cfg.OtelEndpoint)
	if err != nil {
		return nil, err
	}

	if cfg.DatabaseURL == "" {
		return nil, errors.New("DATABASE_URL required")
	}
	// Bound and recycle the pool so it survives a DB restart/failover: cap size,
	// retire connections by age, drop idle ones, and health-check periodically so
	// stale connections are pruned before a request gets one.
	poolCfg, err := pgxpool.ParseConfig(cfg.DatabaseURL)
	if err != nil {
		return nil, err
	}
	poolCfg.MaxConns = 16
	poolCfg.MinConns = 2
	poolCfg.MaxConnLifetime = 30 * time.Minute
	poolCfg.MaxConnIdleTime = 5 * time.Minute
	poolCfg.HealthCheckPeriod = 1 * time.Minute
	db, err := pgxpool.NewWithConfig(ctx, poolCfg)
	if err != nil {
		return nil, err
	}

	if err := db.Ping(ctx); err != nil {
		return nil, err
	}

	if cfg.NatsURL == "" {
		return nil, errors.New("NATS_URL required")
	}
	// Auth/TLS: user/password and tls:// are carried by NATS_URL; NATS_CREDS
	// (a JWT/nkey credentials file) is the standard production mechanism.
	natsOpts := []nats.Option{nats.MaxReconnects(-1), nats.ReconnectWait(500 * time.Millisecond)}
	if cfg.NatsCreds != "" {
		natsOpts = append(natsOpts, nats.UserCredentials(cfg.NatsCreds))
	}
	nc, err := nats.Connect(cfg.NatsURL, natsOpts...)
	if err != nil {
		return nil, err
	}
	js, err := nc.JetStream()
	if err != nil {
		return nil, err
	}

	if err := messaging.EnsureStreams(ctx, js); err != nil {
		return nil, err
	}

	led := ledger.New(db, logger)
	pub := messaging.NewOutboxPublisher(db, js, logger)
	fraud := messaging.NewFraudConsumer(db, js, logger)

	a := &App{
		cfg: cfg, log: logger, db: db, nc: nc, js: js,
		shutdownTracer: shutdown,
		done:           make(chan struct{}),
	}

	r := chi.NewRouter()
	r.Use(web.CORSMiddleware(cfg.CorsAllowOrigins))
	r.Get("/healthz", func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(200); _, _ = w.Write([]byte("ok")) })
	r.Handle("/metrics", promhttp.Handler())

	api := web.NewAPI(cfg.AdminKey, cfg.RateLimitRPS, cfg.TrustProxy, led, logger)
	api.RegisterRoutes(r)

	a.router = r

	// background loops
	go pub.Run(ctx)
	go fraud.Run(ctx)

	return a, nil
}

func (a *App) Router() http.Handler { return a.router }

func (a *App) Done() <-chan struct{} { return a.done }

func (a *App) Close() {
	defer close(a.done)
	if a.nc != nil {
		a.nc.Close()
	}
	if a.db != nil {
		a.db.Close()
	}
	if a.shutdownTracer != nil {
		_ = a.shutdownTracer(context.Background())
	}
}
