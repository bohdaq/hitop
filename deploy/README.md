# Deploying hitop.rws8.tech

hitop.rws8.tech moves from GitHub Pages to the `bohdan-prod-donotdelete` VM
(168.119.17.57), behind the rws that already serves rws8.tech there.

```
browser ──https──▶ front rws :443 (TLS, ACME, rate limit /api/*)
                        └──▶ hitop-server 127.0.0.1:7880
                               ├─ static files: /var/www/hitop (site + /app/)
                               └─ POST /api/proxy ──▶ the API the user asked for
```

## What the proxy allows

`POST /api/proxy` (server/src/proxy.rs) sends one request for the web app.

- Only `http`/`https`, no credentials in the URL, no `CONNECT`/`TRACE`.
- Refuses loopback, private, link local (cloud metadata), CGNAT, multicast
  and reserved addresses, IPv4 mapped and NAT64/6to4 forms included. The check
  runs on the **resolved** addresses inside reqwest's resolver, for every
  connection and every redirect hop, so DNS rebinding doesn't get past it.
- Refuses the Fetch "bad ports" (25, 465, 587, 22, ...), so no mail relaying.
- `HITOP_DENY_IPS` adds the host's own public IP, so the proxy can't call back in;
  an `X-Hitop-Proxy` header marks its own requests and loops get 508.
- Only `Origin: https://hitop.rws8.tech` (and the staging name) is accepted.
- 10 MB request and 5 MB response bodies, 10 s connect and 20 s total timeout,
  16 requests in flight (`MemoryMax=1G`), 60 requests per minute per IP at the
  front rws. Compressed responses are decoded before they are returned.
- Nothing is logged (`RWS_CONFIG_LOG_FORMAT=off`), and directories don't list
  (`RWS_CONFIG_DIRECTORY_LISTING=false`), as on the front rws.

## Build (on the workstation)

```bash
./deploy/build.sh          # → deploy/out/www, deploy/out/hitop-server
```

Ubuntu 24.04 on the VM has the same glibc (2.39) as the build machine.

## Rollout

Additive only: nothing on the VM is deleted or stopped, and the mail server,
PHP demo and rws8.tech keep running. SSH goes through the stage cluster
tunnel (see the rws8.tech host notes).

1. Copy `deploy/` (with `out/`) to the VM and run `./install.sh`. It installs
   `/var/www/hitop`, `/usr/local/bin/hitop-server` and `hitop-server.service`,
   keeping `.bak-<timestamp>` copies of anything it replaces.
2. Front rws: back up `/etc/rws/rws.config.toml`, paste `rws-routes.toml` above
   the `old-docs-*` routes, and add `"hitop.168-119-17-57.sslip.io"` to
   `[acme] domains`. Then `rws --check=/etc/rws/rws.config.toml`.
3. A new ACME name needs the current cert moved aside (rws doesn't check the
   SAN list): `mv /var/lib/rws/cert.pem{,.bak-<ts>}` (same for key.pem), then
   `systemctl restart rws`. Mail TLS reads the same files; `rws-mail-cert.path`
   reloads postfix and dovecot when the new cert lands. Check rws8.tech,
   php demo and `openssl s_client -connect mail.rws8.tech:465` afterwards.
   **If the certificate order fails**, rws (and mail TLS) is left without a
   certificate: move the `.bak` files back, remove the new name, restart at once.
4. Test on https://hitop.168-119-17-57.sslip.io/: the pages, `/app/` (a blanket
   403 from the proxy means the front rws isn't forwarding `Origin`), a request
   to a public API without CORS headers (example.com) and a refused one
   (`http://127.0.0.1:9901/`).
5. DNS (uadns, by hand): `hitop.rws8.tech` CNAME `bohdaq.github.io` → A `168.119.17.57`.
6. Once it resolves, add `"hitop.rws8.tech"` to `[acme] domains`, repeat step 3,
   and test https://hitop.rws8.tech/.
7. Then retire GitHub Pages: delete `docs/CNAME` and `deploy-to-pages.sh`, and
   turn Pages off in the repo settings. Until then Pages keeps serving the old site.

Rollback: remove the hitop routes and names from the rws config, restore the
backup, restart rws; point DNS back to `bohdaq.github.io`.

## Updating

`./deploy/build.sh`, copy `out/` over, `./install.sh`. The front rws needs no change.
