# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub:
[Report a vulnerability](https://github.com/bohdaq/hitop/security/advisories/new).
Don't open a public issue.

Include what is affected (CLI, desktop, web app, the hitop.rws8.tech request proxy, the sync
server), steps to reproduce, and the impact you see. You'll get an answer within a week. Once a
fix is released the advisory is published, with credit if you want it.

## Supported versions

Fixes go into the latest release of each part: the CLI and hitop-core on crates.io, the desktop
app, the web app at hitop.rws8.tech/app/ and the sync server.

## Scope

In scope: the code in this repository and the services it runs at hitop.rws8.tech, including the
request proxy (`/api/proxy`) and its address blocking. Out of scope: denial of service by volume,
and findings that need a compromised device.
