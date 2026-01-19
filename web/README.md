# Time Ledger Sim: Operator Dashboard

This is a static operator console for the Time Ledger simulation (zones + ledger + incidents).

## Local dev

Start infra + backend (Go):

```bash
docker compose -f ../infra/docker-compose.yml up -d
cd ../go
go run ./cmd/sim-go
```

Run the dashboard:

```bash
npm install
npm run dev
```

Open: http://localhost:5173

By default the dev server proxies `/v1/*` to `http://localhost:8080`.

## Deploy to GitHub Pages

A workflow is provided at `.github/workflows/pages.yml`.

The dashboard is built with `base: "./"` so it works from a Pages subpath.

Set the API base in the UI header, or provide `VITE_API_BASE` at build time.

## CORS

If you host the dashboard on GitHub Pages and the backend elsewhere, set:

- `CORS_ALLOW_ORIGINS=https://<user>.github.io` (or your custom domain)

