# hitop sync: threat model and encryption design

Status: design and implementation for testing (server T23, clients and teams T24, SSO and audit
log T25, authenticated key wraps 1.2). **Not externally reviewed.** An external review of this design, of `cli/crates/hitop-sync-crypto` and of
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

The server binary does not link any decryption code: the client crate and the AEAD/HPKE crates
are dev-dependencies only, and `tests/no_crypto_in_server.rs` fails the build's tests if that
changes. `tests/plaintext.rs` runs a full two-member session with a key rotation and checks that
no plaintext, workspace key or device secret appears in the database files or in any answer.

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
  operator can also simply not record an event, or purge. Anchoring heads outside the server
  (signed, or published) would close the first; not in v1.

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
4. A recovery story for lost devices (a recovery key from `DeviceKeyPair::derive`, printed once).
5. The web app's device key in localStorage (a non-extractable WebCrypto key can't be used by the
   Rust HPKE code; an IndexedDB-wrapped key would only move the problem).
6. Done in T24: key fingerprints and trust-on-first-use pinning in every client.
7. Audit log heads anchored outside the server (signed or published), so a rewritten chain is
   detectable without a pinned head.
