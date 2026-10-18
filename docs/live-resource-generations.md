---
type: Reference
title: Live resource generations
description: Authenticated module publishing and cancellable HTTPS resource downloads for desktop job workers.
tags: [live, gateway, nwsync]
generated: { by: astra-2, at: 2026-10-18T00:00:00Z }
sources: [live-gateway-prototypes.md, live-library.md, live-desktop-link.md]
---

# Live resource generations

The authored [clean-client prototype](live-gateway-prototypes.md#clean-client-nwsync)
provides the verified NwSyncCache layer and read-only module. P7 adds HTTPS,
immutable descriptors, atomic activation and authenticated publishing. No game
assets or local install paths are part of this implementation or its fixtures.

## Descriptor and wire extension

`mg_live::resource_files::ResourceGeneration` contains:

| Field | Meaning |
|---|---|
| `boot` | Exact game boot, nonempty and at most 256 bytes |
| `module_manifest` | Module-only manifest SHA-1; also the module content version |
| `haks_manifest` | Pinned hak repository manifest SHA-1 |
| `tlk` | Map from custom TLK resource names to decoded SHA-1 digests |
| `module_url` | `/files/v1/`, resolved against the authenticated gateway origin |
| `haks_url` | Administrator-selected HTTPS repository URL with trailing slash |

Digests are exactly 40 lowercase hexadecimal characters. Generation identity is
SHA-1 over UTF-8 compact JSON `[boot,module_manifest,haks_manifest,tlk]`; TLK keys
are lexically sorted. URLs are locations, not content identity. The standard hak
manifest includes TLK resources; there is no separate unverified TLK download.

Proposed additions to the external protocol specification:

* `welcome.nwsync.generation` optionally contains this complete descriptor.
  Existing `haks_url`, `haks_manifest`, `module_manifest` fields remain unchanged.
  Old frames without `generation` still decode. Only DMs with `read` receive it.
* Request `files` with `a:{}` returns the current descriptor in `res.r`.
  Return `forbidden` for AI/non-read sessions and `unavailable` while no complete
  publication matches the current game boot. Poll after boot or resource updates.
* Authenticated `GET /files/v1/generation` returns the same descriptor; manifest
  and blob routes are `manifests/<sha1>` and `data/sha1/aa/bb/<sha1>`, relative to
  `/files/v1/`. Every request requires a current DM bearer with `read`.
  AI credentials and player identities never authorize these routes. Origin is
  refused. Responses use private/no-store caching; there is no unauthenticated latest.
* Downloads pin both hashes from one descriptor, never independently follow two
  `latest` endpoints. On game boot change the UI must wait for a matching generation
  before drawing its Live geometry. The last four published descriptors may finish
  existing downloads; a newer `files` response supersedes an unfinished job.

No protocol/mirror version bump is needed for the additive optional field. Resource
identity changes independently of live mirror serialization.

## GUI worker API

1. Read `snapshot().welcome.nwsync.generation` or use
   `LinkService::request("files", json!({}))` and decode its successful result.
2. `Generations::select(&descriptor)` records the desired identity. Retain the old
   module while loading within the same boot. On a boot change, call `clear()`
   immediately to reject stale jobs and remove incompatible geometry.
3. On a job worker, call `ResourceClient::new(gateway, token, private_cache, limits)`
   then `download(&descriptor, &cancel, &progress_callback)`.
4. `Progress { bytes, files, current }` reports network bytes (compressed where
   applicable), completed transfers and a generated repository-relative path.
   Cached files are reverified but do not count as downloads. Callbacks run on
   worker threads; send updates to the GUI rather than touching widgets directly.
5. `Cancel::cancel()` is thread-safe. Checked before requests and each body read;
   socket reads have a five-second inactivity cap, whole requests a 30-second cap.
   A failed/cancelled job retains only complete SHA-1 verified blobs for retry.
6. `download` returns `ReadyGeneration` only after both manifests, all blobs, TLK
   identities and read-only module materialization succeed. `Generations::activate`
   replaces one Arc atomically and refuses a stale job after a newer selection.
   Clear renderer caches after a successful switch, then use `module.stack_onto`
   with the normal game resources. Access it through `ReadyGeneration::module()`. The hak lookup uses `LayerClass::NwSync`.

The saved `prepared.json` is an atomic recovery hint, not permission to activate an
old boot. Re-run download against the selected descriptor to verify cached data.
Module folders are named by generation hash; decoded blobs and manifests by SHA-1.
There are at most four parallel transfers. Limits default to 16 MiB/manifest,
15 MiB/decoded resource, 100,000 entries/manifest and 2 GiB decoded bytes across
both manifests. Packed bodies are bounded by the declared decoded size plus 1 MiB;
decompression stops at the declared size. Redirects and non-HTTPS URLs are refused.
Gateway credentials never accompany hak downloads, including at the same origin.
The gateway's saved certificate pin applies only to that origin; other repositories
use WebPKI. The caller owns the cache directory; concurrent processes must not use
it as a writable shared cache. Cache pruning and the GUI job/progress window are
separate UI integration work; this API never deletes active modules or user data.

## Publisher host API

`Config.publisher: Option<Arc<Publisher>>` enables the routes and welcome extension.
Call blocking `Publisher::refresh(module_path, boot, haks_url, hash, manifest_bytes)`
on a background worker. It verifies the hak manifest, preserves the module's IFO
and hak order, drops NSS/NDB/GIC/NCS, and publishes all content before changing the
current descriptor. Content hashes detect edits including same-size module changes.
The host supplies trusted paths/URLs; requests cannot select filesystem paths.
`disable()` removes the publication index; cached files alone never authorize a URL.
The disk repository is retained for reuse, while the authorization index retains at
most four generations. Host operators manage repository retention while offline.

## Verification

Use Rust 1.98.0, `CARGO_INCREMENTAL=0`, nonexistent `NWN_ROOT`, at most two jobs:

```text
cargo +1.98.0 test -p mg-module --test nwsync_clean -j 2
cargo +1.98.0 test -p mg-live-link --test resources -p mg-gateway --test files -j 2
```

Fixtures author every resource. The local TLS server checks four-worker concurrency,
absence of credentials on hak requests, full fetch, verified-file resume, Cancel,
SHA-1 tampering, oversized body rejection and whole-generation/stale-job switching.
Gateway tests check live DM authorization, AI/player refusal, hash allowlisting,
module refresh, boot binding and welcome/files agreement. No installed game, Redis,
external server or container is needed. The GUI and real-server acceptance remain
separate; passing backend tests does not establish renderer invalidation or visuals.
