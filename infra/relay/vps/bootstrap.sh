#!/bin/bash
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

if dpkg -s caddy >/dev/null 2>&1; then
    systemctl disable --now caddy || true
    apt-get purge -y -q caddy
    rm -f /etc/apt/sources.list.d/caddy-stable.list /usr/share/keyrings/caddy-stable-archive-keyring.gpg
fi

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

ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 443/udp
ufw --force enable

echo 'unattended-upgrades unattended-upgrades/enable_auto_updates boolean true' | debconf-set-selections
dpkg-reconfigure -f noninteractive unattended-upgrades

cat >/etc/ssh/sshd_config.d/10-zafe.conf <<'EOF'
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin no
EOF
sshd -t
systemctl reload ssh

install -m755 -o root -g root "$here/zafe-deploy" /usr/local/sbin/zafe-deploy
if [ ! -f /etc/zafe-deploy.conf ]; then
    cat >/etc/zafe-deploy.conf <<'EOF'
TESTNET_DOMAIN=testnet.relay.zafe.cash
MAINNET_DOMAIN=relay.zafe.cash
EOF
fi

setup_user() {
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

if id deploy >/dev/null 2>&1; then
    gpasswd -d deploy docker >/dev/null 2>&1 || true
    : >/home/deploy/.ssh/authorized_keys 2>/dev/null || true
    usermod -L -s /usr/sbin/nologin deploy
    echo "retired the old 'deploy' user (you can delete it: userdel -r deploy)"
fi

install -d -m755 "$app_dir"
chown -R root:root "$app_dir"
chmod 600 "$app_dir"/*.env 2>/dev/null || true

install -d -m700 /var/backups/zafe-relay
install -d -m700 /var/backups/zafe-relay/testnet
chown 10001:10001 /var/backups/zafe-relay/testnet

docker --version
docker compose version
echo "bootstrap done"
