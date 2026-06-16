package app

import (
	"errors"
	"fmt"
	"os"
)

type Config struct {
	CorsAllowOrigins string
	Port             string
	DatabaseURL      string
	NatsURL          string
	NatsCreds        string
	OtelEndpoint     string
	AdminKey         string
	AppEnv           string
}

func LoadConfigFromEnv() Config {
	cfg := Config{
		Port:             "8080",
		DatabaseURL:      os.Getenv("DATABASE_URL"),
		NatsURL:          os.Getenv("NATS_URL"),
		NatsCreds:        os.Getenv("NATS_CREDS"),
		OtelEndpoint:     os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT"),
		AdminKey:         os.Getenv("ADMIN_KEY"),
		AppEnv:           os.Getenv("APP_ENV"),
		CorsAllowOrigins: os.Getenv("CORS_ALLOW_ORIGINS"),
	}
	if p := os.Getenv("PORT"); p != "" {
		cfg.Port = p
	}
	if cfg.CorsAllowOrigins == "" {
		cfg.CorsAllowOrigins = "http://localhost:5173,http://localhost:4173"
	}
	return cfg
}

// Known weak/example admin keys that must never guard a production deployment.
var weakAdminKeys = map[string]bool{
	"dev-admin-key":  true,
	"test-admin-key": true,
	"changeme":       true,
	"admin":          true,
	"password":       true,
}

// Validate enforces production security invariants. When APP_ENV=production the
// admin key must be set and must not be a known weak default. A no-op outside
// production so the local demo keeps working with its dev defaults.
func (c Config) Validate() error {
	if c.AppEnv != "production" {
		return nil
	}
	if c.AdminKey == "" {
		return errors.New("ADMIN_KEY must be set in production")
	}
	if weakAdminKeys[c.AdminKey] {
		return fmt.Errorf("ADMIN_KEY is a known weak default (%q); set a strong secret in production", c.AdminKey)
	}
	return nil
}
