#!/bin/bash
# Runs on the VPS as `deploy`, from /opt/zafe-relay (CI copies this directory's files
# there first). Switches the testnet relay to an image pinned by digest, and rolls back
# to the previous one if the new relay doesn't answer /health.
#
#   deploy.sh <image@sha256:...> <domain>      registry token (optional) on stdin
set -euo pipefail

image=${1:?usage: deploy.sh <image@sha256:...> <domain>}
domain=${2:?usage: deploy.sh <image@sha256:...> <domain>}
cd "$(dirname "$0")"

[[ $image =~ ^[a-z0-9.:/_-]+@sha256:[0-9a-f]{64}$ ]] || { echo "not a pinned image: $image" >&2; exit 1; }
[[ $domain =~ ^[a-z0-9.-]+$ ]] || { echo "bad domain: $domain" >&2; exit 1; }

# Pull first, so a registry problem fails the deploy before anything changes. The token
# (CI's short-lived GITHUB_TOKEN) is only kept for the pull.
token=$(cat || true)
if [ -n "$token" ]; then
    printf '%s' "$token" | docker login ghcr.io -u deploy --password-stdin >/dev/null
    trap 'docker logout ghcr.io >/dev/null 2>&1 || true' EXIT
fi
docker pull -q "$image"

healthy() {
    for _ in $(seq 30); do
        if [ "$(docker compose exec -T caddy wget -qO- http://relay-testnet:8080/health 2>/dev/null)" = ok ]; then
            return 0
        fi
        sleep 2
    done
    return 1
}

# A backup from the running relay before its database meets a new version.
if docker compose ps --status running --services 2>/dev/null | grep -qx backup-testnet; then
    docker compose exec -T backup-testnet /bin/sh /backup.sh once
fi

[ -f .env ] && cp .env .env.prev
cat >.env <<EOF
RELAY_TESTNET_IMAGE=$image
RELAY_TESTNET_DOMAIN=$domain
EOF

switch() {
    docker compose up -d --remove-orphans &&
        # backup.sh and the Caddyfile may change without the service definitions changing.
        docker compose restart backup-testnet &&
        docker compose exec -T caddy caddy reload --config /etc/caddy/Caddyfile &&
        healthy
}

if ! switch; then
    echo "relay unhealthy on $image" >&2
    docker compose logs --tail 50 relay-testnet >&2 || true
    if [ -f .env.prev ]; then
        echo "rolling back to $(grep RELAY_TESTNET_IMAGE .env.prev)" >&2
        cp .env.prev .env
        docker compose up -d --remove-orphans
        healthy && echo "rollback healthy" >&2
    fi
    exit 1
fi

# Keep a week of old images for manual rollbacks.
docker image prune -af --filter until=168h >/dev/null
echo "deployed $image"
