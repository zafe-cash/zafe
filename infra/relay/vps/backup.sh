#!/bin/sh
set -eu
umask 077

db=/data/relay.sqlite
ONCE_KEPT=2

backup() {
    if [ ! -f "$db" ]; then
        echo "backup: no database yet"
        return 0
    fi
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
