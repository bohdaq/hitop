#!/usr/bin/env bash
# Runs ON the host, from a directory holding build.sh's out/ (www/ and
# hitop-server) plus hitop-server.service. Additive: touches only /var/www/hitop,
# /usr/local/bin/hitop-server, hitop-server.service and /etc/hitop-server/env. The front rws config is
# edited by hand (rws-routes.toml), checked with `rws --check=`.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
ts="$(date +%Y%m%d-%H%M%S)"

if [ -d /var/www/hitop ]; then sudo cp -a /var/www/hitop "/var/www/hitop.bak-$ts"; fi
sudo mkdir -p /var/www/hitop
sudo rsync -a --delete "$here/out/www/" /var/www/hitop/
sudo chown -R root:root /var/www/hitop

if [ -f /usr/local/bin/hitop-server ]; then sudo cp /usr/local/bin/hitop-server "/usr/local/bin/hitop-server.bak-$ts"; fi
sudo install -m 0755 "$here/out/hitop-server" /usr/local/bin/hitop-server
sudo install -m 0644 "$here/hitop-server.service" /etc/systemd/system/hitop-server.service
# The token for GET /api/stats; made once, read it with: sudo cat /etc/hitop-server/env
if ! sudo test -f /etc/hitop-server/env; then
  sudo install -d -m 0700 /etc/hitop-server
  echo "HITOP_STATS_TOKEN=$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')" | sudo install -m 0600 /dev/stdin /etc/hitop-server/env
fi
sudo systemctl daemon-reload
sudo systemctl enable hitop-server
sudo systemctl restart hitop-server
sleep 1
systemctl is-active hitop-server
curl -fsS -o /dev/null -w "site %{http_code}\n" http://127.0.0.1:7880/
curl -sS -o /dev/null -w "proxy without Origin %{http_code} (403 expected)\n" -X POST http://127.0.0.1:7880/api/proxy -d '{}'
curl -sS -o /dev/null -w "ping without a body %{http_code} (400 expected)\n" -X POST http://127.0.0.1:7880/api/ping -d '{}'
