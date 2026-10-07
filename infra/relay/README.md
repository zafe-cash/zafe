# Deploying the relay

The relay is one small binary with one SQLite file. It is blind (public keys,
ciphertext and metadata only), so the host never holds anything that spends or
decrypts funds, but it does see who talks to which mailbox and when: pick a host you
trust with that metadata, and keep access logs off.

The hosted testnet relay runs on a VPS through Path B, deployed by CI
(`.github/workflows/relay-deploy.yml`). Two paths:

| | Fly.io | VPS (Docker Compose + Caddy) |
|---|---|---|
| TLS | Fly's edge (`*.fly.dev` or your domain) | Caddy + Let's Encrypt, automatic |
| Files | `fly.toml`, `Dockerfile` | `vps/` |
| Cost (roughly) | shared-cpu-1x 256 MB + 1 GB volume | any 1 vCPU / 1 GB box |
| You do | `fly auth login`, create app/volume, secrets | a server, a DNS record, `bootstrap.sh` |

Runtime contract (both paths):

- `GET /health` returns `200 ok` when the database answers.
- `ZAFE_RELAY_LISTEN` (address) or `PORT` (then `0.0.0.0:$PORT`), default `127.0.0.1:8787`.
- `ZAFE_RELAY_DB`: SQLite path. It runs in WAL mode, so the `-wal`/`-shm` files sit next
  to it: the whole directory must be on persistent storage.
- FCM pushes (optional): `ZAFE_FCM_SERVICE_ACCOUNT=/path/key.json` or
  `ZAFE_FCM_SERVICE_ACCOUNT_JSON='<the JSON>'`. Never commit the key or bake it into an
  image (`.dockerignore` excludes the usual names). Without it pushes are only logged
  and phones rely on background checks.
- Rate limits (on by default): 300 requests/min per signing key (charged only after the
  signature verifies, burst 150) and 1200/min per client IP (burst 600); over them the
  relay answers `429` with `Retry-After`, and the app says the relay is busy. Tune with
  `ZAFE_RELAY_KEY_RATE` / `ZAFE_RELAY_IP_RATE` (per minute, `0` = off), or
  `ZAFE_RELAY_LIMITS=off`. Behind a proxy set `ZAFE_RELAY_CLIENT_IP_HEADER` (Fly:
  `fly-client-ip`, already in `fly.toml`; Caddy: `x-forwarded-for`, in `vps/compose.yml`),
  or every client shares the proxy's address. Never point it at a header clients can set
  directly.
- Creating a vault (mailbox) is free, so creations are capped too: 20 a day per client
  IP (burst 10; `429`), and 8 per signing key (`507`; the app uses a fresh key per
  vault, so one is normal). Tune with `ZAFE_RELAY_CREATES_PER_DAY` and
  `ZAFE_RELAY_MAX_VAULTS_PER_KEY` (`0` = off).
- A **total** cap on vaults for a capped beta: `ZAFE_RELAY_MAX_VAULTS` (default: no
  limit). Over it, new vaults (and restores of vaults the relay doesn't know) get `507`
  with `zafe-quota: capacity`; the app says the beta is full. Existing vaults keep working.
- Request bodies are capped at 1 MiB (`413` above it).
- Long polls (`POST /v1/wait`): an open app holds one request for up to **25 s** so other
  members' activity reaches it at once. Any proxy in front must let a response take
  longer than that: Caddy's `reverse_proxy` has no response timeout by default (keep it
  that way, or set it above 40 s), and Fly's proxy idles out after 60 s. Each open app
  counts as one concurrent request on Fly, so `fly.toml` sets the concurrency limits to
  1000/1500. The relay caps waits at 2 per signing key and 4096 in total (over them:
  `429`); an older relay without the endpoint just makes apps fall back to polling.
- Storage quotas per vault (on by default): 10,000 undelivered envelopes per member,
  256 MiB of undelivered envelopes and 512 MiB of vault log. A write over a quota is
  refused whole with `507 Insufficient Storage`, and the app says the relay's storage for
  the vault is full. Undelivered envelopes expire after 30 days (hourly pruning), which
  frees their share; the log is kept forever. Tune with `ZAFE_RELAY_MAX_INBOX` (envelopes),
  `ZAFE_RELAY_MAX_DELIVERY_MB` / `ZAFE_RELAY_MAX_LOG_MB` (MiB), `0` = that quota off, or
  `ZAFE_RELAY_QUOTAS=off`. A database from schema 1 gets the quota counters added at
  startup (one scan); keep a backup before upgrading as usual.
- SIGTERM shuts down gracefully.
- Exactly **one** instance per database. Don't scale out.

## The image

```bash
# from the repository root
docker build -f infra/relay/Dockerfile -t zafe-relay .
docker run --rm -p 8080:8080 -v zafe-relay-data:/data zafe-relay
curl http://127.0.0.1:8080/health     # ok
```

Debian slim, the relay binary and `sqlite3` (for backups). The entrypoint starts as root
only to `chown` the data volume (Fly volumes and bind mounts arrive root-owned), then
runs the relay as `zafe` (uid 10001) through `setpriv`. `docker run --user 10001 ...`
skips that step if the volume is already writable by uid 10001.

## Path A: Fly.io

One-time setup (you, logged in to your Fly account):

```bash
fly auth login
# 1. pick a unique app name and region, and write them into infra/relay/fly.toml
fly apps create zafe-relay-testnet
# 2. the volume (same region as primary_region)
fly volumes create zafe_relay_data --app zafe-relay-testnet --region fra --size 1
# 3. optional: FCM pushes
fly secrets set --app zafe-relay-testnet \
  ZAFE_FCM_SERVICE_ACCOUNT_JSON="$(cat ~/.config/zafe/fcm-service-account.json)"
```

Deploy (from the repository root; the build context is the Cargo workspace):

```bash
fly deploy --config infra/relay/fly.toml --dockerfile infra/relay/Dockerfile --ha=false .
curl https://zafe-relay-testnet.fly.dev/health
```

`--ha=false` keeps it to one machine (the default would create two, and a second machine
would have its own empty volume). The template turns auto-stop off: the relay must stay
up to receive envelopes and send pushes.

Own domain (optional): `fly certs add relay.example.com --app zafe-relay-testnet`, then
create the DNS records `fly certs show` lists (CNAME to `<app>.fly.dev`, or A/AAAA).

Backups on Fly: volumes get daily snapshots (kept 5 days by default; `fly volumes
snapshots list <vol id>`). For an extra copy:

```bash
fly ssh console --app zafe-relay-testnet -C \
  "sqlite3 /data/relay.sqlite '.backup /data/backup.sqlite'"
fly ssh sftp get /data/backup.sqlite ./relay-backup.sqlite --app zafe-relay-testnet
```

## Path B: a VPS with Docker Compose (the hosted testnet relay)

`vps/compose.yml` runs Caddy (TLS) in front of the relay image, plus a backup sidecar.
The relay runs as uid 10001 with a read-only root filesystem, no capabilities and no
published port; only Caddy reaches it. Access logs stay off.

**Hosted testnet** (`testnet.relay.zafe.cash` on an OVH VPS, Ubuntu 26.04): every push to
`main` that touches the relay builds the image, pushes it to
`ghcr.io/zafe-cash/zafe-relay` (tags `sha-<commit>` and `main`, with build provenance:
`gh attestation verify oci://ghcr.io/zafe-cash/zafe-relay:main -R zafe-cash/zafe`), and
deploys that digest. `vps/deploy.sh` on the server pulls it, backs up the database,
switches, waits for `/health` and **rolls back** to the previous image if it fails.
Actions > Relay deploy > Run workflow redeploys by hand.

One-time setup of a new server (Ubuntu/Debian, you have sudo over SSH):

```bash
# 1. DNS: an A record for the domain pointing at the server, DNS only (no CDN proxy:
#    Caddy needs the real connection, and a proxy would see every client IP)

# 2. a key for CI, then the server setup: Docker, ufw (22/80/443), unattended-upgrades,
#    key-only SSH, the `deploy` user (docker group: root-equivalent), /opt/zafe-relay
ssh-keygen -t ed25519 -N '' -C zafe-relay-ci -f relay-ci
ssh ubuntu@<host> 'sudo bash -s -- "'"$(cat relay-ci.pub)"'"' < infra/relay/vps/bootstrap.sh

# 3. the GitHub environment `relay-testnet` (deploys from main only) and its settings
gh secret set RELAY_SSH_KEY --env relay-testnet < relay-ci
ssh-keyscan -t ed25519 <host> | gh secret set RELAY_KNOWN_HOSTS --env relay-testnet
#    (compare that host key with the one you see over your own SSH session)
gh variable set RELAY_HOST --env relay-testnet --body <host>
gh variable set RELAY_DOMAIN --env relay-testnet --body testnet.relay.zafe.cash
rm relay-ci relay-ci.pub

# 4. deploy: Actions > Relay deploy > Run workflow (or push to main)
curl https://testnet.relay.zafe.cash/health     # ok
```

**Self-hosting without CI:** copy `vps/` to the server, write `.env` with
`RELAY_TESTNET_IMAGE=ghcr.io/zafe-cash/zafe-relay:main` and
`RELAY_TESTNET_DOMAIN=<your domain>`, create `/var/backups/zafe-relay/testnet` owned by
uid 10001, and run `docker compose up -d`.

**FCM pushes** (optional): the relay reads `secrets.env` next to `compose.yml` if it
exists (mode 600, never in the repo or the image), with the Firebase service-account key
on one line, single-quoted so Compose takes it literally:
`ZAFE_FCM_SERVICE_ACCOUNT_JSON='{"type":"service_account",...}'`. The hosted relay gets it
from the `relay-testnet` environment secret `RELAY_FCM_SERVICE_ACCOUNT_JSON`, which the
deploy writes on every run (removing the secret removes the file). The app must be built
with the matching Firebase project's `google-services.json` (`docs/releasing.md`).

**Mainnet** (`relay.zafe.cash`, a capped beta): the same VPS runs a second relay
(`relay-mainnet`, compose profile `mainnet`) with its own volume, its nightly backup
(`backup-mainnet`, `/var/backups/zafe-relay/mainnet`), **Litestream** streaming the
database to object storage (`litestream-mainnet`; `restore-mainnet` brings the database
back from it when the volume is empty) and its own Caddy site block
(`vps/mainnet/relay.caddy`, copied in by the mainnet deploy). It never deploys from a push:
**Actions > Relay mainnet > Run workflow** with the digest testnet runs. The workflow
waits for a **required reviewer** (the `relay-mainnet` environment), then checks on the
server that testnet runs exactly that digest and has for `min_soak_hours` (24), verifies
the image's build provenance, writes the Litestream and FCM secrets, and runs
`deploy.sh mainnet`, which backs up, switches, health-checks and rolls back like testnet.
One-time setup (not done by CI):

```bash
# 1. DNS: an A record relay.zafe.cash -> the VPS, DNS only (no CDN proxy), like testnet
# 2. an S3-compatible bucket for the replica (Backblaze B2, R2, S3...), versioning off,
#    encryption at rest on, and a key that can only read and write that bucket
# 3. the GitHub environment `relay-mainnet`: required reviewers (not the person who
#    triggers the run, when there are two), deployment branches = main only
gh api -X PUT repos/zafe-cash/zafe/environments/relay-mainnet   # then set reviewers in the UI
gh secret set RELAY_SSH_KEY --env relay-mainnet < relay-ci
ssh-keyscan -t ed25519 <host> | gh secret set RELAY_KNOWN_HOSTS --env relay-mainnet
gh variable set RELAY_HOST --env relay-mainnet --body <host>
gh variable set RELAY_DOMAIN --env relay-mainnet --body relay.zafe.cash
gh variable set RELAY_MAX_VAULTS --env relay-mainnet --body 25      # the beta cap
gh secret set LITESTREAM_ACCESS_KEY_ID --env relay-mainnet
gh secret set LITESTREAM_SECRET_ACCESS_KEY --env relay-mainnet
gh variable set LITESTREAM_BUCKET --env relay-mainnet --body <bucket>
gh variable set LITESTREAM_ENDPOINT --env relay-mainnet --body https://<s3 endpoint>
gh variable set LITESTREAM_REGION --env relay-mainnet --body <region>
# 4. the first promotion: the digest testnet runs (/opt/zafe-relay/deployed/testnet.json)
gh workflow run relay-mainnet.yml -f digest=sha256:<digest>
curl https://relay.zafe.cash/health     # ok
```

The app's `kMainnetRelayUrl` is `https://relay.zafe.cash`; a mainnet build shows the beta
label and the per-vault cap (`ZAFE_BETA`, `ZAFE_BETA_CAP_ZAT`, `app/lib/src/core/config/beta.dart`).
After the first deploy, check a restore on a scratch machine before real funds arrive
(see "Backups").

Operations (as `ubuntu` or `deploy`, in `/opt/zafe-relay`):

```bash
docker compose ps
docker compose logs -f relay-testnet
docker compose exec backup-testnet sh /backup.sh once          # backup now
bash deploy.sh testnet ghcr.io/zafe-cash/zafe-relay@sha256:<old> testnet.relay.zafe.cash </dev/null   # manual rollback
# mainnet: the same with `mainnet`, relay-mainnet / backup-mainnet and relay.zafe.cash
```

## Backups

What's lost with the database: mailboxes, member lists, undelivered envelopes and the
encrypted vault logs. Members keep their keys (funds are safe), but the relay holds the
only copy of each vault's log that the members share, so a lost database means vaults
stop coordinating until a member restores it. The database is the thing to protect:
back it up here, and use a managed database with point-in-time recovery once the relay
scales out (Postgres). **A restored (older) database is detected:** every phone keeps its
own copy of each vault's log and refuses a relay that no longer holds its last entry
(`RelayRolledBack` / `RelayForked` / `RelayLostVault` in the app). A member whose phone
has the longest copy then taps "Restore vault on the relay" (Home > sync status): the
relay is recreated or caught up from that phone (`POST /v1/mailbox/reseed`; CLI:
`zafe restore-relay`; spec §6.3). So an old backup loses at most the messages in flight
(signing requests, which are asked for again) and push tokens, not the vault. Still
restore the newest backup you have.

- **Nightly `sqlite3 .backup`** (VPS: the `backup-testnet` sidecar, `vps/backup.sh`):
  an online, consistent copy through SQLite's backup API, checked with
  `integrity_check`, gzipped, kept 14 days in `/var/backups/zafe-relay/testnet`. One
  runs at 03:17 UTC, one whenever the sidecar starts, and one before every deploy. Ship
  that directory off the machine too (restic, rclone, the provider's backups).
- **Litestream** (mainnet on the VPS: `litestream-mainnet`, config `vps/litestream.yml`):
  streams the WAL to S3-compatible storage about every second, snapshots every 6 hours,
  keeps two weeks, and `restore-mainnet` restores the database on a fresh volume before
  the relay starts. Point-in-time restore by hand: stop the relay, then
  `docker compose run --rm --no-deps restore-mainnet restore -config /etc/litestream.yml -timestamp <RFC3339> -o /data/relay.sqlite /data/relay.sqlite`
  (move the old file away first; delete stale `-wal`/`-shm`). The relay's nightly
  `sqlite3 .backup` stays as a second, independent copy. Not wired on Fly; use its
  volume snapshots there.

Restore (VPS, in `/opt/zafe-relay`): stop the relay, replace `relay.sqlite` in the volume
(deleting stale `-wal`/`-shm`), start it. Backups are readable by root only:

```bash
sudo gunzip -c /var/backups/zafe-relay/testnet/relay-<time>.sqlite.gz > /tmp/relay.sqlite
docker compose stop relay-testnet backup-testnet
docker compose run --rm --no-deps --entrypoint /bin/rm backup-testnet -f /data/relay.sqlite-wal /data/relay.sqlite-shm
docker compose run --rm --no-deps -v /tmp/relay.sqlite:/restore.sqlite:ro --entrypoint /bin/cp \
  backup-testnet /restore.sqlite /data/relay.sqlite
docker compose start relay-testnet backup-testnet && rm /tmp/relay.sqlite
```

## Pointing the app at it

Build the app with the testnet preset and the relay URL:

```bash
flutter build apk --dart-define=ZAFE_NETWORK=test \
  --dart-define=ZAFE_RELAY_URL=https://testnet.relay.zafe.cash
```

The testnet preset already uses `https://testnet.zec.rocks:443` (Ironwood-aware
lightwalletd, over TLS); override with `ZAFE_LIGHTWALLETD_URL`. Without
`ZAFE_RELAY_URL` a testnet build points at a placeholder (`relay.zafe.invalid`) and
Settings shows the relay as "Not configured". The CLI takes `--relay https://...` (or `ZAFE_RELAY`).
