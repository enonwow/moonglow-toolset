---
type: Reference
title: Live desktop authentication and connection settings
description: Desktop PKCE login, gateway TLS trust and per-server credentials.
tags: [live, gateway, authentication]
generated: { by: codex, at: 2026-10-18T09:57:30Z }
sources:
  - ../crates/mg-live-link/src/lib.rs
  - ../crates/mg-ui/src/settings.rs
---

# Live desktop link

`mg-live-link` owns desktop authentication. It never opens a browser or starts
a game server. The GUI supplies the gateway origin and optional SHA-256 leaf
certificate pin, starts `Login` on a worker, opens `browser_url`, then calls
`finish` on that worker. The consumed attempt closes its listener on success,
invalid callback, timeout or drop. A slow callback is bounded by the same deadline.

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
login/TLS tests pass, including this case.
