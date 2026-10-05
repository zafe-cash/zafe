#!/bin/sh
# Backup sidecar (compose.yml): an online, consistent copy of /data/relay.sqlite through
# SQLite's backup API (safe while the relay runs, WAL included), checked with
# integrity_check, gzipped into /backups, 14 days kept.
#
#   backup.sh         one backup now, then one every night at 03:17 UTC (the sidecar)
#   backup.sh once    one backup now (deploy.sh runs this before switching images)
#
# /backups is a host directory (/var/backups/zafe-relay/<network>). Ship it off the
# machine too, or add Litestream (see ../README.md).
set -eu
umask 077

db=/data/relay.sqlite

backup() {
    if [ ! -f "$db" ]; then
        echo "backup: no database yet"
        return 0
    fi
    out=/backups/relay-$(date -u +%Y%m%dT%H%M%SZ).sqlite
    sqlite3 "$db" ".backup $out"
    if [ "$(sqlite3 "$out" 'PRAGMA integrity_check')" != ok ]; then
        echo "backup: integrity check failed for $out" >&2
        rm -f "$out"
        return 1
    fi
    gzip "$out"
    find /backups -name 'relay-*.sqlite.gz' -mtime +14 -delete
    echo "backup: $out.gz"
}

if [ "${1:-}" = once ]; then
    backup
    exit
fi

while :; do
    backup || true
    now=$(date -u +%s)
    next=$(date -u -d "$(date -u +%F) 03:17:00" +%s)
    [ "$next" -gt "$now" ] || next=$((next + 86400))
    sleep $((next - now))
done
