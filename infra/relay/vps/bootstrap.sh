#!/bin/bash
# One-time (and re-runnable) setup of a fresh Ubuntu/Debian VPS for the relay: Docker +
# Compose, firewall, automatic security updates, key-only SSH, and one locked-down deploy
# user per network for CI. After this, every deploy goes through
# .github/workflows/relay-deploy.yml (testnet) and relay-mainnet.yml (mainnet).
#
#   scp infra/relay/vps/bootstrap.sh infra/relay/vps/zafe-deploy ubuntu@<host>:
#   ssh -t ubuntu@<host> sudo bash bootstrap.sh "<testnet public key>" "<mainnet public key>"
#
# Deploy access (the point of this script): the CI keys do NOT get a shell or the
# `docker` group (which is root-equivalent). Each key belongs to its own unix user
# (zafe-testnet, zafe-mainnet), is pinned in authorized_keys to a forced command
#   restrict,command="sudo -n /usr/local/sbin/zafe-deploy <network>"
# and sudoers lets that user run exactly that line. /usr/local/sbin/zafe-deploy (root-owned,
# installed here and by nothing else) accepts four fixed commands, validates every
# argument, takes its files from a commit on `main` instead of from the caller, and only
# touches its own network (see the header of zafe-deploy). /opt/zafe-relay is root-owned.
# To change the wrapper, re-run this script.
set -euo pipefail

usage='usage: bootstrap.sh "<testnet public key>" "<mainnet public key>"'
testnet_key=${1:?$usage}
mainnet_key=${2:?$usage}
[ "$testnet_key" != "$mainnet_key" ] || { echo "use two different keys" >&2; exit 1; }
key_re='^ssh-ed25519 [A-Za-z0-9+/=]+( [A-Za-z0-9@._-]+)?$'
for k in "$testnet_key" "$mainnet_key"; do
    [[ $k =~ $key_re ]] || { echo "not an ed25519 public key line: $k" >&2; exit 1; }
done
here=$(cd "$(dirname "$0")" && pwd)
[ -f "$here/zafe-deploy" ] || { echo "zafe-deploy must sit next to bootstrap.sh" >&2; exit 1; }
app_dir=/opt/zafe-relay

export DEBIAN_FRONTEND=noninteractive
apt-get update -q
apt-get install -y -q docker.io docker-compose-v2 ufw unattended-upgrades curl jq sudo

# Caddy runs in compose and needs ports 80/443: drop a host Caddy if one was installed.
if dpkg -s caddy >/dev/null 2>&1; then
    systemctl disable --now caddy || true
    apt-get purge -y -q caddy
    rm -f /etc/apt/sources.list.d/caddy-stable.list /usr/share/keyrings/caddy-stable-archive-keyring.gpg
fi

# Container logs: rotate instead of filling the disk.
install -d /etc/docker
if [ ! -f /etc/docker/daemon.json ]; then
    cat >/etc/docker/daemon.json <<'EOF'
{
  "log-driver": "local",
  "log-opts": { "max-size": "10m", "max-file": "5" }
}
EOF
    systemctl restart docker
fi
systemctl enable --now docker

# Firewall. Docker's published ports (80/443) bypass ufw, so nothing else may be published.
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 443/udp
ufw --force enable

# Security updates every day.
echo 'unattended-upgrades unattended-upgrades/enable_auto_updates boolean true' | debconf-set-selections
dpkg-reconfigure -f noninteractive unattended-upgrades

# Key-only SSH, no root login.
cat >/etc/ssh/sshd_config.d/10-zafe.conf <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
EOF
sshd -t
systemctl reload ssh

# The deploy wrapper and the domains it accepts (edit /etc/zafe-deploy.conf to change them).
install -m755 -o root -g root "$here/zafe-deploy" /usr/local/sbin/zafe-deploy
if [ ! -f /etc/zafe-deploy.conf ]; then
    cat >/etc/zafe-deploy.conf <<'EOF'
TESTNET_DOMAIN=testnet.relay.zafe.cash
MAINNET_DOMAIN=relay.zafe.cash
EOF
fi

# One unix user per network: no docker group, no password, no shell access (the forced
# command is all its key can run). Home and authorized_keys are root-owned, so the user
# cannot edit them.
setup_user() { # <network> <public key>
    local user=zafe-$1
    id "$user" >/dev/null 2>&1 || useradd --create-home --shell /bin/bash "$user"
    gpasswd -d "$user" docker >/dev/null 2>&1 || true
    passwd -l "$user" >/dev/null
    chown root:root "/home/$user"
    chmod 755 "/home/$user"
    install -d -m755 -o root -g root "/home/$user/.ssh"
    printf 'restrict,command="sudo -n /usr/local/sbin/zafe-deploy %s" %s\n' "$1" "$2" \
        >"/home/$user/.ssh/authorized_keys"
    chown root:root "/home/$user/.ssh/authorized_keys"
    chmod 644 "/home/$user/.ssh/authorized_keys"
    cat >"/etc/sudoers.d/zafe-deploy-$1" <<EOF
Defaults:$user env_keep += "SSH_ORIGINAL_COMMAND"
$user ALL=(root) NOPASSWD: /usr/local/sbin/zafe-deploy $1
EOF
    chmod 440 "/etc/sudoers.d/zafe-deploy-$1"
    visudo -cf "/etc/sudoers.d/zafe-deploy-$1" >/dev/null
}
setup_user testnet "$testnet_key"
setup_user mainnet "$mainnet_key"

# Retire the old shared `deploy` user (docker group, one key in both environments).
if id deploy >/dev/null 2>&1; then
    gpasswd -d deploy docker >/dev/null 2>&1 || true
    : >/home/deploy/.ssh/authorized_keys 2>/dev/null || true
    usermod -L -s /usr/sbin/nologin deploy
    echo "retired the old 'deploy' user (you can delete it: userdel -r deploy)"
fi

# The compose directory belongs to root: only the wrapper writes there.
install -d -m755 "$app_dir"
chown -R root:root "$app_dir"
chmod 600 "$app_dir"/*.env 2>/dev/null || true

# Backups land here, written by the relay image's uid 10001.
install -d -m700 /var/backups/zafe-relay
install -d -m700 /var/backups/zafe-relay/testnet
chown 10001:10001 /var/backups/zafe-relay/testnet

docker --version
docker compose version
echo "bootstrap done"
