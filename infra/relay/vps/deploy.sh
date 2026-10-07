#!/bin/bash
# Runs on the VPS as `deploy`, from /opt/zafe-relay (CI copies this directory's files
# there first). Switches one network's relay to an image pinned by digest, and rolls back
# to the previous one if the new relay doesn't answer /health.
#
#   deploy.sh <testnet|mainnet> <image@sha256:...> <domain>     registry token (optional) on stdin
#
# .env keeps both networks' settings; a deploy changes only its own network's lines.
# `deployed/<network>.json` records what runs and since when (the mainnet workflow checks
# that the digest it promotes has been running on testnet long enough).
set -euo pipefail

network=${1:?usage: deploy.sh <testnet|mainnet> <image@sha256:...> <domain>}
image=${2:?usage: deploy.sh <testnet|mainnet> <image@sha256:...> <domain>}
domain=${3:?usage: deploy.sh <testnet|mainnet> <image@sha256:...> <domain>}
cd "$(dirname "$0")"

case $network in
    testnet) NET=TESTNET ;;
    mainnet) NET=MAINNET ;;
    *) echo "network must be testnet or mainnet: $network" >&2; exit 1 ;;
esac
[[ $image =~ ^[a-z0-9.:/_-]+@sha256:[0-9a-f]{64}$ ]] || { echo "not a pinned image: $image" >&2; exit 1; }
[[ $domain =~ ^[a-z0-9.-]+$ ]] || { echo "bad domain: $domain" >&2; exit 1; }

relay=relay-$network
backup=backup-$network

# Pull first, so a registry problem fails the deploy before anything changes. The token
# (CI's short-lived GITHUB_TOKEN) is only kept for the pull.
token=$(cat || true)
if [ -n "$token" ]; then
    printf '%s' "$token" | docker login ghcr.io -u deploy --password-stdin >/dev/null
    trap 'docker logout ghcr.io >/dev/null 2>&1 || true' EXIT
fi
docker pull -q "$image"

# The backup directory must be writable by the relay image's uid 10001 (a bind mount of a
# missing directory would be created root-owned). Idempotent; the deploy user is in the
# docker group, which is how it can do this without sudo.
docker run --rm -v /var/backups/zafe-relay:/b --entrypoint /bin/sh "$image" \
    -c "install -d -m700 -o 10001 -g 10001 /b/$network"

healthy() {
    for _ in $(seq 30); do
        if [ "$(docker compose exec -T caddy wget -qO- "http://$relay:8080/health" 2>/dev/null)" = ok ]; then
            return 0
        fi
        sleep 2
    done
    return 1
}

# A backup from the running relay before its database meets a new version.
if docker compose ps --status running --services 2>/dev/null | grep -qx "$backup"; then
    docker compose exec -T "$backup" /bin/sh /backup.sh once
fi

# Sets KEY=VALUE in .env, keeping every other line.
setenv() {
    { grep -v "^$1=" .env 2>/dev/null || true; printf '%s=%s\n' "$1" "$2"; } >.env.new
    mv .env.new .env
}

[ -f .env ] && cp .env .env.prev
setenv "RELAY_${NET}_IMAGE" "$image"
setenv "RELAY_${NET}_DOMAIN" "$domain"
if [ "$network" = mainnet ]; then
    setenv COMPOSE_PROFILES mainnet
    # The cap on vaults in the capped beta (0 = no limit); only set when given.
    if [ -n "${RELAY_MAX_VAULTS:-}" ]; then
        [[ $RELAY_MAX_VAULTS =~ ^[0-9]+$ ]] || { echo "bad RELAY_MAX_VAULTS" >&2; exit 1; }
        setenv RELAY_MAINNET_MAX_VAULTS "$RELAY_MAX_VAULTS"
    fi
fi

switch() {
    docker compose up -d --remove-orphans &&
        # backup.sh, litestream.yml and the Caddyfile may change without the service
        # definitions changing.
        docker compose restart "$backup" &&
        { [ "$network" != mainnet ] || docker compose restart litestream-mainnet; } &&
        docker compose exec -T caddy caddy reload --config /etc/caddy/Caddyfile &&
        healthy
}

if ! switch; then
    echo "relay unhealthy on $image" >&2
    docker compose logs --tail 50 "$relay" >&2 || true
    if [ -f .env.prev ]; then
        echo "rolling back to $(grep "RELAY_${NET}_IMAGE" .env.prev || echo 'the previous configuration')" >&2
        cp .env.prev .env
        docker compose up -d --remove-orphans
        healthy && echo "rollback healthy" >&2
    fi
    exit 1
fi

mkdir -p deployed
printf '{"image":"%s","domain":"%s","at":%s}\n' "$image" "$domain" "$(date -u +%s)" >"deployed/$network.json"

# Keep a week of old images for manual rollbacks.
docker image prune -af --filter until=168h >/dev/null
echo "deployed $network $image"
