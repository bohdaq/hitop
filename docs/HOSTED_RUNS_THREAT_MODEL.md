# Hosted runs of shared collections: design and threat model

Status: **a proposal for the owner to decide on. Nothing in it is built.** Plan item C8 (owner, 2026-11-24: "hosted runs (changes
the security model for shared collections: THREAT_MODEL first)"). No code that gives the server a workspace key, and no schema or
endpoint for it, is written before the owner has read this and answered the decisions at the end. Answers from a real message of
the owner count; nothing else does.

## The decision in one paragraph

Today a shared collection is end-to-end encrypted: the sync server stores ciphertext and can't read requests, responses or
environment values, and a compromise of its host "can deny service, not read collections" (sync-server/THREAT_MODEL.md, "Shared
host"). Hosted runs mean HITOP Cloud runs a shared collection on its own machines on a schedule. To do that, the service must hold a
key that decrypts the workspace. For every workspace that turns hosted runs on, that sentence stops being true: whoever controls
the server, or breaks into its host, can read that workspace's files (and any environment values shared to the hosted runner), now
and for as long as hosted runs stay on. The conditions the owner set in decision Q8 (spec/CLOUD_SERVICE.md, "Hosted monitors") for
revisiting this are **not met**: there is no confidential-computing runner with remote attestation, and the server runs on a VM
shared with other sites. This design can limit who gets the key, make it visible and make it revocable; it can't make the server
unable to read an opted-in workspace.

## What exists today (not changed by this)

- **Library collections** (personal library, C8): the server can read them by design (owner decision), and it can already run them
  on a schedule: monitors (sync-server/src/monitors.rs, off unless `HITOP_SYNC_MONITOR_BIN` is set): a child process per run under
  `ulimit -v`, killed after 150 s, private and listed addresses refused (`monitor_guard.rs`), script loop and depth limits, at most
  `MAX_REQUESTS` requests, secret values masked in the mailed summary. Nothing here changes them.
- **Shared collections** (end to end, C4): runs come from the customer's own runners (CI or cron with a runner credential, which
  decrypts the workspace and belongs in a CI secret store), which upload a plaintext status row and a sealed report (runs.rs).

## Proposed design (if the owner says yes)

1. **Opt-in per workspace, by an owner, on a client that knows about it.** Only an owner of the workspace can turn it on, after a
   statement that names what the server will be able to read. An org can forbid it (org setting, off by default = not allowed
   until an org admin allows it; personal workspaces: the owner decides). Paid plans only, if plans are on.
2. **A hosted runner is its own member kind, not a runner credential.** One per workspace: an account `hosted-<ws>@runner.invalid`,
   role viewer, flag `hosted: true` in every member list. Its X25519 device key is made on the server; the secret key is stored
   sealed under the host key (`hostkey.rs`, AES-256-GCM, as the server's other own secrets) and never leaves the server. The
   owner's client shows its fingerprint, pins it and wraps the current workspace key to it (format 2 wrap, made by the owner's
   device): **the server can't enrol itself**. New clients never accept a workspace key wrap whose sender is a hosted runner
   (a new rule in hitop-sync-crypto `accept_wraps`: today a pinned sender's wraps are accepted, and pinning the hosted runner to
   wrap to it would make it one).
3. **Rotation stops hosted runs until an owner re-wraps.** After any key rotation (a member removed, a device removed), the hosted
   runner has no key for the new epoch; runs fail closed ("an owner must allow hosted runs again") until an owner's client wraps the
   new key to it, showing the statement again.
4. **Secret values only when named.** Shared environment values (C4) reach the hosted runner only when a share names it
   explicitly. New clients never count it as one of "all runners" (`allRunners`); see the residual risk on released clients.
5. **Decryption in its own binary.** The run happens in a child process of a separate binary (`hitop-hosted-run`, built from the
   CLI's code), not in `hitop-sync-server`, so the server binary keeps linking no client crypto
   (`tests/no_crypto_in_server.rs` stays as it is). The server opens the runner's sealed secret with the host key and passes it to
   the child on stdin with the blobs; the child decrypts, runs under the monitor rules (address guard, `ulimit -v`, 150 s, request
   cap, script limits), seals the report to the workspace key like a customer runner (runs.rs, `seal_run`), prints a status row and
   exits; it keeps nothing on disk. This is defence in depth, not a boundary: both processes run on the same host as the same
   service user.
6. **Visible everywhere.** New clients show "HITOP Cloud can read this workspace" on the collection, in the member list and in the
   Share dialog while hosted runs are on. The audit log records turning it on and off, every re-wrap and every run.
7. **Turning it off.** An owner removes the hosted runner: the server deletes its sealed secret key at once and the client rotates
   the workspace key (as for any removed member), so later changes are unreadable to it. What it could read before stays readable
   in old ciphertext to anyone holding the old epoch key; the child kept no plaintext.
8. **Self-hosted servers:** off unless the operator sets `HITOP_SYNC_HOSTED_RUNS=1` and a host key.

## Threats and answers

| # | Threat | Answer | Left over |
| --- | --- | --- | --- |
| H1 | Host compromise (the VM is shared with other sites) | None that keeps the data secret: an attacker with the host key and the database reads every opted-in workspace. Limited to opted-in workspaces; visible to members | **Accepted only if the owner says yes (decision 1)** |
| H2 | Operator or a legal demand to read a workspace | Same as H1 for opted-in workspaces; the audit log and public docs say the service can read them | Accepted with H1 |
| H3 | Backups | Backups hold the database only, encrypted to the owner's offline age key (deploy/backup.sh); the host key is never in a backup, so a backup alone can't open a runner key. Keep it that way: a test that the backup set has no host key | A backup plus a stolen host key reads opted-in workspaces |
| H4 | The server enrols itself in a workspace that never opted in, or hands members a key of its choosing | Only an owner's pinned device can wrap a workspace key to the hosted runner; new clients refuse any wrap whose sender is a hosted runner (new `accept_wraps` rule) | Released clients that pinned it (below) |
| H5 | Secret values reach the runner without anyone choosing (`allRunners`) | New clients exclude hosted runners from `allRunners`; a share must name it | Released clients: see below |
| H6 | A collection attacks the host (SSRF, internal addresses, metadata endpoint) | The monitor rules: `monitor_guard` on every address incl. redirects and IP literals, `HITOP_SYNC_DENY_IPS` | As for library monitors |
| H7 | A collection exhausts the host (loops, memory, time, requests) | `ulimit -v`, 150 s kill, request cap, boa loop and depth limits, one run at a time per workspace, runs per day per plan | As for library monitors |
| H8 | One customer's run sees another's data | One child process per run, nothing written to disk, nothing shared between runs | A kernel or child escape (H1) |
| H9 | Plaintext in logs or mails | The child prints a masked status row only; mails carry names and statuses as for monitors; the report is sealed to the workspace key | None new |
| H10 | A removed owner keeps the workspace readable through the hosted runner | Removing a member rotates; rotation stops hosted runs until a remaining owner re-wraps (design 3) | None new |
| H11 | The server lies about whether hosted runs are on | The hosted runner is a member with a wrap only an owner's device made; clients show it from the member list and their own wraps | A server hiding a member from a client that has never seen it |

## Residual risks

- **Released clients** (CLI 1.2, iOS 2.0, Android before this change, older web app tabs) show the hosted runner as an ordinary
  runner named `hosted-…@runner.invalid`, without the "can read" statement. A device of theirs that pins it treats it as a runner,
  so a share made with `allRunners` on that device would be wrapped to it at that device's next sync. Pinning needs an explicit
  trust step on that device; the server can't pin for it. Such a device would also accept workspace key wraps made by the hosted
  runner (a pinned sender), so the server, which holds that key, could hand it a key of its own for a new epoch: it learns nothing
  it couldn't already read, but it could split that device from the others. New clients refuse it (design 2).
- **Everything in H1 and H2.** This is the cost of the feature, not a bug in it.
- **The same VM.** Running the child on a separate host would narrow H1 to that host only if the key never touched the sync
  server; in this design the server opens the sealed key, so a separate host helps against H7/H8, not H1.

## Public statements that become false (none changed until the owner decides)

- `site/content/legal/security.md`, "What the server can't read".
- `site/content/guides/share-a-workspace-with-your-team.md` line 155: "HITOP never runs it on its machines".
- `site/src/pages/pricing.js` FAQ "Do you run monitors for me?" ("No…"). Already inexact for library collections since monitors
  exist on the server (switched off on HITOP Cloud): to fix either way.
- `site/content/guides/hitop-vs-postman.md`: "doesn't replace … monitors yet" (would become true for monitors).
- `spec/CLOUD_SERVICE.md`: decision Q8, "No hosted monitors", "Hosted monitors (deferred until confidential computing)".
- `sync-server/THREAT_MODEL.md`: "Server-side runs are not offered", the C4 section's first sentence ("HITOP never runs a
  collection on its machines"), and "Shared host" ("can deny service, not read collections").
- `site/content/legal/security-questionnaire.md`; the DPA (`dpa.md`) and the subprocessor list (`subprocessors.md`) to be checked
  for what they say the service processes.

## Decisions for the owner

1. **Go or no-go**, given that there is no confidential computing and the host is shared: hosted runs for shared collections
   (this design), or keep Q8 (no hosted runs of shared collections; library monitors only, already built).
2. **If go, who may turn it on:** workspace owners, with org admins able to forbid it, default **forbidden** in orgs (proposed);
   or allowed by default.
3. **Secret values:** allowed only when a share names the hosted runner (proposed), or never (hosted runs only for collections
   whose requests need no shared secret).
4. **Where it runs:** on the sync server's VM (proposed for now), or a separate host (more operations work; narrows H7/H8 only).
5. **Plans:** which plans get it, and whether library monitors (built, off) are switched on for HITOP Cloud at the same time.
6. **Public text:** the statements above are rewritten in the same change that turns it on, not before (proposed).

## Tests the code must have (if built)

- An owner turns it on; a viewer, an editor, an org that forbids it, and a plan without it are refused.
- The server can't wrap a workspace key to its runner itself; a client refuses a wrap made by the hosted runner.
- After a rotation a run fails closed until an owner re-wraps; removing the runner deletes its sealed secret.
- A share with `allRunners` from a new client isn't wrapped to the hosted runner; one that names it is.
- `tests/no_crypto_in_server.rs` stays green; the run child links the client crypto and the server binary doesn't.
- The child: private and listed addresses (also via redirects and IP literals) refused, killed at the time limit, request cap,
  nothing written to disk, the report sealed and the status row masked.
- The backup set has no host key.
