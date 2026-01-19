package main

import (
  "context"
  "log"
  "net/http"
  "os"
  "os/signal"
  "syscall"
  "time"

  "time-ledger-sim/go/internal/app"
)

func main() {
  cfg := app.LoadConfigFromEnv()

  ctx, cancel := context.WithCancel(context.Background())
  defer cancel()

  a, err := app.New(ctx, cfg)
  if err != nil {
    log.Fatalf("init: %v", err)
  }

  srv := &http.Server{
    Addr:              ":" + cfg.Port,
    Handler:           a.Router(),
    ReadHeaderTimeout: 5 * time.Second,
  }

  go func() {
    <-a.Done()
    _ = srv.Shutdown(context.Background())
  }()

  go func() {
    sig := make(chan os.Signal, 1)
    signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
    <-sig
    cancel()
    a.Close()
  }()

  log.Printf("sim-go listening on :%s", cfg.Port)
  if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
    log.Fatalf("http: %v", err)
  }
}
