#!/usr/bin/env bash
# Certificate renewal for relay.alazlab.com. Run as root, ON the server.
# ACME client: certbot, HTTP-01 standalone (needs 80/tcp open while it runs).
#
#   renew-cert.sh issue    install the renewal hook, get the first certificate (once)
#   renew-cert.sh renew    manual test: run "certbot renew"; the hook fires if it renewed
#   renew-cert.sh deploy   copy the current certificate and send SIGHUP (what the hook calls)
set -euo pipefail

domain="relay.alazlab.com"
dest="/etc/alaz-relay/tls"
self="$(readlink -f "${BASH_SOURCE[0]}")"

[[ $EUID -eq 0 ]] || { echo "run as root" >&2; exit 1; }

deploy() {
    # certbot sets RENEWED_LINEAGE for hooks; fall back to the default path for manual runs.
    local live="${RENEWED_LINEAGE:-/etc/letsencrypt/live/$domain}"
    [[ -f "$live/fullchain.pem" && -f "$live/privkey.pem" ]] || { echo "no certificate in $live" >&2; exit 1; }

    # /etc/letsencrypt is root-only, so the service gets its own copies.
    install -d -m 0750 -o root -g alaz-relay "$dest"
    install -m 0644 -o root -g alaz-relay "$live/fullchain.pem" "$dest/fullchain.pem.new"
    install -m 0640 -o root -g alaz-relay "$live/privkey.pem" "$dest/privkey.pem.new"
    mv -f "$dest/fullchain.pem.new" "$dest/fullchain.pem"
    mv -f "$dest/privkey.pem.new" "$dest/privkey.pem"

    # The relay reloads certificates on SIGHUP; a stopped service just picks them up at start.
    if systemctl is-active --quiet alaz-relay.service; then
        systemctl kill --signal=SIGHUP alaz-relay.service
    fi
}

# The hook lives in certbot's own hooks directory, so the copy + SIGHUP happens
# whichever scheduler renews (the distro certbot.timer, cron, or a manual run).
install_hook() {
    install -m 0755 "$self" /usr/local/sbin/alaz-relay-renew
    install -d -m 0755 /etc/letsencrypt/renewal-hooks/deploy
    cat >/etc/letsencrypt/renewal-hooks/deploy/alaz-relay <<'HOOK'
#!/bin/sh
exec /usr/local/sbin/alaz-relay-renew deploy
HOOK
    chmod 0755 /etc/letsencrypt/renewal-hooks/deploy/alaz-relay
}

case "${1:-}" in
    issue)
        install_hook
        certbot certonly --standalone --non-interactive --agree-tos \
            --preferred-challenges http -d "$domain" \
            --register-unsafely-without-email
        deploy
        ;;
    renew)
        certbot renew --quiet
        ;;
    deploy)
        deploy
        ;;
    *)
        echo "usage: $0 issue|renew|deploy" >&2
        exit 2
        ;;
esac
