---
type: Reference
title: Live chat translation pipeline
description: Opt-in gateway translation, immediate Redis transport, language preferences, bounded scheduling and native acceptance measurements.
tags: [live, gateway, chat, translation]
generated: false
sources:
  - ../crates/mg-gateway/src/translation.rs
  - ../crates/mg-gateway/src/translation_tests.rs
  - ../crates/mg-live/src/bridge.rs
  - ../tools/live/nwn/mglive_xl.nss
  - ../tools/live/nwn/mglive_xllang.nss
  - ../tools/live/test_chat_translation.py
  - ../crates/mg-live/src/store_translation.rs
  - ../tools/live/test_translation_config.py
  - ../tools/live/test_translation_budget.py
  - ../tools/live/test_translation_budget_unit.py
  - ../tools/live/fixtures/events/mget_xlbudget.nss
---

# Live chat translation pipeline

This is the public P8 implementation of the [interception prototype](live-gateway-prototypes.md).
Rebuild the prepared module with `mg live-build` to include the new language
entry/NUI scripts; the generated server configuration already subscribes to
`<prefix>.xl`. Keep `MGLIVE_XL` off until the host pipeline is attached.
`MGLIVE_XL` remains off by default. The trusted host explicitly synchronizes the
module switch and command prefixes through `LiveStore::configure_translation`. The game chooses listeners at speech time, delivers
same-language speech immediately, and owns the 0.9-second original-text fallback.
Its one-use request record prevents duplicate or late responses from delivering a
second line. Delivery retains the NWNX hook re-entry guard.

## Host API

`mg_gateway::Translator` is an object-safe, asynchronous private-engine boundary:
`translate(text, from, to) -> TranslationFuture` (the future resolves to
`Result<String, String>`). Implementations must be cancellable
and must not block the executor. Credentials and engine configuration remain in
the host. `Extensions::translator()` defaults to `None` and can return the host's
`Arc<dyn Translator>`.

Create `TranslationPipeline::new(engine, config, language_book_path)` inside a
Tokio runtime, outside the store mutex. Keep an `Arc` for panel configuration and
attach it using `core.with_store(move |s| s.set_translation_hook(Some(pipeline)))`.
The existing `CoreHandle` API needs no new transport method. Setting the hook to
`None` returns an immediate miss; the game still guarantees original delivery.
`configure(TranslationConfig)` changes enabled channels, per-player requests per
minute, command prefixes and the overall switch. Default configuration is off;
the default limit is 60 requests per player per minute. `/` and `!` always remain
protected, alongside configured prefixes. For modules with additional native
command prefixes, the host calls `store.configure_translation(enabled, prefixes)`
after installing/configuring its pipeline. `prefixes` contains at most 32 ASCII
graphic initial characters. This is a trusted Rust API, not an HTTP/agent game
operation; reserved `MGLIVE_*` writes remain forbidden. Disabling a channel rejects queued and
in-flight results before delivery.

## Transport and bounds

The bridge subscribes to `<prefix>.xl.req` beside `.event`, decodes the same
explicit text-byte companion used by chat events, and invokes `TranslationHook`
immediately. It does not wait for a report, event stream flush or inbox replay.
A separate bounded writer publishes `<prefix>.xl`; it never retries old replies.
Speech replies contain `{req, slot, to:{lang:text}}` or `{req, slot, miss:true}`.
The prototype's `slot` is retained as a lookup hint; `req` remains authoritative.

The pipeline admits at most 128 queued and four running requests. Tells and talk
precede other channels; shout is last. FIFO deadlines break ties. The 650 ms
deadline starts at pipeline admission and includes queue time; expired queued
requests never call the engine. Target languages run concurrently. Missing or
rejected target languages receive original text in the game, even when another
language succeeds. The whole ASCII-escaped reply stays within the game's 16 KiB
limit; excess target translations are omitted and therefore fall back.

The LRU is keyed by exact text and source/target language pair, with at most 1024
entries and 1 MiB of text accounting. Player rate buckets expire after a minute
and have a 4096-player cap. The bridge's response queue holds at most 128 replies.
Overflow, unavailable engine, cancellation and invalid requests fail to original
delivery; they never turn into a command retry.

Only plain text segments go to the translator. Emote delimiters and exact colour
tags stay in the pipeline while emote contents are translated. Unbalanced or
unsupported markup is refused. Results must preserve the exact marker sequence,
must not start with command prefixes, contain newlines, exceed 1024 characters or
grow beyond four times the input length plus 32 bytes. The game independently
checks output again. The cache stores only validated translations and cached
results are checked against current configuration.

## Player language

The module installs a chained OnClientEnter handler. When enabled it requests
the saved language by `GetPCPublicCDKey` and opens the NUI language picker.
`/language` reopens it. Six Latin-script choices are exposed: English, Polish,
German, French, Spanish and Italian; the picker labels are ASCII. No arbitrary
engine language list is exposed to clients.

Preferences use the same immediate lane with `kind:"language"`, character ID,
public key and a one-use request ID. A reply is accepted only by the matching
character request. The gateway saves a validated map keyed by public CD key,
using a synced temporary file and atomic rename before confirming a selection.
The book is capped at 4096 keys / 1 MiB on load; disk work runs on blocking
workers outside the Live mutex. Keep this file in host-private persistent data.
The public key is not included in responses or the durable chat event stream.

## Reproduction

Use Rust 1.98.0, `NWN_ROOT=/nonexistent`, `CARGO_INCREMENTAL=0`, at most two jobs:

```text
cargo +1.98.0 test -p mg-gateway --lib translation -j 2
cargo +1.98.0 test -p mg-live --lib immediate_translation_lane -j 2
cargo +1.98.0 build -p mg-gateway --example translation_engine -j 2
```

The native acceptance uses the existing private WSL `tools/live/test-events.py`
helper (also wrapped by `test-events.sh`) and its scratch module/user directory. Set `MGLIVE_XL_BRIDGE` to the built
`target/debug/examples/translation_engine.exe` and `MGLIVE_FIXTURE_TEST` to a
built guarded `private_fixture` test executable. Run `--phase chat_translation`
and `--phase chat_translation_stress`. The Python harness only observes requests
and native OnConversation deliveries in this mode. The Rust example supplies a
fake `Translator` to the real bridge and production pipeline, with delays of
0, 200 and 600 ms. Its higher fixture rate limit permits the burst assay.

The helper verifies channel audiences, exactly-once delivery, the module's
default-off switch, malformed output, fallback and NUI window JSON. NPC/PC/DM
surrogates are explicitly fixture-only. A passing headless run does not prove
connected-client NUI rendering, positive player tokens, real button clicks or
native player-ID tell echo. Those require a connected client and remain a
separate acceptance point.


## Native results (2026-10-18)

Both the ordinary and existing stress fixture passed through the production
bridge and pipeline. Each delay used 60 authored lines / 180 held deliveries;
latency starts inside the game interception and ends in the listener's native
OnConversation callback. Same-language and NPC original deliveries are excluded
from the translated-share denominator. No duplicate listener callback occurred.

| Load | Fake engine delay | Captured lines/s | p50 ms | p95 ms | Translated | Original |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| normal | 0 ms | 18.74 | 31.917 | 34.653 | 180/180 (100.0%) | 0/180 |
| normal | 200 ms | 18.72 | 227.207 | 265.536 | 180/180 (100.0%) | 0/180 |
| normal | 600 ms | 18.79 | 663.439 | 672.766 | 12/180 (6.7%) | 168/180 |
| stress | 0 ms | 14.53 | 26.904 | 54.743 | 180/180 (100.0%) | 0/180 |
| stress | 200 ms | 14.07 | 242.415 | 564.600 | 180/180 (100.0%) | 0/180 |
| stress | 600 ms | 14.12 | 676.863 | 1004.157 | 12/180 (6.7%) | 168/180 |

The stress fixture has five areas, at least 1000 reported objects and the existing
combat/event driver. At 600 ms, four running request slots cannot sustain this
burst rate: queued work consumes its deadline and falls back to original text.
These figures measure scheduling and transport with a fake engine, not linguistic
quality or the throughput of a deployed translator. Raising concurrency is not
proven safe for a real CPU-bound engine by this test.

The ordinary run observed 975 native callbacks and the stress run 910. Both
runs' module/compiler/server/plugin hashes are preserved in the helper reports.
The 0.9-second game fallback is a scheduled callback, not a hard wall-clock
delivery bound: the busy game thread reached 1004 ms p95 in the 600 ms cohort.
The NUI JSON exposes all six labels and the chained OnClientEnter script is
installed in the engine. Connected-client rendering/click acceptance remains
unverified. Both private servers and both bridge children exited after proof.

Exact native inputs (SHA-256):

| Input | SHA-256 |
| --- | --- |
| Server | `d6d952826b671fc62c376dca912e87f1aa45d4aa81c620e912ceb4d8e051111d` |
| Compiler test executable | `2d2f5c7f786c21de891da3849625b65ca2235a7e1ea6487ffebd881b11806595` |
| NWNX Core | `ee0d0bddb8fd434291954ad2d9868e734d67e0538113b49a66053f1dc16c2865` |
| NWNX Redis | `7bb283967d01a65952c03204b0a16b26366056148f34da0f2a9295075816d30a` |
| NWNX Chat | `068923bc4988c45ffa340243dc7ec9fda59a397ce174b16e3505f8749769392b` |
| Normal fixture module | `37bbed075e4f1246461026e8a990d923a367a1baf705db2b56e473b1ff252c3a` |
| Stress fixture module | `53abd532e6ab44f1b24a2fdbfac47742ad82a9a6752f201e0e75c51125f8f54d` |


## Rust verification

The focused run `cargo +1.98.0 test -p mg-live -p mg-gateway --lib translation -j 2`
passed nine tests: eight pipeline tests and an actual RESP bridge regression that
receives and replies to `.xl.req` before any state report. Pipeline coverage
includes preserved emotes/colour markup, untrusted output, per-pair cache, bounded
LRU, per-player quota, priority, expired queued work, parallel language deadlines,
in-flight disable and language persistence across restart.


## Ordered language writes

Blocking-pool workers can acquire the language-book mutex in a different order
from request arrival. Without an ordering check, an older choice could overwrite
a newer saved choice. Each admitted preference request now receives a monotonic
revision before entering that pool; storage rejects a write older than the last
committed revision for that player. Other players have independent revision
checks. The revision map is bounded by the same 4096 saved keys and needs no
restart persistence because outstanding workers belong to the old process.

`reversed_language_workers_cannot_overwrite_a_newer_choice` applies writes in the
opposite order, verifies another player's earlier revision still saves, then
reopens the book and checks the newest choice. All nine pipeline unit tests pass
after this follow-up (ten distinct translation tests including the bridge test).
The chat scheduler, game scripts and measured transport path are unchanged by
this persistence fix; the native measurements above belong to `84b30a3`.


## Final verification snapshot

P8 source checkpoints are `84b30a3`, the language-ordering fix `61d5407`, and
Clippy-only decoder placement `c87fe5c`. The owner cancellation hook is `4c06993`;
the six already-committed MCP dependency lock entries were confirmed with
`cargo +1.98.0 metadata --locked --format-version 1 --no-deps` and committed in
`9e469ec`. No push was performed.

All Cargo runs used Rust 1.98.0, a nonexistent `NWN_ROOT`,
`CARGO_INCREMENTAL=0` and at most two compiler jobs. Shared-workspace verification
was coordinated as one run rather than competing full builds.

| Gate | Result |
| --- | --- |
| `cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2` | The first run passed 1817 tests, zero failures, 24 ignored. The repeat after the language-ordering fix stopped during compilation: concurrent GUI work declared `live_resources` before creating its module file. No tests executed in that repeat. |
| `cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings` | Passed on the frozen P8 checkpoint `c87fe5c`. |
| `cargo +1.98.0 fmt --check` | The final snapshot found formatting changes in concurrent GUI tests `gateway_resources.rs` and `support/resource_host.rs`. |
| `cargo +1.98.0 test -p mg-live -p mg-gateway --lib --bins --tests -j 2` | Final P8 source passed: 312 tests, zero failures, four ignored manual tests. |
| `cargo +1.98.0 clippy -p mg-live -p mg-gateway --all-targets -j 2 -- -D warnings` | Passed. |
| `cargo +1.98.0 fmt -p mg-live -p mg-gateway --check` | Passed. |

The four ignored tests require external TLS/Redis, WSL service lifecycle or
engine snapshot fixtures. The two native translation proofs above were run
separately. The final scoped pass is not a claim that the moving full workspace
is green. The GUI owner must repeat its full gate after that work stabilizes.
OKF tooling was unavailable; frontmatter, source paths and link targets were
checked directly.

Ten task-owned temporary logs and proof-runner build files were removed after
preserving these results (31,393,053 logical bytes). The current native fixture,
its reports, per-listener observations and compact verification records remain
for review/reproduction. Shared build caches were preserved. Concurrent compiler activity prevents attributing changes in free
disk space to this cleanup.

## Trusted game configuration

`configure_translation` stores desired settings, wakes the publisher, and emits
`translation:{id,enabled,prefixes}` on the existing boot/prefix-bound `.cmd`
envelope. The revision is 32 lowercase hex characters; enabled is 0 or 1. The
fresh-report/reconnect gate applies to this envelope as it does to commands.
The game validates the whole configuration before setting `MGLIVE_XL`,
`MGLIVE_XL_COMMANDS` and its revision local. Reports echo `translation_config`;
the host retries until a fresh report acknowledges the current revision, then
stops sending it. Changed settings, a new boot or endpoint invalidate the ack.
Identical panel settings retain the revision. There is no agent-visible operation
that can change this state, and mirror stores refuse the host API.

Rebuild prepared modules with `mg live-build`. This additive protocol retains
script version 2; older scripts cannot acknowledge the control field. The module
is still off when no host configuration was supplied. The store regression
`trusted_translation_configuration_retries_until_reported_and_reapplies_on_boot`
covers retry, revision matching, boot changes, disable and invalid prefixes.

Native configuration proof (private scratch WSL helper, **needs WSL run**):

```sh
python3 tools/live/test-events.py --phase translation_config
```

This verifies default-off, a live prefix/enable update, wrong-boot/invalid-prefix
rejection and disable without restart. The sandbox only checked Python syntax;
no NWScript compilation or native success is claimed for this M3 change.

## Bounded audience enumeration

Tells inspect their target and speaker only. Other channels inspect at most 128
clients, filtered by range, party or DM audience. Public speech adds listening
NPCs through native spatial/creature filtering, using the server hearing radius
for talk/whisper and the current area for shout. Placeables, items and unrelated
object types no longer consume a NWScript loop iteration per chat line. Player
hearing-distance overrides remain checked in the bounded client loop.

At most 256 creatures are inspected. If the audience exceeds the cap, interception
returns false before creating a pending slot or sending any manual line, leaving
the entire line to normal game delivery. This bounds VM work without dropping
listeners or partially delivering the line. Oversized client/audience lists have
the same fallback. The native spatial query itself still depends on engine data
structures; this bounds NWScript work, not necessarily native CPU cost.

The authored stress proof compares native NWNX instruction counts before and
after adding 4096 placeables, then adds 288 nearby NPCs and requires native
fallback. Talk, whisper, shout, tell, party and DM channels are measured; tell,
party and DM must remain independent of unrelated NPC density. Every capture
must use less than half the configured VM instruction limit; the test never
raises that limit. A failure preserves the helper report for diagnosis.

Run on the private WSL helper with its scratch user/module directory:

```sh
python3 tools/live/test-events.py --phase chat_translation_budget
python3 tools/live/test-events.py --phase chat_translation
python3 tools/live/test-events.py --phase chat_translation_stress
```

All three **need WSL run** for this M3 change. No new p50/p95 or native instruction
figures were collected in the sandbox. The earlier P8 measurements above are
historical evidence for their recorded checkpoints, not acceptance of this edit.

## M3 verification snapshot

The shared working tree based on `328953c` was checked with Rust 1.98.0,
`NWN_ROOT=/nonexistent`, `CARGO_INCREMENTAL=0`, and at most two jobs. The new
closed-publish-gate regression, timestamped draft approval regression and both
trusted-configuration tests passed, as did the existing publish/revocation race.
The focused configuration rerun passed both ack/retry and endpoint/wakeup tests.

```text
cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2
cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings
cargo +1.98.0 fmt --check
```

The full test command stopped after 465 passing tests, one failure and four
ignored tests. In mg-live itself, 238 passed, one failed and four were ignored.
The failure was `api::tests::real_tls_status_command_and_stop`: Windows curl's
Schannel backend returned `SEC_E_NO_CREDENTIALS` in the sandbox. Later workspace
test binaries were not executed, so this is not a full green workspace gate.
Repeat the exact test command outside that restriction. No TLS test was skipped
or weakened to obtain a passing result.

Clippy initially identified three existing test calls still passing `&mut` to the
now-read-only batch builder. Removing those redundant mutable references is part
of the handoff fix. The final all-targets Clippy command and workspace format
check both passed. The native configuration and instruction-budget phases above
remain **needs WSL run**; these Rust checks do not prove NWScript compilation,
real-client audiences, instruction counts or new latency measurements.

## Compiled instruction-budget measurement

The native M3 follow-up supplied by the runner passed `translation_config`,
`chat_translation` and `chat_translation_stress`. The first
`chat_translation_budget` baseline returned all-zero counters and capture status;
that run does not establish an instruction budget or new latency percentiles.

The runner had called NWNX through `ExecuteScriptChunk`. NWNX Core disables those
functions in chunks by default (`NWNX_CORE_ALLOW_NWNX_FUNCTIONS_IN_EXECUTE_SCRIPT_CHUNK=0`;
[upstream configuration](https://nwnxee.github.io/unified/dark/group__nwnx.html)).
The measurement now runs as the compiled `mget_xlbudget` fixture via `run_script`.
Chunks only set its input locals and read its result. The result carries the
channel and unique sample label; absent, stale, zero and over-budget measurements
fail explicitly. No NWNX permission or VM instruction limit is raised.

Local regression: `python -B tools/live/test_translation_budget_unit.py` (three
unit tests). The fixture builder already copies and compiles every events NSS
entry point. `tools/live/test-events.sh` is pinned to LF by `.gitattributes`.

Native acceptance still **needs WSL run**, from the repository root, using the
same configured private helper and scratch directories as the preceding run:

```sh
MGLIVE_PWSH="<PowerShell executable in WSL notation>" \
NWNX_ROOT="$HOME/nwnx-redis-reconnect" \
python3 tools/live/test-events.py --phase chat_translation_budget
```

Use the PowerShell executable configured in `tools/live/local.ps1`. Expected:
all six baseline channels capture, the 4096-placeable comparison stays within
5000 instructions of baseline, the 288-extra-creature talk case falls back,
and tell/party/DM remain bounded. Every sample must use less than half the actual
VM limit. Retain the helper's measurement report; do not infer instruction counts
from these Python unit tests.
