# Preflight: EU-first Cloud Run Deployment (with optional AU/SA/AF later)

Goal: deploy the API to **EU (main demo)** with minimal monthly cost, while keeping AU/SA/AF as optional stacks we can deploy later **without copy/paste drift**.

We start with:
- No domain
- No load balancer
- Cloud Run `*.run.app` URL
- Dashboard on GitHub Pages calling the API via CORS allowlist

---

## 0) One minute sanity rules (don’t skip)

- **Never commit secrets** (DB passwords, admin keys, private keys).
- **Operator endpoints are protected** by `ADMIN_KEY`.
- **CORS** is allowlist-only (GitHub Pages origin).
- **EU is first**. AU/SA/AF are off by default.

---

## 1) Create / select a GCP project and enable billing

```bash
export PROJECT_ID="YOUR_PROJECT_ID"
gcloud projects create "$PROJECT_ID"

# Link billing (you need a billing account ID)
gcloud billing accounts list
gcloud billing projects link "$PROJECT_ID" --billing-account="BILLING_ACCOUNT_ID"
```

---

## 2) Enable required APIs

```bash
gcloud services enable \
  run.googleapis.com \
  artifactregistry.googleapis.com \
  cloudbuild.googleapis.com \
  secretmanager.googleapis.com \
  iam.googleapis.com \
  sqladmin.googleapis.com \
  compute.googleapis.com \
  --project "$PROJECT_ID"
```

---

## 3) Create Artifact Registry (EU)

This stores container images built by CI.

```bash
export REGION_EU="europe-west1"

gcloud artifacts repositories create tlsim \
  --repository-format=docker \
  --location="$REGION_EU" \
  --project "$PROJECT_ID"
```

---

## 4) Create deploy Service Account (for GitHub Actions)

```bash
gcloud iam service-accounts create tlsim-deployer --project "$PROJECT_ID"
export DEPLOY_SA="tlsim-deployer@${PROJECT_ID}.iam.gserviceaccount.com"
```

Grant minimum roles (start here; tighten later if needed):

```bash
gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:$DEPLOY_SA" \
  --role="roles/run.admin"

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:$DEPLOY_SA" \
  --role="roles/iam.serviceAccountUser"

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:$DEPLOY_SA" \
  --role="roles/artifactregistry.admin"

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:$DEPLOY_SA" \
  --role="roles/cloudbuild.builds.editor"

gcloud projects add-iam-policy-binding "$PROJECT_ID" \
  --member="serviceAccount:$DEPLOY_SA" \
  --role="roles/secretmanager.secretAccessor"
```

---

## 5) Create secrets in Secret Manager (EU)

You need:
- DB password
- Admin key (protect controls + snapshot/restore)

Generate strong values:

```bash
openssl rand -base64 48 | tr -d '\n' > /tmp/tlsim_db_pass
openssl rand -base64 48 | tr -d '\n' > /tmp/tlsim_admin_key
```

Create secrets:

```bash
gcloud secrets create tlsim-db-pass-eu --data-file=/tmp/tlsim_db_pass --project "$PROJECT_ID"
gcloud secrets create tlsim-admin-key-eu --data-file=/tmp/tlsim_admin_key --project "$PROJECT_ID"
```

Later (optional stacks), repeat with:
- `tlsim-db-pass-au`
- `tlsim-admin-key-au`
(and same for `sa`, `af`)

---

## 6) Create Cloud SQL Postgres (EU)

Pick a small instance for the demo. HA can wait.

```bash
export SQL_INSTANCE_EU="tlsim-pg-eu"

gcloud sql instances create "$SQL_INSTANCE_EU" \
  --database-version=POSTGRES_16 \
  --region="europe-west1" \
  --cpu=1 --memory=3840MB \
  --storage-type=SSD --storage-size=10GB \
  --project "$PROJECT_ID"
```

Create DB + user:

```bash
gcloud sql databases create tlsim --instance="$SQL_INSTANCE_EU" --project "$PROJECT_ID"

gcloud sql users create tlsim_app \
  --instance="$SQL_INSTANCE_EU" \
  --password="$(cat /tmp/tlsim_db_pass)" \
  --project "$PROJECT_ID"
```

---

## 7) NATS JetStream (EU) - cheapest option

Cloud Run is stateless; JetStream is stateful. For MVP, run NATS on a small VM in-region.

Create a VM (example; adjust machine type later):

```bash
export NATS_VM_EU="tlsim-nats-eu"
gcloud compute instances create "$NATS_VM_EU" \
  --zone="europe-west1-b" \
  --machine-type=e2-small \
  --boot-disk-size=20GB \
  --image-family=debian-12 \
  --image-project=debian-cloud \
  --project "$PROJECT_ID"
```

SSH in and install NATS (simple path: docker):

```bash
gcloud compute ssh "$NATS_VM_EU" --zone="europe-west1-b" --project "$PROJECT_ID"
```

Inside VM:

```bash
sudo apt-get update && sudo apt-get install -y docker.io
sudo systemctl enable --now docker

sudo docker run -d --name nats \
  -p 4222:4222 -p 8222:8222 \
  -v /var/lib/nats:/data \
  nats:2.12 \
  -js --sd /data
```

---

## 8) Database migrations

Run migrations locally once, or via a CI job, or via Cloud Run Job.
For EU demo, simplest is local (after DB is reachable).

---

## 9) GitHub Actions: keyless auth (WIF/OIDC)

We deploy from GitHub without storing JSON keys.

High-level steps:
1) Create Workload Identity Pool + Provider for GitHub
2) Bind your repo identity to the service account `tlsim-deployer`
3) Add GitHub secrets:
   - `GCP_WIF_PROVIDER`
   - `GCP_DEPLOY_SA_EMAIL`
   - `GCP_PROJECT_ID`

---

## 10) Configure CORS for the dashboard

Set:
- `CORS_ALLOW_ORIGINS=https://YOURNAME.github.io`

---

## 11) First deploy (EU only)

Push to `main`. CI should deploy `tlsim-api-eu` and output a `run.app` URL.

Point the dashboard to it:
- `VITE_API_BASE=https://tlsim-api-eu-xxxxx.run.app`

---

## 12) Optional regions later (AU/SA/AF)

Deploy them only when needed:
- create secrets for that stack
- create Cloud SQL instance in that region
- create NATS VM in that region
- run deploy workflow with that stack enabled

No code changes. Only config.

---

## Common mistakes (avoid these)

- Turning on HA DB too early (doubles cost).
- Running multiple regions 24/7 without need (baseline cost multiplies).
- Forgetting CORS allowlist (dashboard “works locally” then dies in prod).
- Hosting JetStream on Cloud Run (state needs a home).

---

## When to buy a domain / add a load balancer

Do it only when:
- you want a clean URL
- you want multi-region routing under one hostname

Load balancer adds baseline cost even at low traffic.
