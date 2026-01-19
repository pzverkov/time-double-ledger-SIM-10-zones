.PHONY: test-go cover-go test-rust

test-go:
	cd go && go test ./...

cover-go:
	cd go && go test ./... -coverprofile=cover.out && go tool cover -func=cover.out

test-rust:
	cd rust/sim && cargo test
