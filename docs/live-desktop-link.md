---
type: Reference
title: Live desktop authentication and connection settings
description: Desktop PKCE login, gateway TLS trust and per-server credentials.
tags: [live, gateway, authentication]
generated: { by: codex, at: 2026-10-18T09:57:30Z }
sources:
  - ../crates/mg-live-link/src/lib.rs
  - ../crates/mg-ui/src/settings.rs
  - ../crates/mg-ui/src/options_live.rs
  - ../crates/mg-ui/tests/live_connection.rs
  - ../crates/mg-ui/tests/screens.rs
---

# Live desktop link

`mg-live-link` owns desktop authentication. It never opens a browser or starts
a game server. The GUI supplies the gateway origin and optional SHA-256 leaf
certificate pin, starts `Login` on a worker, opens `browser_url`, then calls
`finish` on that worker. The attempt closes its listener on success, timeout, the probe limit or drop.
Invalid callbacks do not consume the login; all I/O shares the original deadline.

## Login and trust

The OS random source generates independent 32-byte PKCE verifier and state.
The verifier is unpadded base64url; the challenge is S256. `/auth/start` receives
`challenge`, `port` and `state`. Only `127.0.0.1:0` is bound. The single GET
`/cb?grant=...&state=...` must contain exactly one matching state and nonempty
grant. A text-only response has no external resources, no-referrer, no-store,
and a restrictive content policy. The listener closes before token redemption.

`POST /auth/token` sends `{grant, verifier}` and expects `{token}`. Errors and
Debug output never include the grant, verifier or token. Responses are bounded
to 8 KiB; redirects are refused. HTTPS uses ureq 2.12.1 and rustls 0.23 with
WebPKI roots. A saved pin authenticates the exact DER leaf certificate and
handshake signatures. An IP origin without a pin returns `TrustRequired`
with origin and SHA-256 fingerprint, after an aborting TLS discovery handshake
that sends no HTTP data. The caller must obtain explicit trust before retrying.
WSS origins are normalized to HTTPS for login; credentials, paths, queries and
fragments in configured origins are rejected.

## Reproduce

Use Rust 1.98.0, `CARGO_INCREMENTAL=0`, and `NWN_ROOT` pointing to a nonexistent
folder. Run `cargo +1.98.0 test -p mg-live-link -j 2`. No game install, external
service or real credentials are required. The fake gateway uses an ephemeral
loopback TLS listener and a generated certificate. Tests cover PKCE redemption,
callback response policy, wrong state, timeout, second callback refusal, pin
mismatch and TOFU without HTTP. This is local protocol proof, not acceptance
against a deployed Discord gateway.

Windows accepted sockets inherited nonblocking mode in the first fixture run,
causing `WouldBlock` during the handshake. Both accepted callback and fixture
sockets now explicitly switch to blocking mode before deadline-bound reads.
The happy-path TLS regression checks that behavior.

TLS API references: [ureq 2.12.1](https://docs.rs/ureq/2.12.1/ureq/),
[rustls](https://docs.rs/rustls/0.23/rustls/).

## Credential storage

`credentials::TokenStore` uses keyring 3.6.3 with Windows Credential Manager,
macOS Keychain or Linux Secret Service, one entry per server ID under
`org.moonglow.live`. Its backend is injectable. Only `Unavailable` permits a
fallback in the application's `live-credentials` directory; denied access fails
closed. IDs are hashed to filenames, so they cannot escape that directory.
Files are replaced atomically and synced. Unix uses directory 0700 / file 0600;
Windows uses a protected DACL granting only the current user full control,
applied through a hidden PowerShell process. Symbolic links/reparse points are
refused. The fallback is imported back into the keyring when it recovers;
replacement and sign-out remove obsolete fallback files. No session token is
serialized into settings. Tests use mock backends and temporary directories,
never the user's OS credential store. Unix modes require a Unix test run;
Windows fallback creation, replacement, migration and deletion are tested here.
Copying a complete existing Windows security descriptor through `Set-Acl`
required an unrelated audit privilege on this workstation. The implementation
instead builds a fresh access-only descriptor and applies it with .NET's
`SetAccessControl`; it requires no privilege elevation.

Keyring API: [keyring 3.6](https://docs.rs/keyring/3.6.3/keyring/).

## Settings and migration

`Settings.live_servers` is version 1 with stable IDs, names, gateway origins,
optional certificate pins and exactly one selection when nonempty. Missing
legacy data defaults to `Local (dev)`; Redis host, port and prefix remain in the
legacy `live` block until the development transport is retired. Redis ports
are not gateway HTTP ports, so they are never guessed into a gateway URL.
The initial development origin is `https://127.0.0.1:8080`; edit it or supply
`MOONGLOW_GATEWAY_URL` for the actual development gateway.

`Settings::migrate_live` imports the old token into the `local-dev` credential
entry and clears the in-memory plaintext only after success. Saving old
settings also imports an outstanding legacy token before serializing the
non-secret fields; storage failure refuses serialization. An existing newer
credential wins over an old legacy token. The emitted JSON/RON omits the old
`token` field. JSON round trips, migration with a mock store and desktop RON
round trips (including `keep_wsl: Some(false)`) are regression-tested.

Named legacy clients now use their unique case-insensitive name as the client
ID, with credential keys `local-dev/client/<id>`. Settings retain the name,
groups, delivery mode and rate limit, never the bearer field. Explicit migration
imports each old secret and clears it only after storage succeeds; newer stored
credentials win. Saving an unmigrated or rotated client imports its current
secret before emitting the reference. Local Live startup resolves these names
from secure storage; a missing credential fails startup instead of granting access.

The `named_client_migration_saves_references_without_plaintext` regression first
failed because JSON contained the client secret. It covers two separate client
entries, secret-free JSON, RON references and restoring credentials for runtime
use with an injected memory backend.

`LiveServers::resolve` takes `MOONGLOW_GATEWAY_URL` and
`MOONGLOW_GATEWAY_TOKEN` over saved settings, without persisting either.
A URL override uses a distinct environment identity and cannot inherit a saved
server's certificate pin or credential. `resolve_with` tests this without
mutating process-global environment variables. Unsupported schema versions
and empty/duplicate IDs fail deserialization; selection is normalized.

## Certificate discovery deadline

TLS certificate discovery has one absolute deadline spanning connection and
handshake I/O. Socket timeouts alone renew on each received fragment; a peer
sending an incomplete record slowly could otherwise hold a login worker.
The `slow_certificate_discovery_cannot_extend_the_deadline` loopback regression
trickles an incomplete TLS record and verifies bounded termination. All seven
original login/TLS tests passed, including this case.

## Options and visual proof

Options > Live opens the server list with Add, Remove and selection, name and
URL editors, connection/account status and Discord sign-in/sign-out actions.
Authentication and credential I/O run on a worker; the UI opens the returned
browser URL through egui `open_url`. An IP gateway presents its origin and leaf
fingerprint for an explicit Trust or Reject decision. Existing saved sessions
are shown without exposing their token. Sign-out deletes the local credential;
remote revocation is outside this endpoint contract. Account actions are
immediate, while Apply saves the server list. A saved session locks the URL until
sign-out. Environment overrides are displayed as development credentials and
never saved by the account controls.

`Local development (legacy)` retains the prior local tools during the P2
transition. The default panel does not start Redis, a game server or a Live
WebSocket. HAK/module cache and My AI are explicit placeholders. P2 must wire
the authenticated link to the [gateway protocol](live-gateway-protocol.md);
P7 supplies the cache implementation. Account names and roles will come from
the gateway welcome message, rather than being guessed from token presence.

The two `live_connection` kittest regressions use an injected memory credential
backend and the same fake TLS gateway as the protocol tests. They click
Add/Select/Remove, apply the model, capture egui's browser URL, deliver the
callback, verify secure token persistence and click Sign out.

Render with `cargo +1.98.0 test -p mg-ui --test screens -j 2
live_gateway_options -- --ignored` (one command). The inspected image is
`target/test-output/screens/live-gateway-options.png`: the actual Options
window, isolated settings, no game install and no real account. The list,
fields, buttons and placeholder lines fit without clipping. This is visual
verification, separate from user acceptance.

Initial implementation verification (before the M2 audit fixes): `cargo +1.98.0 test -p mg-live-link -p mg-ui --lib
--test login --test credentials --test live_connection -j 2` passed 7 login,
4 credential, 271 UI library and 2 UI integration tests (one existing ignored
library test). The manual screen test passed. `cargo +1.98.0 fmt --check` passed.
The final workspace gate passed 1690 tests with 24 ignored:
`cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2`.
`cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings` and the
separate `cargo +1.98.0 clippy -p mg-ui --test screens -j 2 -- -D warnings`
passed without warnings. An earlier shared-checkout gate stopped on three
Live authorization/receipt regressions; their owning changes resolved them
before this successful gate. No game runtime, deployed Discord integration,
macOS Keychain or Linux Secret Service acceptance is claimed.

The OKF executable was unavailable; the index was updated manually and local
source/document links checked. Keep sources, the lockfile and the one PNG as
reproduction inputs/evidence. Disposable verification logs are summarized in
the private task ledger and removed at closure; shared Cargo outputs remain
in use by concurrent work and are not deleted by this task.

## Loopback probe tolerance

The listener accepts at most 32 connections until one valid callback arrives.
Each connection has a 250 ms absolute read budget, capped by the overall login
deadline. Invalid complete requests receive 400 without redeeming a grant; idle
connections expire independently. Only a valid state/grant closes the listener
and begins token redemption. Reaching the attempt limit fails the login.

The garbage-then-real-callback regression failed with ConnectionRefused before
the fix. All ten login/TLS tests now pass, including a wrong-state probe followed
by success, an idle probe, the 32-attempt cap, the shared deadline and refusal
of a second callback after successful acceptance.

## Windows credential path handling

The hidden PowerShell ACL helper receives the directory/file path exclusively
through the child process environment variable `MG_ACL_PATH`; script text uses
`$env:MG_ACL_PATH` with `Get-Item -LiteralPath`. Escaping ASCII apostrophes was
insufficient because PowerShell also treats Unicode smart quotes as delimiters.
The Windows-only regression creates a directory containing U+2019 and exercises
fallback set/get/delete. It failed with `Permissions` before the fix and passes
afterwards. Paths are never interpolated into executable script text.
