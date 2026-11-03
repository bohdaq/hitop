"""Puts a person with a device key, a workspace (wrap, members, audit chain), an org and a recovery
blob into a running sync server through its API, as a client would (the content is random bytes:
the server can't tell). For the ops tests (cloud/ops/test/ops-e2e.sh, vm-sim.sh).

    python3 populate.py <server url> <server log file> <email>
"""
import base64, json, os, re, sys, time, urllib.request

url, log, email = sys.argv[1].rstrip("/"), sys.argv[2], sys.argv[3]
b64 = lambda b: base64.urlsafe_b64encode(b).decode().rstrip("=")


def call(method, path, body=None, session=None):
    req = urllib.request.Request(url + path, method=method, data=json.dumps(body).encode() if body is not None else None)
    req.add_header("Content-Type", "application/json")
    if session:
        req.add_header("Authorization", "Bearer " + session)
    with urllib.request.urlopen(req, timeout=20) as r:
        text = r.read()
        return json.loads(text) if text else None


call("POST", "/v1/auth/email", {"email": email})
code = None
for _ in range(100):
    found = re.findall(r"mail to " + re.escape(email) + r":.*?/signin#([A-Za-z0-9_-]+)", open(log, errors="replace").read(), re.S)
    if found:
        code = found[-1]
        break
    time.sleep(0.1)
assert code, "no sign-in mail in the log"
session = call("POST", "/v1/auth/email/verify", {"token": code})["session"]
key = call("POST", "/v1/me/keys", {"name": "ops test", "publicKey": b64(os.urandom(32))}, session)["id"]
ws = os.urandom(16).hex()
call("POST", "/v1/workspaces", {"id": ws, "wraps": [{"keyId": key, "wrapped": b64(os.urandom(80))}]}, session)
call("POST", "/v1/orgs", {"name": "Ops " + email.split("@")[0]}, session)
call("PUT", "/v1/me/recovery", {"blob": b64(b"\x01" + os.urandom(64))}, session)
print(json.dumps({"session": session, "workspace": ws}))
