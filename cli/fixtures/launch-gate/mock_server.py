#!/usr/bin/env python3
"""The launch-gate scenario's API: one server per environment.

    mock_server.py <port> <environment> <password>

POST /login with the right password returns a token made up on each start;
GET /orders?limit=2 and GET /orders/<id> answer only with that token as a
Bearer token. Listens on 127.0.0.1 only. Used by .github/workflows/ci.yml
(job launch-gate); tests/launch_gate.rs runs the same scenario on httpmock.
"""
import json
import secrets
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

port, environment, password = int(sys.argv[1]), sys.argv[2], sys.argv[3]
token = f"tok-{environment}-{secrets.token_hex(8)}"
orders = [{"id": f"ord-{environment}-{n}", "total": 10 * n} for n in (1, 2, 3)]


class Handler(BaseHTTPRequestHandler):
    def send(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def authorized(self):
        return self.headers.get("Authorization") == f"Bearer {token}"

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        try:
            body = json.loads(self.rfile.read(length) or b"{}")
        except ValueError:
            return self.send(400, {"error": "not JSON"})
        if urlparse(self.path).path != "/login":
            return self.send(404, {"error": "no such route"})
        if body.get("username") != f"ci-{environment}" or body.get("password") != password:
            return self.send(401, {"error": "wrong username or password"})
        self.send(200, {"token": token})

    def do_GET(self):
        url = urlparse(self.path)
        if url.path == "/health":
            return self.send(200, {"ok": True})
        if not self.authorized():
            return self.send(401, {"error": "missing or wrong bearer token"})
        if url.path == "/orders":
            limit = int(parse_qs(url.query).get("limit", ["10"])[0])
            return self.send(200, {"environment": environment, "orders": orders[:limit]})
        for order in orders:
            if url.path == f"/orders/{order['id']}":
                return self.send(200, order)
        self.send(404, {"error": "no such route"})

    def log_message(self, *args):
        pass


ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
