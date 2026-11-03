# hitop sync: threat model and encryption design

Status: design and implementation for testing (server T23, clients and teams T24, SSO and audit
log T25, authenticated key wraps 1.2, recovery C2, passwords and Google/GitHub sign-in C1b). **Not externally reviewed.** An external review of this design, of `cli/crates/hitop-sync-crypto` and of
the clients (`hitop_core::sync`, `hitop sync` in the CLI, `webapp/src/sync.js`) is required before
any public launch.

## What is protected

hitop sync keeps a team's collection folders (request files, environments, scripts, saved
responses) in step across devices. Those files can hold URLs of internal systems, tokens in
headers, and test data. Secret variable values never sync (they stay in each device's keychain,
spec section 4), but everything else in a workspace is treated as confidential.

Goal: **the server operator, anyone who copies the server's database or backups, and anyone on
the network learn nothing about workspace contents beyond sizes and timing**, and can't change
contents without clients noticing.

## Parties

| Party | Trusted for | Not trusted for |
| --- | --- | --- |
| A member's device | the workspaces it belongs to, while it is a member | anything after removal (see "Removal") |
| Sync server (and its operator, host, backups) | availability; storing what it's given; enforcing roles and versions as a courtesy | confidentiality and integrity of contents |
| Mail server, network | delivering sign-in links | anything else (TLS everywhere) |
| Identity provider (SSO, when configured) | saying who a person is (verified address, groups) | anything else: it never sees a key or a file |
| Google, GitHub (when configured, C1b) | saying a person controls a verified address | anything else: they never see a key or a file; signing in with them never unlocks keys |

## What the server stores

- Accounts: email address, passkey public credentials (`webauthn-rs`), SHA-256 hashes of sign-in
  link tokens and session tokens (never the tokens).
- Device public keys (X25519), per account. Secret keys never leave the device.
- Workspaces: random 128-bit id, current key epoch, change sequence number, total size.
- Members and roles (owner, editor, viewer).
- With SSO (T25): each linked identity (issuer, subject, address, first and last use).
- The audit log (T25): security events with the account, target, time, truncated client IP and
  user agent; see "Audit log".
- Wrapped workspace keys: for each epoch and each member device, the epoch's key sealed with HPKE
  to that device's public key, and (format 2, since 1.2) the key id of the device that made the
  wrap and the epoch it was at. Opaque to the server, which checks only their shape (and that a
  format 2 wrap names one of the caller's own device keys).
- Blobs: client-chosen id, version, version vector (device id → counter), epoch, tombstone flag,
  and the encrypted envelope.
- Organizations (HITOP Cloud C1, migration 4): an org's name (typed in the portal), its members
  and their org roles, pending org invites (addresses), which workspace ids it owns, its plan,
  seats and limits (entitlements), and daily counters of invites and blob writes. A personal org
  per account is named "Personal". Org roles never give access to a workspace: that still needs
  the workspace key wrapped by a member's client. Nothing about workspace contents is added.
- Personal access tokens: name, scopes, expiry, last use, and the SHA-256 of the token.
- Passwords (C1b, migration 6): the client's KDF parameters (Argon2id costs and a random salt) and
  the server's Argon2id hash of the client's **auth key** (never the password, never the bundle
  key); pending sign-ups and resets as SHA-256 of the mailed code, the address, and (sign-up) the
  same hash; a random server secret for made-up parameters. Google and GitHub identities are
  linked like SSO ones (issuer `https://accounts.google.com` or `https://github.com`, subject).
- Key bundles (C2, `recovery_blobs`): per account and unlock method (`phrase`, `password`),
  sealed on a client; see "Recovery" and "Passwords".
- Run history (C4, migration 8, `runs`): per uploaded run a **plaintext status row** (passed or
  failed, start and upload time, duration, request and failure counts, the collection's and the
  environment's names as the uploader sent them, failed request names only when the workspace's
  owners allow it, which account or runner credential uploaded it) and the full report **sealed**
  under the workspace key; see "Run history and alerts".
- Alerts (C4, `alert_settings`, `alert_state`, `alert_deliveries`): per workspace the owners'
  settings (members' addresses, webhook and Slack URLs, whether failed names are allowed, the
  missed-run interval), each series' last status, and a 30-day log of alerts sent (event, channel
  kind and position, outcome; never the URL).
- SSO per organization (C5, migration 9, `org_sso`, `org_domains`): each org's provider settings
  (issuer, client id, scopes, button name, groups claim and required groups, "required") with
  the **client secret sealed under the host key**, and the domains it claimed (the TXT token,
  when verified). Audit (C5, `org_audit_settings`, `audit_anchors`): each org's anchor settings
  and every daily anchor it made (chain ids and heads, signed). Admin (C5): when an org admin
  asked a workspace to rotate its key, and who.
- SCIM (C6, migration 10, `scim_*`): per org the SCIM tokens' hashes, the directory's users
  (address, externalId, display and name parts, active) and groups (name, members, the org and
  workspace roles an admin gave them), and the workspace grants SCIM made; see "SCIM
  provisioning".
- Shared environment values (C4, `env_shares`, `env_share_wraps`): per shared environment a random
  share id, a version, the sealed content and one wrap of the share's key per recipient device
  (with the device's key id and its account); see "Shared environment values".

The server binary does not link any decryption code: the client crate and the AEAD/HPKE crates
are dev-dependencies only, and `tests/no_crypto_in_server.rs` fails the build's tests if that
changes. `tests/plaintext.rs` runs a full two-member session with a key rotation and checks that
no plaintext, workspace key or device secret appears in the database files or in any answer. Since C5 the server links `ring` (already in the tree through the mail client's TLS) for
**its own** secrets only: AES-256-GCM to seal per-org OIDC client secrets under the host key, and
Ed25519 to sign audit exports and anchors. Neither ever touches a blob, a wrap or a report; the
banned client crates stay out (`tests/no_crypto_in_server.rs`).

## Construction

No new primitives. Two standard constructions from audited or reviewed RustCrypto crates:

- `chacha20poly1305` 0.11 (XChaCha20-Poly1305). The `chacha20poly1305` crate was audited by NCC
  Group in 2020 (no significant findings).
- `hpke` 0.15 (RFC 9180), base mode for format 1 wraps and Auth mode for format 2. No paid audit;
  Cloudflare reviewed version 0.8 for their use. Its KEM is built on
  `x25519-dalek`/`curve25519-dalek` (audited by Quarkslab, 2019). Its Auth mode implementation is
  checked against the RFC 9180 test vectors in the crate, and our format 2 vectors were checked
  independently with `@hpke/core` 1.7.4.

### Blobs

```
key       = the workspace key of epoch e (32 random bytes from the OS RNG)
nonce     = 24 random bytes per encryption
aad       = "hitop-sync/v1/blob" || u16 len(workspace id) || workspace id
            || u16 len(blob id) || blob id || u64 version || u32 epoch       (big endian, UTF-8)
envelope  = 0x01 || u32 epoch || nonce || XChaCha20-Poly1305(key, nonce, aad, plaintext)
```

The associated data binds a ciphertext to its workspace, its blob id, the version it was written
as, and the key epoch. So the server can't move a blob to another id or workspace, can't serve an
old envelope as a newer version (the client asks for and checks version *n*), and can't relabel
the epoch. 24-byte random nonces make collisions negligible for any realistic number of writes
per key.

### Key wrapping (format 2, authenticated, since 1.2)

```
suite     = HPKE Auth mode (0x02), DHKEM(X25519, HKDF-SHA256) 0x0020, HKDF-SHA256 0x0001,
            ChaCha20-Poly1305 0x0003; skS = the X25519 key of the device that makes the wrap
head      = the newest epoch the wrapping device knows (the workspace's current epoch), >= epoch
info      = "hitop-sync/v2/wrap" || u16 len(workspace id) || workspace id || u32 epoch || u32 head
            || u16 len(sender key id) || sender key id || u16 len(recipient key id) || recipient key id
aad       = empty
wrapped   = 0x02 || u32 head || u16 len(sender key id) || sender key id || enc (32)
            || Seal(workspace key) (32 + 16 tag)
```

A format 2 wrap opens only with the recipient's secret key *and* the public key of the device
that made it, so a recipient that checks it against the sender's **pinned** key knows a trusted
device made it. `info` binds it to its workspace, epoch, head and both key ids, so the server
can't replay it for another workspace, epoch or device, or relabel who made it or how far the
epochs went. Byte layouts and vectors: `vectors/v2.json`.

**Why HPKE Auth mode and not signatures.** A separate signing key (Ed25519) per device would need
binding to the X25519 key people already compare and pin: nothing ties a new signing key to an
existing encryption key, so during the upgrade the server could pair its own signing key with a
member's encryption key, and every pin and fingerprint would need a second round of trust on first
use. Auth mode authenticates the wrap with the X25519 key that is *already* pinned and
fingerprinted: no second key type, no key-binding step, fingerprints and pins unchanged, the same
80-byte HPKE part, no new crate (the `hpke` crate implements Auth mode). The costs, accepted:

- Only the recipient can verify a wrap (it isn't a transferable signature). That is all a wrap
  needs: only its recipient uses it. There is no signed record of a rotation or of the member list
  that a third member could check (see "What a malicious server can still do").
- Key-compromise impersonation: someone holding a recipient's secret key can make wraps that look,
  *to that recipient*, as if any device made them. They can already read everything that device
  can, and compromised devices are out of scope.

### Format 1 wraps (hitop 1.1, legacy)

```
suite     = HPKE base mode, DHKEM(X25519, HKDF-SHA256) 0x0020, HKDF-SHA256 0x0001,
            ChaCha20-Poly1305 0x0003
info      = "hitop-sync/v1/wrap" || u16 len(workspace id) || workspace id || u32 epoch
aad       = empty
wrapped   = enc (32) || Seal(workspace key) (32 + 16 tag)          (exactly 80 bytes)
```

Base mode doesn't say who made a wrap: anyone with the recipient's public key (the server) can make
one. hitop 1.2 never makes them, and accepts them only as described under "Accepting keys".

### Accepting keys (1.2)

Every client runs one policy, `hitop_sync_crypto::accept_wraps` (the CLI natively, the web and
desktop apps through hitop-wasm), with the device's pins and what it remembers about the workspace
(`WorkspaceMemory`: CLI `keys.json`, web app `hitop_sync.memory`):

- **Sender pinned**: a format 2 wrap is used only when it opens under the pinned key of the device
  it names. A wrap that names a pinned device but doesn't open under its pinned key is refused as
  forged; one from a device that isn't pinned is refused with that device's fingerprint (when the
  server lists it), so the person can compare it and trust it (`hitop sync trust`, or the Sync
  page's "Trust this device…"). A wrap for an older epoch from an untrusted device is skipped.
- **Invites carry the inviter's fingerprint**: the invite code is `<token>.<fingerprint of the
  inviting device>` (link `/invite#<token>.<fingerprint>`), shown by the inviter's client. Accepting
  it records the fingerprint, and the first wrap from a device the server lists *with that
  fingerprint* pins it. The server ignores the part after the `.`. A **mailed** invite can't carry
  a trustworthy fingerprint (the server writes the mail), so the inviter's client also shows the
  full code to send by another channel; with only the mailed token, the invitee's client refuses the
  key until the person has compared and trusted the sender's fingerprint.
- **Strict workspaces**: a device that made the workspace, accepted an invite with 1.2, or once
  accepted a format 2 wrap for it refuses format 1 wraps for its current epoch (older-epoch ones are
  skipped). Only a device with no memory of the workspace (a member since hitop 1.1) accepts format
  1 wraps, with a warning on every sync.
- **Keys don't change**: the device keeps a commitment (SHA-256 with a label, the workspace id and
  the epoch) to each epoch's key it accepted; a later wrap for that epoch must open to the same key.
- **Epochs don't go back**: the device remembers the newest epoch it has seen, including every
  accepted wrap's head; a server that says the current epoch is older is hiding a rotation (to make
  the device write under a key a removed member has), and is refused. A new member learns the head
  from the wraps the owner made for it, so even a device with no memory notices.
- **Loud, not silent**: forged wraps, changed keys and hidden rotations are reported as security
  errors and nothing is used; a device that pinned a key id and sees the server list another key
  for it gets a warning (and its pinned key is still the one checked).
- **Removal unpins**: the member's devices are unpinned on the remover's device, so wraps they make
  later are refused there.

**Upgrading hitop 1.1 workspaces** (the hosted test server has them): nothing breaks. Devices that
knew a workspace before keep using its format 1 wraps, with a warning. When an owner syncs (or runs
`hitop sync members`) with 1.2, their client re-wraps every epoch it holds, as format 2, for every
member device it has pinned, and the server replaces those format 1 wraps (a format 2 wrap may
replace a format 1 wrap, never the reverse; `GET /v1/workspaces/:ws` lists each key's
`legacyEpochs`). A member whose device accepted the old wrap uses the re-wrapped one even before
trusting the owner's device, because it opens to the key it already holds (nothing new is learned);
for the next rotation it must have trusted the owner's fingerprint. Runner credentials made by 1.1
carry no pins, so their wraps stay format 1 and they keep working, with a warning to make a new
credential (1.2 credentials carry the pins of the devices that may wrap for them and are strict).
hitop 1.1 clients can't open format 2 wraps, so once an owner has upgraded a workspace (or a 1.2
client has added a member or rotated the key), its members need hitop 1.2; the server must be
upgraded first, since a 1.1 server refuses format 2 wraps.

### Rotation and removal

Removing a member (`DELETE /v1/workspaces/:ws/members/:user`) is accepted only together with the
next epoch's key, freshly generated by the owner's client and wrapped for every device of every
*remaining* member (the server checks the set of key ids exactly). From then on the server refuses
writes under the old epoch, and clients only accept new blobs under the new epoch.

What removal can and can't do: the removed member keeps everything they could already decrypt
(old epochs' keys, any blob they downloaded). They can't read anything written after the
rotation. Old blobs are re-encrypted under the new epoch when a client next writes them; a client
may also re-encrypt everything after a removal.

### Test vectors

`vectors/shared.json` (hitop-sync-crypto 0.5, HITOP Cloud C4): sealed run reports, shares and
share key wraps, with the failures every client must refuse (a report moved to another run, a
file blob with a run's id, a share served as an older version, a share wrap made by another key or
a workspace key wrap offered as one); hitop-wasm and the web app's unit checks run it too.


`cli/crates/hitop-sync-crypto/vectors/v2.json` (1.2) has format 2 wrap vectors (open direction,
sender and recipient key pairs from DeriveKeyPair, one wrap handed out while the workspace is at a
later epoch), cases that must fail (made by another sender key, verified against another sender
key, head or sender id rewritten, flipped bit, wrong epoch, wrong recipient key id, wrong
recipient, wrong workspace, format 1 bytes), and key commitments; checked by
`published_v2_test_vectors`, in WASM by the web app's unit tests, and independently with
`@hpke/core` 1.7.4 and node:crypto.

`cli/crates/hitop-sync-crypto/vectors/v1.json` has blob vectors (both directions, fixed nonce), key
wrap vectors (open direction; HPKE encapsulation is randomized, and the recipient key pair comes
from RFC 9180 DeriveKeyPair on a published IKM), and cases that must fail (flipped bit, wrong
version, wrong blob id, wrong key, wrong epoch, wrong recipient, wrong workspace). The Rust test
`published_test_vectors` checks them; they were also checked independently with `@noble/ciphers`
and `@hpke/core` in JavaScript. Every client implementation (the web app in T24) must pass them.

## Authentication

- **Email magic link**: 32 random bytes, valid 15 minutes, single use, stored hashed; at most 5
  per address and 500 in total per hour; the answer is the same whether or not the address has an
  account. The link opens `/signin#<code>`: the code stays in the URL fragment, which browsers
  don't send to servers or in `Referer`.
- **Passkeys** (`webauthn-rs` 0.6, user verification required): registered while signed in;
  challenges live in memory for 5 minutes and are single use. The relying party id is
  `hitop.rws8.tech`, so the web app and the sync server share passkeys.
- **Sessions**: 32 random bytes as a bearer token (never a cookie, so no CSRF), stored hashed,
  30 days.

- **Passwords** (C1b): see "Passwords". **Google** (OpenID Connect, the same code as SSO: PKCE,
  JWKS, nonce, `iss` either spelling Google uses) and **GitHub** (OAuth 2: code + PKCE + client
  secret, then `/user` and `/user/emails`, only the primary *verified* address) end in the same
  one-time code handoff as SSO (oidc.rs): the code goes to the client's loopback listener or an
  allowed web origin, and finishing needs the client's verifier. Accounts link by verified
  address only (as SSO); an address in a "sign in with SSO only" domain is refused.

Sign-in protects *access to the ciphertext and the membership list*, not the contents. Anyone who
takes over an account (mailbox compromise) can't decrypt anything without a device secret key, but
can register a new device key; members must not wrap workspace keys for a device they haven't
confirmed, so the clients require a fingerprint check first (next section).

## Clients (T24)

All three clients (CLI, desktop, web app) run the same Rust for the decisions and the
cryptography: `hitop_core::sync` (what to pull, push or flag) and `hitop-sync-crypto`; the web app
and the desktop app through hitop-wasm, built for `wasm32-unknown-unknown` with randomness from
`crypto.getRandomValues` (getrandom `wasm_js`). The web app runs `vectors/v1.json` and
`vectors/v2.json` through that build in its unit tests.

- **What syncs**: a collection folder's files (`hitop.yaml`, `requests/**`, `environments/*`,
  `globals.yaml`), one blob per file under a random 128-bit id. The plaintext of a blob is
  `{"v":1,"path":...,"text":...}`, so file names are encrypted too. Environment files hold secret
  variables as `{secret: true}`: **secret values never sync**, only their names (tests check the
  server's database and the synced files for a secret value). Paths from the server are checked
  against that layout before anything is written (no `..`, absolute, hidden or other paths), so a
  member can't make another member's client write outside the folder.
- **Device keys**: generated on the device. The CLI and the desktop keep the secret key and the
  session in the OS keychain (CLI without a keychain: only in the file `HITOP_SYNC_KEY_FILE`
  names, mode 0600). **The web app keeps them in the browser's localStorage**, like its other
  secret values: anything that runs script on the web app's origin (an XSS bug, a malicious
  extension) can read them. Prefer the desktop app or the CLI for sensitive workspaces.
- **Fingerprints and trust on first use**: a fingerprint is the first 16 bytes of
  SHA-256(`"hitop-sync/v1/fingerprint"` || public key), as eight groups of four hex digits. A
  client wraps a workspace key only for device keys it has pinned (key id → public key), and pins a
  key only after the person confirms its fingerprint (`--trust`, a terminal prompt, or the app's
  Trust dialog). A pinned key id whose public key changes is refused outright. Since 1.2 the same
  pins decide which devices a client accepts workspace keys *from* ("Accepting keys"); invite
  codes carry the inviter's fingerprint, and every client shows each member device's fingerprint
  (CLI `hitop sync whoami` / `members`, the Sync page) so people can compare them out of band.
- **Versions**: the client asks for and opens blob *n* under associated data that includes *n*;
  it ignores a change whose version isn't newer than what it already has, so the server can't
  roll a known blob back.
- **Conflicts** are decided on the client (three-way against the last synced text): `hitop.yaml`,
  environments and globals merge key by key; a request changed on both sides is shown to the user
  and never pushed until resolved. Two blobs for one path (a create race) converge on the smaller
  blob id; the client whose blob lost merges and deletes it.

## Teams (T24)

- **Roles** (owner, editor, viewer) are checked by the server on every call (`tests/roles.rs`
  runs every role, a runner credential, an outsider and a removed member against every
  operation). As before, roles protect against mistakes, not against a member colluding with the
  server: every role holds the workspace key.
- **Invites** are a rendezvous, not a grant. The owner creates one (mailed through the server's
  SMTP, or a link to share); the token is 32 random bytes, stored hashed, valid 7 days, accepted
  once, and an emailed invite only by that address. Accepting gives nothing: the owner's client
  then shows the invitee's device fingerprints and wraps the workspace key only after the owner
  confirms them. The server alone can never add anyone. Since 1.2 the code the owner's client
  shows carries the owner's device fingerprint, so the invitee's client also checks the other
  direction: that the workspace key it gets comes from that device ("Accepting keys").
- **Removal** rotates the key in the same call (unchanged from T23), and the owner's client then
  re-encrypts every live blob under the new epoch. What removal guarantees: the removed member's
  sessions can't reach the workspace, their wrapped keys are deleted, and nothing written after
  the rotation (including the re-encrypted copies) is readable with the keys they had
  (`tests/roles.rs`, `a_removed_member_cant_decrypt_anything_written_after_the_rotation`). What it
  can't do: take back what they already had. A removed member keeps every file and key epoch they
  synced before; rotate any secret they could have seen. A removed member who kept old blobs from
  the server's database can still open those old ciphertexts with the old key.
- **Runner credentials** (scheduled runs in CI or cron): an account of its own
  (`<id>@runner.invalid`, no sign-in, no other workspace, no new keys, no invites), the viewer role
  in one workspace, one device key made by the owner's client, and a one-year session token. The
  credential string (`hitop-runner-1.…`, the token and the device secret key) **decrypts the
  workspace**: it belongs in a CI secret store only. Revoking it is a removal (key rotation).
- **Server-side runs are not offered.** Running monitors on the sync server would need a key that
  decrypts the workspace on the server, which is what end-to-end encryption rules out: the server
  would see requests, responses and environment values while running them. The supported path is a
  runner you control (`hitop schedule --github --sync`, or cron). An operator who wants the server
  host to run monitors runs a runner credential there as a separate service, and that host can
  then read the workspace; this is a choice for self-hosters, documented, never a default.

## Single sign-on (T25)

A self-hosted server can sign people in with an OpenID Connect provider (`src/oidc.rs`; set up in
README.md). The server is the OIDC client: a confidential client whose secret lives only in the
server's environment (`HITOP_SYNC_OIDC_CLIENT_SECRET` or a `_FILE`), never in a hitop client.

- **Flow**: authorization code with PKCE (S256). The server makes `state` (32 random bytes,
  single use, 10 minutes, kept as a hash in memory), `nonce` and its own PKCE verifier per
  sign-in; the callback exchanges the code with the client secret (HTTP Basic) and the verifier.
- **ID token checks**: an asymmetric signature only (RS/PS 256-512, ES256/384, EdDSA; `none` and
  HMAC are refused, so the client secret can't be used to forge a token), with a key from the
  provider's JWKS; `iss` equal to the configured issuer (and the discovery document must name the
  same issuer); `aud` containing the client id, and `azp` equal to it when present or when there
  are several audiences; `exp` and `iat` within 60 seconds of the server's clock; `nonce` equal to
  the one this sign-in sent. The JWKS is cached for an hour and fetched again when a token names
  an unknown key id (provider key rotation), at most once a minute. Provider endpoints must be
  https (http only on localhost, for tests).
- **Handing the session to the right client**: the browser that signed in gets a one-time code
  (32 random bytes, 5 minutes, single use): by a redirect to the CLI's loopback listener
  (`http://127.0.0.1:<port>/`), to a page of an allowed web origin (in the URL fragment), or on a
  page to paste from. Finishing (`POST /v1/auth/sso/finish`) also needs the verifier whose SHA-256
  the client sent when it started, and that verifier never leaves the client. So someone who
  starts a sign-in and sends the provider link to a victim can't finish it: the code only goes to
  the victim's browser (or a loopback address on the victim's machine), and the attacker's
  verifier doesn't match a code started elsewhere. What remains is the paste page: a victim who
  pastes a code into an attacker's hitop gives them the session, as with a mailed sign-in code.
  The page says not to.
- **Accounts**: an identity is (issuer, subject). The first SSO sign-in links it to the account
  with the same address only when the provider says the address is verified
  (`email_verified: true`); an unverified address is refused, never linked and never used to make
  an account (otherwise anyone who can set an address at the provider would take over the hitop
  account with that address). Providers that don't send `email_verified` (Entra ID) need
  `HITOP_SYNC_OIDC_ASSUME_VERIFIED=1`, which the server accepts only together with a list of
  allowed domains: the operator asserts the provider owns those addresses. After the first link,
  sign-in follows the subject, so an address changed at the provider doesn't move the account.
- **Policy**: allowed email domains, required groups (from a groups claim), and "SSO required"
  (no sign-in mails and no passkey sign-in for the allowed domains, checked when a mail is
  requested, when its link is used, and at passkey sign-in). Turning SSO on or off doesn't end
  existing sessions (30 days); an operator who must lock someone out at once deletes their
  sessions (README.md).
- **What SSO doesn't change**: it protects access to the ciphertext and the membership list, like
  the other sign-ins. A provider (or anyone who controls an account there) can sign in as that
  person and register a device key, but can't decrypt anything: members only wrap workspace keys
  for device keys whose fingerprints they confirmed. The provider is trusted for authentication
  only, never for confidentiality.

## Audit log (T25)

`src/audit.rs`. An append-only table of security-relevant events: sign-in (with the method),
failed sign-in (with a reason), sign-out, passkeys and device keys added, SSO links and SSO
configuration errors; and per workspace: created and deleted, members added and removed, role
changes, invites created, deleted and accepted, key rotations and keys wrapped for new devices,
runner credentials created and their sessions revoked. Never blob contents, file names, keys,
wrapped keys or tokens (`tests/audit.rs` checks every token, code and the workspace key against
every row).

- **Who and from where**: the account id and address of whoever did it, the target (an account,
  invite or workspace id), a small JSON detail, the time, the client IP and the user agent (at most
  200 characters). Behind a proxy the IP comes from the last `X-Forwarded-For` entry, and only when
  the peer is a trusted proxy (`HITOP_SYNC_TRUSTED_PROXIES`, default loopback).
- **IP privacy**: the server stores IPv4 addresses truncated to their /24 and IPv6 to their /48
  (`203.0.113.0`, `2001:db8:1::`), or nothing (`HITOP_SYNC_AUDIT_IP=off`). Not hashed: there are
  only 2^32 IPv4 addresses, so a hash is reversible by trying them all and would only look
  private. A truncated address still tells a country and a network, which is what an owner looking
  at a strange sign-in needs.
- **Who can read it**: a workspace's owners (`GET /v1/workspaces/:ws/audit`, paginated, filters
  for event, actor and time); every person their own account events (`GET /v1/me/audit`). Account
  events of other people (failed sign-ins for an address, for instance) are for the operator only.
  A deleted workspace's log stays, unreadable over the API, until the retention purge.
- **Retention**: rows older than `HITOP_SYNC_AUDIT_RETENTION_DAYS` (default 365, 0 keeps them)
  are deleted at start and hourly.
- **Tamper evidence**: one hash chain per workspace, and one for account events:

  ```text
  hash = hex(SHA-256("hitop-sync/v1/audit" || 0x00 || for each field: u32 big-endian length || UTF-8 bytes))
  fields = prev_hash, workspace_id, seq, at, event, actor_id, actor_email, target, detail, ip, user_agent
  ```

  `seq` and `at` in decimal; `prev_hash` of the first row is 64 zeros. `audit_chain` holds each
  chain's head (the last row's seq and hash, so a deleted last row shows) and anchor (the last
  purged row, so a purged chain still verifies from where it now starts). Verification
  (`GET .../audit/verify`, `hitop-sync-server audit-verify`) reports the first row that is
  missing, out of place or changed. `hitop sync audit --verify` recomputes the chain on the
  client from the rows (`detailJson` carries the hashed detail text) and remembers the head it
  saw; a later answer whose row at that seq differs is reported as a rewritten log.
- **Limits**: this is tamper evidence, not tamper proofing. Someone who can write the database
  (the operator, or an attacker on the host) can rewrite a whole chain with fresh hashes and a new
  head; only a client that pinned an earlier head notices, and only for rows up to that head. The
  operator can also simply not record an event, or purge. Daily anchors mailed to org owners
  close the first for the orgs that get them (C5, "Organizations").

## Recovery (HITOP Cloud C2)

A person whose only device is lost would lose every workspace no other member holds. `hitop sync
recovery create` (or the Sync page) makes a **recovery phrase**: 32 random bytes as 24 BIP-39
English words (the last carries a checksum, so a typo is caught; `hitop_sync_crypto::RecoveryPhrase`).

- **What is sealed**: the device's **key bundle**: its key id and X25519 secret key, its name, its
  pins and its `WorkspaceMemory` per workspace, as JSON. It is sealed with XChaCha20-Poly1305 under
  SHA-256(`"hitop-sync/v1/recovery-key"` || phrase bytes), associated data
  `"hitop-sync/v1/recovery" || u16 len || account id` (`seal_recovery`; layout pinned in a test).
  The server keeps it per account and unlock method (`phrase`), returns it only to a signed-in
  device of that account (not access tokens or runner credentials), and audits set, fetch and
  delete. It never sees the phrase or the key.
- **Recovering**: on a new device, after signing in to the same account, the phrase opens the
  bundle; the client checks that the server still lists the recovered key id **with the same public
  key**, then becomes that device (its pins and key memory included, so the acceptance policy
  carries on: no new trust decisions, no lowered strictness). The key the new sign-in registered is
  deleted while it holds no wrap (`DELETE /v1/me/keys/:id`). Teammates see nothing new: the device
  key is the one they already trust.
- **Kept current**: the device that made (or used) the phrase keeps its bytes next to its secret
  key and re-seals the bundle when its pins or memory change (CLI after each sync command, the
  Sync page after each action), so a recovery restores what it knew last.
- **Threats**: the phrase plus the person's sign-in (mailbox, passkey or SSO) is the device: keep it
  offline. A server can't open the bundle (256-bit phrase), move it to another account (associated
  data) or change it (authenticated encryption); it can withhold or
  delete it (denial of recovery) or serve an old version (the device then knows less than it did:
  rollback of pins and memory to that point, which `accept_wraps` still bounds by the epochs in the
  old memory). The password (next section) seals the same bundle under a slow-KDF key as a second
  method; its strength is the password's, which is why the phrase stays recommended.

## Passwords (HITOP Cloud C1b)

One password signs a person in **and** unlocks their keys on a new device, without the server
being able to do either on its own. The design splits one password-derived master key in two, like
Bitwarden's and 1Password's account passwords:

```text
master    = Argon2id(NFC(password), salt, m, t, p, ad = "hitop-sync/v1/password", 32 bytes)
authKey   = HKDF-SHA256(master, info = "hitop-sync/v1/password/auth")                  -> server
bundleKey = HKDF-SHA256(master, info = "hitop-sync/v1/password/bundle" || canonical(params))
```

(`hitop_sync_crypto::derive_password_keys`; vectors in `vectors/password.json`, run by the crate,
hitop-wasm and the web app's unit tests.) Defaults: m = 64 MiB, t = 3, p = 1, 16-byte random salt
per password (a new salt at every change or reset).

- **What the server sees and keeps**: the parameters (it hands them to any client that asks for an
  address) and `authKey` at each sign-in, of which it stores only its own Argon2id hash (19 MiB,
  t = 2, its own salt; `HITOP_SYNC_PASSWORD_PEPPER` as the Argon2 secret when set, flagged per row
  so a pepper added later doesn't break older hashes). The comparison is the `argon2` crate's
  constant-time verify.
- **Domain separation**: the two keys come from one master through HKDF with different labels, so
  `authKey` reveals nothing about `bundleKey` short of guessing the password: each guess costs the
  client's Argon2id (64 MiB). A copy of the database costs an attacker the server's Argon2id **and**
  the client's per guess, and even a right guess at `authKey` alone (from a stolen hash) gives no
  bundle key. The parameters are part of `bundleKey`'s info, so a server that reports other
  parameters than the bundle was sealed with gets a key that opens nothing.
- **Weakened parameters**: the server does see `authKey`, so a malicious server could hand a client
  low costs and turn that into a cheap offline oracle for the password. Every client refuses
  parameters below m = 64 MiB / t = 3 (and above 1 GiB / t = 10 / p = 4, a denial-of-service
  ceiling) before deriving anything (`PasswordKdf::check`), and the server refuses to store them.
- **No account enumeration**: `POST /v1/auth/password/params` answers every address; one without a
  password gets parameters made up from HMAC(server secret, address) (stable between asks). Sign-in
  answers one `invalid_credentials` for a wrong address, an account without a password and a wrong
  password, after the same hashing work (a dummy verify on misses). Sign-up and forgot answer 202
  always; the mail tells the truth (it goes to the address's owner). Residual: the made-up salt
  changes when an address first gets a password, so someone watching an address over time can see
  that it did.
- **Guessing online**: the IP rate limit on `/v1/auth/*` (60 a minute) plus a per-address backoff
  after 5 wrong passwords (30 s, doubling to 15 minutes; addresses with and without an account
  alike; in memory). Mails (sign-up, reset) share the 5 per address per hour of sign-in links.
- **Sign-up** stores nothing usable until the mailed code (15 minutes, single use, SHA-256 stored)
  confirms the address: the pending row holds the server's hash of `authKey`, not the key. An
  account made by code, passkey or SSO adds a password the same way, or while signed in (`POST
  /v1/me/password`).
- **The key bundle under the password**: the same bundle as the phrase ("Recovery"), sealed with
  `seal_key_bundle(bundleKey, account id, ...)`, stored as method `password`. The device whose key
  is in it keeps `bundleKey` and re-seals it when its pins or memory change, but only while the
  server's bundle still opens with that key and holds that device (otherwise it forgets the key, so
  a device with an old key never overwrites a newer bundle).
- **A new device** signs in with the password, fetches the bundle and **becomes the device in it**
  (the same checks as a recovery: the server must still list that key id with the same public key).
  No new key is registered and nobody has to trust anything: teammates already pinned that key, and
  owners' wraps already target it. That is the choice over "register a new key and have the bundle
  sign it into trust": hitop's trust is per device key (pins, `accept_wraps`), and a new key would
  need an owner to wrap the workspace keys for it, the step the owner wanted gone. The cost: every
  device unlocked this way **is** the same device key (as with the phrase), so one of them can't be
  revoked alone; removing that key removes all of them. A device that already has a key of its own
  (a second computer signed in by code) keeps it; `hitop sync password unlock` (or "Unlock with
  password") switches it to the bundle's device.
- **Change** (`POST /v1/me/password`, current `authKey` required when there is one): the client
  opens the bundle with the old `bundleKey` and sends it re-sealed under the new one in the same
  request (atomic); without a bundle the old one is deleted (it could no longer be opened). Other
  sessions end; a mail tells the person.
- **Reset by mail** (forgot password) can't recover the bundle: nobody can open it without the old
  password. The server deletes it, ends every session and says `bundleLost`; every client warns
  before and after. The keys come back only with the recovery phrase or from a device still signed
  in (whose next password sign-in seals its bundle again). A device that was the account's already
  keeps its identity through a reset; a new one gets a new key and doesn't claim the bundle.
- **Google, GitHub, a mailed code or a passkey alone never unlock keys**: they prove an address,
  which the server could fake to itself. A device signed in that way asks for the password (or the
  phrase) to become the account's device.
- **What a malicious server can do here**: run an offline guess on a hash it keeps or on an
  `authKey` it sees (costing the client's Argon2id per guess for `bundleKey`; a weak password
  falls); withhold or delete the bundle; serve an old bundle (the same rollback bound as a recovery);
  send a reset mail to itself (it controls sessions anyway, but still can't open the bundle).
  It can't open the bundle without the password, move it to another account, or weaken the KDF.
- **TOTP second factor**: not in C1b (a follow-up). Passkeys already give a phishing-resistant
  sign-in; a TOTP step would sit after `password/login` and before the session.

## Hosted service (HITOP Cloud C3)

HITOP Cloud runs this server for paying customers. What that adds:

- **Billing never touches workspace data.** Stripe takes the payment as merchant of record
  (Stripe Managed Payments: it collects the buyer's details and handles tax); the server learns an
  org's plan, seats, status and the Stripe ids (customer, subscription, item, price, checkout
  session), and keeps the ids and outcomes of webhook events, never their bodies (buyer name,
  email, address). Plans only decide which creates are allowed; no plan state ever deletes, locks
  or reads a blob, and reads and pulls work on every plan (`limits.rs`).
- **Webhooks are authenticated before they are read**: `Stripe-Signature` must carry a `v1` = hex
  HMAC-SHA256 of `"<t>.<raw body>"` under the endpoint's signing secret (constant-time compare)
  and a `t` within 5 minutes of the server's clock, so a captured delivery can't be replayed
  later. A redelivered event is recognised by its event id; an older subscription state is ignored
  (event `created` per subscription).
- **A paid event can't be pointed at someone else's org.** The org comes from a subscription id the
  server already tied to it, or from Checkout Session and subscription metadata the server signed
  (HMAC of org and buyer under a server secret) when an admin of that org started the checkout.
  Unsigned or mismatched data is recorded as `unmatched` and changes nothing. Events of the other
  mode than the server's key (a test event on a live key, or the reverse) are ignored.
- **The Stripe secret key** can create checkouts, change seats, open customer portal sessions and
  read prices; a restricted key with only those permissions limits what a stolen env file does.
  Stripe (or Link's support) may refund a purchase on its own as merchant of record; a full refund
  arrives as `charge.refunded` and ends the plan like the owner's.
- **Billing actions need a signed-in session** of an org admin (owners for the trial); a personal
  access token can read the plan but never buy, change seats or open the billing portal.
- **Shared host.** The service runs on one VM shared with other sites (spec section 5). A
  compromise of that host is a compromise of the server in this document: it sees the metadata
  above and can deny service, not read collections. The security page says so, and that no
  external review of the encryption has been done yet.

## Run history and alerts (HITOP Cloud C4)

HITOP never runs a collection on its machines (owner decision Q8): that needs the workspace key,
which the server is built never to hold. The customer's runners (CI with a runner credential,
cron as a member device) run it and upload the result (`hitop run --sync-report`, `hitop sync
report`); only runner credentials, editors and owners may upload, and uploading is the only write
a runner credential has.

- **Sealed report**: `hitop-sync-crypto` `seal_run(workspace key of the current epoch, workspace
  id, run id, epoch, report.json)`, XChaCha20-Poly1305 with the blob envelope layout (`0x01 || u32
  epoch || nonce || ciphertext || tag`) but its own associated data, `"hitop-sync/v1/run" || u16
  len || workspace id || u16 len || run id || u32 epoch` (`vectors/shared.json`). The server can't
  read it, can't serve it as a file (another label), move it to another run or workspace, or claim
  another epoch. The report is the one `--reporter json` writes: secret values are already `••••`.
  Members open it on their devices (`hitop sync runs --id`, the Sync page); the portal holds no
  keys and shows status rows only.
- **Status rows are plaintext by design** (Q10): the server needs them for history and alerts.
  They carry no URL, header, body or value, but they do carry the collection and environment
  names, and failed request names when the owners turn that on (off by default). A workspace can
  turn status rows off ("history only, no alerts"): uploads then carry the sealed report only.
  The server can lie in a status row (say a failing run passed, or drop alerts); the client that
  opens the report compares its result with the row and warns when they differ.
- **Alerts** are sent from status rows only: on a series' status change (passed → failed, failed
  → passed, a first run that fails) and once per missed interval. Mail goes only to addresses of
  the workspace's current members (checked when saved and again when sent); webhooks are `https`
  to hosts whose every address is public (no loopback, private, link-local, CGNAT or documentation
  ranges; no redirects; 10 s), checked when saved and before each call; at most
  `HITOP_SYNC_ALERTS_PER_HOUR` (30) alerts a workspace an hour. Webhook and Slack URLs are secrets
  of the workspace's owners: only owners read or change them, and they're never logged or put in
  the audit log. A residual risk: DNS can change between the address check and the connection
  (rebinding); the host check is a guard, not a sandbox.
- **Retention and limits**: rows past the plan's history (Free 7 days, Pro 30, Team 90, Business
  365; self-hosted `HITOP_SYNC_RUN_RETENTION_DAYS`, 365) are purged hourly and their bytes freed;
  sealed reports count towards the workspace's bytes and the org's storage; uploads a day per org
  are capped (an abuse ceiling); Free keeps status rows only.
- Runs are rows of their own, not workspace blobs: they never appear in the change feed, so
  clients that don't know them (1.1, 1.2) sync as before.

## Shared environment values (HITOP Cloud C4)

Secret values never sync unless someone shares them (Q9: opt-in per environment, end to end to
pinned devices). `hitop sync secrets share --env <env>` (owners and editors, Pro and up on HITOP
Cloud) seals `{environment, values, viewers, runners, allRunners}` with a fresh random **share
key** (`seal_share`, XChaCha20-Poly1305, associated data `"hitop-sync/v1/env-share" || u16 len ||
workspace id || u16 len || share id || u32 version`) and wraps that key for each recipient device
with HPKE Auth mode made by the sharing device (`seal_share_wrap`: like a format 2 workspace key
wrap, but with the `info` label `"hitop-sync/v1/env-share-wrap"`, the share id and the version, so
a share wrap never passes for a workspace key wrap or the other way round). Recipients: the
trusted (pinned) devices of the workspace's owners and editors, the viewers' only with
`--viewers`, runner credentials only when named. The CLI names the recipients and asks first.

- A recipient uses a share only when its wrap opens under the **pinned** key of the device that
  made it (runners: the pins in their credential), so the server can't hand out values of its own.
  Every write is the next version with a new key (the server refuses anything but current + 1),
  and clients remember the highest version they opened and refuse an older one (a server rolling
  a share back to values or recipients from before a removal).
- New devices: whoever adds a member or hands the workspace key to a new device also wraps every
  share it can open for the intended devices (only trusted ones); the sharer's next sync does the
  same for any device still without a copy. Removing a member deletes their devices' share wraps
  on the server, and the remover's client re-keys every share it can open (new key, new version,
  wraps for whoever is still meant to have it). **Values the removed person saw stay theirs**: the
  CLI and the Sync page say to rotate those secrets at their source.
- The server sees which devices hold a share and when it changes, never an environment name (it
  is inside the sealed content) or a value. A run merges shared values **under** the person's own
  (`HITOP_SECRET_*`, `--secret` and the keychain win); the CLI keeps them in memory for the run,
  the web app in memory for the session; they are masked like any secret.
- Not covered: a member who could open a share and colludes with the server can re-share the
  values (they had them); the share key's recipients are only as good as the fingerprints people
  compared.

## Organizations: SSO, domains, audit anchors, admin (HITOP Cloud C5)

Business customers bring their own identity provider, prove their domains, take the audit log
out of the server and manage every workspace of the org. What that adds:

- **The host key** (`HITOP_SYNC_HOST_KEY` or `_FILE`, hostkey.rs) seals the orgs' OIDC client
  secrets at rest (AES-256-GCM, a fresh nonce, associated data naming the org), so a database dump
  or backup alone doesn't give anyone an org's client secret. The server holds the key: its
  operator, or an attacker on the host, can open them. The secret is never answered, logged or put
  in the audit log. Without a host key per-org SSO is off entirely (the hosted test server, owner
  decision Q15).
- **An org's provider signs in only its own verified domains.** A domain is verified by a TXT
  record `_hitop-verify.<domain>` = `hitop-verify=<random token>` looked up over DNS over HTTPS
  (`HITOP_SYNC_DNS_URL`); a domain is verified for one org at a time, and the first to prove it
  keeps it (a later TXT change doesn't move it; the operator settles disputes); free-mail domains
  can't be claimed. Every sign-in through org B's provider is checked against B's verified domains
  (oidc.rs's domain check with `allowed_domains` = those domains, never "any"), so B's provider,
  or B's admin who controls it, can't sign in `ada@a.example` of org A whatever its token says
  (`tests/sso.rs`, `tests/keycloak.rs` with two realms). The flow is oidc.rs's (PKCE, state,
  nonce, every ID token check); the sign-in's state names the org, and there is one callback URL.
  "SSO required" for an org's domains turns off sign-in mails, passwords, passkeys, Google,
  GitHub and the server's own SSO for those addresses (checked in memory, from the domains table,
  wherever those sign-ins are checked). What remains: the resolver (and DNS) is trusted to answer
  the TXT record truthfully when it is checked, so whoever controls a domain's DNS at that moment
  can claim it. A provider can sign in its own domains' people and register device keys for them,
  but, as for the server's SSO, can't decrypt anything (members wrap keys only for devices whose
  fingerprints they confirmed).
- **Audit anchors (closes open item 7).** Once a day the server signs (Ed25519, its own key, a
  server secret; public half at `GET /v1/audit/key`) the heads of every chain of an org (the
  org's chain and each of its workspaces') and mails them to the org's owners when a head moved;
  public at `/v1/audit-anchors/<org>/<day>.json` only when the owners opt in (a public anchor
  shows how many workspaces the org has and their ids). The signed export (`GET
  /v1/orgs/:org/audit/export`, JSON lines, an Ed25519 signature over `"hitop-sync/v1/audit-export"
  || 0x00 || SHA-256(the lines before)`) carries every row with its hash inputs. `hitop sync audit
  --verify --anchor <anchor.json>` recomputes every chain and checks that each anchored head is in
  the export at that seq with that hash; it pins the server's key on first use. A server (or
  someone with its database) that rewrites a chain after an anchor left is caught by anyone holding
  that anchor (`tests/audit_export.rs`, `cli/crates/hitop/tests/sync.rs`). What remains: the server
  holds the signing key, so a signature says the server made the file, not that it is honest; the
  protection is the anchor already in the owners' mailboxes. Rows written and rewritten between two
  anchors, an event never recorded, and rows purged by retention ("purged, can't check") aren't
  caught. A server that suppresses the mail is noticed only by owners who expect it.
- **The admin console** shows org owners and admins every workspace of the org (ids, sizes,
  members and roles) and every device key of the org's members: metadata the server holds anyway,
  never contents. It transfers a workspace's ownership to one of its current members (they already
  hold the key), asks a workspace's owners to rotate (a flag: the server can't rotate, it holds no
  key; `hitop sync rotate` does it and clears the flag), and removes a member's device from **the
  org's workspaces only** (owner decision 2026-11-03): its wraps and shared-value wraps there go,
  each of those workspaces excludes the key from then on (`workspace_excluded_keys`: the server
  neither lists it to the owners' clients nor stores a wrap for it there, from anyone), and they ask
  for a rotation. The device key itself and its access to the member's personal and other orgs'
  workspaces stay: an org admin can't touch what isn't the org's. Removing a device stops the server
  handing it new keys of those workspaces; the device keeps whatever it already decrypted, and the
  keys of past epochs it holds open those epochs' blobs until the owners rotate and rewrite. A
  malicious server could ignore the exclusion and keep serving the device (as with any removal:
  the owners' pinned keys and the rotation are the defence); an org workspace the member joins
  later isn't covered until an admin removes the device again.

## SCIM provisioning (HITOP Cloud C6)

An org's identity provider adds and removes members over SCIM 2.0 (scim.rs). What it adds:

- **A SCIM token per org** (`hitop_scim_` + 32 random bytes, stored as its SHA-256, at most 5 per
  org, created and revoked only by org owners with a signed-in session, never by an access token).
  It names its org, so a token can't read or change another org's directory, users or workspaces
  (`tests/scim.rs`). Whoever holds it can do what the IdP can: add and remove the org's members
  (within its seats) and change group membership, so it is as sensitive as an org admin's
  session. It can't make owners, demote or remove them (409), change SSO, billing or the token
  list, or read anything but the directory it wrote. Every change is on the org's audit chain
  (and the workspace's) with `scim:<token name>` as the actor.
- **Access, never keys.** As with org roles, a SCIM group's workspace role can't open a
  workspace: the server can't wrap the key. For someone not yet in the workspace SCIM opens an
  invite that shows as accepted with the person's device keys; a workspace owner's client adds
  them only after its fingerprint check (`hitop sync members`, `hitop sync add`), exactly as for
  an accepted invite. So a stolen SCIM token, or a malicious IdP, can get a person proposed to
  the owners, not into the workspace. The invite carries a random token nobody knows, so it can't
  be accepted by code.
- **Removal is immediate on the server, not on devices.** Deactivating a user, or taking them out
  of a group, removes the org membership and every workspace membership SCIM brought, with that
  person's wrapped keys and shared-value wraps, and sets the workspace's rotation request. Their
  devices keep what they already decrypted and the keys of past epochs until an owner rotates
  (`hitop sync rotate`), as for any removal the server makes (C5 admin console). Accounts and
  sessions stay (the person may have personal workspaces); SSO-required orgs stop them signing in
  with the org's domain. Memberships that existed before SCIM are never removed by it.
- **What the server stores** (migration 10): the tokens' hashes, names and last use; per org the
  directory's users as the IdP sent them (userName = address, externalId, display and name
  parts, active), groups (name, externalId, members, the org role and workspace roles an admin
  gave them) and the grants SCIM made (workspace, address, role, kind). Other attributes the IdP
  sends (title, phone, manager...) are dropped.
- **Residual**: the IdP (or whoever holds its token) is trusted for who belongs to the org; a
  directory larger than the plan stops at the seat limit (402) without applying half of a
  change.

## What a malicious server can still do

- **See metadata**: account emails, who is in which workspace, number and size of blobs, blob ids
  (clients use random ids, never file names), when and how often they change, version vectors.
- **Deny service or withhold updates**: drop writes, serve an older (still authentic) state of a
  blob, or hide a blob. Clients detect a version going backwards for a blob they've seen; they
  can't detect a hidden blob they've never seen. A signed workspace manifest would close this (not
  in v1).
- **Substitute public keys**: the server returns members' device public keys. A malicious server
  could return its own key so an owner wraps the workspace key for it. Mitigation in the clients
  (T24, done): fingerprints shown and confirmed before a key is used, pinned afterwards (trust on
  first use); a changed key for a pinned id is refused. The first confirmation is only as good as
  the channel people compare fingerprints over.
- **Hand out a workspace key of its own choosing**: closed in 1.2 ("Accepting keys"). Before,
  HPKE base mode didn't say who wrapped a key, so the server could give a new member, or any
  device a fake *next* epoch, a key it knew, and read what that device then wrote. Now a client
  uses a key only from a wrap made by a device it pinned; the inviter's device is pinned from the
  fingerprint in the invite code; keys of known epochs can't change and epochs can't go back.
  `cli/crates/hitop/tests/sync.rs` and `webapp/tests/sync.cjs` play such a server by editing its
  database (an unauthenticated wrap, one forged in the owner's name, one from a device the server
  added to the owner's account, a swapped key, a hidden rotation, an old wrap replayed as the new
  epoch's, a fake next epoch) and check that the client refuses each and writes nothing. What
  remains:
  - **The first fingerprint comparison** is only as good as the channel. A mailed invite carries
    no fingerprint the server couldn't change; a person who trusts a fingerprint without comparing
    it trusts whoever the server says.
  - **Members since hitop 1.1** accept format 1 wraps (with a warning on every sync) until their
    device has accepted a format 2 wrap; until the owner's client upgrades the workspace, the
    server could still give *those* devices a key for a new epoch. 1.1 runner credentials keep
    accepting format 1 wraps until replaced.
  - **A pinned member colluding with the server**: any member device others pinned can make valid
    wraps. A removed member's device stays pinned on the other members' devices (only the remover's
    device unpins it), so a removed member and the server together could hand the others a next
    epoch's key the removed member knows; they would notice only that the rotation came from that
    device (the CLI prints who handed out a key only when it isn't trusted). Signing the member
    list and rotations (a signed manifest) would close this.
  - **The member list itself** is unauthenticated: the server can hide a member or a device from
    the list it shows. It can't give anyone the key that way: only members' clients wrap keys, and
    only for devices their person trusted.
- **Role enforcement is advisory**: a malicious server can let a viewer write, but such a write
  still has to be encrypted with the workspace key, which only members have. Any member, viewer
  included, holds the key and could produce valid ciphertext; roles protect against honest
  mistakes and well-behaved clients, not against a malicious member colluding with the server.
  Per-member signatures on blobs would close this (not in v1).
- **Lose data**: it is a single SQLite (or Postgres) database; operators keep backups.

## Out of scope

Compromised member devices; side channels on client devices; traffic analysis; the server
process's own TLS (the front proxy terminates TLS); post-quantum security (X25519 only).

## Open items before a public launch

1. External review of this design, of `cli/crates/hitop-sync-crypto` and of the clients
   (`hitop_core::sync`, `cli/crates/hitop/src/commands/sync/`, `webapp/src/sync.js`).
2. Done in 1.2: authenticated key wraps (HPKE Auth mode with the pinned device key), invite codes
   with the inviter's fingerprint, key commitments and epoch rollback detection ("Accepting
   keys"). The review should now focus on: `accept_wraps` (`keyring.rs`) and its use in both
   clients, the format 2 `info` and wire layout, the legacy path (who still accepts format 1 wraps,
   and the owner's upgrade), the runner credential's pins, and the residual risks listed under
   "Hand out a workspace key of its own choosing".
3. Consider a signed workspace manifest (hidden-blob detection) and per-member signatures.
4. Done in C2: a recovery story for lost devices (recovery phrase sealing the device's key bundle,
   "Recovery"). The review should check the bundle's binding to the account and the listing check
   on use. C1b adds the password unlock ("Passwords"): the review should check the auth/bundle key
   split, the parameter floor in every client, the enumeration answers and the reset path.
5. The web app's device key in localStorage (a non-extractable WebCrypto key can't be used by the
   Rust HPKE code; an IndexedDB-wrapped key would only move the problem).
6. Done in T24: key fingerprints and trust-on-first-use pinning in every client.
7. Done in C5: audit log heads anchored outside the server (daily signed anchors mailed to org
   owners, public opt-in; `hitop sync audit --verify --anchor`; "Organizations"). The review should
   check the export and anchor formats, the CLI's checks, and the per-org SSO domain rule.
8. C6: SCIM (see "SCIM provisioning"): the review should check that a SCIM token can't reach
   another org, that SCIM never adds a workspace member itself (invites only, added by an
   owner's client after its fingerprint check), and what removal leaves on devices.
