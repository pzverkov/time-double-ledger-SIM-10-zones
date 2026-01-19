# Version manifest

Both backends expose a simple endpoint:

- `GET /v1/version`

The Go implementation uses `runtime/debug.ReadBuildInfo()` so that:
- local `go run` returns version `(devel)`
- builds with VCS info embed commit/time automatically when built with Go's default VCS stamping

The dashboard reads this endpoint and shows the backend build info in the header so operators always know
which build they are looking at.
