#!/usr/bin/env bash
# Runs ON the host from a directory holding hitop-sync-server and hitop-sync.service. Touches only
# /usr/local/bin/hitop-sync-server, /etc/systemd/system/hitop-sync.service, /etc/hitop-sync/env
# (adds missing settings, never changes existing lines) and /var/lib/hitop-sync (backed up first).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ts="$(date +%Y%m%d-%H%M%S)"

if sudo test -f /var/lib/private/hitop-sync/sync.db; then
  # A consistent copy even while the server runs (WAL mode).
  sudo sqlite3 /var/lib/private/hitop-sync/sync.db ".backup /var/lib/private/hitop-sync/sync.db.bak-$ts" 2>/dev/null \
    || sudo cp -a /var/lib/private/hitop-sync/sync.db "/var/lib/private/hitop-sync/sync.db.bak-$ts"
fi
if [ -f /usr/local/bin/hitop-sync-server ]; then sudo cp /usr/local/bin/hitop-sync-server "/usr/local/bin/hitop-sync-server.bak-$ts"; fi
sudo install -m 0755 "$here/hitop-sync-server" /usr/local/bin/hitop-sync-server
sudo install -m 0644 "$here/hitop-sync.service" /etc/systemd/system/hitop-sync.service

sudo install -d -m 0700 /etc/hitop-sync
sudo touch /etc/hitop-sync/env && sudo chmod 0600 /etc/hitop-sync/env
sudo cp -a /etc/hitop-sync/env "/etc/hitop-sync/env.bak-$ts"
add() { sudo grep -q "^$1=" /etc/hitop-sync/env || echo "$1=$2" | sudo tee -a /etc/hitop-sync/env > /dev/null; }
add HITOP_SYNC_PUBLIC_URL "${HITOP_SYNC_PUBLIC_URL:-https://sync.hitop.rws8.tech}"
add HITOP_SYNC_RP_ID "${HITOP_SYNC_RP_ID:-hitop.rws8.tech}"
add HITOP_SYNC_RP_ORIGINS "${HITOP_SYNC_RP_ORIGINS:-https://sync.hitop.rws8.tech,https://hitop.rws8.tech}"
add HITOP_SYNC_ALLOWED_ORIGINS "${HITOP_SYNC_ALLOWED_ORIGINS:-https://hitop.rws8.tech}"
add SMTP_TLS_SERVER_NAME "${SMTP_TLS_SERVER_NAME:-mail.rws8.tech}"
add SMTP_FROM "${SMTP_FROM:-hitop sync <noreply@rws8.tech>}"

sudo systemctl daemon-reload
sudo systemctl enable hitop-sync
sudo systemctl restart hitop-sync
sleep 1
systemctl is-active hitop-sync
curl -fsS http://127.0.0.1:7881/healthz; echo
curl -sS -o /dev/null -w "db not served: %{http_code} (404 expected)\n" http://127.0.0.1:7881/sync.db
