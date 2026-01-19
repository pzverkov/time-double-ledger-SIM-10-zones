package app

import "os"

type Config struct {
  CorsAllowOrigins string
  Port        string
  DatabaseURL string
  NatsURL     string
  OtelEndpoint string
  AdminKey    string
}

func LoadConfigFromEnv() Config {
  cfg := Config{
    Port: "8080",
    DatabaseURL: os.Getenv("DATABASE_URL"),
    NatsURL: os.Getenv("NATS_URL"),
    OtelEndpoint: os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT"),
    AdminKey: os.Getenv("ADMIN_KEY"),
    CorsAllowOrigins: os.Getenv("CORS_ALLOW_ORIGINS"),
  }
  if p := os.Getenv("PORT"); p != "" { cfg.Port = p }
  if cfg.CorsAllowOrigins == "" { cfg.CorsAllowOrigins = "http://localhost:5173,http://localhost:4173" }
  return cfg
}
