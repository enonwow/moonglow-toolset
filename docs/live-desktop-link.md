---
type: Reference
title: Live desktop authentication and connection settings
description: Desktop PKCE login, gateway TLS trust and per-server credentials.
generated: 2026-10-18
sources:
  - ../crates/mg-live-link/src/lib.rs
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

Keyring API: [keyring 3.6](https://docs.rs/keyring/3.6.3/keyring/).
