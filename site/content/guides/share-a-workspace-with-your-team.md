---
title: "Share a workspace with your team"
description: "Share a HITOP collection with your team through hitop sync: end-to-end encrypted, with invites, roles, conflict resolution and read-only runner credentials for scheduled runs. Secret values never sync unless you share them."
---

## What hitop sync is

hitop sync keeps a collection in step between people and devices through a hitop sync server. It works in the CLI (`hitop sync`), the desktop app and the web app (the Sync page), and they all share the same workspaces.

Everything is encrypted on your device before it leaves: each request file, environment and the collection file is sealed with the workspace's key (XChaCha20-Poly1305), and that key is wrapped for each member's devices with HPKE (RFC 9180). The server stores ciphertext, version numbers and wrapped keys. It can see who is in a workspace, how many files there are and when they change; it can't read them.

Secret variable values never sync unless you share them. Environment files carry only the names of secret variables (`token: {secret: true}`); each person keeps the values in their own keychain (or, in the web app, in their browser). An owner or editor can [share an environment's values](#shared-secret-values) end to end with the devices they pick; they still never go into the files.

The sync server is open source (AGPL), and you can run your own: see the [self-host guide](https://github.com/bohdaq/hitop/tree/main/sync-server). A test server runs at `https://sync.hitop.rws8.tech`. hitop sync is in testing: the encryption design hasn't been externally reviewed yet, so don't put production credentials or customer data in a shared workspace until it has.

## Sign in

Make an account with your email and a password; the server mails you a code to confirm the address:

```bash
hitop sync signup --email you@example.com
# choose a password (8 characters or more), then paste the code from the mail
hitop sync login --email you@example.com     # later, on any computer: asks for the password
```

In the app, open **Sync** in the sidebar, enter your email and a password, and choose **Create account** (or **Sign in**). Your device makes its own key pair; the secret half stays on the device (the OS keychain for the CLI and the desktop app). hitop shows the device's **fingerprint**: eight groups of four hex digits. Your team uses it to check that a key is really yours.

**Your password also unlocks your keys on a new computer.** The password never leaves your device: hitop turns it into two keys, sends the server one (which it keeps only as a slow hash) and seals your device's keys with the other. Sign in with the password on a new computer and it becomes your device again: same key, same fingerprint, your workspaces open with no new trust step for your team. Change it with `hitop sync password set` (or **Password** on the Sync page).

**Forgot it?** `hitop sync password forgot --email you@example.com`, then `hitop sync password reset --code <code>` (or **Forgot password?** in the app) sets a new one and signs you in, but **can't unlock your encrypted workspaces on a new computer**: only the old password could open those keys. Get them back with your [recovery phrase](#recovery-phrase) or from a computer that is still signed in (its next sign-in with the new password seals the keys again; then `hitop sync password unlock`). A computer that already was your device keeps its keys through a reset.

Other ways to sign in:

- **A code by mail**, no password: `hitop sync login --email you@example.com --code` (in the app: **Use a code instead**).
- **Continue with Google** or **Continue with GitHub**, when the server offers them: `hitop sync login --google` or `--github` (opens a browser; over SSH add `--no-browser` and paste the code). Your Google or GitHub address must be verified; it signs you in to the account with that address. On a new computer it doesn't unlock your keys: `hitop sync password unlock` (or **Unlock with password**) does.
- **A passkey** in the app, once you added one.

### Single sign-on

If your organization runs its own sync server with single sign-on (Okta, Entra ID, Google Workspace, Keycloak or another OpenID Connect provider), sign in with your work account instead:

```bash
hitop sync login --sso
# opens your organization's sign-in page; over SSH: hitop sync login --sso --no-browser
```

On a server where organizations bring their own provider (HITOP Cloud Business), add your work address so the server finds yours: `hitop sync login --sso --email you@company.example`.

In the app choose **Sign in with SSO**, sign in on the page that opens, and paste the code it shows. The server may require SSO for your company's domain, and then sign-in mails don't work for it. SSO only replaces how you sign in: your device still makes its own key, and your team still checks its fingerprint before sharing a workspace with it.

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

The invite code hitop shows you ends with your device's key fingerprint (`<code>.<fingerprint>`, also in the link). Send that code or link yourself, in a chat or a message you trust: with it, the other person's hitop accepts the workspace key only from your device. The code in a mailed invite has no fingerprint, because the sync server writes that mail; it works, but then their hitop asks them to compare your fingerprint before it uses the key.

The person signs in, then accepts the invite (`hitop sync accept <code>`, or **Accept invite** on the Sync page). Accepting doesn't give them access yet: only a member's device can hand over the workspace key, and the server could lie about whose key is whose.

So the owner checks the fingerprint:

```bash
hitop sync members -c ./my-api          # shows accepted invites with their fingerprints
hitop sync add ada@example.com --trust 'ab12 cd34 …' -c ./my-api
```

Compare the fingerprint with the person directly: in person, on a call, or in a chat you already trust. In the app, the accepted invite shows the fingerprint and a **Trust…** button; **Add** stays off until the key is trusted. Once you trust a key, hitop remembers it; if the server ever presents a different key for the same device, hitop refuses to share the workspace key with it.

The new member then opens the workspace: `hitop sync link <workspace> -c ./my-api`, or **Open here** on the Sync page.

## Fingerprints and warnings

Every device has a key, and its **fingerprint** (eight groups of four hex digits) is what people compare. Yours: `hitop sync whoami`, or the top of the Sync page. Everyone's: `hitop sync members`, or **Members**.

hitop checks both directions. It hands the workspace key only to devices you trusted, and it takes a workspace key only from a device you trusted (or the one your invite code named). So a sync server can't slip its own key to a new member, or to anyone at the next key change, and read what they write. When hitop refuses a key, it says why:

- **"handed to this device by device …, key fingerprint …, which you haven't trusted"**: the key came from a device you haven't confirmed (for example, you accepted a mailed invite, or another device of the owner added you). Compare the fingerprint with that person; only if it matches, run `hitop sync trust '<fingerprint>' -c ./my-api` or choose **Trust this device…**, then sync again.
- **"SECURITY: … doesn't verify against the key you trusted"**, **"… is not the one this device accepted before"**, **"… a key rotation is being hidden"**: someone, most likely the server, tried to give you a key it chose, swap a key, or roll the workspace back. hitop used nothing and wrote nothing. Don't trust anything new; tell the workspace owner and whoever runs the server.
- **"KEY CHANGED"** next to a device in the member list, or **"WARNING: the server now lists a different key"**: the server shows a new key under a device you already trusted. hitop keeps using the key you trusted and refuses the new one. A person who really has a new key signs in on a new device instead.
- **"handed out without authentication (made by hitop before 1.2)"**: a workspace shared with hitop 1.1. It keeps working; the owner's next sync with hitop 1.2 or newer replaces those keys with authenticated ones, and members then confirm the owner's fingerprint once.

hitop 1.1 can't read the authenticated keys 1.2 hands out, so once a workspace has a member on 1.2, everyone in it updates (CLI, desktop app and runner images to 1.2 or newer; the web app updates itself).

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

The workflow pulls the workspace with the credential, then runs it with the [HITOP GitHub Action](run-hitop-in-github-actions.html). The credential decrypts the workspace inside your CI job, so the sync server still sees only ciphertext. For cron on your own machine, `hitop schedule ./my-api --cron "0 6 * * *" --sync` prints a crontab line that pulls as your device, then runs. Revoke a credential like a member: `hitop sync members` lists it, `hitop sync remove <its id>` rotates the key. Generated workflows need an image with `hitop sync`, so use hitop 1.1.0 or later (`--hitop-version`). A credential made with hitop 1.2 also names the devices allowed to hand it the workspace key (yours and the owners' and editors' devices you trusted) and accepts keys only from them, which needs a hitop 1.2 image; one made with 1.1 keeps working and warns that it should be replaced.

**Running on the sync server instead** would mean giving the server a key that decrypts the workspace: while it runs your collection it sees your requests, responses and environment values, which is exactly what end-to-end encryption keeps from it. hitop's test server doesn't offer that. If you run your own server and want it to run your monitors, run the runner credential on that host yourself, as its own service with its own limits, and treat that host as one that can read the workspace.

## Run history and alerts

Your CI or cron runs the collection (HITOP never runs it on its machines: that would need a key the server is built never to hold). Each run can go to the workspace's history:

```bash
hitop run ./my-api --env staging --sync-report      # as a member, or with HITOP_SYNC_RUNNER
hitop sync report hitop-reports/report.json -c ./my-api   # a report a run already wrote
hitop sync runs -c ./my-api                         # the history; --id <run> opens one report here
```

`hitop schedule --sync` adds it for you (cron: `--sync-report --sync-secrets` on the run; GitHub: a step after the Action that uploads its report.json, with hitop after 1.2). The server gets a **status row** in plain text: passed or failed, the time, how long it took, how many requests ran and failed, the collection's and the environment's names, and the names of failed requests only when the owners allow it. The **full report** (report.json, secret values already ••••) is sealed with the workspace key on the runner: only members' devices open it (`hitop sync runs --id`, or **Run history** on the Sync page). The portal shows the status rows.

**Alerts** come from the status rows: when a collection and environment start failing, pass again, or (with **missed-run** minutes set) don't run. Owners set them with `hitop sync alerts` (or **Run history** → **Alert settings**): mail to members, a JSON webhook (https), a Slack incoming webhook; `--failed-names on` lets them name the failed requests, `--status-rows off` keeps history without status rows or alerts. An alert carries the status and a link, never a response, a body or a secret value. HITOP Cloud plans set how long history is kept and which channels there are (Free: status rows for 7 days and mail; sealed reports and webhooks from Pro; Slack from Team).

## Shared secret values

Off unless you share. An owner or editor shares an environment's secret values end to end:

```bash
hitop sync secrets share --env prod -c ./my-api       # names who gets them, then asks
hitop sync secrets share --env prod --runner <id>     # also a runner credential (id: hitop sync members)
hitop sync secrets ls -c ./my-api                     # names and who has them, never values
hitop run ./my-api --env prod --sync-secrets          # uses them where you have no value of your own
```

The values come from your keychain (or `HITOP_SECRET_*`) and are sealed for the trusted devices of the workspace's owners and editors (`--viewers` adds viewers, `--runner` or `--runners` runner credentials); the server can't read them, and a viewer you didn't pick sees the names only. Your own values (keychain, `--secret`, `HITOP_SECRET_*`) always win. Whoever adds a member or trusts a new device here hands them the values in the same step, and the sharer's next sync does it for any device still without a copy. Removing someone re-keys every share your device can open, but **can't take back the values they saw**: rotate those secrets where they come from (the CLI and the Sync page say so).

## Recovery phrase

A workspace only you are in lives only on your devices. Write down a recovery phrase in case you lose them all and forget your password:

```bash
hitop sync recovery create     # prints 24 words once: keep them offline
hitop sync recovery use        # on a new computer, after signing in
```

On the Sync page it is under **Recovery phrase**. Like the password, the phrase seals your device's keys for the server to keep; the server can't open them.

## Audit log

The server keeps a log of what happened in each workspace: members added and removed, role changes, invites, key rotations, runner credentials, and the workspace being created or deleted. Owners can read it:

```bash
hitop sync audit -c ./my-api            # newest first; --json, --event member_added, --limit 0 for all
hitop sync audit --verify -c ./my-api   # checks the log's hash chain on your computer
hitop sync audit --mine                 # your own sign-ins and failed sign-ins
```

In the app it is under **Members** → **Show audit log**. Each entry is chained to the one before it with a hash, so an edited or deleted entry shows up in `--verify`; your computer also remembers the last entry it checked, so a log rewritten from there on is caught next time. Entries hold who did what and when, the first three parts of the IP address (the last part is dropped) and the app's user agent, never file contents or keys. The test server keeps them for a year.

On HITOP Cloud Business the server also mails the organization's owners a signed **anchor** of the log's latest entries once a day, and owners and admins can download the whole organization's log, signed:

```bash
hitop sync audit --export --org "Acme"                 # every workspace of the org, signed, checked here
hitop sync audit --verify --anchor anchor.json         # a fresh export against an anchor you kept (the saved mail works too)
```

A log rewritten after an anchor was mailed no longer matches it.

## What stays where

| | Where |
| --- | --- |
| your password | nowhere but your head; the server keeps a slow hash of a key made from it, and your device's keys sealed under another key from it |
| request files, environments (without secret values), `hitop.yaml`, globals | encrypted on the server; plain on each member's device |
| secret values | each device's keychain (web app: the browser); never synced unless shared |
| shared secret values (opt-in) | sealed on the server for the devices the sharer picked; plain only on those devices |
| run history: status rows (pass/fail, timing, counts, collection and environment names) | visible to the server; kept for the plan's history |
| run history: full reports | sealed with the workspace key; open on members' devices |
| alert settings, webhook URLs | on the server, readable by owners |
| device secret key | the device (keychain); the web app keeps it in the browser |
| who is in which workspace, file counts, sizes, times | visible to the server |
| audit log: who did what, when, truncated IP, user agent (a year on the test server) | on the server, readable by workspace owners |

The full design and what the server can and can't do are in the [threat model](https://github.com/bohdaq/hitop/blob/main/sync-server/THREAT_MODEL.md).
