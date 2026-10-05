#!/bin/bash
# One-time (and re-runnable) setup of a fresh Ubuntu/Debian VPS for the relay:
# Docker + Compose, firewall, automatic security updates, key-only SSH, and a `deploy`
# user for CI. After this, every deploy goes through .github/workflows/relay-deploy.yml.
#
#   ssh ubuntu@<host> 'sudo bash -s -- "<deploy public key>"' < infra/relay/vps/bootstrap.sh
#
# The deploy user is in the `docker` group, which is root-equivalent on this host: guard
# its key (GitHub environment secret, deploys only from main).
set -euo pipefail

deploy_key=${1:?usage: bootstrap.sh "<deploy public key>"}
app_dir=/opt/zafe-relay

export DEBIAN_FRONTEND=noninteractive
apt-get update -q
apt-get install -y -q docker.io docker-compose-v2 ufw unattended-upgrades curl

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

# The deploy user: owns the compose directory, runs docker, no sudo, no password.
id deploy >/dev/null 2>&1 || useradd --create-home --shell /bin/bash deploy
usermod -aG docker deploy
install -d -m700 -o deploy -g deploy /home/deploy/.ssh
printf '%s\n' "$deploy_key" >/home/deploy/.ssh/authorized_keys
chown deploy:deploy /home/deploy/.ssh/authorized_keys
chmod 600 /home/deploy/.ssh/authorized_keys
install -d -m750 -o deploy -g deploy "$app_dir"

# Backups land here, written by the relay image's uid 10001.
install -d -m700 /var/backups/zafe-relay
install -d -m700 /var/backups/zafe-relay/testnet
chown 10001:10001 /var/backups/zafe-relay/testnet

docker --version
docker compose version
echo "bootstrap done"
