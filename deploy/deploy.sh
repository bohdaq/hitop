#!/usr/bin/env bash
# Builds everything and installs it on the host in one go, then checks the
# live site from the host itself (it can't reach its own public IP).
#   ./deploy/deploy.sh [user@host] [ssh port]
# Defaults: ubuntu@168.119.17.57 on port 22. From a machine that can't reach
# the host directly, open a tunnel first and pass ubuntu@127.0.0.1 <port>.
set -euo pipefail
dest="${1:-ubuntu@168.119.17.57}"
port="${2:-22}"
here="$(cd "$(dirname "$0")" && pwd)"
ssh_cmd=(ssh -p "$port" -o BatchMode=yes)

"$here/build.sh"
rsync -az --delete -e "${ssh_cmd[*]}" --exclude 'out/*.log' "$here/" "$dest:hitop-deploy/"
"${ssh_cmd[@]}" "$dest" 'bash -s' <<'REMOTE'
set -euo pipefail
cd ~/hitop-deploy && ./install.sh
H=hitop.rws8.tech
R="--resolve $H:443:127.0.0.1 -m 20"
for p in / /app/ /terms/ /privacy.html; do
  code=$(curl -s $R -o /dev/null -w '%{http_code}' "https://$H$p")
  echo "$code $p"
  test "$code" = 200
done
curl -fs $R -X POST "https://$H/api/proxy" -H "Origin: https://$H" -d '{"method":"GET","url":"https://example.com/"}' | grep -q '"status":200'
echo "proxy ok"
systemctl is-active rws hitop-server
REMOTE
echo "deployed to $dest"
