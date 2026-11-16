#!/bin/sh
# Backup sidecar (compose.yml): an online, consistent copy of /data/relay.sqlite through
# SQLite's backup API (safe while the relay runs, WAL included), checked with
# integrity_check, gzipped into /backups.
#
#   backup.sh         a backup every night at 03:17 UTC (the sidecar); one at start only
#                     when no nightly copy is less than a day old (deploys restart it)
#   backup.sh once    one backup now (deploy.sh runs this before switching images)
#
# The copies on disk are bounded, because compose.yml sizes the database cap from them:
# nightly ones are kept 14 days (at most 15 files), `once` ones only the newest 2 (testnet
# deploys on every push to main). /backups is a host directory
# (/var/backups/zafe-relay/<network>). Ship it off the machine too, or add Litestream
# (see ../README.md).
set -eu
umask 077

db=/data/relay.sqlite
ONCE_KEPT=2

# backup <suffix>: "" for a nightly copy, "-once" for one taken by hand or by a deploy.
backup() {
    if [ ! -f "$db" ]; then
        echo "backup: no database yet"
        return 0
    fi
    # Two backups in one second (a deploy's, then a restart's) must not share a name:
    # gzip would refuse and leave an uncompressed copy no rule deletes.
    out=/backups/relay-$(date -u +%Y%m%dT%H%M%SZ)$1.sqlite
    while [ -e "$out" ] || [ -e "$out.gz" ]; do
        sleep 1
        out=/backups/relay-$(date -u +%Y%m%dT%H%M%SZ)$1.sqlite
    done
    sqlite3 "$db" ".backup $out"
    if [ "$(sqlite3 "$out" 'PRAGMA integrity_check')" != ok ]; then
        echo "backup: integrity check failed for $out" >&2
        rm -f "$out"
        return 1
    fi
    gzip "$out"
    find /backups -name 'relay-*.sqlite.gz' ! -name '*-once.sqlite.gz' -mtime +14 -delete
    # Names sort by time; drop all but the newest ONCE_KEPT.
    ls -1 /backups/relay-*-once.sqlite.gz 2>/dev/null | sort -r | tail -n +$((ONCE_KEPT + 1)) |
        xargs -r rm -f
    echo "backup: $out.gz"
}

if [ "${1:-}" = once ]; then
    backup -once
    exit
fi

if ! find /backups -name 'relay-*.sqlite.gz' ! -name '*-once.sqlite.gz' -mmin -1440 |
    grep -q .; then
    backup "" || true
fi
while :; do
    now=$(date -u +%s)
    next=$(date -u -d "$(date -u +%F) 03:17:00" +%s)
    [ "$next" -gt "$now" ] || next=$((next + 86400))
    sleep $((next - now))
    backup "" || true
done
