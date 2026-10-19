---
type: Proposal
title: 'Live mode: the whole roadmap'
description: Everything asked for in the conversation that built live mode, in one ordered roadmap - what is done, what is in the current plan, and the new work - visual effects in Live, a Live menu, live logs (chat, combat, server, commands), reports as deltas, several servers, scenes and macros, the NWNX_Redis reconnect fix, editor operations, the web view and the owner's own steps.
tags: [proposal, live, roadmap, vfx, logs, menu, dm, llm]
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-12T16:30:00Z }
---

# Live mode: the whole roadmap

> **Superseded in part (2026-10-19).** The desktop no longer has a local Live mode: Moonglow reaches a game server only through a Live gateway. The local HTTPS API, its token and named clients, the session descriptors, `Allow LLM` consent, the Options › Live API/Redis/WSL/Logs controls and `moonglow-mcp`'s local discovery (`--descriptor`, `--sessions`) described here were removed; read it as the history of that design. Current behaviour: [Live mode](manual/19-live-mode.md), [desktop contract](live-desktop-link.md), [gateway protocol](live-gateway-protocol.md).

Status: accepted by the owner on 2026-10-12 ("everything from this
conversation has to be done"). This is the master list. Detailed specs
live in [live mode over Redis](live-mode-redis.md) (done),
[live mode, next](live-mode-next.md) (in progress) and, for the new
items, in this document. Work on branch `live-mode` with the same rules:
never `develop`, never push, cargo through `tools/live/check.ps1`, the
server only in WSL and private, stop on a wrong fact, a picture before
and after every visual change, one commit per phase.

## Order

| # | Work | Spec | State |
|---|---|---|---|
| 1 | Protocol, store, HTTPS API, Redis link, Options › Live, scripts, WSL demo, `moonglow-mcp` | [Redis plan](live-mode-redis.md) A–G, F | done |
| 2 | Targeting cursor, player model, clear errors | [next](live-mode-next.md) Phase 1 | done (`e6d9a03`) |
| 3 | Attack that fights, dead lie down, password-free private demo, spawning on purpose | next 1b | done (`759684d`) |
| 4 | Readable view: labels, Live panel, filters, combat shown, faction colours, smooth movement | next 2 (+2.3a) | E2.1–E2.6 complete; combined E2+E3 owner acceptance pending |
| 5 | **Live menu** in the menu bar | R1 below | new |
| 6 | Events and **live logs**: chat, combat, joins, deaths, server log, commands; log window; files; LLM access | next 3 + R2 below | new scope |
| 7 | **Visual effects in Live** (optional): auras, glows, summons, spells, projectiles | R3 below | new |
| 8 | DM controls: hold, stop combat, factions, freeze, speech volume, spawn options, multi-select, player actions | next 4 | planned |
| 9 | Placeables and doors in Live: select, rotate, scale, visual transform, move, animate | next 4b | planned |
| 10 | Polish text, keep WSL running, scoped tokens, audit, chunk return values, LLM recipe | next 5 | planned |
| 11 | Reports as deltas for large areas | R4 | moved in from "later" |
| 12 | Several servers at once | R5 | moved in from "later" |
| 13 | Scenes and macros (timed command sequences) | R6 | moved in from "later" |
| 14 | NWNX_Redis reconnect after a Redis restart | R7 | moved in from "deferred" |
| 15 | Editor operations for agents (`editor_*`) | [Redis plan](live-mode-redis.md) Phase H | moved in |
| 16 | Documentation for all of the above | next 6, extended | planned |
| 17 | Web view of a running server (aurora-web) using the same API | R8 | separate repository |
| — | Owner's own steps | [list below](#the-owners-own-steps) | — |

Owner acceptance (two monitors) after rows 4–6, after 7, after 8–9, and
at the end.

## R1 — a Live menu (S)

A **Live** menu in the main menu bar (`crates/mg-ui/src/lib.rs`, beside
Build/Plugins), its items enabled according to the service's state:

* **Start Live / Stop Live** (starts or stops the service with the saved
  Options; the same as Options › Live › Enable + Apply).
* **Status…** — a small window: API address, Redis connection, last
  report age, reported areas (click opens the area with Live on),
  queued commands, recent results.
* **Show in This Area** (toggles the area view's Live; disabled without
  an open area) — the toolbar toggle stays too.
* **Live Panel**, **Live Log**, **Event Panel** (open the dock tabs).
* **Labels ▸** All / Near and selected / Hover / None; **Colours ▸**
  Faction / Hostility / Kind; **Effects ▸** Off / Markers / Full;
  **Hide Module Markers** (check).
* **Commands ▸** Stop fighting here, Destroy all own spawns, Unfreeze all,
  Cancel queued commands.
* **Copy Client Token**, **Copy curl Example**, **MCP Setup…** (shows the
  registration lines from the manual for the common clients).
* **Live Options…** (Options on the Live page), **Help: Live Mode** (the
  manual section).

Keyboard: `Ctrl+Shift+L` toggles Live in the open area. Tests: app tests
that each item does what it says and is disabled when it cannot act; a
picture of the menu.

## R2 — live logs (M, merged with Phase 3 of "next")

Phase 3 of [next](live-mode-next.md) (derived events, speech, event
panel, `live_events`/`live_find`/`live_summary`) is extended into logs:

* **Sources** (game side unless noted):
  * **Chat**: every channel the server sees — talk, whisper, shout, party,
    tell, DM, server messages — via `NWNX_Chat_RegisterChatScript`
    (`NWNX_CHAT_SKIP=n`), with speaker, target (tell), channel, text.
  * **Combat**: attacks (roll, result, attacker, target) and damage
    (amount by type) via `NWNX_Damage_SetAttackEventScript` /
    `SetDamageEventScript` (`NWNX_DAMAGE_SKIP=n`); deaths (derived from
    `dead`); spell casts (`NWNX_ON_CAST_SPELL_AFTER`: caster, spell, target
    or location).
  * **Players**: connect/disconnect (`NWNX_ON_CLIENT_CONNECT_AFTER` /
    `DISCONNECT_AFTER`: player name, character; no CD keys or IPs),
    area changes (derived), level-ups, deaths, respawns.
  * **Server log**: new lines of the server's log (`logs.0/nwserverLog1.txt`)
    — read by the WSL helper and published on `<prefix>.log`, or by the
    game script reading nothing (the helper is simpler); script errors
    highlighted.
  * **Commands** (Moonglow side): every command from the view, the API and
    MCP with client name, result and code (the audit of next Phase 5).
* **Transport**: the game batches log entries into its report (`events`,
  ≤ 256 per post, oldest dropped with a `dropped` count); the server-log
  helper publishes on `<prefix>.log`, which the bridge subscribes to.
* **Live Log window** (dock tab): tabs Chat, Combat, Players, Server,
  Commands, All; search; filters (channel, player, area, object); pause;
  click an entry selects its object in the view; **Export…** (JSONL or
  text).
* **Files**: `<data_dir>/live/logs/<server boot>/<date>.jsonl`, one file
  per kind; retention in Options › Live › Logs (default 14 days, max size
  500 MB; oldest removed); a switch *Keep chat logs* (on) so the owner
  decides about storing players' messages.
* **For LLMs**: `live_log {kinds?, since?, text?, player?, limit ≤ 500}`
  in the operation table (so `moonglow-mcp` lists it); chat is included
  only when the client token's scope has `read_chat` (next Phase 5 scopes).
* Tests: each source in the WSL end to end (a chat line from the owner's
  client in the manual run; scripted `SpeakString` and an attack in the
  automated one), retention and export unit tests, the window's app tests,
  a picture.

## R3 — visual effects in Live (L, optional per view)

What the screenshots showed: ghosts, glows, bubbles and dust in the game,
only markers in Moonglow. Effects default to Markers (owner's updated E4.4 handoff); **Effects ▸
Markers** draws an icon and the effect's name over the object; **Full**
draws the effect's models and particles.

Current E4.4 implementation and proof are in the
[cumulative record](live-mode-next.md#e44-working-record-and-file-ownership).
The coordinator's narrowed handoff uses attached durable models/particles,
marker rings for model-less glows, and short-lived marker arcs/flashes for
spell, projectile and damage events. General instantaneous VFX models remain
deferred: the event hooks do not report every visual effect (see the
[measured wire contract](live-events-wire.md)). This supersedes the original
instant-model tests below. Four-second chat bubbles are included; E4 still
requires the coordinator's menu merge and the owner's two-monitor acceptance.

* **Durable effects on objects**: in full reports each object lists its
  visual effects: loop `GetFirstEffect`/`GetNextEffect`,
  `GetEffectType(e) == EFFECT_TYPE_VISUALEFFECT`, id from
  `GetEffectInteger(e, 0)` (verify which index holds the row on first
  run), duration type; field `vfx: [row,…]` (≤ 16 per object).
* **Instant effects and spells** (corrected by Claude): effect events exclude
  visual effects, so instant VFX are deferred to
  `NWNX_ON_CAST_SPELL_AFTER` and `NWNX_ON_BROADCAST_SAFE_PROJECTILE` for casts and projectiles
  (source, target or location, spell or projectile type) → log entries
  of kind `vfx` with a time.
* **Moonglow**: read `visualeffects.2da` (new reader in `mg-rules` or
  `mg-preview`: the row's model columns — `Imp_HeadCon_Node`,
  `Imp_Impact_Node`, `Imp_Root_S_Node`…, `Dur_*`, `Cess_*`,
  `ProgFX_*`, `OrientWithGround`, `Type_FD`); load those models through
  the existing model path and draw them attached to the object's node (or
  at the location), with their emitters through `mg-render`'s particles
  (`crates/mg-render/src/particles.rs`, already used for placeables'
  flames); durable ones while listed, instant ones for their model's
  animation length (cap 5 s); projectiles as an arc line with a moving
  marker from source to target over 0.5–1 s.
* **Cost limits**: at most 64 effect models and 20,000 particles on screen
  (the farthest dropped first); Full falls back to Markers past that.
* Tests: the 2da reader against the game's table (corpus test, skipped
  without the game); unit tests of the row → model mapping; pictures of
  a few known effects (a glow, a summon, an impact) in Markers and Full;
  WSL: apply `EffectVisualEffect` durable and instant through `run_chunk`
  and see them reported and drawn.

## R4 — reports as deltas (M)

For areas where the full report grows past ~100 KB: after a full report,
the game sends only objects whose fields changed (by a per-object hash)
plus the ids that disappeared, with `base_seq`; a full report every 30 s
or when Moonglow asks (`resync` command when it sees a gap in `seq`).
Store merges deltas; tests for merge, gap → resync, restart.

## R5 — several servers at once (M)

Channel prefix per server (already configurable); Options › Live lists
servers (name, Redis host/port, prefix); the store is keyed by server;
the area view's Live picks the server reporting that area (or asks when
two do); `moonglow-mcp` and the API take an optional `server` field;
the descriptor lists them. Tests with two fake servers on two prefixes.

## R6 — scenes and macros (M)

A **scene** is a named list of steps: a command (any op) with a delay
after the previous step, optional "wait for result" or "wait until near
(object, point, radius)". Edited in a Scene window (add steps from the
view's context menu with *Record*), saved per module in
`<data_dir>/live/scenes/<module>/<name>.json`, played/stopped from the
Live menu or by MCP (`live_scene_run {name}`, `live_scene_stop`). Players
are never moved by a scene unless the player switch is on. Tests:
playback timing with a fake clock, waits, stop, persistence.

## R7 — NWNX_Redis reconnect (M)

The installed NWNX_Redis connects once (`Reconfigure()` only from the
constructor; old `cpp_redis`), so a Redis restart needs a game-server
restart. Fix in NWNX (C++, in the WSL checkout `~/unified`):
on a disconnect of the command pool or the pub/sub client, rebuild the
pool and resubscribe the configured channels with back-off; build the
plugin into a **separate** folder (never overwrite the Binaries other
work uses); point the demo's `NWNX_CORE_LOAD_PATH` at it; run the Redis
outage test with the game *not* restarted. Proposing the patch to
nwnxee/unified is an outward action: prepare the branch and the PR text,
and ask the owner before opening it.

## R8 — the web view (aurora-web, separate repository)

aurora-web's planned DM mode (spawn a monster from the browser, see an
area) can use this API instead of its own NWNX lane: a backend proxy to
`moonglow-agent/1` (or Moonglow's Redis channels directly) and a top-down
map of an area with players and NPCs as markers, chat and the event log.
It needs its own plan in that repository (`C:\Projects\aurora-web`); this
row only records that the API is meant to serve it. Start after rows 1–10.

## Earlier follow-ups (tracked here so none is lost)

* Overlapping labels → next Phase 2.1. `hold` → next Phase 4 / 1b.
  `discovery.rs` long lines → next Phase 2.5.
* Polish characters through the whole path → next Phase 5.
* Associates of players (henchmen, familiars, summons) are not protected
  from `destroy`/orders → add to the protected set unless the player
  switch is on (next Phase 4, player ops).
* The area view repaints every frame while Live is on → repaint on a new
  report or while something animates/interpolates (next Phase 2.4).
* The Grid button moved for everyone in v1 → check the toolbar layout at
  1280 px with Live off and restore the old order if it fits (next 2).
* `apps/mg/build.rs` (8 MiB stack for `mg` on MSVC) is a general fix → keep;
  mention in the PR description so it can go to `develop` on its own.

## The owner's own steps

These need the owner (their accounts, configuration or decisions):
register `moonglow-mcp` in Claude Code and Codex (one line each, in the
manual); revert the old live-mode experiment files in the earlier private
Moonglow fork;
decide when `live-mode` goes to `develop` (a PR) and whether the NWNX patch
is offered upstream; the two-monitor acceptances.
