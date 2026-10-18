---
okf_version: "0.2"
---

Documentation of Moonglow Toolset, the reimplementation of the Aurora Toolset for Neverwinter Nights: Enhanced Edition: the architecture and the record of the phases, what was deferred, the findings about the game and Aurora, the user manual, the research behind the code, the Aurora parity inventory and checklists, and the plugin author's material. The overview is in the repository's `README.md`; rules for agents are in `CLAUDE.md`. Screenshots used by the README are in `images/`. History: [log.md](log.md).

<!-- okf index: generated; edit freely, or rerun `okf index` -->

# Backlog

* [Deferred: what was left out, for a decision later](deferred.md) - What finished work left undone or unchecked, filed by area of the program (an entry's last words say which report, issue or review it came from), each awaiting a verdict - fix, add or drop; what is settled by decision or design is gathered at the end.

# Finding

* [Findings](findings.md) - What Moonglow's authors learned about NWN:EE that is undocumented or documented wrongly - item costs, creatures, areas and tiles, talk tables and 2DAs, haks, GFF fields, what Aurora writes - each saying how it was checked, where Moonglow implements it and what nwn.wiki says.

# Proposal

* [A 2DA editor and merger: a proposal](2da-editor-proposal.md) - Editing a 2DA's cells and rows inside Moonglow and taking rows from another hak's copy - what exists to build on, the decisions that are the owner's (where a table is edited, what a merger is, rows in the middle, cells by what they hold), four phases with their sizes and tests, what is not known yet, and what was built of it, with pictures.
* [Lights placed on the fly: a proposal](lights-proposal.md) - Placing a light in an area without making a custom placeable by hand first - what the game offers for lights, what was built and measured (the stock light placeables from the area's right-click menu, their reach in the client), and the open proposal - lights of any color and reach by generated content, scripted light effects, and two questions not yet measured.
* [Plugins: a proposal](plugin-proposal.md) - The decision record of Moonglow's plugins, proposed and accepted 2026-10-03 - what plugins are for, the principles, safety, the runtimes compared and why sandboxed Luau, the decisions and what was not proposed - with what shipped noted beside it. The API as it is now is in the manual and the API change list.
* [Live mode: master roadmap](live-mode-roadmap.md) - The owner-approved complete order, extending the next plan with the Live menu and logs, visual effects, deltas, multiple servers, scenes, NWNX reconnect and editor operations. Implementation ownership and merge windows are coordinated in the agent mailbox.
* [Live mode, next: implementation plan](live-mode-next.md) - The accepted plan after the two-monitor sessions: targeting, player models, command errors, labels and filters, combat, events, DM controls, live placeable properties, text and scoped tokens. Phase 1 acceptance is recorded; Phase 1b has engine and picture proof. E2 adds readable labels, a filtered and sorted object panel, combat overlays, faction colours and smooth movement. E4.4 adds a per-view Effects control, lasting effect models/particles, spell/projectile/damage cues and four-second speech bubbles; owner acceptance remains separate. E7/E8 UI work adds named-client Options, Windows keepalive control and the module Scene editor/recorder/player. The owner view review caches report-derived data and defers posing to actual redraws (with crowd timing evidence), and adds localized area-spell circles, a collapsible per-view legend; RTS group selection, queued group commands and numeric live properties/drag manipulation and intrinsic lightshaft emitters are added, with exact-target Stop Attack for NPCs, allowed players and mixed groups (the nearby-NPC radius action remains separate). Patrol targeting adds numbered routes, Loop/Run, per-NPC queue submission and observed progress. Confirmed live single-tile painting renders observed server changes without editing the module; transition brushes and full terrain snapshots remain deferred. The second owner acceptance adds closable Live targeting/tile windows and palette-driven single-tile painting with a preview, Q/E rotation and Advanced numeric inputs. Open numeric Visual Transform from the object menu. Draw live placeables and doors by reported appearance. Filter Live marker categories and factions per view. The third acceptance removes the duplicate Colour key and unifies place-action aiming, radius circles and library NPC ghosts, and expands group commands to match the single-object tools, and adds a complete Stop painting/Escape exit. Scenes gains readable steps, prominent recording and validated JSON Save As/Import/Export. A Markers hint exposes Full effects, with a player-look lasting-effect regression; Markers stays default after a heavy-effect GPU timeout. Creature animation menus validate named/custom inputs and queue NPC, permitted-player and group actions. Selection shows cached native portraits with click-to-focus. Exact Take Gold/XP and Remove Level controls also apply to eligible selections. A map notice explains hidden reported objects and Show all resets panel and kind filters. Select live groups with plain/Shift drag and configurable control keys; Ctrl+drag pans and Ctrl+click toggles one marker. Dock Live tools to the right when Live turns on. Enter action values and open tools for the clicked target. Offer player orders and explicit message, journal and Boot dialogs. Expose Run Here and both group Run destinations. Unified teleport adds all-kind/group landing ghosts, facing and a destination-area coordinate map. The second acceptance UI batch and explicit Run commands are complete with focused UI/picture proof and a clean 1,137-test workspace gate; connected-client acceptance remains separate. The master roadmap sets acceptance checkpoints; the cumulative record preserves findings, evidence, working-file ownership and next steps. The fourth acceptance record reconciles the integrated UI, gives a click checklist and records the guarded workspace/GUI checks and remaining verification gaps. Includes the 2026-10-16 shared portrait fallback diagnosis, real-report app proof and first-level NPC card/Memory menu checks.
* [Live mode over Redis: implementation plan](live-mode-redis.md) - Built Redis transport, Options, script commands, portable demo tooling and independent MCP discovery. A–E/G/F checks and curl/MCP Morning acceptance pass; records pictures, cleanup, the required game restart after Redis outages and remaining manual window/client acceptance.
* [Live mode: protocol and implementation](live-mode-proposal.md) - Live mode v1: HTTPS API, viewer/actions, NWScript reporting, private WSL demo, phase checks, reproducibility, limits and cleanup. Implemented; manual client acceptance remains.
* [Proposed corrections to nwn.wiki](wiki-proposal.md) - Proposed corrections to fifteen nwn.wiki pages that say something the game or Aurora does differently - for each, what the page says, what it should say and how that was checked - in order of how much trouble the current text causes.

# Reference

* [Live resource generations](live-resource-generations.md) - Immutable NWSync resource identity, HTTPS worker API, authenticated publisher and protocol extension.

* [Live desktop authentication and gateway GUI](live-desktop-link.md) - PKCE, TLS trust, secure credentials, shared books, AI management, validated gateway replies, verified resource downloads and UI verification.

* [Live gateway protocol](live-gateway-protocol.md) - moonglow-gw/1 frames, passive mirrors and session delivery.

* [Running a live campaign with an LLM](live-campaign.md) - Checked MCP/shell operation inputs for narration, player Roll prompts, encounters, movement, rewards, effects and animations; session freshness, waits and human approval; explicit scene-API gaps.

* [Hexxar Live windows: effects, library, skills and inventory](live-windows.md) - Atmosphere area tabs, world palette aiming (encounter-removal limitation), saved macro hotkeys, private object notes/search and conversation-client attribution. Saved GM/NPC cards, per-player NPC memory and DM review of named-client speech in Chat/Focus, with scoped pending/outcome results and current-permission checks. VFX browser, appearance confirmation and original-row restore, NPC library, player skill checks and live inventory paperdoll/backpack with automatic loading, item context menus, searchable module/standard Give Item picker, native item icons and confirmed mutations, interactive player Roll prompts with DM bonus, partial results and timeouts, runtime freshness guards, menu handoff, fake-store tests and inspected pictures. Fourth-acceptance menu structure, Scenes workflow, kind colours and per-window polish/capture checks are recorded here too. Sixth acceptance removes VFX colour and Live Shaders; native effect transforms remain.

* [Live game operations: wire contract and engine proof](live-ops-wire.md) - Centralized authorization, per-owner watch limits, sent/unknown outcomes, bounded result batches and receipts, script-version diagnostics, compact command identities, revocable polls, replay deduplication, runtime retention and unlocked command/card persistence with store/engine regressions. Automatic area subscriptions with boot/reconnect replay and authoritative watched-area status; exact-target/radius calm, skill-check NUI and connected-client checklist, terrain tiles, obstruction-retrying NPC patrols with blocked/resumed events, radius skill checks, object library, appearance and tagged VFX operations, owner regressions, forced kill, demo player switch, quest-fight factions, complete DM-action map, UI handoff, typed locals, inventory, traps, world and creature controls; game and Rust DM, player and placeable command fields, local/server permission, Unicode publisher, returned chunk JSON, delta reports/resync, API schemas, refusal codes and private engine proofs. The operation catalogue: automatic player-speech replies via native DM channel or nearest/named NPC, inventory native use/transfer with slot masks, give/equip/unequip and immediate ground drop with explicit new identity, creature animation playback, gold/XP removal and actual level reports, player-gated exact Stop Attack, player skill targets and compact Roll NUI with native bonus breakdown, loot, runtime appearances/static state, PC actions and unified cross-area teleport with bounded saved-state recreation, destination bounds and identity handoff; native object animation replacement and reset; optional server-authoritative faction catalogue and DM private feedback; native VFX transforms and the negative colour propagation proof; retired colour assets and HAK removed.
* [Live game events: wire contract and engine proof](live-events-wire.md) - Chat, combat, spells, lasting VFX, projectiles and server-log wire fields with private engine proof; client lifecycle pending and general instant VFX deferred. Includes proven silent AI channel tagging and default display filtering. Module speech/name/tag and log bytes now share one Unicode receive decoder, with native/API mixed-encoding proof. Creature portrait base resrefs are reported for Selection/chat. Includes mixed UTF-8/cp1250 speech and native JSON byte-loss diagnosis (B17).
* [Agent and CLI discovery protocols](agent-protocol.md) - Portable describe/call contracts, conditional schema cache and persistent calls, JSONL event watch and [latency evidence](agent-protocol.md#shell-and-stream-latency-measurements-2026-10-15), flat schemas, legacy descriptors, gateway MCP with environment-only AI credentials and shared TLS, CLI safety controls, explicit Allow LLM consent, hot discovery/presence and MCP heartbeats, scoped Live clients/audit, the owned WSL service keepalive and an LLM recipe with JSON input examples for discovery, event cursors, target selection, commands, waits and observed completion.
* [Moonglow Toolset: Architecture and Record of the Phases](PLAN.md) - What Moonglow Toolset reproduces of Aurora, the landscape, its principles, crate architecture and key designs, a record of the phases it was built in with the tests that hold each, the testing strategy and licensing. The plan it began as, kept as the description of what was built.

# Report
* [Live GUI performance: measurement phase](live-gui-performance.md) - Native debug/dev-opt/release crowd and lasting-effects replay, repaint causes, frame stages, startup and bounded memory attribution; reusable guarded measurement helpers, ranked candidates, measured implementation follow-up, and post-sixth macro/history optimizations.
* [Live game and wire performance: measurement phase](live-game-performance.md) - Private 218-object engine timings, looks/JSON/event-buffer costs, full/delta bytes, NPC heartbeat and CPU comparisons, Rust replay, GFF look decoder repair, bounded cold-start look refresh, large-look and VM-budget isolation regression, post-acceptance in-place collections, measured event/collection/store optimizations and post-acceptance regression comparisons; connected-player limits explicit. Immediate chat and condition-variable event watches include same-host before/after latency, loss/reset and nearby context.
* [Immediate chat and agent watches](live-events-wire.md#immediate-chat-and-agent-watches-2026-10-15) - Fast chat envelope, player/DM filters, reported proximity, long-poll wakeups and explicit history loss.
* [Live optimization plan](live-optimization-plan.md) - The order in which the measured game, wire and Moonglow costs are cut, with the gate every step passes.
* [Audit: the documentation at 1.21.3](docs-audit.md) - An audit of all 46 documents in docs/ against the code at 1.21.3 - per document, whether it is correct, what was wrong and fixed, what is wrong and left, and whether it is still worth keeping (keep, update, trim, deprecate) - with the decisions it leaves to the owner.
* [Review: test necessity, test speed and codebase cleanliness](test-review.md) - A dated record (2026-10-11, version 1.21.1) of where the time between a finished change and a release went, the decisions taken about it, the measurements before and after, and the state of the code by size and shape. How the tests run today is in the plan's §7 and CLAUDE.md.

# Subdirectories

* [manual](manual/index.md) - The user manual, a chapter per file, also built into the program (Help › User Manual, which shows each chapter without its frontmatter) and shipped beside it. (18 Manual Page, 1 Manual)
* [parity](parity/index.md) - Parity with Aurora: the generated inventory of every Aurora form and control, and the control-by-control checklists of Moonglow against it. (1 Reference, 1 Checklist)
* [plugins](plugins/index.md) - For plugin authors: the starting page, the API's changes, the example plugins (`examples/`) and the editor type file (`types/moonglow.luau`). (1 Changelog, 1 Guide)
* [research](research/index.md) - Research behind the code: briefs on NWN:EE's formats, rendering, tilesets and models, what Aurora computes and how that was measured, prior art, and builders' pain points. (16 Research Note, 1 Reference)

* [VFX transforms and colour proof](live-ops-wire.md#vfx-transforms-and-colour-fourth-acceptance-b10) - Native transform readback and the negative attached-VFX colour experiment.

* [Session factions](live-ops-wire.md#session-factions-fourth-acceptance-b15) - Offline reserves, session-only labels and native directional reputation controls.

* [Custom live faction IDs](live-ops-wire.md#fifth-acceptance-custom-module-factions-c8) - Native and Rust proof for arbitrary existing repute.fac rows, including test and reserved session factions.
* [Confirmed DM Drop](live-ops-wire.md#fifth-acceptance-dm-inventory-drop-c1) - Non-droppable NPC potions, plot protection, native ground readback and disabled-action hover reasons.
* [Live item palette tree](live-windows.md#fifth-acceptance-c3-the-actual-item-palette-tree) - Standard/Module categories directly from palette nodes, search and icons.

* [Fifth acceptance GUI](live-windows.md#fifth-acceptance-gui-2026-10-14) - Portrait resource proof, shader eligibility, examples, destination maps and effects follow-ups.

* [Focus chat routing](live-events-wire.md#focus-chat-recipients-and-speaker-position-2026-10-14) - Explicit recipient, audience, speaker position, native callback proof and connected-client limits.

* [Focus dialogue events](live-events-wire.md#focus-opt-in-dialogue-node-events-2026-10-14) - Native Actions Taken collector, speaker/listener/node fields, Polish bytes, read_chat scope and exact coverage gaps.

* [Allow LLM consent and presence](live-windows.md#allow-llm-consent-and-presence-ui-2026-10-15) - One immediate consent control in the area toolbar, Live menu and Options, recent-client status and event-driven expiry; [connection guide](manual/19-live-mode.md#allow-llm).

* [Focus watch lists and conversation routing](live-windows.md#focus-window-implementation-2026-10-14) - Saved per-module players/NPCs/groups, replies, alerts and capture limits; [user workflow](manual/19-live-mode.md#focus-watched-conversations-and-events).

* [Inventory context confirmation](live-windows.md#inventory-context-confirmation-sixth-acceptance-d2) - Adjacent Drop/Take confirmation, inspected pictures and native potion proof.

* [Sixth acceptance GUI](live-windows.md#sixth-acceptance-ui-2026-10-14) - Shared area tabs, Focus members, separate Legend, follow across areas, To DM, reported NPC equipment, server factions, grouped capture clipboard/removal and visible saved macros; final checks and retention.

- [Stable Live look identity: H1 proof and sparse delta contract](live-game-performance.md#stable-appearance-hashes-audit-h1-2026-10-15)

- [NWNX Redis restart: H2 native recovery proof](live-mode-redis.md#patched-redis-restart-proof-audit-h2-2026-10-15)

- [Full-scale Live instruction guard: H3 coverage and headroom](live-game-performance.md#full-scale-instruction-guard-audit-h3-2026-10-15)

- [Durable Live command identity: H4 storage and restart rules](agent-protocol.md#durable-command-ids-audit-h4-2026-10-15)

* [Live atmosphere operations and native proof](live-ops-wire.md#atmosphere-audit-h6-2026-10-15) - Weather, fog, lighting, skybox and audio.

* [Additional live spawn kinds](live-ops-wire.md#additional-spawn-kinds-audit-h8-2026-10-15) - Native blueprint creation, trigger geometry and encounter removal limit.

- [Automatic Live dialogue capture](live-events-wire.md#focus-dialogue-node-events-automatic-live-build-capture-h11) - Output-only instrumentation and native coverage.

- [Agent command authorship](agent-protocol.md#command-authorship-audit-h15) - Authenticated labels, causal events and native proof.

- [Audit H final native guard and retention](live-game-performance.md#audit-h-final-native-guard-and-handoff-2026-10-15) - Final headroom, checks, limits and cleanup.

- [Large Live GUI stress replay](live-gui-performance.md#stress-replay-t1-t3-2026-10-15) - Completed 30-minute five-area/1,000-object baseline, memory and startup limits, separate 12k-event control, and matched Chat/Log optimization measurements.

- [Session-sized native/HTTPS stress](live-game-performance.md#session-stress-preparation-and-scope-2026-10-15) - Five areas, 1,000 objects, native look surrogates, real agent clients, bounded replay, rotating-log guards and explicit timing limits.

- [Session stress: staggered NPC refresh](live-game-performance.md#s3-spread-periodic-npc-look-refreshes) - Before/after report gaps and native appearance regression.

- [Durable Live event queue: stream replay, overflow, crash recovery and proof](live-mode-redis.md#durable-event-queue-2026-10-16) - Per-instance XREADGROUP/XACK, native bounded XADD and persistence requirements.

- [Final session stress: limits and remaining transport gap](live-game-performance.md#final-session-stress-result-and-supported-limits) - Thirty-minute native/service measurements, instruction headroom, replay handoff and retained evidence.

# Implementation

* [Prepare a module for Live](live-build.md) - Shared embedded Rust builder, CLI/GUI, byte-preserving dialogue instrumentation, deployment settings and private proofs.

* [Live library for headless consumers](live-library.md) - Dependency boundary, passive mirrors, gateway transport and shared scoped HTTP routing.

* [Live gateway feasibility prototypes](live-gateway-prototypes.md) - Verified clean-client NWSync cache and opt-in native chat translation with fallback, channel proofs and latency measurements.

* [Live chat translation pipeline](live-translations.md) - Immediate bridge hook, bounded translation scheduling, host engine interface, NUI preferences and native acceptance.
