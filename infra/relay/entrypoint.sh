#!/bin/sh
set -eu

data_dir=$(dirname "${ZAFE_RELAY_DB:-/data/relay.sqlite}")

if [ "$(id -u)" = "0" ]; then
    mkdir -p "$data_dir"
    chown zafe:zafe "$data_dir"
    find "$data_dir" -maxdepth 1 -name '*.sqlite*' -exec chown zafe:zafe {} +
    exec setpriv --reuid=zafe --regid=zafe --init-groups --inh-caps=-all \
        /usr/local/bin/zafe-relay "$@"
fi

exec /usr/local/bin/zafe-relay "$@"
