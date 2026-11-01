"""Local mock API for the HitopKit tests (no public internet).

python3 -I mock_server.py  -> prints the port it listens on (127.0.0.1), then serves until killed.

Routes:
  GET  /users/1               {"name": "Ada", ...}
  POST /orders                201 {"id": 42, "status": "new"} (needs a JSON body with "product")
  GET  /orders/42             {"status": "paid"} with an X-API-Key header, else 401
  ANY  /echo...               the request as JSON: method, path, query, headers (lower case), body
  GET  /status/<n>            that status
  GET  /set-cookie            sets session=abc
  GET  /needs-cookie          200 with the session cookie, else 401
  GET  /__requests            every request so far (JSON list), /__reset clears it
"""
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit

RECORDED = []
LOCK = threading.Lock()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def reply(self, status, body, headers=None):
        data = body if isinstance(body, bytes) else json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def handle_any(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length).decode("utf-8", "replace") if length else ""
        parts = urlsplit(self.path)
        headers = {k.lower(): v for k, v in self.headers.items()}
        req = {"method": self.command, "path": parts.path, "query": parts.query, "headers": headers, "body": body}
        if parts.path == "/__requests":
            with LOCK:
                return self.reply(200, list(RECORDED))
        if parts.path == "/__reset":
            with LOCK:
                RECORDED.clear()
            return self.reply(200, {})
        with LOCK:
            RECORDED.append(req)
        p = parts.path
        if p == "/users/1" and self.command == "GET":
            return self.reply(200, {"name": "Ada", "email": "ada@example.com"})
        if p == "/orders" and self.command == "POST":
            try:
                ok = "product" in json.loads(body)
            except ValueError:
                ok = False
            return self.reply(201 if ok else 400, {"id": 42, "status": "new"} if ok else {"error": "bad body"})
        if p == "/orders/42" and self.command == "GET":
            if headers.get("x-api-key"):
                return self.reply(200, {"status": "paid"})
            return self.reply(401, {"error": "no key"})
        if p.startswith("/echo"):
            return self.reply(200, req)
        if p.startswith("/status/"):
            return self.reply(int(p.rsplit("/", 1)[1]), {"status": p})
        if p == "/set-cookie":
            return self.reply(200, {"ok": True}, {"Set-Cookie": "session=abc; Path=/"})
        if p == "/needs-cookie":
            ok = "session=abc" in headers.get("cookie", "")
            return self.reply(200 if ok else 401, {"cookie": headers.get("cookie", "")})
        return self.reply(404, {"error": "not found", "path": p})

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = handle_any


def main():
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    print(server.server_address[1], flush=True)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    # The tests hold stdin open; when they end (or crash) it closes and so do we.
    sys.stdin.read()
    return 0


if __name__ == "__main__":
    sys.exit(main())
