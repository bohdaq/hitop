#!/usr/bin/env bash
# postgres-setup.sh in a fresh ubuntu:24.04 container (the VM's release): installs the distribution
# Postgres, makes the socket-only `hitop` cluster, and the server binary connects with the URL it
# wrote. Also: no TCP listener, a second run changes nothing, --rotate-password works, and the
# role can't reach the postgres database.
#
#   BIN=sync-server/target/debug/hitop-sync-server cloud/ops/test/postgres-setup-docker.sh
set -euo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
BIN="$(cd "$(dirname "${BIN:?BIN: hitop-sync-server built with --features postgres}")" && pwd)/$(basename "$BIN")"
docker run --rm -v "$repo/sync-server/deploy:/deploy:ro" -v "$BIN:/usr/local/bin/hitop-sync-server:ro" ubuntu:24.04 bash -euo pipefail -c '
  apt-get update -q >/dev/null && DEBIAN_FRONTEND=noninteractive apt-get install -y -q iproute2 python3 >/dev/null
  /deploy/postgres-setup.sh
  url="$(cat /etc/hitop-sync/postgres.url)"
  [ "$(stat -c %a /etc/hitop-sync/postgres.url)" = 600 ] && echo "PASS the URL file is root only"
  HITOP_SYNC_DATABASE="$url" hitop-sync-server db-stats | tail -n 1 | grep -q "0 rows" && echo "PASS the server connects over the unix socket and migrates"
  if ss -ltn | grep -q ":5433"; then echo "FAIL a TCP listener on 5433"; exit 1; fi; echo "PASS no TCP listener for the hitop cluster"
  if pg_lsclusters | grep -E "main +[a-z]+ +online"; then echo "FAIL the unused main cluster runs"; exit 1; fi; echo "PASS the package main cluster is off"
  /deploy/postgres-setup.sh > /tmp/second
  [ "$(cat /etc/hitop-sync/postgres.url)" = "$url" ] && echo "PASS a second run keeps the password"
  /deploy/postgres-setup.sh --rotate-password > /dev/null
  new="$(cat /etc/hitop-sync/postgres.url)"
  [ "$new" != "$url" ] && HITOP_SYNC_DATABASE="$new" hitop-sync-server db-stats >/dev/null && echo "PASS --rotate-password: the new one works"
  if HITOP_SYNC_DATABASE="$url" hitop-sync-server db-stats >/dev/null 2>&1; then echo "FAIL the old password still works"; exit 1; fi; echo "PASS the old password no longer works"
  other="${new/\/hitop_sync?/\/postgres?}"
  if HITOP_SYNC_DATABASE="$other" hitop-sync-server db-stats >/dev/null 2>&1; then echo "FAIL the role reaches another database"; exit 1; fi; echo "PASS the role reaches only its database"
'
