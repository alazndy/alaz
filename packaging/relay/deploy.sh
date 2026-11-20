#!/usr/bin/env bash
# Installs or updates the relay on the server. Run as root, ON the server.
# Usage: deploy.sh /path/to/alaz-relay-binary
set -euo pipefail

bin="${1:?usage: deploy.sh /path/to/alaz-relay-binary}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ $EUID -eq 0 ]] || { echo "run as root" >&2; exit 1; }
[[ -f "$bin" ]] || { echo "binary not found: $bin" >&2; exit 1; }
[[ -f "$here/alaz-relay.service" ]] || { echo "alaz-relay.service not next to this script" >&2; exit 1; }

if ! id alaz-relay >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin alaz-relay
fi

# Directory is traversable by the group only; the key file itself is 0640 (see renew-cert.sh).
install -d -m 0750 -o root -g alaz-relay /etc/alaz-relay /etc/alaz-relay/tls

install -m 0755 -o root -g root "$bin" /usr/local/bin/alaz-relay.new
mv -f /usr/local/bin/alaz-relay.new /usr/local/bin/alaz-relay
install -m 0644 -o root -g root "$here/alaz-relay.service" /etc/systemd/system/alaz-relay.service

systemctl daemon-reload
systemctl enable alaz-relay.service

if [[ ! -f /etc/alaz-relay/tls/fullchain.pem || ! -f /etc/alaz-relay/tls/privkey.pem ]]; then
    echo "certificate missing in /etc/alaz-relay/tls: run renew-cert.sh first (see RUNBOOK.md); service not started" >&2
    exit 0
fi

systemctl restart alaz-relay.service
sleep 1
systemctl --no-pager --lines=5 status alaz-relay.service
