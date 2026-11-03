---
path: "/security.html"
title: "Security · HITOP Cloud"
description: "How HITOP Cloud protects your collections: end to end encryption, what the server sees, where it runs, and what has not been checked yet."
---
# Security

**No external security review of HITOP's encryption has been done yet. HITOP Cloud runs on a single server shared with other sites.** Both sentences stay on this page until they are no longer true. When a review is done, its summary is linked here.

## What the server can't read

hitop sync encrypts every file of a shared workspace on your device before it is sent (XChaCha20-Poly1305 under a workspace key). The workspace key is wrapped for each member's devices with HPKE, and your app accepts a key only from devices you have pinned. The server stores ciphertext, wrapped keys and public keys. It never holds a key that opens a workspace, and the server program contains no decryption code at all (a test fails the build if it ever links one).

Passwords never reach the server: your app derives two keys from the password (Argon2id), sends one, and the server keeps only a slow hash of it. The other key seals your device keys, which the server stores but can't open.

## What the server does see

- Your email address, your devices' names and public keys, and hashes of your session tokens.
- Who is in which workspace, how many files it has, their sizes and when they change.
- Organizations, members, plans and seats, and the Lemon Squeezy ids of a subscription (never card data: Lemon Squeezy takes the payment).
- A security log per workspace and account (sign-ins, members, invites, key rotations), with IP addresses cut to their first three parts. It is hash chained, so a changed or removed entry shows.

If someone took over the server, they could refuse service, show you old versions or hide changes, and see the metadata above. They could not read your collections, and your app refuses keys it has not pinned. The full list of what is and isn't protected is in the [threat model](https://github.com/bohdaq/hitop/blob/main/sync-server/THREAT_MODEL.md).

## Where it runs

One virtual machine in the EU, shared with other sites run by the same operator. Each HITOP service runs sandboxed by systemd with its own memory and CPU limits, the database (Postgres) listens on a local socket only, and backups are encrypted before they leave the machine. A problem with a neighbouring site can still take HITOP Cloud down, which is why the availability target is 99.5% at most. The [sub-processor list](/legal/subprocessors.html) names every company that handles your data.

## SOC 2

Groundwork in progress, no report yet. The written policies and access reviews are in the [repository](https://github.com/bohdaq/hitop/tree/main/cloud/policies).

## Reporting a problem

Mail [bohdaq@gmail.com](mailto:bohdaq@gmail.com) with "security" in the subject, or see [SECURITY.md](https://github.com/bohdaq/hitop/blob/main/SECURITY.md). Please don't open a public issue for a vulnerability. The code is open (AGPL for the server, MIT for the clients), so you can check every claim on this page.
