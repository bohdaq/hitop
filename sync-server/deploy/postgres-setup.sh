#!/usr/bin/env bash
# HITOP Cloud C2: a Postgres cluster of its own for the sync server, on the shared VM. Run as root
# ON the host. Idempotent: a second run changes nothing (pass --rotate-password to set a new one).
#
# - Installs the distribution's Postgres if there is none (apt). If Postgres is already there (for
#   another site), nothing of it is changed: HITOP gets a separate cluster `hitop` with its own data
#   directory (/var/lib/postgresql/<major>/hitop), config and service (postgresql@<major>-hitop).
#   A `main` cluster that the package install itself created just now is stopped and set to
#   `disabled` (it was never used); one that existed before is left alone.
# - The cluster listens on no TCP address at all: only its unix socket in /run/postgresql
#   (port 5433 names the socket). pg_hba: the postgres superuser by peer, the role hitop_sync with a
#   password (scram-sha-256) on database hitop_sync, nothing else.
# - The role's password is generated here (32 random bytes) and kept only in the root-only file
#   /etc/hitop-sync/postgres.url (0600) as the URL the server needs. Nothing else changes: the
#   switch of HITOP_SYNC_DATABASE is cutover-postgres.sh.
# - Limits (resource-limits drop-in for the cluster's unit): cloud/ops/systemd/.
#
# Settings (environment): PG_PORT (5433), CLUSTER (hitop), URL_FILE, PG_MAJOR (default: the
# installed one, else what apt installs).
set -euo pipefail

PG_PORT="${PG_PORT:-5433}"
CLUSTER="${CLUSTER:-hitop}"
URL_FILE="${URL_FILE:-/etc/hitop-sync/postgres.url}"
ROLE=hitop_sync
DB=hitop_sync
rotate=0
[ "${1:-}" = "--rotate-password" ] && rotate=1

[ "$(id -u)" = 0 ] || { echo "run as root" >&2; exit 2; }
log() { echo "postgres-setup: $*"; }
ts="$(date -u +%Y%m%dT%H%M%SZ)"

majors() { ls /usr/lib/postgresql 2>/dev/null | sort -n; }
installed_now=0
if [ -z "$(majors)" ]; then
  log "no Postgres installed: installing the distribution's package"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -q
  apt-get install -y -q postgresql >/dev/null
  installed_now=1
else
  log "Postgres already installed ($(majors | tr '\n' ' ')): its clusters stay as they are"
fi
PG_MAJOR="${PG_MAJOR:-$(majors | tail -n 1)}"

# The package install just now also made an (empty) cluster `main` on localhost:5432: off, so the
# only Postgres HITOP adds is its own socket-only cluster. Never done to a Postgres that was here.
if [ "$installed_now" = 1 ] && [ -d "/etc/postgresql/$PG_MAJOR/main" ]; then
  log "stopping the unused cluster $PG_MAJOR/main the package created, and setting it to disabled"
  pg_ctlcluster "$PG_MAJOR" main stop 2>/dev/null || true
  echo disabled > "/etc/postgresql/$PG_MAJOR/main/start.conf"
fi

conf="/etc/postgresql/$PG_MAJOR/$CLUSTER"
if [ ! -d "$conf" ]; then
  log "creating cluster $PG_MAJOR/$CLUSTER (socket only, port $PG_PORT)"
  pg_createcluster "$PG_MAJOR" "$CLUSTER" --port "$PG_PORT" -- --auth-local=peer --auth-host=reject --encoding=UTF8 --locale=C.UTF-8 >/dev/null
fi
mkdir -p "$conf/conf.d"
cat > "$conf/conf.d/hitop.conf" <<EOF
# Written by hitop sync-server/deploy/postgres-setup.sh. No TCP at all: the unix socket only.
listen_addresses = ''
port = $PG_PORT
unix_socket_directories = '/run/postgresql'
max_connections = 20
shared_buffers = 128MB
work_mem = 4MB
maintenance_work_mem = 64MB
password_encryption = scram-sha-256
log_min_duration_statement = 1000
log_connections = off
EOF
cp -a "$conf/pg_hba.conf" "$conf/pg_hba.conf.bak-hitop-$ts" 2>/dev/null || true
cat > "$conf/pg_hba.conf" <<EOF
# Written by hitop sync-server/deploy/postgres-setup.sh: the admin by peer, the sync server by
# password, both over the unix socket. Nothing over TCP.
local   all        postgres                 peer
local   $DB  $ROLE              scram-sha-256
EOF
chown postgres:postgres "$conf/pg_hba.conf" "$conf/conf.d/hitop.conf"
chmod 0640 "$conf/pg_hba.conf"

if command -v systemctl >/dev/null && [ -d /run/systemd/system ]; then
  systemctl enable "postgresql@$PG_MAJOR-$CLUSTER" >/dev/null 2>&1 || true
  systemctl restart "postgresql@$PG_MAJOR-$CLUSTER"
else
  pg_ctlcluster "$PG_MAJOR" "$CLUSTER" restart 2>/dev/null || pg_ctlcluster "$PG_MAJOR" "$CLUSTER" start
fi
for _ in $(seq 30); do su postgres -c "psql -At -p $PG_PORT -c 'SELECT 1'" >/dev/null 2>&1 && break; sleep 1; done
psql_admin() { su postgres -c "psql -v ON_ERROR_STOP=1 -At -p $PG_PORT $*"; }

install -d -m 0700 "$(dirname "$URL_FILE")"
have_role="$(psql_admin "-c \"SELECT 1 FROM pg_roles WHERE rolname = '$ROLE'\"")"
if [ -z "$have_role" ] || [ "$rotate" = 1 ] || [ ! -s "$URL_FILE" ]; then
  pw="$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
  if [ -z "$have_role" ]; then
    log "creating role $ROLE"
    psql_admin "-c \"CREATE ROLE $ROLE LOGIN PASSWORD '$pw'\"" >/dev/null
  else
    log "setting a new password for $ROLE"
    psql_admin "-c \"ALTER ROLE $ROLE PASSWORD '$pw'\"" >/dev/null
  fi
  [ -s "$URL_FILE" ] && cp -a "$URL_FILE" "$URL_FILE.bak-$ts"
  umask 077
  printf 'postgres://%s:%s@localhost/%s?host=/run/postgresql&port=%s\n' "$ROLE" "$pw" "$DB" "$PG_PORT" > "$URL_FILE"
  chmod 0600 "$URL_FILE"
fi
if [ -z "$(psql_admin "-c \"SELECT 1 FROM pg_database WHERE datname = '$DB'\"")" ]; then
  log "creating database $DB"
  psql_admin "-c \"CREATE DATABASE $DB OWNER $ROLE ENCODING 'UTF8' TEMPLATE template0\"" >/dev/null
fi
psql_admin "-c \"REVOKE ALL ON DATABASE $DB FROM PUBLIC\"" >/dev/null

log "ready: cluster $PG_MAJOR/$CLUSTER, data $(su postgres -c "psql -At -p $PG_PORT -c 'SHOW data_directory'"), socket /run/postgresql/.s.PGSQL.$PG_PORT"
log "the server's URL is in $URL_FILE (root only); HITOP_SYNC_DATABASE is unchanged until cutover-postgres.sh"
