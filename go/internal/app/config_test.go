package app

import "testing"

func TestValidateNoopOutsideProduction(t *testing.T) {
	cases := []Config{
		{AppEnv: "", AdminKey: ""},
		{AppEnv: "development", AdminKey: "dev-admin-key"},
	}
	for _, c := range cases {
		if err := c.Validate(); err != nil {
			t.Fatalf("expected no error outside production, got %v", err)
		}
	}
}

func TestValidateRejectsWeakProductionAdminKey(t *testing.T) {
	cases := []string{"", "dev-admin-key", "test-admin-key", "admin", "changeme", "password"}
	for _, k := range cases {
		c := Config{AppEnv: "production", AdminKey: k}
		if err := c.Validate(); err == nil {
			t.Fatalf("expected error for weak production admin key %q", k)
		}
	}
}

func TestValidateAcceptsStrongProductionAdminKey(t *testing.T) {
	c := Config{AppEnv: "production", AdminKey: "s3cr3t-long-random-value"}
	if err := c.Validate(); err != nil {
		t.Fatalf("expected strong admin key to pass, got %v", err)
	}
}
