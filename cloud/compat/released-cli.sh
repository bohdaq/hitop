#!/usr/bin/env bash
# Client compatibility (spec/CLOUD_SERVICE.md, C1 "Done when"): a released hitop CLI (the binary
# from GitHub Releases, not this tree) runs the whole sync flow unchanged against this tree's sync
# server (migration 4, organizations) with HITOP Cloud's Free plan limits on, as the test server
# runs it. Login, init, push, pull, invite, accept, add, a conflict, a viewer, a runner credential,
# remove with rotation, the audit log; then a create beyond Free is refused with the server's
# message, nobody is over a limit (`hitop-sync-server over-limits`) and the database holds no
# plaintext. The personal org is implied: the released client never names one.
#
#   cloud/compat/released-cli.sh <hitop binary> <hitop-sync-server binary>
#
# CI (job "compat") runs it for every released version from 1.1.0 on.
set -euo pipefail

hitop="$(realpath "$1")"
server="$(realpath "$2")"
work="$(mktemp -d)"
pid=""
cleanup() {
  [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  echo "--- server log (last 40 lines) ---" >&2
  tail -n 40 "$work/server.log" >&2 || true
  exit 1
}

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
url="http://127.0.0.1:$port"
export HITOP_SYNC_DATABASE="sqlite:$work/sync.db"
(
  cd "$work"
  RWS_CONFIG_IP=127.0.0.1 RWS_CONFIG_PORT="$port" HITOP_SYNC_PUBLIC_URL="$url" HITOP_SYNC_RP_ID=localhost \
    HITOP_SYNC_RP_ORIGINS="http://localhost:$port" HITOP_SYNC_MAIL=log HITOP_SYNC_LIMITS=plans \
    exec "$server" >/dev/null 2>"$work/server.log"
) &
pid=$!
for _ in $(seq 100); do
  curl -fs "$url/healthz" >/dev/null 2>&1 && break
  sleep 0.1
done
curl -fs "$url/healthz" >/dev/null || fail "the sync server didn't start"
echo "client: $("$hitop" --version)"

# One person on one computer: hitop with its own config directory and key file.
h() {
  local who="$1"
  shift
  mkdir -p "$work/$who"
  env -u HITOP_SYNC_RUNNER HOME="$work/$who" XDG_CONFIG_HOME="$work/$who/config" HITOP_KEYCHAIN=off \
    HITOP_SYNC_KEY_FILE="$work/$who/key.json" HITOP_SYNC_SERVER="$url" NO_COLOR=1 "$hitop" "$@"
}
# Runs, captures stdout in $out and stderr in $err, and checks the exit code.
expect() {
  local want="$1"
  shift
  local code=0
  out="$("$@" 2>"$work/stderr")" || code=$?
  err="$(cat "$work/stderr")"
  [ "$code" = "$want" ] || fail "$* exited $code, not $want
stdout: $out
stderr: $err"
}
# The last code mailed to $1 on the line containing $2 (the server prints mails with HITOP_SYNC_MAIL=log).
mailed() {
  for _ in $(seq 50); do
    local c
    c="$(awk -v to="mail to $1:" -v m="$2" 'index($0, to) { inmail = 1; code = "" } inmail && index($0, m) { n = split($0, p, /[# ]/); code = p[n] } END { print code }' "$work/server.log")"
    [ -n "$c" ] && { echo "$c"; return; }
    sleep 0.1
  done
  fail "no mail to $1"
}
login() {
  expect 0 h "$1" sync login --email "$2"
  local code
  code="$(mailed "$2" "/signin#")"
  expect 0 h "$1" sync login --email "$2" --code "$code"
  echo "$out" | sed -n 's/.*key fingerprint //p' | head -n 1
}
request() {
  mkdir -p "$(dirname "$1")"
  printf 'name: %s\nmethod: GET\nurl: %s\n' "$2" "$3" >"$1"
}
same() {
  diff -r --exclude='.*' "$1" "$2" >/dev/null || fail "$1 and $2 differ: $(diff -r --exclude='.*' "$1" "$2" | head -n 20)"
}
# The invite code: from the inviter's output when it prints one (1.2: with its fingerprint),
# else from the mail (1.1).
invite_code() {
  local c
  c="$(echo "$out" | sed -n 's/^ *hitop sync accept //p' | head -n 1)"
  [ -n "$c" ] && { echo "$c"; return; }
  mailed "$1" "hitop sync accept"
}

ada_fp="$(login ada ada@example.com)"
bob_fp="$(login bob bob@example.com)"
cy_fp="$(login cy cy@example.com)"
[ -n "$ada_fp" ] && [ -n "$bob_fp" ] && [ -n "$cy_fp" ] || fail "no key fingerprints"

A="$work/ada/shop"
expect 0 h ada init "$A" --name Shop
request "$A/requests/list-orders.yaml" "List orders" "https://internal.example.test/orders?unique=7f3a9c"
expect 0 h ada sync init -c "$A"
ws="$(echo "$out" | sed -n 's/.*workspace \([0-9a-f]\{32\}\).*/\1/p' | head -n 1)"
[ -n "$ws" ] || fail "no workspace id in: $out"
expect 0 h ada sync ls
echo "$out" | grep -q "$ws" || fail "sync ls doesn't list $ws"

# Invite Bob (editor), he accepts, Ada adds him after checking his fingerprint.
expect 0 h ada sync invite bob@example.com --role editor -c "$A"
code="$(invite_code bob@example.com)"
expect 0 h bob sync accept "$code"
expect 0 h ada sync add bob@example.com --trust "$bob_fp" -c "$A"
B="$work/bob/shop"
expect 0 h bob sync link "$ws" -c "$B"
same "$A" "$B"

# Edits both ways, then a conflict resolved by taking theirs.
request "$B/requests/list-orders.yaml" "List orders" "https://internal.example.test/v2/orders"
expect 0 h bob sync push -c "$B"
expect 0 h ada sync pull -c "$A"
same "$A" "$B"
request "$A/requests/create.yaml" "Create order" "https://internal.example.test/orders"
expect 0 h ada sync push -c "$A"
expect 0 h bob sync pull -c "$B"
same "$A" "$B"
request "$A/requests/list-orders.yaml" "List orders" "https://ada.example.test"
request "$B/requests/list-orders.yaml" "List orders" "https://bob.example.test"
expect 0 h ada sync push -c "$A"
expect 1 h bob sync push -c "$B"
expect 0 h bob sync resolve requests/list-orders.yaml --theirs -c "$B"
expect 0 h bob sync push -c "$B"
same "$A" "$B"

# A viewer pulls but can't push.
expect 0 h ada sync invite cy@example.com --role viewer -c "$A"
code="$(invite_code cy@example.com)"
expect 0 h cy sync accept "$code"
expect 0 h ada sync add cy@example.com --trust "$cy_fp" -c "$A"
C="$work/cy/shop"
expect 0 h cy sync link "$ws" -c "$C"
same "$A" "$C"
request "$C/requests/list-orders.yaml" "List orders" "https://cy.example.test"
expect 2 h cy sync push -c "$C"

# A runner credential pulls into a fresh folder.
expect 0 h ada sync runner create --name nightly -c "$A"
cred="$(echo "$out" | grep -o 'hitop-runner-1\.[A-Za-z0-9_-]*' | head -n 1)"
[ -n "$cred" ] || fail "no runner credential in: $out"
R="$work/ci/checkout"
mkdir -p "$R"
expect 0 env HITOP_SYNC_RUNNER_CRED="$cred" bash -c 'HOME="$0/ci" XDG_CONFIG_HOME="$0/ci/config" HITOP_KEYCHAIN=off HITOP_SYNC_SERVER="$1" HITOP_SYNC_RUNNER="$HITOP_SYNC_RUNNER_CRED" "$2" sync pull -c "$3"' "$work" "$url" "$hitop" "$R"
same "$A" "$R"

# Ada removes Bob: the key rotates; Bob is out, everyone else still syncs.
expect 0 h ada sync remove bob@example.com -c "$A"
echo "$out" | grep -q "epoch 2" || fail "no rotation: $out"
expect 2 h bob sync pull -c "$B"
request "$A/requests/after.yaml" "After" "https://after.example.test"
expect 0 h ada sync push -c "$A"
expect 0 h cy sync pull -c "$C"
[ -f "$C/requests/after.yaml" ] || fail "cy didn't get after.yaml"
expect 0 env HITOP_SYNC_RUNNER_CRED="$cred" bash -c 'HOME="$0/ci" XDG_CONFIG_HOME="$0/ci/config" HITOP_KEYCHAIN=off HITOP_SYNC_SERVER="$1" HITOP_SYNC_RUNNER="$HITOP_SYNC_RUNNER_CRED" "$2" sync pull -c "$3"' "$work" "$url" "$hitop" "$R"
[ -f "$R/requests/after.yaml" ] || fail "the runner didn't get after.yaml"
expect 0 h ada sync audit --verify --limit 1 -c "$A"
echo "$out" | grep -q "audit log intact" || fail "audit: $out"

# Free allows 3 workspaces in the personal org: two more fit, the next is refused with the
# server's message as the released client prints it (exit 2), and nothing is linked.
for n in 2 3 4; do
  F="$work/ada/shop$n"
  expect 0 h ada init "$F" --name "Shop $n"
  request "$F/requests/a.yaml" "A" "https://example.test/a"
  if [ "$n" = 4 ]; then
    expect 2 h ada sync init -c "$F"
    for part in "plan limit" "free plan" "3 shared workspaces" "402"; do
      echo "$err" | grep -q "$part" || fail "'$part' not in: $err"
    done
  else
    expect 0 h ada sync init -c "$F"
  fi
done
expect 0 h ada sync ls
[ "$(echo "$out" | grep -c '[0-9a-f]\{32\}')" = 3 ] || fail "3 workspaces expected: $out"
# Reads still work at the limit.
expect 0 h ada sync pull -c "$A"

# Nobody is over a limit, and the server holds no plaintext.
"$server" over-limits >"$work/over" 2>&1 || fail "over-limits: $(cat "$work/over")"
for needle in internal.example.test ada.example.test list-orders "List orders"; do
  if cat "$work"/sync.db* | grep -a -q -F "$needle"; then fail "$needle is in the server's database"; fi
done
echo "compat ok: $("$hitop" --version) against this tree's sync server (migration 4, Free limits on)"
