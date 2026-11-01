---
title: "Share a workspace with your team"
description: "Share a HITOP collection with your team through hitop sync: end-to-end encrypted, with invites, roles, conflict resolution and read-only runner credentials for scheduled runs. Secret values never leave your device."
---

## What hitop sync is

hitop sync keeps a collection in step between people and devices through a hitop sync server. It works in the CLI (`hitop sync`), the desktop app and the web app (the Sync page), and they all share the same workspaces.

Everything is encrypted on your device before it leaves: each request file, environment and the collection file is sealed with the workspace's key (XChaCha20-Poly1305), and that key is wrapped for each member's devices with HPKE (RFC 9180). The server stores ciphertext, version numbers and wrapped keys. It can see who is in a workspace, how many files there are and when they change; it can't read them.

Secret variable values never sync. Environment files carry only the names of secret variables (`token: {secret: true}`); each person keeps the values in their own keychain (or, in the web app, in their browser).

The sync server is open source (AGPL), and you can run your own: see the [self-host guide](https://github.com/bohdaq/hitop/tree/main/sync-server). A test server runs at `https://sync.hitop.rws8.tech`. hitop sync is in testing: the encryption design hasn't been externally reviewed yet, so don't put production credentials or customer data in a shared workspace until it has.

## Sign in

Sign in by email: the server mails you a code.

```bash
hitop sync login --email you@example.com
# paste the code from the mail when asked
```

In the app, open **Sync** in the sidebar, enter the server and your email, choose **Send code**, then paste the code. Each device makes its own key pair; the secret half stays on the device (the OS keychain for the CLI and the desktop app). hitop shows the device's **fingerprint**: eight groups of four hex digits. Your team uses it to check that a key is really yours.

## Share a collection

From a collection folder:

```bash
hitop sync init -c ./my-api
```

In the app, pick the collection under **Share a collection** on the Sync page and choose **Share**. hitop makes a new workspace, encrypts every file and pushes it. You are its owner.

## Invite your team

```bash
hitop sync invite ada@example.com --role editor -c ./my-api   # mailed
hitop sync invite --role viewer -c ./my-api                    # a link to share yourself
```

In the app: **Members**, then the email and role, then **Invite**.

The person signs in, then accepts the invite with the code from the mail or link (`hitop sync accept <code>`, or **Accept invite** on the Sync page). Accepting doesn't give them access yet: only a member's device can hand over the workspace key, and the server could lie about whose key is whose.

So the owner checks the fingerprint:

```bash
hitop sync members -c ./my-api          # shows accepted invites with their fingerprints
hitop sync add ada@example.com --trust 'ab12 cd34 …' -c ./my-api
```

Compare the fingerprint with the person directly: in person, on a call, or in a chat you already trust. In the app, the accepted invite shows the fingerprint and a **Trust…** button; **Add** stays off until the key is trusted. Once you trust a key, hitop remembers it; if the server ever presents a different key for the same device, hitop refuses to share the workspace key with it.

The new member then opens the workspace: `hitop sync link <workspace> -c ./my-api`, or **Open here** on the Sync page.

## Roles

| Role | Can |
| --- | --- |
| owner | everything: invite, add and remove members, change roles, make runner credentials, delete the workspace |
| editor | read and change requests and environments; give the key to members' new devices |
| viewer | read |

The server enforces roles. Every member holds the workspace key, though, so roles protect against mistakes and honest clients, not against a member who works with a compromised server to write anyway.

## Sync and conflicts

```bash
hitop sync status -c ./my-api   # what would be pulled and pushed
hitop sync pull -c ./my-api     # get changes (yours stay unpushed)
hitop sync push -c ./my-api     # pull, then push your changes
```

In the app: **Sync now** next to the collection.

Changes made on one side are simply pulled on the other. When two people change different keys of `hitop.yaml`, an environment or the globals (two people adding requests, say), hitop merges them. When two people change the **same request**, it's a conflict: your version stays in your folder and isn't pushed until you choose.

```bash
hitop sync resolve requests/get-user.yaml --show -c ./my-api     # base, mine and theirs
hitop sync resolve requests/get-user.yaml --theirs -c ./my-api   # take theirs
hitop sync resolve requests/get-user.yaml --mine -c ./my-api     # keep mine, then push
```

In the app the conflict appears under the collection with **View both**, **Keep mine** and **Take theirs**.

## Remove someone

```bash
hitop sync remove ada@example.com -c ./my-api
```

Removing a member makes a new workspace key, wraps it for everyone who stays, and encrypts every file again under it. What removal guarantees: the removed person can't read anything written from then on, and their sessions can't reach the workspace. What it can't do: take back what they already had. Anything they synced before is on their device, along with the old key; change any secret they could have seen.

## Scheduled runs

To run a shared collection on a schedule, make a read-only **runner credential**: a separate key for one workspace, with the viewer role, that can't do anything else.

```bash
hitop sync runner create -c ./my-api | gh secret set HITOP_SYNC_RUNNER
hitop schedule ./my-api --github --sync --env staging
```

The workflow pulls the workspace with the credential, then runs it with the [HITOP GitHub Action](run-hitop-in-github-actions.html). The credential decrypts the workspace inside your CI job, so the sync server still sees only ciphertext. For cron on your own machine, `hitop schedule ./my-api --cron "0 6 * * *" --sync` prints a crontab line that pulls as your device, then runs. Revoke a credential like a member: `hitop sync members` lists it, `hitop sync remove <its id>` rotates the key. Generated workflows need an image with `hitop sync`, so use a hitop release after 1.0.0 (`--hitop-version`).

**Running on the sync server instead** would mean giving the server a key that decrypts the workspace: while it runs your collection it sees your requests, responses and environment values, which is exactly what end-to-end encryption keeps from it. hitop's test server doesn't offer that. If you run your own server and want it to run your monitors, run the runner credential on that host yourself, as its own service with its own limits, and treat that host as one that can read the workspace.

## What stays where

| | Where |
| --- | --- |
| request files, environments (without secret values), `hitop.yaml`, globals | encrypted on the server; plain on each member's device |
| secret values | each device's keychain (web app: the browser); never synced |
| device secret key | the device (keychain); the web app keeps it in the browser |
| who is in which workspace, file counts, sizes, times | visible to the server |

The full design and what the server can and can't do are in the [threat model](https://github.com/bohdaq/hitop/blob/main/sync-server/THREAT_MODEL.md).
