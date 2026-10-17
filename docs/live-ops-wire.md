---
type: Reference
title: 'Live game operations: wire contract and engine proof'
description: Game and mg-live operation schemas, automatic area watches, local/server player permission, Unicode publisher, API validation and private-server proofs. UI integration remains with the coordinating workers.
tags: [live, operations, nwnx, protocol, dm]
generated: { by: codex/astra2, at: 2026-10-14T00:00:00Z }
sources: [live-mode-next.md, live-mode-roadmap.md, live-events-wire.md]
---

# Live game operations

Status: the coordinator accepted E3/E4 at `f4e2a92` and game E5/E6/E7.1 at
`00cb8b8`. The coordinator also accepted the subsequent mg-live Rust integration and private
HTTPS engine proof (`e1da522` through `9977c32`). The coordinator accepted E7.2/E7.3/E7.4/E8.1. The owner-regression/DM quality-of-life
batch below passes the stated server/Rust checks and awaits coordinator review;
connected-player and rendering limitations are explicitly listed. In-place door facing remains unsupported; unified teleport now provides a proven recreation fallback. Options UI
and menus remain with the coordinating workers. Client rendering is claimed only for the explicitly captured cases below.

## Transport and validation

Publish UTF-8 JSON to `<prefix>.cmd`, escaping all non-ASCII string characters
as JSON `\uXXXX` sequences at the Redis publisher (including surrogate pairs
where applicable). Literal UTF-8 is not accepted losslessly by native JsonParse;
see [the proven encoding boundary](#polish-text-and-the-encoding-boundary).

```json
{"v":1,"commands":[{"id":"unique-1","area":"events","op":"npc_move","target":"2","x":15,"y":10,"z":0,"hold":true,"timeout":30}]}
```

`id` is 1-64 bytes and unique within a boot. `area` is the area resref,
`target` an opaque hex engine ID where required. Acknowledgements and `results`
arrive in `<prefix>.state`, alongside the observed world. Success is
`{"id":"unique-1","ok":true}`; asynchronous actions must be checked in a
later report. Refusal is `{"id":"unique-1","ok":false,"code":"invalid","err":"..."}`.
Codes: `invalid` (type/range/kind), `not_found` (missing area/object/resource),
`other_area` (object outside command area), `dead`, `protected`, `disabled`.
Existing script operations retain their existing script-specific codes.

## npc_move: existing hold verified

Fields: `target`, finite numeric `x,y,z`; optional boolean `run` (false),
boolean `hold` (false), integer `timeout` (default 30, range 1-120 seconds).
Creature only; dead/protected targets are refused. Success has no extra fields.
Hold releases on arrival, timeout, death or replacement by a new order. Timeout
restores routine control; it does not itself cancel the queued movement.
No implementation change was needed. `--phase move_hold` checks changed position,
native commandability while held, timeout recovery and arrival recovery in
reports, plus invalid hold/run types and timeout bounds. The compiled fixture
exposes native `GetCommandable` through the name so probing does not cancel hold.

## stop_combat

Supply exactly one selection form: a creature `target` ID, or all three finite
numeric `x,y,radius`, with `0 < radius <= 50`. Radius is measured in horizontal
meters in `area`. Mixed/empty/partial forms return `invalid`. Success adds
`affected:["2","5"]`, the selected living, non-protected creature IDs (empty
is a successful no-op). The command releases live holds, clears actions and
sets decaying personal friendship towards creatures in the area on each selected
NPC for up to 180 seconds. Target mode also holds that creature's commandability
against stock AI for 180 seconds; a new live order releases it immediately. The
existing generation check prevents an older calm timeout releasing a newer order.
Radius mode keeps its prior behavior. The operation does not directly change faction
IDs or unselected creatures' action queues/reputations; releasing an existing live
attack hold can restore that actor's original faction. New orders or scripts that
explicitly override commandability can restart combat.

```json
{"id":"calm-1","op":"stop_combat","area":"events","x":9,"y":10,"radius":4}
{"id":"calm-1","ok":true,"affected":["2","5"]}
{"id":"calm-2","op":"stop_combat","area":"events","target":"2"}
{"id":"calm-2","ok":true,"affected":["2"]}
```

Target mode returns `not_found`, `other_area`, `dead`, `protected` or `invalid`
for a missing object, another area, corpse, protected creature or non-creature /
malformed ID respectively. Targeted PCs additionally return `disabled` when the
player switches/scope are not enabled. DM/possessed targets remain protected.
Player-owned associates require both switches. Radius mode still skips direct
PCs/DMs/possessed creatures even with Players enabled. Scope remains `npc`,
readOnly=false; PC targets also require the client's `players` group. The current
catalogue has71 operations; this extends an existing op. Radius wire commands
remain valid.


### Targeted PC stop (owner item25)

A living PC selected by `target` is supported with server
`MGLIVE_ALLOW_PLAYER_OPS=1` and command `players:true`. Moonglow supplies players
only when both Options Players and the submitting client's Players group permit
it. The game independently validates permission. The result retains the existing
`affected:[target]` contract:

```json
{"id":"pc-stop-1","op":"stop_combat","area":"events","target":"a","players":true}
{"id":"pc-stop-1","ok":true,"affected":["a"]}
```

The PC branch calls `MgLiveStopPlayerAttack`: clear Moonglow's hold/patrol
bookkeeping, set commandable true and ClearAllActions(TRUE) on this PC. It never
calls faction setters, ClearPersonalReputation, temporary friendship or the NPC
180-second calm hold. It deliberately does not call the NPC hold-release helper,
whose faction restoration could alter the PC's faction. This is Stop Attack,
not a truce: the player can issue a new order immediately, and attackers or module
scripts may resume combat. An already processed hit is not reversed. GetIsInCombat
may remain true while an enemy attacks the PC; the cleared action queue and
attack target are the relevant readbacks. No other character's queue changes.

Connected-client checklist: start an attack on a living PC, invoke exact-target
Stop Attack with both switches enabled, and see the queued attack cancelled; no
friendliness/alignment/faction change or 180s loss of control should occur. A new
movement or attack command must work immediately. With either switch off, expect
disabled and no effect; dead targets return dead. Radius mode must leave PCs alone.
DM/possessed target protection remains unchanged.

The `pc_stop_combat` private phase invokes the exact compiled production helper
on an NPC stand-in with stock NPC scripts removed (a PC has no such auto-attack
AI scripts). It records a held active attack first, then native action/target,
commandability, both faction IDs and both directional reputations, and reports
across five later ticks. A subsequent movement checks control release. A stale
NPC-faction marker is injected into the stand-in to verify that PC stop does not
restore an NPC faction. This proves the helper's engine effects, not native
GetIsPC dispatch or a connected client's display. No fake PC flags are used.
Rust tests separately exercise PC acceptance with Players enabled, Options and
named-client refusal, DM/possessed/dead guards and exact outbound players:true.

Reproduce with `wsl.exe -e python3
/mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase
pc_stop_combat` (one command line), using only UDP5126/Redis6392 and the existing
owned ~/mglive-events fixture. `pc_stop_combat.json` and
`pc_stop_combat-state.json` under target/test-output/live-events retain identities,
hashes, native readbacks and reports. The observed native values are action3,
attack_target:true, combat:true, commandable:false, held1 before; action65535,
attack_target:false, combat:false, commandable:true, held0 after. Factions remain
1 and2 and both reputations remain0. The original `stop_combat_target` phase
checks the unchanged NPC target/radius behaviour as a regression.

The guarded mg-live suite passes96 unit tests and the fixture safe-default test
(four explicit opt-in tests ignored). OpenAPI description is regenerated (same
71 operations); the normal schema test and all-target Clippy with warnings denied
use tools/live/check.ps1 -Jobs1 -NoUiDebug -UiUnoptimized. No apps are rebuilt.
The source fixture/helper and passing proof streams are retained; task ownership
is in target/test-output/live-pc-stop/files.json under session
01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a. Active session history is untouched, with the
separate-retention restriction and exact path recorded in the closure section.
These checks establish native/helper and Rust behaviour, pending coordinator and
connected-client acceptance. The native NPC target/radius regression also passed.
After verifying exited owned children, marker/path ownership and vacant private
ports, cleanup removed 13 private-run transient/empty-log files (131656 logical
bytes). The temporary private commit index is removed separately and recorded
in the ledger. No shared caches or other worker files were removed; physical
free-space change is not attributed during concurrent work.

UI handoff: for a multi-selection, send one target command per selected NPC/PC;
do not include x/y/radius. For the radius tool, omit target. Rust Op::StopCombat
now has `target: Option<String>` and `x/y/radius: Option<f32>`; radius initializers
need `target: None`, `x: Some(x)`, `y: Some(y)`, `radius: Some(radius)`.
No mg-ui files were edited. The flat API schema marks the four selection fields
optional and documents mutual exclusion; Command::validate enforces the complete
choice on both API routes.

Private `--phase stop_combat_target` proves two concurrent attacking NPCs, stops
only the selected ID, observes its initial combat/target clear and commandability
held, then sees its queue idle (act65535), no attack target, and the other attacker
still fighting after six ticks. GetIsInCombat may return true again while an
unselected opponent attacks it; this is not an active queued attack by the calmed
NPC. Multi-selection should include all participants to stop both sides. A new movement order
releases the calm hold and reaches its destination. Plot fixture creatures avoid
deaths while stock AI remains active; target/radius malformed inputs and missing,
non-creature and dead targets are refused. A later radius command still calms
the second pair. Rust tests cover other-area refusal, PC switch-off refusal and
switch-on acceptance, DM/possessed protection, exact target/radius wire JSON, and
finite numbers. Retained evidence: `target/test-output/live-events/stop_combat_target.json`
and `stop_combat_target-state.json`.

Engine finding: clearing one NPC initially stopped its combat, but stock AI
requeued the cached attack within six ticks. Native readback showed its attitude
was friendly (96) while the opponent's was hostile (0). Disabling the opponent's
actions did not fix it; even mutual friendship (96/96, neither enemy) did not keep
the old attack from returning. That attempted mutual-reputation change was removed.
Target mode instead uses the existing cancellable order hold, so it changes only
the selected actor's own queue, commandability and personal attitudes. The hold
lasts 180s or until any new order; it does not paralyze or grant immunity. Its full
180s expiry is the same generation-bound release helper used by movement holds;
the new engine phase measures six ticks and new-order release, not a 180s wait.

`--phase stop_combat` proves fighting beforehand, both combat flags/targets
cleared, calm across seven later reports, outside-radius exclusion and unchanged
outside reputation, plus radius/type refusals. Immortal fixture creatures avoid
death while preserving real combat. Personal-reputation probes are native values
exposed in names, independent of acknowledgements.

Engine findings: nested area iteration resets the outer iterator, so selection
is snapshotted first. `SetIsTemporaryNeutral` left the hostile pair at 0 and
combat resumed; decaying friendship works. The installed binary's
`ExecuteCommandPersonalReputationAccess` at `0x6b5e30` modifies its first object
argument, despite the include's source/target wording. The outside-reputation
check guards that direction. `SurrenderToEnemies` has a fixed radius and clears
nearby players' actions too, so it cannot implement the requested NPC-only scope.
The binary was only read, never patched. Plot flags were also tried in an early
fixture; removing them alone did not solve the failed neutrality route.
The native `GetReputation` wrapper at `0x6a5ed0` evaluates its second object's
feelings towards the first, also contrary to the include's wording. The probe
uses that observed argument order; its initial reversed check reported the
selected NPC's friendly attitude as if it belonged to the outside NPC.

## set_faction

Fields: `target` (living NPC), `faction` as `hostile|commoner|merchant|defender`
or integer raw module faction ID (0-65535, must exist). Names map to the module's
standard IDs 1/2/3/4; numeric values are **not** the `STANDARD_FACTION_*` enum.
Invalid names/types/bounds/nonexistent IDs return `invalid`; missing/dead/
other-area/protected targets use their corresponding code. Success adds the
actual integer `faction`. Any live hold is released, including its saved faction,
before the final assignment so an old timeout cannot undo it.

```json
{"id":"faction-1","area":"events","op":"set_faction","target":"2","faction":"hostile"}
{"id":"faction-1","ok":true,"faction":1}
```

Every creature report now includes `faction` (raw module integer, -1 if unknown).
`--phase set_faction` checks hostile/merchant/defender and numeric commoner, comparing
that report field with independent native `ObjectToJson(...).FactionID.value`
exposed by a compiled fixture. Invalid IDs do not change the faction; missing
objects and dead targets are refused. The NWNX setter checks the manager for a
real faction before adding a member; the script confirms the resulting faction
before releasing a hold, so a nonexistent-ID refusal retains that order.

## freeze / unfreeze

`freeze` requires `target` and finite numeric `seconds`, `0 < seconds <= 300`.
`unfreeze` requires only `target`. Both require a living creature and obey player
protection. Success is the common `ok:true` result. Freeze refreshes the duration
of Moonglow's own effect; unfreeze is successful even if no owned effect exists.
The effect is supernatural cutscene paralysis tagged `MGLIVE_FREEZE`; only effects
with that exact tag are removed. Other scripts should reserve that tag for Live.

```json
{"id":"freeze-1","op":"freeze","area":"events","target":"2","seconds":30}
{"id":"unfreeze-1","op":"unfreeze","area":"events","target":"2"}
{"id":"unfreeze-1","ok":true}
```

Every creature report adds boolean `frozen`: the tagged effect is currently
present, independent of unrelated paralysis. `--phase freeze` queues movement,
observes unchanged position while frozen, removes only the owned tag while
`OTHER_FREEZE` survives, observes movement after that unrelated fixture effect
is separately removed, and checks natural expiry and duration/type refusals.

## npc_say: volume

Existing `target` and `text` (1-200 nonblank NWScript bytes) gain optional
`volume: talk|whisper|shout`, default `talk`. Wrong type or unknown enum is
`invalid`; existing dead/protected/area checks apply. NPCs and placeables can
speak. Success has no extra fields. The next chat event carries the exact text,
speaker and native channel: talk 1, whisper 3, shout 2. All three are proved by
`--phase speech_volume`, including invalid-volume refusals.

```json
{"id":"say-1","op":"npc_say","area":"events","target":"2","text":"Hello","volume":"whisper"}
{"id":"say-1","ok":true}
```

## spawn: count and faction

Existing fields: `kind:creature|placeable|item`, blueprint `r`, finite `x,y,z,f`
(facing in degrees). Optional integer `count` defaults to 1, range 1-10.
Optional `faction` uses the set_faction schema and is valid only for creatures.
The resource must exist for the requested kind (`not_found` otherwise).
Invalid count/faction types and bounds are refused before creation. If creation
or a numeric faction assignment fails, this command removes its created batch
and returns `invalid`; it never destroys earlier spawns.

Count 1 retains the requested position. Multiple objects request a centered
grid, at most three columns, with two-meter spacing. The engine may adjust
points to walkable positions; the report contains the actual positions.
Success preserves legacy `obj` (first object) and adds `objects` (all IDs in
creation order), `count`. Every created object has `own:true` in reports.

```json
{"id":"spawn-1","op":"spawn","area":"events","kind":"creature","r":"nw_bartender","x":10,"y":10,"z":0,"f":45,"count":3,"faction":"merchant"}
{"id":"spawn-1","ok":true,"obj":"2","objects":["2","3","4"],"count":3}
```

`--phase spawn_many` proves ten distinct owned creatures, merchant faction on
each, separated report positions, all count bounds/type checks, missing-resource
refusal and no leftover creatures after an invalid-faction batch. The initial
test assumed exact two-meter placement; native walkmesh adjustment gave a
minimum about 1.43 m, still separated. The proof checks actual separation.

## Player permission and associate protection

Every `pc_*` operation requires **both** server environment
`MGLIVE_ALLOW_PLAYER_OPS=1` at boot and JSON boolean `players:true` in the
command. With the server switch absent/off, return `disabled` before target
lookup. With it on, absent/false `players` is `disabled`, wrong type is `invalid`.
The game checks independently of Moonglow Options. Script/chunk operations
retain their existing deliberately broad authorization.

NPC commands also protect a creature whose `GetMaster` is a PC/DM/possessed
creature. Both switches permit these associate commands. Direct players remain
protected unless the operation has an explicit forced-PC path (the later PC
operation sections list them); destroy still refuses them. Targeted stop_combat
now has the separate PC Stop Attack path above, requiring both switches without
altering faction/reputation. Its radius selection still excludes direct players.

Player operations accept creature targets, including owned NPC stand-ins for
the headless engine proof explicitly allowed by Phase 4. This proves native
effects and permission gates; a real client is still needed to exercise actual
PC/master classification and client UI. `--phase players_disabled` proves
server-off refusals for all six op names. Enabled phases use server switch 1
and check the independent command flag. No fake PC flag/client is injected.

### pc_heal

Fields: `target`, `players:true`, optional integer `amount` (1-100000).
Omitting amount applies max-HP healing, capped naturally by the engine.
Requires a living creature; `dead` otherwise. Success has no extra fields.
`--phase pc_heal` proves HP loss followed by requested/full healing in reports,
missing/false/wrong-type command flags, and invalid amount bounds/types.

```json
{"id":"heal-1","op":"pc_heal","area":"events","target":"2","players":true,"amount":10}
{"id":"heal-1","ok":true}
```

### pc_raise

Fields: `target`, `players:true`. Requires a dead creature (`invalid` if already
alive). Applies native resurrection followed by max-HP healing, cancels a live
hold and logs the command. Success has no extra fields. `--phase pc_raise`
observes `dead:true` followed by `dead:false` and full HP, and proves healing a
dead target returns `dead`. This is an owned NPC stand-in, not a client UI test.

```json
{"id":"raise-1","op":"pc_raise","area":"events","target":"2","players":true}
{"id":"raise-1","ok":true}
```

### pc_teleport

Fields: `target`, `players:true`, finite numbers `x,y,z`, and the common `area`
resref. Same-area only: the target must already be in that area (`other_area`
otherwise; unknown area is `not_found`). A living creature is required. Clears
actions/hold and uses native JumpToLocation, preserving facing. The engine may
adjust an unwalkable destination; use reported coordinates as the actual result.
Success has no extra fields. `--phase pc_teleport` checks changed coordinates and
preserved facing on an owned NPC, plus invalid inputs and missing permission.
The flat-tile fixture snaps requested Z 0 to ground Z about -1.30; the proof
checks that native ground height, not exact preservation of requested Z.

```json
{"id":"teleport-1","op":"pc_teleport","area":"events","target":"2","players":true,"x":15,"y":15,"z":0}
{"id":"teleport-1","ok":true}
```

### pc_give_gold

Fields: `target`, `players:true`, integer `amount` 1-1000000. Requires a living
creature; refuses signed-total overflow. Calls native GiveGoldToCreature and
logs the amount. Every creature report now includes `gold` (integer balance).
Success has no extra fields. `--phase pc_give_gold` checks the exact balance
increase on an owned NPC and refuses missing permission/bad amounts.

```json
{"id":"gold-1","op":"pc_give_gold","area":"events","target":"2","players":true,"amount":25}
{"id":"gold-1","ok":true}
```

### pc_give_xp

Fields: `target`, `players:true`, integer `amount` 1-100000. Requires a living
creature; refuses signed-total overflow. Calls native GiveXPToCreature and logs
the amount. Every creature report includes `xp` (integer XP total). Success has
no extra fields. `--phase pc_give_xp` proves the exact increase on an owned NPC
and invalid-amount/permission refusals; level-up client UI is outside this test.

```json
{"id":"xp-1","op":"pc_give_xp","area":"events","target":"2","players":true,"amount":25}
{"id":"xp-1","ok":true}
```

### pc_give_item

Fields: `target`, `players:true`, `resref` (1-16 resource-name bytes). Requires
a living creature and an existing UTI blueprint (`not_found` when absent).
Creates one item through native CreateItemOnObject, logs the resref, and returns
`obj` (created item ID). Inventory is not added to periodic reports. The engine
can merge stackable items; consumers should refresh inventory separately when
needed. `--phase pc_give_item` independently iterates native inventory in a
compiled fixture and reports its count/item ID through the actor's name,
proving the grant rather than trusting `ok:true`. The target is an owned NPC.
The WSL launcher passes `MGLIVE_ALLOW_PLAYER_OPS` explicitly through its clean
environment, defaulting to 0.

```json
{"id":"item-1","op":"pc_give_item","area":"events","target":"2","players":true,"resref":"nw_it_mpotion001"}
{"id":"item-1","ok":true,"obj":"3"}
```

## Placeable and door operations

These operations require a placeable or door in the named area. Wrong object
type is `invalid`; common missing-target/area/protection checks apply. Reports
for these objects remain in full area snapshots every five ticks. Tests wait
for the next applicable full report, not just the command acknowledgment.
The fixture authors a non-static chest UTP and a door in GIT; native CreateObject
cannot create doors (verified installed API and the failed empty-door probe).

### set_facing

Fields: `target`, finite `f` in degrees, 0 inclusive to 360 exclusive. Calls
native SetFacing for placeables; success has no extra fields. Doors return
`disabled`: the engine ignores native SetFacing/SetFacingPoint for this type.
The first engine probe retained door `f:90` after requesting 123, while the chest
changed to 123. Read-only inspection of the pinned server's
`ExecuteCommandSetFacing` at 0x69cd10 confirms its object-type filtering; no
server/plugin patch was made. Use visual-transform Z rotation for visual changes;
true door orientation needs a later NWNX facility. `--phase set_facing` proves
the chest change, door refusal/preservation, and wrong types/range.

```json
{"id":"facing-1","op":"set_facing","area":"events","target":"2","f":123}
{"id":"facing-1","ok":true}
```

### set_visual_transform

Fields: `target`, optional `scale` (>0, <=10), `rotate:{x,y,z}` (each -360..360
degrees), `translate:{x,y,z}` (each -1000..1000 meters). All values must be finite
JSON numbers. Each supplied vector requires all three axes; omitted groups
retain their current values. At least one group is required. Alternatively send
`reset:true` with no groups; reset clears the **base scope** through native
ClearObjectVisualTransform. `reset` must be boolean when present. All fields
are validated before mutation. No interpolation is requested. Success has no
extra fields. Transforms are visual; physical position/facing remain separate.

Full placeable/door reports contain `vt:{scale,rotate:{x,y,z},translate:{x,y,z}}`
with all seven base-scope values **only when not identity**. Omitted `vt` in a
full object snapshot means identity, so consumers must clear any cached value.
An absent object list in an incremental area report has no reset meaning.
Animation-speed transforms and non-base scopes are outside this field.
`--phase visual_transform` checks all seven native values, partial group updates,
refusal without mutation, reset omission and unchanged physical coordinates for
both object types. This proves server state, not client rendering or collision.

```json
{"id":"vt-1","op":"set_visual_transform","area":"events","target":"2","scale":1.5,"rotate":{"x":10,"y":20,"z":30},"translate":{"x":1,"y":2,"z":3}}
{"id":"vt-1","ok":true}
{"id":"vt-reset","op":"set_visual_transform","area":"events","target":"2","reset":true}
{"id":"vt-reset","ok":true}
```

### set_position

Fields: `target`, finite numbers `x,y,z`. Moves an existing placeable/door within
the common `area` using NWNX_Object_SetPosition with subarea updates enabled.
Success has no extra fields. Facing is preserved. The next full snapshot reports
actual physical coordinates; this is separate from visual translation.
`--phase set_position` proves exact XYZ changes on both types and rejects bad
coordinate types. Collision/pathing on a client is not asserted.

Requires `NWNX_OBJECT_SKIP=n` and `nwnx_object.nss`. The WSL launcher enables the
plugin; the include extractor copies it from the pinned NWScript.zip, and demo
preparation no longer treats it as a retired dependency. The private harness
records the Object plugin hash alongside the other plugins.

```json
{"id":"position-1","op":"set_position","area":"events","target":"2","x":15,"y":15,"z":2}
{"id":"position-1","ok":true}
```

### play_animation

Fields: `target`, `name`. Placeable names: `activate`, `deactivate`, `open`,
`close`; doors support `open`, `close`. Unknown/wrong-type names and door
activation/deactivation are `invalid`. Opening a locked door is `invalid`.
Placeables call native PlayAnimation; doors queue ActionOpenDoor/ActionCloseDoor
on themselves. Direct placeable animation constants did not change the door's
state; dedicated actions did. Success has no extra fields.
The model determines visible animation support. Full placeable/door reports
add `anim`, the native current animation integer from NWNX Object (not the
NWScript ANIMATION_* command constant). The existing `open` remains the native
open-state flag. `--phase play_animation` checks actual animation/open-state
transitions, not acknowledgment. No client-rendered animation frames are claimed.
The chest's native animation states are 73/74/75/76 for the four names;
open/close also change its `open` flag. The door's `open` flag changes both ways.

```json
{"id":"anim-1","op":"play_animation","area":"events","target":"2","name":"open"}
{"id":"anim-1","ok":true}
```

### set_useable

Fields: `target`, required JSON boolean `useable`. Applies native SetUseableFlag
to a placeable/door. Static placeables return `disabled`, since native useability
changes require non-static objects. Success has no extra fields. Every full
placeable/door snapshot adds `useable` (native boolean). `--phase set_useable`
proves both transitions on each authored fixture and rejects non-booleans.
Engine getter caveat: GetUseableFlag returns false for doors regardless of the
actual flag. Read-only native inspection at 0x69b390 shows it only accesses
placeables; SetUseableFlag at 0x6bc0e0 updates the common object field. Door
reports therefore read `ObjectToJson(door).Useable.value`. A separate compiled
fixture exposes that native serialized byte through the name, proving both
setter transitions. This adds one native door serialization per full report;
a direct NWNX getter could optimize it later without changing the wire format.

```json
{"id":"use-1","op":"set_useable","area":"events","target":"2","useable":false}
{"id":"use-1","ok":true}
```

### set_name

Fields: `target`, `name` (1-200 nonblank NWScript bytes). Supports creatures,
placeables, doors and items; existing protected/area checks apply. Native SetName
changes the existing `name` report field. Empty names are refused, so reverting
to a blueprint name by an empty native SetName is not exposed. Success has no
extra fields. `--phase set_name` proves exact names including quotes/backslash
on a creature, chest and door, plus empty/control-only/oversize/type refusals.
The shared speech/name blank check obtains tab/CR/LF bytes via JSON parsing:
the former source-literal check accepted tab-only input on the pinned compiler.

```json
{"id":"name-1","op":"set_name","area":"events","target":"2","name":"New name"}
{"id":"name-1","ok":true}
```

## Polish text and the encoding boundary

E7.1 sends `Zażółć gęślą jaźń` in both `set_name.name` and `npc_say.text` through
Redis and observes native name/chat reports. `--phase polish` diagnoses the
default configuration; `--phase polish_cp1250` proves the corrected route.
The command results alone are not used as text evidence.

| Wire / server setting | Name and chat report | Native byte length |
|---|---|---|
| Literal UTF-8 / default | `ZaÅ¼Ã³Å?Ä? gÄ?Å?lÄ? jaÅºÅ?` | 26 |
| Unicode escapes / default | `Za?ó?? g??l? ja??` | 17 |
| Unicode escapes / `cp1250` | `Zażółć gęślą jaźń` exactly | 17 |

The installed `nwscript.nss` explicitly documents these conversions:
JsonParse takes **game-local bytes**, JsonGetString decodes UTF-8 JSON values to
the game charset, JsonString converts game bytes back to UTF-8, and JsonDump
escapes all non-ASCII output. Redis supplies bytes unchanged. Therefore raw
UTF-8 fails first at JsonParse; escaped Unicode reaches JsonGetString intact but
the default charset cannot represent most Polish letters. The independent
compiled `mget_textbytes` probe measures the native name length after speech.

**One conversion location, now implemented in Rust:** escape non-ASCII code points
in the final JSON serialization at the Redis command publisher. Keep API/store
strings as Unicode and do not transcode the whole JSON packet to cp1250. Native
JSON functions already handle conversion of values both ways. On the game side,
configure the server's own `settings.tml` before startup:

```toml
[game.language]
codepage = "cp1250"
```

The generated server schema explicitly lists cp1250/cp1251/cp1252. The harness
sets only its task-owned scratch configuration and records the selection; all
other phases reset it to default. No real user/install configuration is changed.
Literal UTF-8 still fails with cp1250, proving that the two requirements are
independent. The production publisher now follows this route; see the Rust
integration and real HTTPS proof below.

Example escaped payload (ASCII is valid UTF-8):

```json
{"id":"pl-1","op":"set_name","area":"events","target":"3","name":"Za\u017c\u00f3\u0142\u0107 g\u0119\u015bl\u0105 ja\u017a\u0144"}
{"id":"pl-1","ok":true}
```

The corrected report's `name`, chat `text` and chat speaker `name` all equal the
input. A client using Polish fonts/codepage has not joined this headless test;
rendered glyphs and mixed-language clients remain an explicit client check.

Module-origin names, chat and logs now preserve raw byte companions and use the
[single receive decoder](live-events-wire.md#module-text-receive-boundary-owner-bug28).
This also handles UTF-8 literals found in the owner's module; the earlier
outbound-only proof did not cover that source.

## Added report fields at a glance

Every creature report adds raw module faction ID, owned-freeze presence, gold
balance and XP total. Example fragment:

```json
{"id":"3","faction":3,"frozen":false,"gold":25,"xp":25}
```

Every full placeable/door snapshot adds native animation and useability, with
the optional base visual transform. Example fragment:

```json
{"id":"2","anim":50,"useable":true,"vt":{"scale":1.5,"rotate":{"x":10,"y":20,"z":30},"translate":{"x":1,"y":2,"z":3}}}
```

These are additions to existing object rows, not complete state envelopes.
Consumers must not confuse native `anim` values with NWScript animation command
constants. Identity `vt` is omitted and must clear the cached transform on full
snapshots. Existing creature `vfx` and event fields are defined in
[the event contract](live-events-wire.md).

## Not covered and integration handoff

- True door facing: native setter is a no-op, so this operation returns
  `disabled`. Visual rotation works; true orientation needs a later NWNX setter.
- Real PC/master classification, client resurrection/level-up/inventory UI,
  rendered animations/transforms, collision and Polish glyphs require a client.
  The authorized owned-NPC stand-ins prove mechanics, not those client paths.
- Rust integration is recorded below. Options UI and menus remain with the
  coordinating workers; this batch changes mg-live only.
- Static placeables are refused for useability. The two-way engine proof uses
  non-static fixtures. Model-specific animation availability remains native.

## Proof and ownership

Use `bash tools/live/test-events.sh --phase <phase>` in WSL from this worktree.
The same owned harness uses only UDP 5126 / Redis 6392 / `~/mglive-events`,
private server, no player/DM passwords and a random unprinted admin password.
NWNX: `~/nwnx-redis-reconnect`; server:
`~/server/bin/linux-x86/nwserver-linux`. It never stops another server.
The existing shared `...-claude/target/debug/mg.exe` compiles scripts; if a
rebuild is necessary, use `CARGO_TARGET_DIR=C:\Projects\moonglow-toolset-original-claude\target`
and `CARGO_BUILD_JOBS=1`. Cargo tests use `tools/live/check.ps1` or the coordinator-authorized
non-existent `NWN_ROOT` guard documented below.

Session/task ledger continues in `target/test-output/live-events/owned-files.json`.
Required new source is `tools/live/nwn/mglive_exec.nss`, minimal report/config
helpers, `tools/live/fixtures/events/` and per-operation phases in
`tools/live/test-events.py`. Required documentation: this file, index and log.
Reuse the existing `module/`, `events.mod` and Linux runtime copy. Per-phase
`<phase>.json`, `<phase>-state.json` and `<phase>-log.json` are the small proof
records, with exact source/binary hashes; do not create another build target.
See [the events record](live-events-wire.md#ownership-evidence-and-cleanup) for
session identity, safety rules and the prior accepted phase's cleanup.

### Game batch closure, 2026-10-12

All 21 E5/E6/E7 phase manifests pass; each records the native effect or a
documented unsupported/encoding diagnosis and confirms its own children exited.
The final shared-code chat regression also passes, including overflow and
subscriber-gap retention. The final working set compiles 35 scripts with zero
failures. No Cargo build/test was needed. Shell syntax, Python parsing, document
links and whitespace checks are part of closure; no GUI/Rust acceptance is claimed.

The accepted source recovery point remains Git `f4e2a92`; the authorized merge
is `cba1cfa`. Game operation commits, in task order:

| Scope | Commits |
|---|---|
| Hold, calm, factions, freeze, volume, spawn | `b625ed9`, `28d1555`, `c74df9a`, `456157b`, `92e678e`, `6415916` |
| Player gate/heal, raise, teleport, gold, XP, item | `136e5ee`, `4d85b01`, `f0c65d5`, `435cef9`, `feb2a3f`, `0ae501c` |
| Facing, transforms, position, animation, useability, name | `48b825f`, `7bcc3c4`, `5840e98`, `a41d696`, `dfaa3dd`, `440df00` |
| Polish diagnosis and corrected route | `0290afc` |

Retain the one current module working set, current packed MOD and private runtime
copy for review, source scripts, required installed dependencies and small phase
proofs. Earlier phase module hashes identify their Git-reproducible sources; the
single working MOD is replaced between runs, not archived once per operation.
Close the runtime stage by removing only its regenerated scratch logs, settings,
server secret, SQLite caches and the local compile metadata cache after checking
ownership marker, symlinks, lock and free ports. The small `ops-cleanup.json`
records exact deleted paths/sizes, and `owned-files.json` records retained files.
No shared target, other worktree, installed game or other server is cleaned.
Cleanup removed 12 generated files totaling **133,103 logical bytes**. UDP 5126
and Redis 6392 were free, and 157 required working/evidence files remain listed.
No physical disk-space gain is attributed because other workers are active.

Session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a` remains active during closure.
Its history is preserved at
the agent's local Codex session file.
This record registers it for separately authorized cleanup after the session
ends; no active history or application index is modified.

## Rust integration (2026-10-12)

The authorized merge of `live-mode-claude` fast-forwarded this branch to
`859b0ef`. `store.rs` now models every operation above, with typed faction,
volume, animation and transform values. `Policy.players` defaults to false
(including old serialized settings). Player operations are refused as `disabled`
when it is off, and queued player operations are cancelled when it is revoked.
Only this local policy inserts `players:true` into the game command envelope;
API callers cannot grant themselves permission. The independent server switch
still applies. The game remains authoritative for masters, static objects,
resource existence and faction existence, which reports cannot fully establish.

Report fields `faction`, `vfx`, `frozen`, `gold`, `xp`, `vt`, `useable` and `anim`
have backward-compatible defaults. Full snapshots clear omitted transforms;
partial reports preserve the last placeable/door snapshot. Results accept both
the existing string attack-faction annotation and numeric faction IDs, plus
spawn `objects`/`count` and calm `affected`. UI initializer updates for the new
policy, spawn count/faction and speech volume are delegated to the coordinator.
The public Command shape is unchanged.

Store validation and exact wire-payload tests live in `store_ops_tests.rs`.
The first Rust gate passed 50 tests, with two pre-existing manual tests ignored.
Use the shared target, one build job and toolchain 1.98.0; all Cargo tests in
this batch set `NWN_ROOT` to this worktree's non-existent
`target/test-output/live-events/no-native-game` (the other authorized guard is
`tools/live/check.ps1`). No Windows game server or mg-ui build is involved.
The cumulative working-file ledger is `target/test-output/live-events/rust-ops-files.json`.

The operation catalogue contains **29 operations** with all capabilities on.
The 16 new commands use `live_<op>` names and the same Handler::Command path;
spawn count/faction and speech volume extend existing entries. Discovery omits
the six pc_* operations when `players` is off. Both HTTP command routes reject
caller-supplied `players`; direct disabled player calls return `disabled`, even
for unknown targets. Flat argument schemas document faction name-or-ID unions,
complete transform vectors, bounds and defaults; Command validation enforces
cross-field rules. `docs/live-api.openapi.json` is regenerated from this table
with all capabilities on. Actual `/v1/describe` and `/v1/openapi.json` reflect
local policy. Catalogue dispatch tests cover all new/extended payloads through
both routes, and the guarded suite now passes 51 active tests.

The Redis publisher now applies the E7.1 conversion in `bridge::game_json`:
serialize the Unicode value once, then escape each non-ASCII character as JSON
UTF-16 code units (surrogate pairs for supplementary characters). Existing JSON
quotes, backslashes and control escapes remain unchanged. API, store and log
values remain Unicode; the server's own codepage still determines representable
characters. Unit tests cover both Polish command fields, supplementary Unicode,
literal escape text, quotes, controls, ASCII identity and JSON round-trip.
Both publisher/reconnect tests pass (the manual Redis test stays ignored).
A first test fixture passed through PowerShell's default ASCII stdin lost its
Polish characters; explicit Rust Unicode escapes fixed the fixture without
changing the conversion. This is separate from the engine encoding diagnosis.

### Real HTTPS proof and Rust closure

`tools/live/test-events.sh --phase rust_api` starts the same private server and
Redis, then launches `crates/mg-live/examples/live_ops_proof.rs`. That helper
starts the **production LiveService** with an ephemeral loopback HTTPS listener
and a random unprinted API token. Its TLS client trusts only that service's
certificate, sends literal UTF-8 request bodies to `/v1/call/live_*`, and reads
results with `live_wait`, `live_area_state` and `live_events`. It never injects
reports into the Rust store or publishes commands itself. The Python harness
independently records native `.state` frames and captures `.cmd` publications.
Before commands, the helper requires the exact random boot ID passed by the
owned harness; it cannot act on a different server accidentally.

Reproduce from this worktree in PowerShell (one Cargo process at a time):

```powershell
$env:CARGO_TARGET_DIR='C:\Projects\moonglow-toolset-original-claude\target'
$env:CARGO_BUILD_JOBS='1'
$env:NWN_ROOT=Join-Path $PWD 'target/test-output/live-events/no-native-game'
# The guard path must not exist.
cargo +1.98.0 test -p mg-live --lib
cargo +1.98.0 clippy -p mg-live --all-targets --no-deps -- -D warnings
cargo +1.98.0 build -p mg-live --example live_ops_proof
wsl -e bash /mnt/c/Projects/moonglow-toolset-original-astra2/tools/live/test-events.sh --phase rust_api
```

To regenerate the all-capabilities OpenAPI snapshot, set
`$env:MOONGLOW_UPDATE_OPENAPI='1'` for the guarded
`cargo +1.98.0 test -p mg-live openapi` invocation, then remove that environment
variable. No mg-ui build or source edit is part of these checks.

The passing boot `1d495fced8f1b229e6c8c792d8923527` spans report sequence 2-24:

| Operation through HTTPS | Observed effect through HTTPS and raw native reports |
|---|---|
| `freeze`, `unfreeze` on NPC `3` | `frozen:false -> true -> false` |
| `set_faction` on NPC `3` | `faction:2 -> 3`, result also `faction:3` |
| `set_visual_transform` on placeable `5` | Identity -> scale 1.5, rotation (10,20,30), translation (1,2,3); reset -> identity |
| `pc_give_gold` with local players off | `disabled`, gold stays 0; no gold command reaches Redis even with the server switch on |
| Polish `set_name` and `npc_say` (shout) | Exact requested Polish string returns in name and chat text/speaker name; channel 2 |

All nine captured command envelopes contain ASCII JSON and omit `players` with
local policy off. The private server enables its independent player switch and
uses **its own** cp1250 settings for the Polish check. Raw native observations
agree with the API. Real PC/client rendering, fonts and inventory behavior remain
outside this headless proof, as already identified in the game batch.

The proof's first attempt reached the effects and correct Polish chat, but the
client asserted speaker name inside `data`; the API normalizes it to the event's
top-level `name`. Correcting that assertion made the full rerun pass. This was a
proof-client error, not an encoding failure. The current proof overwrites the
single working module and phase artifacts; no per-attempt module copies remain.

Evidence in `target/test-output/live-events/`: `rust_api.json` binds native
binary/plugins/includes/scripts/module/helper hashes and exited child identities;
`rust_api-state.json` contains the raw reports; the phase manifest embeds API
observations and captured commands. `rust-validation.json` binds current
Rust sources and the checks. The module SHA-256 is
`cf05005e37eb11535976add646b3f1e1c22a1006cf55a64ecd8f3ce0a9a48fc3`;
proof-client executable SHA-256 is
`d9c7e9658789bdd9ef3f5dded34d5cb645024a1dbd36b1fc3e144d9b5e19c37a`.

Validation: **52 unit tests pass**, two existing manual tests remain ignored;
all-target mg-live clippy passes with warnings denied; Rust formatting, Python
syntax and shell syntax pass. New commands have exact payload checks through
both command routes and per-operation refusal checks. This is mg-live component
and native transport proof, not a whole-workspace/UI build or coordinator
acceptance of the Rust batch.

The runtime stage is closed. Preserve current module/source, required installed
dependencies and the small passing evidence. Clean only regenerated private
scratch logs/settings/secrets/SQLite caches and the task-local compile metadata,
using the same marker, canonical-path, lock and free-port checks as the prior
closure. `rust-ops-cleanup.json` records actual removals. Shared Cargo output
is in use by other workers and is retained; no deletion there is authorized.
The session identity and history-retention restriction recorded above remain
unchanged. This task does not remove an active session or application index.

Rust closure removed **14 files, 142,505 logical bytes**: 12 regenerated
scratch/cache files plus an empty log and a superseded duplicate API report.
The current phase manifest embeds the API observations, so that intermediate
copy is unnecessary. Both private ports are free and both native children
exited. Current sources/module, raw reports, canonical manifest and dependencies
are retained for review. No physical/free-space gain is attributed to this task.

## E7.4 JSON chunk returns

The coordinator accepted the preceding Rust batch through `9977c32`; this batch
starts from the authorized fast-forward to `c22576b`. `run_chunk` adds optional
boolean `return` (default false). With `return:true`, the server prepends the
`mglive_ret` include and provides `void MgReturn(string s)`, where `s` is JSON
text. The helper parses it and returns the last synchronous value as a JSON-valued
`value` field, not a double-encoded string. `wrap:true` (default) wraps the body
in main; `wrap:false` accepts a complete source with main. Both include the helper.

```json
{"id":"query","area":"events","op":"run_chunk","return":true,"code":"MgReturn(JsonDump(JsonObjectSet(JsonObject(), \"answer\", JsonInt(42))));"}
```

Result: `{"id":"query","ok":true,"value":{"answer":42}}`. Explicit JSON null
and a chunk that never calls MgReturn both return `value:null`; old chunks omit
`value`. `live_wait` preserves this distinction. The raw helper input and compact
ASCII-escaped JSON value must each be at most **16384 bytes** (including JSON
quotes). Invalid/oversized JSON returns `code:invalid`; compilation failures
retain `compile_error` and omit value. Runtime effects are not rolled back by a
return-value refusal. The return slot, error and active flag are cleared around
each execution; delayed calls after the chunk ends are ignored. Last synchronous
call wins. Only the existing local chunks permission AND server
`MGLIVE_ALLOW_CHUNK=1` allow the command.

Rust uses `Op::RunChunk.return_value` (wire `return`) and
`CommandResult.value: Option<Value>`; explicit JSON null is `Some(Null)`.
UI literals must initialize `return_value:false`; UI remains coordinator-owned.
Old report parsing and old command serialization are preserved.

Private proof: rebuild the shared `live_ops_proof` example with the guarded
commands above, then run `test-events.sh --phase chunk_return`. The production
HTTPS/Redis/native/HTTPS path proves object/null/missing returns, full source,
invalid JSON, overflow, compile error, clearing after errors and old behavior.
`chunk_return.json` binds the module/scripts/native binaries and helper, embeds
API results and command publications; `chunk_return-state.json` independently
records native results. The guarded mg-live suite passes **53 active tests**, with
the two existing manual tests ignored. Tests cover the exact 16384-byte boundary,
oversize refusal, explicit null, old omission, boolean validation and live_wait.

## E8.1 delta reports and resync

Protocol v1 remains compatible with old reports. When a selected-areas full
snapshot reaches **100000 JSON bytes**, the game enables deltas for the rest of
that boot. Small sessions retain their existing creature-every-tick and full-
every-five-ticks behavior. Delta sessions examine all four object kinds each
one-second tick and hash each **complete object row** with native HashString.
Unchanged rows are omitted. A changed row replaces the whole cached object;
omitted fields clear old values (for example target, vt or vfx).

Delta envelopes include `base_seq`, the immediately previous server report
sequence. Each area has `full:false`, changed `cre`/`plc`/`doors`/`items` arrays
and `removed:[id,...]` for vanished objects of any kind:

```json
{"v":1,"boot":"same-boot","seq":11,"base_seq":10,"areas":[{"area":"events","full":false,"cre":[],"plc":[{"id":"247","name":"Changed","r":"mget_chest","x":0,"y":0,"z":0,"f":0,"hp":[50,50],"own":false,"open":false,"lock":false,"anim":0,"useable":true}],"doors":[],"items":[],"removed":["233"]}],"ack":[],"results":[],"events":[],"dropped":0}
```

`removed` is applied to all four lists in that area; upserts replace any former
kind under the same ID. Lists not changed remain present in the store. A newly
selected area has `full:true` even inside a delta envelope. The hash cache is
module-owned per area/resref, so destroyed objects remain detectable. Native
HashString is a 32-bit change detector; periodic snapshots also bound any hash
collision drift. A full snapshot (no `base_seq`, all selected areas `full:true`)
arrives every **30 ticks / nominal 30 seconds**, or on resync. Full rows include
all normal creature appearance and placeable/door fields. Hashing saves transport
bytes; it still constructs current rows and does not claim a CPU optimization.

A delta is accepted only with a known baseline, exactly consecutive seq/base_seq,
and a known area unless that area is full. Otherwise the store keeps its last
valid objects/area age, still processes command results and game events, and
publishes this internal command:

```json
{"v":1,"commands":[{"id":"resync-unique-id","op":"resync"}]}
```

It is module-wide, with no area or target, and returns the ordinary ok result.
The server forces the next report full. The bridge retries the control until a
full baseline arrives; an acknowledged resync without a full response gets a new
ID. Controls take one of the existing 256 command slots and are not exposed as
agent operations. Duplicates/stale deltas do not mutate state or create another
resync. A boot change discards the old baseline. Status adds `delta` (latest
report type), `resyncPending`, and cumulative service-store `resyncs`.

`store.rs` bounds merged object counts and rejects duplicate area/object IDs;
legacy reports without base_seq retain their original full/partial semantics.
Unit tests cover merge, deletion, omitted-field reset, gap/retry, lost recovery,
stale/duplicate input, reboot, new-area baseline and retained-object limits.
The guarded mg-live gate passes **55 active tests**, two existing manual tests
ignored.

Engine reproduction: rebuild `live_ops_proof` with the guarded shared-target
command above, then `test-events.sh --phase delta`. The production API populates
600 placeables, observes a renamed row and a removal (599 retained), injects one
sequence gap, observes Moonglow's actual resync publication and recovered state,
and waits for the next unforced full report. Independent raw Redis capture
confirms updates/removals and counts actual received bytes (not reserialized JSON).
The passing `delta.json` / `delta-state.json` record boot
`183b6c9829f78b4daf9d255493150c29`, sequences 2-47, full snapshots at 10/16/46,
**110495 full-report bytes** and **212-byte mean across 32 quiet deltas**.
These are per-report payload sizes, not a claim about total bandwidth including
periodic fulls. The first 400-placeable attempt measured only 73582 bytes and
correctly stayed below the activation threshold; 600 exercises the actual large-
report path. Both owned native children and the proof client exited.


## Client-scoped operations (E7.3)

The complete credential/configuration contract is in
[Agent protocol: named Live clients](agent-protocol.md#named-live-clients-and-audit-e73).
No command input field grants client permissions. Both HTTPS routes enforce the
same group checks; missing scope returns HTTP 403/code `forbidden`, and an exhausted
named-client command quota returns HTTP 429/code `rate_limited` plus `Retry-After`.
The game's existing refusal codes are unchanged.

The game-side `players:true` flag is now the conjunction of local `Policy.players`
and the submitting client's `players` permission. This is captured per command,
then checked against current local policy again when publishing. API input still
cannot set `players`. Local UI/default-token commands retain existing permissions.
Read-only named clients do not receive chat/server logs without `read_chat`, nor
other clients' commands or return values. `MgReturn`/`live_wait` results continue to
round-trip unchanged for the submitting client and for the default token.

Service-only `LiveConfig.keep_wsl: Option<bool>` and `clients: Vec<ClientToken>` do
not change game commands/reports. See [the WSL lifecycle contract](agent-protocol.md#live-service-wsl-keepalive-e72)
for defaults and its proven shutdown behavior. No mg-ui source is changed here.


## E7/E8 reproduction and evidence closure (2026-10-12)

This batch starts from `c22576b` on `live-mode-events`. Item commits: `b626de1`
(chunk returns), `abc6514` (deltas), `0b21438` (owned WSL helper), `7d2afd0`
(named tokens/audit); `cea0ab8` repairs the independent MCP adapter's existing
schema subset for operation unions/exclusive bounds. The final token review also
applies policy before opening the listener and formats each audit row before its
single append, so concurrent service instances cannot interleave JSON fragments.
No mg-ui code, other server, real NWN user folder or game installation was changed.

Reproduce with the repository's guarded environment, one build at a time:

```powershell
$env:CARGO_TARGET_DIR='C:\Projects\moonglow-toolset-original-claude\target'
$env:CARGO_BUILD_JOBS='1'
$env:NWN_ROOT=Join-Path $PWD 'target/test-output/live-events/no-native-game'
# The guard path must not exist.
cargo +1.98.0 test -p mg-live
cargo +1.98.0 clippy -p mg-live --all-targets -- -D warnings
cargo +1.98.0 build -p mg-live --example live_ops_proof
wsl.exe -e bash /mnt/c/Projects/moonglow-toolset-original-astra2/tools/live/test-events.sh --phase chunk_return
wsl.exe -e bash /mnt/c/Projects/moonglow-toolset-original-astra2/tools/live/test-events.sh --phase delta
cargo +1.98.0 test -p mg-live keep_wsl_service_lifecycle -- --ignored --nocapture
```

The native harness enforces its own marker, locks, available UDP 5126/Redis 6392,
private launch, random unprinted admin credential and process identities. It uses
one current working module and `~/mglive-events`; its manifests bind the module,
compiler, helper, NWNX includes/plugins, server and proof client by SHA-256. It
never starts a Windows nwserver. The final chunk proof boot is
`e19d2ee95db8630121e6c4dd17cfb47e`; its API and native `value` observations agree.
The busy-area delta proof boot is `183b6c9829f78b4daf9d255493150c29` with the
110495-byte full / 212-byte mean quiet delta measurements recorded above.

Minimal evidence remains under `target/test-output/live-events/`: `chunk_return.json`
and `chunk_return-state.json`, `delta.json` and `delta-state.json`, and
`keep-wsl-proof.txt`. `e7-e8-validation.json` records the guarded test outcomes;
`e7-e8-files.json` records exact owned paths, purposes and retention state;
`e7-e8-cleanup.json` records individual removed paths and measured logical sizes.
The current source module, packed module, runtime module and extracted include
inputs remain for reproduction. The private children exited and their two ports
were free before cleanup. Closed engine phases no longer need their regenerated
settings, random secret, logs, SQLite caches, compiler cache, empty channel logs
or preliminary empty WSL snapshot: those 15 owned files total 133610 logical bytes.
They are removed after matching marker/locks/paths and inactive process identities.
The shared Cargo target and all earlier accepted proof packets are retained.
No physical allocation or free-space gain is attributed while other workers build.

Session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a` and its active history
the agent's local Codex session file
are preserved. History cleanup requires a separately authorized supported process
after this session; no such cleanup is performed here. Passing tests and engine
observations are evidence, not coordinator acceptance. Remaining integration is
the coordinator's Options editor/checkbox and UI initializer/wiring changes.

## DM creature controls

Every example below adds the common `id` and `area` fields. All are asynchronous
commands in client group `npc`; `live_wait` returns `{id,ok:true}` or the usual
`code` refusal. `target` is a hexadecimal creature ID. Dead creatures refuse
kill/heal/rest/limbo with `dead`. Missing/foreign-area objects return
`not_found`/`other_area`; wrong types and malformed values return `invalid`.
PCs, DMs, possessed creatures and PC associates follow the existing protection
rules. Direct player targets additionally require local Players, token `players`
and server `MGLIVE_ALLOW_PLAYER_OPS=1`. Limbo never accepts direct players.

| API / wire op | Additional command fields | Native effect |
| --- | --- | --- |
| `live_kill` / `kill` | `{"target":"3"}` | Instant supernatural `EffectDeath(FALSE,TRUE)`; temporarily clears and restores plot/immortal flags. Returns death readback and overridden flags (see owner regression below). |
| `live_heal` / `heal` | `{"target":"3","amount":2}` | Instant heal; optional integer amount 1..100000, omitted means maximum HP. NPCs need no player switch. |
| `live_set_invulnerable` / `set_invulnerable` | `{"target":"3","enabled":true}` | `SetPlotFlag`; disabling requires `false`. |
| `live_set_immortal` / `set_immortal` | `{"target":"3","enabled":true}` | `SetImmortal`. |
| `live_set_ai` / `set_ai` | `{"target":"3","level":"normal"}` | `SetAILevel`; selectors `default`, `very_low`, `low`, `normal`, `high`, `very_high`. |
| `live_force_rest` / `force_rest` | `{"target":"3"}` | Cancel Moonglow hold, then native `ForceRest` (instant engine rest). |
| `live_limbo` / `limbo` | `{"target":"3"}` | Cancel hold/actions, `NWNX_Creature_JumpToLimbo`: removes from area and adds to module limbo. Object remains valid. |
| `live_unlimbo` / `unlimbo` | `{"target":"3","x":12,"y":8,"z":0}` | Jump the same area-less creature to finite coordinates in command `area`. Already in an area: `invalid`. |

Unlimbo intentionally does not require a cached report row (limbo removed it).
The native operation checks the live object, creature type and absence of an
area, and refuses direct players. Never implement limbo by destroying/recreating
the creature: inventory, locals and object identity must survive.

UI handoff: creature menu Kill, Heal (optional positive amount), Invulnerable
and Immortal boolean toggles, AI six-value selector, Rest and Send to limbo.
Keep the returned limbo object ID in a recoverable list; Return from limbo takes
a destination area and point. Direct PC controls require the existing Players
switch; hide/disable limbo for them. No `mg-ui` changes are part of this group.

Native AI qualification: `AI_LEVEL_VERY_HIGH` is 4, but built-in `SetAILevel`
immediately reads back 3 on this engine; the next tick also reads 3. An attempted
dynamic `NWNX_Object_SetAILevel` call also left the value at 3, but dynamic NWNX
getters later proved unreliable in the ability-score probe below, so that attempt
does not independently establish what a precompiled NWNX setter would do. Other levels read 0/1/2/3 as requested. Default (-1)
restores automatic scheduling (0 in the empty fixture). `set_ai` returns
`value:{"requested":4,"level":3}` (analogous integers for every selector), so
show the effective level in the UI. This is a measured native limitation.

Proof: `wsl -e bash /mnt/c/Projects/moonglow-toolset-original-astra2/tools/live/test-events.sh --phase dm_creatures`.
`target/test-output/live-events/dm_creatures.json` records source/plugin/server
hashes, all native readbacks and child cleanup; `dm_creatures-state.json` retains
reports/results. The phase passed on UDP 5126 / Redis 6392 in the owned
`~/mglive-events` scratch. HP is reported as `[current,maximum]`; testing against
a scalar was a fixture error. The stock bartender death script removes its
corpse, so the proof disables that script and destruction before kill; production
kill deliberately still runs module death behavior. Guarded `mg-live --lib`
tests: 67 passed, 3 ignored; regenerated OpenAPI has 37 operations with all
optional policies enabled. Queries are native measurements, not visual-client
acceptance. No player client was attached in this phase.

## DM local variables

`get_variable` and `dump_locals` are read-only catalogue entries in group `read`.
They still queue an engine request and deliver `value` through `live_wait`.
`set_variable` requires the independent `variables` client group. All use the
common `id,area`; `target` is an object hex ID, `"module"`, or `"area"` (the
command's area). Object targets must be in that area; reading PCs is permitted,
writing protected players/associates needs the existing Players intersection.

Examples (add `id,area`):

```json
{"op":"get_variable","target":"module","type":"int","name":"quest"}
{"op":"set_variable","target":"area","type":"float","name":"water","value":1.25}
{"op":"set_variable","target":"3","type":"string","name":"note","value":"agent local"}
{"op":"set_variable","target":"3","type":"object","name":"friend","value":"4"}
{"op":"set_variable","target":"module","type":"location","name":"home","value":{"area":"events","x":12,"y":8,"z":0,"f":90}}
{"op":"dump_locals","target":"3"}
```

Names are 1..128 ASCII letters, digits, spaces or `_ . -`. Case-insensitive
`MGLIVE_` and `NWNX_` prefixes are reserved: reads/writes return `invalid`, dumps
omit them. This prevents variables-scoped tokens changing server permission
switches or reading transport/chat buffers. Native module code may still use
those locals normally.

Types: `int` signed 32-bit; `float` finite 32-bit; `string` at most 4096 game
characters; `object` valid ID or null; `location` `{area,x,y,z,f}` or null (clear).
Get/set success is `{id,ok:true,value:...}` with the normalized native value.
Unset int/float/string return 0/0/empty string; invalid object/location return
null. Unknown value object/location area returns `not_found`, wrong target area
`other_area`, wrong value/type/name `invalid`, protected write `protected`.
A read result exceeding 16384 serialized game bytes returns `invalid`.

Dump result example:
`{id,ok:true,value:{locals:[{name:"quest",type:"int",value:42}],total:3,omitted:2,truncated:false}}`.
It inspects at most 256 native entries, returns at most 128 rows and 15000
serialized row bytes. `total` is the native count, `omitted` the reserved entries
actually examined, `truncated` means unexamined entries remain. The NWNX iterator
has unstable ordering and omits default-valued locals (8193.14+); dump does not
claim to enumerate unset values. Native JSON locals are returned as type `json`;
unknown/cassowary locals have type `unknown`, value null. Writing those two types
is deliberately unsupported. No stable paging is promised.

Proof: `tools/live/test-events.sh --phase dm_variables` on the same private
5126/6392 fixture passes all five types on object, area and module, independent
native getter readbacks, null clearing, reserved/type/size refusals and a
160-local bounded dump. Evidence: `target/test-output/live-events/dm_variables.json`
and `dm_variables-state.json`. Guarded Rust wire/type tests and both HTTP-route
scope tests cover the three operations; OpenAPI catalogue total is 40.
UI handoff: local inspector target choice (object/module/area), refresh/dump,
name/type editor and type-specific value editor. Show truncation/omission and
native-default caveats. Add `variables` to the named-client group selector.

## DM inventory

`view_inventory` is read-only, group `read`; `take_item` is group `npc`.
Both require the common `id,area,target` (holder creature or placeable). PC/DM
holders and PC associates require the existing Players permission intersection;
without it, `protected`. Dead NPC inventories are readable. Wrong holder type
is `invalid`. Targets must be in the command area.

```json
{"op":"view_inventory","id":"inv","area":"events","target":"3"}
{"op":"take_item","id":"take","area":"events","target":"3","item":"8"}
{"op":"take_item","id":"take2","area":"events","target":"3","resref":"nw_it_mpotion001"}
```

View result: `{id,ok:true,value:{items:[{id:"8",resref:"nw_it_mpotion001",tag:"DM_POTION",name:"Potion of Cure Light Wounds",stack:3,slot:-1}],truncated:false}}`.
`slot` is the native `INVENTORY_SLOT_*` integer, -1 for backpack contents.
Equipped items are scanned explicitly before the backpack and deduplicated:
this engine's `GetFirstItemInInventory` does not enumerate equipped gear.
The result is bounded to 128 items / 15000 serialized row bytes. Show the
`truncated` indicator; there is no stable paging contract.

Take requires exactly one of `item` (hex ID) or `resref`. Resref selects the first
backpack match, then the first equipped-slot match. It destroys one entire
item/stack, returns `{id,ok:true,value:"8"}` (the removed ID), and the next view
must omit it. It does not transfer to an unreported DM avatar. A foreign-holder,
missing or non-item ID returns `not_found`; plot items return `protected`.

Proof phase `dm_inventory` records independent native slot/stack readbacks,
backpack and equipped removal, invalid/mutually-exclusive selectors, foreign
inventory refusal and plot-item protection. The initial equipment probe showed
that a newly created second weapon could not equip over the NPC's existing
weapon; reading native slots exposed the iterator omission. The final proof
uses the stock NPC's actual equipped item. Evidence lives in
`target/test-output/live-events/dm_inventory{,-state}.json`. Guarded mg-live:
70 passed, 3 ignored; catalogue has 42 operations with optional policies enabled.
UI handoff: Inventory opens an asynchronous item table with slot/stack and a
truncation badge. Remove uses the selected row's ID and clearly states that the
whole stack is removed. Resref removal is also available to agents.

## DM world controls

Common `id,area` is required for command context. `set_time`/`set_date` require
client group `npc`; `jump_all_players` requires `players` and both existing
local/server switches (missing permission is `disabled`). Success is normal
`{id,ok:true}`; gathering additionally returns `affected:[id,...]`.

```json
{"op":"set_time","id":"clock","area":"events","hour":13,"minute":17}
{"op":"set_date","id":"date","area":"events","year":1372,"month":6,"day":2}
{"op":"jump_all_players","id":"gather","area":"events","x":12,"y":8,"z":0,"players":true}
```

Hour/minute are integers 0..23/0..59. An earlier clock time advances to the next
day. The API minute is a civil minute: implementation converts using
`NWNX_Util_GetMinutesPerHour`. Contrary to the bounds comment in the game's
`nwscript.nss`, native `SetTime(13,17,0,0)` on a 2-minutes-per-hour module reads
21:01, because the minute is a real-time offset within the compressed hour.
The proven conversion is `seconds = minute * minutesPerHour`, then native
`SetTime(hour,seconds/60,seconds%60,0)`. Clock time continues advancing normally.

Dates require year 0..32000, month 1..12, day 1..28 (every NWN month has 28 days).
Past dates return `invalid`: the native calendar cannot move backwards. No
rollover coercion for invalid input. Getters can be used in a returned chunk to
read calendar/time; no new report fields are introduced here.

Gather snapshots living connected PCs, excluding DMs/DM-possessed avatars and
dead PCs, clears their actions and jumps them to the finite destination point.
NPCs are never gathered. `{affected:[]}` is valid on a clientless server.
`dm_world` proves native date, converted time and next-day rollover plus range
and server-switch refusals. `pc_dm_world` proves the command flag requirement,
empty-client selection and unchanged NPC position. **Actual connected-PC
movement remains unproven without a private client.** Retained manifests and
state streams are under `target/test-output/live-events/` with those phase names.
Rust guarded tests: 71 passed, 3 ignored before the description-only OpenAPI
refresh; catalogue has 45 operations with all policies enabled.

`change_difficulty` is not exposed. The inspected NWNX unified includes provide
an event notification but no safe difficulty setter; Administration play options
0..28 also lack difficulty. No engine memory patch or server-config rewrite is
performed. DM avatar navigation/possession is outside this service's scope.
UI handoff: module menu date (28-day calendar), hour/minute clock, and Gather
players with destination area/point under Players. Show the forward-only
calendar and next-day clock behavior. Do not add a difficulty control.

## DM traps

`disable_trap` and `spawn_trap_on_object` are group `npc`, writable catalogue
entries with common `id,area,target`. Target must be a door/placeable in the
selected area. Success is `{id,ok:true}`; verify trap state with native getters.

```json
{"op":"disable_trap","id":"disarm","area":"events","target":"2"}
{"op":"spawn_trap_on_object","id":"trap","area":"events","target":"2","trap":0}
```

`trap` is an integer 0..65535, a row in the running module's `traps.2da`.
The row must have a Label; missing rows return `not_found`. Existing active
traps refuse replacement with `invalid` (disable first). Native creation uses
Hostile faction and the trap row's default trigger script. Disable calls
`SetTrapDisabled`; it is harmless on an untrapped object. Wrong type/row format
returns `invalid`; missing and other-area targets use the normal codes.

Do not use `GetTrapActive` alone to determine whether an object has a trap:
untouched fixture placeables returned true even when untrapped. The operation
checks `GetIsTrapped && GetTrapActive`; proof checks `GetIsTrapped` and
`GetTrapBaseType` after attachment/disable/replacement. `dm_traps` passes on
both the authored chest and door, rows 0 and 1, including missing row 65535,
range/type refusals and active replacement refusal. Evidence:
`target/test-output/live-events/dm_traps{,-state}.json`.
UI handoff: door/placeable menu Disable trap, Add trap with traps.2da row/label
picker. Existing open/close/lock operations already cover DM door controls.

## DM player progression, abilities and reputation

`give_level`, `give_alignment` and `set_stat` require client group `players`,
local Policy.players, server `MGLIVE_ALLOW_PLAYER_OPS=1` and wire `players:true`,
even for NPC stand-ins. Missing switches return `disabled`. Target must be a
living creature in the command area. Since B15, the two reputation operations
also work on NPCs without Players: read uses group `read`, write uses `npc`.
Protected targets still require both player switches and the token Players group.

```json
{"op":"give_level","id":"level","area":"events","target":"3","players":true}
{"op":"give_alignment","id":"alignment","area":"events","target":"3","law_chaos":10,"good_evil":-20,"players":true}
{"op":"set_stat","id":"strength","area":"events","target":"3","ability":"strength","value":19,"players":true}
{"op":"get_faction_reputation","id":"rep","area":"events","target":"3","faction":"merchant","players":true}
{"op":"set_faction_reputation","id":"setrep","area":"events","target":"3","faction":5,"reputation":75,"players":true}
```

`give_level` reads current hit dice (supported 1..39) and the next threshold from
`exptable.2da` row=current level, column XP. It grants the XP difference, without
choosing a class or forcing a level-up. Already enough XP, or max level, returns
`invalid`; absent/bad threshold returns `not_found`. Success example:
`value:{xp:1000,eligible_level:2,granted:1000}`. The reported `xp` changes; the
player still performs normal level-up choices.

`give_alignment` requires both integer deltas -100..100. Positive means law/good,
negative chaos/evil. It clamps the resulting axes to 0..100 and changes only the
target via NWNX_Creature alignment setters. Native `AdjustAlignment` was rejected
for this contract: the engine moved good/evil 50 minus 20 to **15**, not 30,
when crossing an alignment category. Success example:
`value:{law_chaos:60,good_evil:30}`.

`set_stat` selects strength/dexterity/constitution/intelligence/wisdom/charisma
and integer raw `value` 0..255. It calls `NWNX_Creature_SetRawAbilityScore` and
returns the raw score in `value` (e.g. 19); racial/effect modifiers are excluded.
Constitution uses the native setter's HP recalculation. No class levels or feats
are fabricated. Unknown selector, out-of-range or noninteger values: `invalid`.

Reputation direction is **specified faction towards target creature**. Faction
is the existing standard name or raw ID. Standard IDs 1..4 use native standard
faction getters/setters (enum = raw ID minus one); source faction 0 (PC) is
`disabled`. Custom factions use an extant, non-plot creature representative,
`GetFactionAverageReputation` and `AdjustReputation`. Search is bounded to 256
areas/4096 objects; absent eligible representative returns `not_found`, including
empty/limbo-only factions. Reputation is integer 0..100. Native faction behavior
can affect more than this individual, particularly when the target is an NPC in
a shared faction. Results report actual native values:
`value:{faction:5,before:50,reputation:75}`. Reads are `readOnly:true` and return asynchronously. Since B15, NPC targets
do not require Players; PC/DM/associate targets still require both switches. Custom faction averages follow native
personal/faction reputation semantics; the API reports actual readback rather
than promising an independent per-object database value.

UI handoff: Grant next level XP action; two signed alignment inputs; ability
selector plus raw score; faction selector plus read-current and 0..100 editor.
Always show actual returned values, and explain that XP eligibility does not
complete player level-up. Never forward a user-provided `players` field through
the API; the established Policy/token intersection owns that wire field.

Proof phases `pc_dm_players` and `dm_players_disabled` pass on the private
5126/6392 server. The enabled phase checks next XP in the following area report,
independent native base-score getters for all six abilities on the human NPC
fixture, exact signed alignment changes, and both merchant and custom Agents
faction standings via native getters. Missing command flag and server-disabled
cases are refused. No client was connected: PC character-file persistence,
client level-up UI and party interactions remain for private-client acceptance.
`ExecuteScriptChunk` calling the NWNX raw-score getter returned 0 despite the
compiled operation returning 19; native `GetAbilityScore(...,TRUE)` read back 19
on the human fixture. Therefore the proof uses independent built-in getters,
not the unreliable dynamic NWNX call. This observation is not evidence that a
real PC is updated. Keep it as a diagnostic limitation for future chunk work.

Guarded `mg-live --lib`: 73 passed, 3 intentionally ignored (external Redis,
manual TLS smoke, WSL lifecycle); OpenAPI regenerated with 52 operations when all
optional policies are enabled. Exact per-op game JSON, target/type/value refusals,
player policy and client-scope coverage are included.

## NWNX DM event coverage and UI handoff

The source inventory is `Plugins/Events/NWScript/nwnx_events.nss` and
`Plugins/Events/Events/DMActionEvents.cpp` in the NWNX unified checkout. BEFORE
and AFTER notifications represent the same action. The table maps every distinct
`NWNX_ON_DM_*` action; it does not equate a notification with a supported setter.

| DM action(s) | Live operation / UI input |
| --- | --- |
| KILL, HEAL, FORCE_REST | kill, heal (optional amount), force_rest; creature target |
| TOGGLE_INVULNERABLE, TOGGLE_IMMORTAL, TOGGLE_AI | set_invulnerable / set_immortal boolean; set_ai selector and actual-level readback |
| LIMBO | limbo; retain ID for unlimbo destination area/XYZ |
| GET_VARIABLE, SET_VARIABLE, DUMP_LOCALS | get_variable / set_variable / dump_locals; object/module/area target, name/type/value editor; separate variables token group |
| VIEW_INVENTORY, TAKE_ITEM | view_inventory / take_item; selected inventory holder and item ID or resref; whole-stack removal |
| SET_TIME, SET_DATE | civil hour/minute and forward-only NWN year/month/day |
| JUMP_ALL_PLAYERS_TO_POINT | jump_all_players; destination area/XYZ, Players permission; connected-client proof pending |
| GOTO, JUMP_TO_POINT, JUMP_TARGET_TO_POINT | existing pc_teleport with explicitly selected creature and destination; DM-avatar selection/camera navigation remains client UI behavior, no implicit service avatar |
| SET_FACTION | existing set_faction; standard faction name/raw ID; existing target protections remain |
| GET_FACTION_REPUTATION, SET_FACTION_REPUTATION | faction towards creature, actual reputation 0..100, Players permission for protected targets |
| GIVE_LEVEL, GIVE_ALIGNMENT, SET_STAT | XP eligibility action; two signed deltas; ability/raw-score editor, Players permission |
| GIVE_GOLD, GIVE_XP, GIVE_ITEM | existing pc_give_gold / pc_give_xp / pc_give_item; amount or item resref, Players permission |
| SPAWN_OBJECT | existing spawn; blueprint kind/resref, count/faction and destination |
| SPAWN_TRAP_ON_OBJECT, DISABLE_TRAP | traps.2da row picker / disable on door or placeable |
| TOGGLE_LOCK | existing set_state lock boolean on door/placeable |
| CHANGE_DIFFICULTY | omitted: no supported safe setter found in NWNX includes/Administration options |
| POSSESS, POSSESS_FULL_POWER, APPEAR, DISAPPEAR | explicitly out of scope: client/avatar controls |
| PLAYERDM_LOGIN, PLAYERDM_LOGOUT | lifecycle notifications, not remotely invocable DM editing actions |

Per-section UI handoffs above describe defaults, limits and returned values.
No `crates/mg-ui` file was edited by this worker; the required live-mode merge
included the other workers' existing UI changes. The coordinator owns menu wiring
and acceptance. Test passes here do not imply coordinator or owner acceptance.

## Reproduction and closing checks for the DM batch

Native harness: `tools/live/test-events.sh --phase PHASE` through WSL, phases
`dm_creatures`, `dm_variables`, `dm_inventory`, `dm_world`, `pc_dm_world`,
`dm_traps`, `pc_dm_players`, `dm_players_disabled`, `pc_dm_bounds`.
Use the existing owned scratch `~/mglive-events`, UDP 5126 and Redis 6392. Each
phase holds the ownership lock, refuses occupied ports, starts private children
with a random unprinted admin password, and verifies identity before reaping
only those children. No external server is stopped. Sources compile using the
existing `mg.exe` in the coordinator's shared target; no new Cargo target exists.

NWNX source checkout `~/unified` and reconnect checkout
`~/nwnx-redis-reconnect` report commit
`3f504d0f1463e49d405639bd3d454ae7416b1a98`. The reconnect Redis plugin includes the
coordinator's local patch; the proof manifests hash each actual plugin, include
archive and `~/server/bin/linux-x86/nwserver-linux`, so the checkout
revision alone is not claimed as exact binary provenance. The authored fixture,
current compiled module and all phase manifests/state streams are retained in
`target/test-output/live-events/`; the module is rebuilt in the same directory
for each phase. No additional game installation or resource tree is copied.

Rust checks (PowerShell, in this worktree):

```powershell
$env:CARGO_TARGET_DIR='C:\Projects\moonglow-toolset-original-claude\target'
$env:CARGO_BUILD_JOBS='1'
$env:NWN_ROOT=Join-Path $PWD 'target/test-output/live-events/no-native-game'
$env:MOONGLOW_UPDATE_OPENAPI='1'
cargo +1.98.0 test -p mg-live --lib
cargo +1.98.0 clippy -p mg-live --all-targets -- -D warnings
```

The NWN_ROOT path must not exist. Latest full tests: 73 passed/3 ignored;
Clippy all targets passed with warnings denied. `pc_dm_bounds` additionally
proves that wide JSON integers cannot wrap into valid NWScript int values:
`JsonGetInt` truncates to 32 bits, so integer fields are checked against their
serialized full value before range validation. Nine overflow probes were refused
across clock/date/trap/heal/faction/alignment/stat/reputation/local operations.

Knowledge index is maintained by hand (okf unavailable). The per-task exact-path
ledger is `target/test-output/live-events/dm-files.json`, task `live-dm-actions`,
session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`. Sources and minimal proof packets
remain required for review. The shared Cargo target, game dependencies, earlier
accepted phase proofs and active session history are retained. No physical disk
recovery is attributed while other workers are active.

Final retention/cleanup: closed phases removed 20 identified transient/empty-log
files (131000 logical bytes), then the final Polish regression regenerated its
scratch state and cleanup removed 12 file instances (131913 logical bytes).
Total deletion work: 262913 logical bytes, with repeat paths clearly listed in
`dm-files.json`; this is not a claimed physical free-space delta. All phase
manifests, nonempty state evidence, source/packed modules, original inputs and
the shared Cargo target remain. Private children exited and ports 5126/6392 were
free before removal. `tools/live/cleanup-events.py PHASE` reproduces guarded
cleanup in WSL, checking the marker, last-run process identities, ownership lock,
passed proof and idle ports; it records exact candidates before deletion.

Final review restored the existing Polish fixture phrase using Unicode escapes
after an intermediate PowerShell-default encoding read had changed that test
literal. `polish_cp1250` was rerun successfully with the final game scripts;
its manifest/state packet is retained. No production Unicode conversion changed.
Documentation links were checked locally and resolve. The active session history
path recorded earlier remains untouched; separate history cleanup is still
unapproved and has not run. Remaining proof gaps concern a connected private
player client (gather, persistence and actual level-up UI), custom-faction
personal-reputation edge cases, and the noted dynamic NWNX getter discrepancy.

## Owner regression fixes and quest-fight factions

Current `npc_attack` adds optional `faction` (standard name or raw module ID,
1..65535). It must exist and differ from the victim's faction; same victim/PC
faction is `invalid`, missing faction `not_found`. Without it, same-faction
attacks use Commoner (2) against Hostile (1), otherwise Hostile (1). Personal
hostility is then applied across the distinct factions. Switching always uses
a bounded hold and restores the original faction on timeout, victim death or a
new order. Result example: `faction:{original:1,temporary:2}` (replaces the old
`"hostile_temporarily"` string). This supersedes earlier Hostile-only behavior.

```json
{"id":"quest-fight","op":"npc_attack","area":"events","target":"3","victim":"6","faction":5,"hold":true,"timeout":60}
```

For a quest-specific fight, author a spare faction in the module faction editor,
with a stable raw ID in `repute.fac`, a clear name (e.g. QuestFightA), and neutral
reputation towards unrelated factions. Leave it empty until an encounter uses
it. Pass that ID in `faction`; the command creates temporary personal hostility
only for the pair. Distinct opposing groups need distinct spare factions. Do not
reuse PC faction 0 or the victim's faction. The attack command does not create factions or rewrite faction reputations.
Reserve factions in the module before loading it. The B15 reputation operations
below can change standings explicitly during a session.
Temporary membership can still affect third-party AI whose faction standings
are hostile to that spare faction, so choose module reputations deliberately.

Private `hostile_repro` retained the old successful dispatch/no-attack state.
`hostile_fight` proves attack events, Commoner substitution, restoration after
timeout, explicit custom faction 5 and restoration on a new movement order.
The fixture fight was moved away from its closed door after native ACTION_OPENDOOR
identified an unrelated path obstruction. Evidence: corresponding manifest/state
JSON in `target/test-output/live-events`. Rust: 75 passed/3 ignored, OpenAPI
regenerated. UI handoff: optional Fighting faction picker on Attack; omitted is
automatic. Rust `Op::NpcAttack` initializers need `faction:None` when unused.

### Forced kill: protection override

`kill` now temporarily clears plot and immortal, applies supernatural EffectDeath,
reads native GetIsDead, and restores both original flags. A living target that
survives returns `invalid`, never a successful kill. Existing PC/DM/associate
permission gates remain in force; an already-dead target returns `dead`.
Example result: `{"id":"k1","ok":true,"value":{"dead":true,"overridden":{"plot":true,"immortal":true}}}`.
The command does not destroy the object; module OnDeath scripts may still remove
corpses. UI: keep the Kill action, show overridden protections from result.value.

Private proof `test-events.sh --phase kill_override` preserves the corpse in the
fixture and verifies dead in the next report, object identity still valid, and
both protections restored by independent native readback. Historical pre-fix
`kill_repro.json` proves this revision refused a plot+immortal NPC as `protected`;
this differs from the owner's reported success-with-no-death and does not claim
to reproduce that exact success response. The fixed engine result explicitly
reports death, closing that ambiguity.

### Demo player switch and lifecycle

The next deliberate demo launch may use `tools/live/Start-LiveDemo.ps1 -AllowPlayerOps`
(with the owner's normal other arguments). This explicitly passes
`MGLIVE_ALLOW_PLAYER_OPS=1`; omitting it always passes `0`, even if the parent
shell has an enabling environment variable. Moonglow Options `players` must
also be on, and named clients need their operation group plus `players`.
The switch affects the server launch, not an already-running server.

For a living PC: `heal` / `pc_heal` heal, `kill` kills with the override behavior
above, `pc_raise` returns `invalid` (already alive). For a dead PC: heal and kill
return `dead`; `pc_raise` applies resurrection then full heal. These are engine
operation semantics; a module's PC OnDeath/OnRespawn scripts may add behavior.

Proof: `tools/live/test-launcher-players.ps1` parses the launcher and exercises
only an extracted helper with mocks (no launcher or process started), including
ambient env=1 with switch omitted. `test-events.sh --phase pc_lifecycle` proves
living/healed, killed, dead refusals and full-HP resurrection on the owned NPC
stand-in with the real server switch. Rust tests use actual `pc:true` reports
for both living/dead permission and refusal cases. **Real connected-PC proof is
pending**: no private client is available; no claim is made about client death
UI or module player scripts. The running owner demo was not touched.

### Tagged visual effects (NPC scope)

- `apply_vfx`: `{"id":"v1","area":"events","op":"apply_vfx","target":"3","row":411,"duration":"temporary","seconds":3}`.
  Supply **exactly one** of target object ID or `"location":{"x":5,"y":5,"z":0}`
  in the command area. Row is integer 0..65535 and must exist in visualeffects.2da.
  Duration is `instant`, `temporary`, or `permanent`; seconds is required only for
  temporary and must be finite, >0 and <=86400. Lasting effects require Type_FD=D.
  Missing row returns `not_found`, bad combinations/types/ranges `invalid`.
  Result: `{"id":"v1","ok":true,"value":{"row":411,"duration":"temporary"}}`.
- `clear_vfx`: `{"id":"v2","area":"events","op":"clear_vfx","target":"3","row":411}`.
  Optional row (0..65535) filters; omit it to clear all live-owned effects on that
  target. Result: `{"id":"v2","ok":true,"value":{"removed":1}}`.

Both are `npc` group mutations. Protected PC/DM/associate targets require the
existing server/command/local player permissions. Location effects have no
individual target. Effects use tag `MGLIVE_VFX`; clear removes only that tag and
visual-effect type, never another script's effect, including the same row.
Location effects are not attached to a selectable target: clear_vfx cannot remove
those; use temporary duration when location cleanup is required.

Proof `test-events.sh --phase apply_vfx`: report shows two row411 effects with
different ownership; row filter removes exactly ours, native tag readback proves
the foreign effect survives; temporary row412 appears then expires; clear-all
removes ours only. Instant object/location commands execute successfully, but
**instant and location rendering are not independently proved without a client**.
The result acknowledges application, not a render observation. Lasting creature
rows are independently visible in report `vfx` (including duplicate rows).
UI inputs: target/location choice, VFX row picker, duration selector, conditional
seconds; Clear VFX offers row/all-ours. Endpoint names `live_apply_vfx` and
`live_clear_vfx`; read results via live_wait.

### Creature appearance (NPC scope)

`{"id":"a1","area":"events","op":"set_appearance","target":"3","row":0}`
sets `SetCreatureAppearanceType`. Row is integer 0..65535; the game checks the
appearance.2da LABEL cell (`not_found` for absent rows) and native readback.
Result: `{"id":"a1","ok":true,"value":{"app":0}}`. Creature reports now carry
`"app":0` on full and changed delta rows. Rust uses optional `LiveObject.app`;
old reports parse with None. Non-creatures return `invalid`; protected targets
need player permission. No body-part/equipment fields are implicitly edited.
UI: creature appearance row picker; endpoint `live_set_appearance`, group `npc`.
Proof `test-events.sh --phase appearance` changes a bartender to row0, checks
app in the next report and independently GetAppearanceType=0; checks absent
and overflowing rows. This proves native identity, not client model rendering.

### Object library: capture and restore

`capture_object` (`live_capture_object`, read-only, group `read`):
`{"id":"c1","area":"events","op":"capture_object","target":"3"}`.
Returns `{"id":"c1","ok":true,"object_json":{"__data_type":"UTC ","...":"native typed GFF fields"}}`.
The example ellipsis is explanatory, not a valid field. `object_json` is a new
result field; use it directly as spawn_json.json. It is limited to 262144 JSON
bytes (256 KiB); oversized capture returns `invalid`, never truncates a library
entry. Ordinary result.value/chunk returns retain their separate 16 KiB limit.
The snapshot uses `ObjectToJson(target,FALSE)` without saved runtime state.
PC/DM snapshots are refused as `protected`; supported types are creatures,
placeables, doors and items (`UTC ` / `UTP ` / `UTD ` / `UTI `).

`spawn_json` (`live_spawn_json`):
`{"id":"c2","area":"events","op":"spawn_json","json":<object_json>,"x":6,"y":5,"z":0,"f":90}`.
`json` is a JSON object, not a JSON-encoded string. Requires **both spawn and
scripts client groups**, plus Options scripts enabled, because native snapshots
include event-script references. Invalid shape/type/size/PC/DM data returns
`invalid`. Rust validates depth<=32, finite numbers, 256 KiB in publisher JSON,
and passes the tree through the existing checked mg-gff decoder. The game
independently checks size, supported data type, typed field shape, scalar ranges,
labels, and nesting before `JsonToObject(data,location,OBJECT_INVALID,FALSE)`.
Result: `{"id":"c2","ok":true,"obj":"6"}`; the created object is live-owned.
No dependency resources are embedded: the destination module must still resolve
appearance rows, models, item resources and scripts used by the snapshot.

**Native behavior:** creation runs the restored OnSpawn script. The bartender's
script added a Spirits item during private proof; captured items and stack sizes
survived. This operation does not suppress module scripts or promise an immutable
inventory after they run. UI library should save the full snapshot with module/
resource provenance, show this behavior, and offer a destination xyz/facing picker.
It does not need a target when restoring. Large snapshots are returned through
live_wait; Live Log events record only capture_bytes, not a duplicate GFF tree.

Proof `test-events.sh --phase capture`: capture named, altered-appearance NPC
with a three-item potion stack; destroy the source; recreate at another point;
verify new identity, position/name/app in report and captured inventory stack via
view_inventory. Reject malformed, PC, overflow and oversized input. This proves
serialization independence from the old object, not week-long storage or client
rendering. `capture_probe` documents native JSON type names (`cexostring`, not
`string`), the UTC marker and source size.

Library command transport batches at most four captures and 1 MiB of commands;
the game defers captures when pending results need draining. The retained native
capture packets also pass the Rust GFF decoder in the ignored
`library_engine_snapshot_ingestion` test with `MGLIVE_CAPTURE_PROOF` set to the
absolute `target/test-output/live-events/capture-state.json` path.

### Nearby player skill check

Current third-acceptance flow: **the player must click Roll**. This replaces the
previous immediate GetIsSkillSuccessful/OK-only flow.

`live_skill_check` / `skill_check` requires group `players` and both switches.
Supply a living PC `target` OR finite x/y/radius exclusively. Radius is >0..30m;
DMs/possessed/dead PCs are excluded. A skill is an existing skills.2da row
0..65535; DC is integer 0..255. Messages are optional, empty or 1..200 nonblank
game characters. New optional fields:

| Field | Type / default | Meaning |
|---|---|---|
| `bonus` | integer -100..100, default0 | DM bonus; locked against player edits |
| `show_dc` | boolean, defaulttrue | Show DC in the player's window/log equation |
| `timeout` | integer seconds 1..600, default120 | Time allowed for the Roll click |

```json
{"id":"s1","area":"events","op":"skill_check","target":"9","skill":1,"dc":15,"bonus":2,"show_dc":true,"timeout":120,"success_text":"You notice tracks.","fail_text":"","players":true}
{"id":"s1","ok":true,"pending":true,"value":{"skill":1,"name":"Concentration","dc":15,"bonus":2,"show_dc":true,"timeout":120,"players":[{"id":"9","status":"waiting","nui_token":1}],"truncated":false}}
{"id":"s1","ok":true,"pending":false,"value":{"skill":1,"name":"Concentration","dc":15,"bonus":2,"show_dc":true,"timeout":120,"players":[{"id":"9","status":"rolled","nui_token":1,"roll":14,"modifier":6,"total":22,"passed":true}],"truncated":false}}
```

The command is accepted immediately; `live_wait` returns the latest snapshot.
While `pending:true`, poll again; each player response replaces that result under
the same command ID. The command is never republished to reroll it. Final
`pending:false` can be omitted by Rust serialization (false is the default).
Per-player `status` is `waiting`, `rolled`, `no_roll` (timeout), or `busy` (already
has a pending request). Only `rolled` includes roll/modifier/total/passed. No PCs
produces an immediate final empty list. At most 64 recipients/request and 16
active requests; `truncated:true` means additional radius recipients were not
invited. A second single-player request reports busy without replacing its
existing prompt. Server restart or missing final transport response expires the
Rust pending state (timeout plus 5s delivery allowance), not a fabricated roll.
Progress/final audit phases remain payload-free. Named clients can read only
their own command and skill-roll results; the default token and local UI see all.

NUI ID `mgliveSkill` has title **<Skill> check**, a DC (or DC hidden) line,
ranks/ability/other-bonuses breakdown, a large Roll button and OK. Nonzero DM
bonus is a signed label; zero adds no field. There are no editable or locked
bonus fields and no empty message scroll box. Initial display makes no random call. On the player's click, the server reads its stored inputs, calls
`d20()` exactly once and adds `GetSkillRank(skill, pc, FALSE)` plus DM bonus.
The comparison is `total >= dc`, without automatic 1-failure/20-success. Only the
player's matching window/token can trigger it; a duplicate click is ignored.
The full equation goes to SendMessageToPC and the NUI, followed by optional
success/failure text. When show_dc=false, the player's equation also omits DC;
the DM result still contains DC. Empty text leaves only the equation/result.
NUI unavailable means a chat notice and eventual no_roll, never an automatic
roll. Closing before rolling also leaves no_roll at timeout. OK or 30s after
completion closes the owned window; token checks protect later windows.

Example game event (also visible in Live Log):

```json
{"kind":"skill_roll","seq":12,"time":"1791900000.000001","id":"9","name":"Mira","area":"events","target":null,"request":"s1","status":"rolled","skill":1,"dc":15,"bonus":2,"roll":14,"modifier":6,"total":22,"passed":true,"text":"Concentration: 14 + 6 + 2 (DM) = 22 vs DC 15: success"}
{"kind":"skill_roll","seq":13,"time":"1791900120.000001","id":"a","name":"Second player","area":"events","target":null,"request":"s1","status":"no_roll"}
```

`name` in result value is the skill name; the player's identity is each row's
`id`. This result uses text_kind=skill for the shared raw-byte receive decoder.
The module NUI hook retains/forwards the previous handler for all other window
IDs. Installation is idempotent; modules changing their handler later must
preserve that chain. No extra plugin or copied NUI library is needed.

Native proof `test-events.py --phase pc_skill_rolls` uses only UDP5126/Redis6392
and owned ~/mglive-events. It proves `GetSkillRank(FALSE)` = 5 base ranks +4 CON
+3 Skill Focus feat +4 equipped item +5 effect =21, with independent base=5
throughout. The exact helper then produces one d20+21+2 result despite duplicate
invocation, no roll on invitation, a two-second no_roll timeout, busy isolation,
DC-hidden failure, handler forwarding and native no-client NuiCreate return0.
Exact installed declarations and NUI constructor signatures are extracted by
the guarded fixture test into native-script-api.txt / native-nui-api.txt; inputs
and script hashes are in target/test-output/live-events/pc_skill_rolls.json.
The dialogue/API proof's helper-build route is unchanged. Rust tests cover
wire defaults/bounds, progress merging, timeout/restart, late replays and scope.

**Connected-client check still required:** invite a living PC, verify no roll
until clicking, locked bonus and optional DC, one equation in its game log,
matching DM row, correct optional outcome text, duplicate-click immunity, OK and
30s close. Leave a prompt untouched for the configured timeout and expect No
roll; verify another module NUI still works. Headless NPC helpers and JSON do
not establish the positive player token, real click delivery or visual result.

UI handoff: `Tab::LiveSkill` / `Action::LiveSkill`; new state/Op fields are
`bonus:i16`, `show_dc:bool`, `timeout:u16`. Shared aiming copies them when clicking
radius centre; one-player sends directly. Result table and Live Log show full
rolls and waiting/no-roll states. See [window proof](live-windows.md#skill-check).

### NPC patrols

`live_patrol` / `patrol` (group `npc`):
`{"id":"p1","area":"events","op":"patrol","target":"3","points":[{"x":6,"y":5,"z":0},{"x":3,"y":5,"z":0}],"loop":true,"run":false}`.
1..16 finite XYZ points; loop/run default false. Non-player creature only;
player-owned associates need player permission. Dead target returns `dead`.
Result `{"id":"p1","ok":true}` accepts the route. The module-owned polling
chain holds commandability against routine AI, advances on actual arrival within
0.6m **horizontal** distance, and repeats when requested. Native walking projects
Z onto the walkmesh, so arrival does not require exact requested Z. An unreachable
point keeps retrying until cancelled. Three seconds without at least 0.15m
improvement toward the goal triggers a fresh move with an eight-second native
timeout. Alternate retries first try a small sideways/backward step (1.5m
sideways, 0.5m back, alternating sides, 1.5s timeout and inside area bounds).
Death or leaving the area cancels. No teleport is used.

A new movement, attack, speech, faction, rest, limbo, teleport, freeze/unfreeze,
script or chunk order cancels a patrol; a new patrol replaces it. Generation
checks prevent old callbacks restarting it. Cosmetic changes and read queries
do not count as movement orders. Explicit `{"id":"p2","area":"events","op":"stop_patrol","target":"3"}`
(`live_stop_patrol`) clears owned movement/restores commandability and returns
ok; if no patrol exists it leaves other holds alone.

New event example:
`{"seq":12,"kind":"patrol","time":"1791820000.123456","id":"3","name":"Bartender","area":"events","target":null,"generation":1,"point":2,"lap":1,"status":"reached","position":{"x":3,"y":5,"z":-1.3}}`.
Point/lap are zero-based; reached point2/lap1 in a three-point route finishes
lap two. `status:"blocked"` marks the first three-second stall at that point;
`status:"resumed"` marks renewed progress (or arrival) after a stall. For example,
`{"seq":13,"kind":"patrol","time":"1791820004.123456","id":"3","name":"Bartender","area":"events","target":null,"generation":1,"point":2,"lap":1,"status":"resumed","position":{"x":3.4,"y":5,"z":-1.3}}`.
Repeated retries while still blocked do not repeat the blocked event. Neither
event ends the patrol or changes its generation. Existing event
buffer/overflow rules apply. These are measured actor positions, not requested
coordinates echoed as arrival. UI: ordered waypoint editor (up to16), loop/run
checkboxes, Start/Stop buttons, progress from events.

Proof `test-events.sh --phase patrol`: six real arrivals, ordered 0,1,2 twice,
measured XY tolerance; a new move reaches its destination and no old arrivals
resume; explicit stop and one-shot completion restore commandability; freeze
cancels. Initial 3D-distance attempt stalled at XY destination because native
walkmesh Z was -1.3 instead of requested0; horizontal checks correct that failure.

Obstruction regression (2026-10-15): `test-events.py --phase patrol_obstacle`
puts a native chest on the first destination and holds it for 126 seconds.
With `MGLIVE_PATROL_BASELINE=1`, the fixture takes only the old patrol include
from commit `c51cec4` (never replaces working-tree files): active generation
becomes zero at the old 120-second cutoff, and removing the chest produces no
arrival. With the repaired include, generation 1 remains active after 40
retries; removing the chest produces `resumed`, then `(point,lap)` arrivals
`(0,0),(1,0),(0,1),(1,1)` without another patrol order. Explicit stop restores
commandability and no later callbacks emit progress. The separate `--phase
patrol` gate still proves ordinary two-lap movement, new-order cancellation,
explicit stop, one-shot completion and freeze cancellation.

The blocker is deliberately on the destination, so going around it cannot
falsely complete the leg while it remains there. Retry paths use native walking
and cannot guarantee a route through permanently impassable geometry; the
order stays blocked/retrying until stopped. Progress readbacks run on the area
and address the actor by ID: a `run_chunk` targeted at the actor is itself a
new order and would cancel the patrol before reading its active flag.

Before/after receipts (including source/module/plugin hashes and owned-process
exit identities) are `target/test-output/live-regression-patrol/patrol-before.json`
and `patrol-after.json`; the ordinary gate is `target/test-output/live-events/patrol.json`.
All use private UDP5126/Redis6392 and `~/mglive-events`; no demo lifecycle or app
build ran. UI consumes the existing patrol fields and the added `resumed`
status; no command schema or selection changes are needed.

Closeout for this patrol/large-look regression: the task ledger records six
redundant generated captures/duplicate diagnostics removed, **1,365,288 logical
bytes**. Minimal native receipts, decoder samples, source and the one reusable
private fixture workset remain. Allocated disk blocks/free-space movement were
not attributed to this cleanup. Session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`
history is untouched; separate history deletion has no approved retention rule.

### Live terrain tiles

`live_set_tile` / `set_tile` (group `npc`):
`{"id":"t1","area":"events","op":"set_tile","x":1,"y":1,"tile_id":51,"orientation":1,"height":1}`.
XY are **tile grid indices**, unsigned integers 0..65535, additionally checked
against native area width/height. The script converts them to world tile centres
(x*10+5,y*10+5). Tile ID 0..65535 must have a `[TILEn]` section in the active
ResMan-resolved tileset .set resource, including module/HAK overrides; absent
resources/IDs return `not_found`. Orientation 0..3 is quarter-turns anticlockwise,
height is integer 0..255 in native tile elevation units. Bad bounds return
`invalid`; a native readback mismatch also returns `invalid` with observed value.

Always passes RECOMPUTE_LIGHTING (4) and RELOAD_GRASS (1); edge coordinates also
pass RELOAD_BORDER (2). Result example:
`{"id":"t1","ok":true,"value":{"x":1,"y":1,"tile_id":51,"orientation":1,"height":1,"flags":7,"before":{"tile_id":50,"orientation":0,"height":0}}}`.
New event `kind:"tile"` carries the same value fields at top level, plus normal
seq/time/area/id/name/target fields; id/target null, name empty, area is the area
resref. Example:
`{"seq":1,"kind":"tile","time":"1791841618.158293","area":"events","id":null,"target":null,"name":"","x":1,"y":1,"tile_id":51,"orientation":1,"height":1,"flags":7,"before":{"tile_id":50,"orientation":0,"height":0}}`.
Consumers update live terrain from this event; the static authoring module/GIT
is not rewritten. Event overflow/gaps use the existing event rules; tile events
are not a full terrain snapshot, so a late subscriber must query the native
terrain explicitly rather than infer every tile from object reports.

UI: active-area tile picker/grid click, tile ID from that tileset, orientation
0/90/180/270, native height. Adjacent terrain matching and non-creature object
height changes are not automatic native SetTile behavior. A live terrain editor
should show the before/after value and retain it for explicit reversal.

Proof `test-events.sh --phase tile`: four-tile native baseline; change grid(1,1)
from tile50/orientation0/height0 to 51/1/1, independent native readback of **all
four tiles** verifies only that cell changed; state stream carries tile event;
invalid/outside/missing IDs refused; restore returns the entire baseline.
Server fields proven; connected-client geometry/light/grass rendering remains
unverified. The first test used the wrong fixture area tag and returned -1;
using the area script's OBJECT_SELF fixed the probe, without changing terrain.

## Owner-regression batch: reproduction and UI handoff

Current catalogue: **61 operations** with every capability enabled (earlier
52-operation counts above describe the prior DM batch). This batch merged
live-mode at `8699635`, then committed the faction fix `92ee48d` (result parser
follow-up `90697b9`), forced kill `f683a74`, launcher switch `8ad3f13`, tagged
VFX `1f9cbb4`, appearance `11e237d`, object library `2fc5e16`, skill checks
`7161a21`, patrol `921f2f8`, and terrain `14088e4`.

| UI action | Inputs / integration |
| --- | --- |
| Attack | Optional fighting faction name/raw ID; Rust NpcAttack initializers need `faction: None` for the old UI default. Display result faction.original/temporary. |
| Kill | No new input; show value.dead and overridden.plot/immortal. |
| Player actions | Next deliberate demo launch: -AllowPlayerOps; retain separate Options players switch. No live-server toggle implied. |
| Apply / clear VFX | Object or clicked location; row; duration; temporary seconds. Clear row or all ours on target. |
| Appearance | Creature row picker; refresh from report app. |
| NPC/object library | Capture target; persist full result.object_json plus resource/module provenance; restore using json object, area and xyz/facing. Scripts permission needed on restore. |
| Skill check | Centre, radius, skill row, DC, two optional texts; show returned player IDs and pass/fail. |
| Patrol | Ordered 1..16 points; loop/run; Stop. Progress is measured patrol events. |
| Tile | Grid coordinate, active-tileset ID, orientation, native height; apply tile event to live terrain and use before for reversal. |

No mg-ui files were edited or built. Its exhaustive matches/initializers and
renderer/menu integration belong to the coordinator; passing mg-live checks is
not a claim that the downstream GUI has already been wired or rebuilt.

Reproduce from this worktree with the existing shared compiler binary (no demo
launcher): `wsl -e bash /mnt/c/Projects/moonglow-toolset-original-astra2/tools/live/test-events.sh --phase <phase>`.
Final phases: hostile_fight, kill_override, pc_lifecycle, apply_vfx, appearance,
capture, skill_check_disabled, pc_skill_check, patrol, tile. Historical *_repro
artifacts were made against the pre-fix game code and are not current passing
regression phases. Each phase owns UDP5126/Redis6392, private scratch
`~/mglive-events`, prefix `mglive-events`, and writes `<phase>.json` plus
`<phase>-state.json` under `target/test-output/live-events/`. Manifests pin the
module, native server, compiler, plugin/include and script hashes and confirm
identity-bound child shutdown. Required dependencies and paths are declared in
`tools/live/test-events.py`; it refuses occupied private ports or an unowned
scratch. Never use demo 5121/5122/5125 or the real user directory for these proofs.

Final Rust validation: `CARGO_TARGET_DIR=C:\Projects\moonglow-toolset-original-claude\target`,
`CARGO_BUILD_JOBS=1`, `NWN_ROOT` set to the nonexistent worktree
`target/test-output/live-events/no-native-game`, toolchain +1.98.0:
`cargo test -p mg-live --lib` (85 passed, four opt-in tests ignored),
`cargo clippy -p mg-live --all-targets -- -D warnings`, and the explicitly run
`library_engine_snapshot_ingestion -- --ignored` (passes). Regeneration uses
`MOONGLOW_UPDATE_OPENAPI=1` during the library suite; index maintained manually
because okf is unavailable. The launcher test runs only parsed/mocked code.

Extended capture proof: a **31,560-byte** native snapshot (larger than the normal
16 KiB result limit) survives capture, Rust ingestion and restoration, including
an 18,000-character description. A **295,752-byte** snapshot returns `invalid`
without truncating or publishing it. A first oversized-description attempt
stopped at the engine's 131,072-character description cap; two inventory item
descriptions produce the real over-limit fixture. These findings are measured
in the retained capture manifest, not assumptions about the JSON boundary.

Open proof requirements: real connected-PC lifecycle/module scripts and skill
check messages; client rendering for instant/location VFX, appearance and tile
geometry/light/grass. The running owner demo was untouched. NPC/native state
proofs and successful commands do not substitute for those client observations.

Cleanup/retention for this batch: session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`, ledger
`target/test-output/live-events/qol-files.json`. After all owned server/Redis
children exited and ports were verified free, `cleanup-events.py capture
--ledger qol-files` removed only the last closed run's transient logs/settings,
secret, disposable SQLite caches and empty capture log; the ledger then removed
12 other verified-empty logs from this batch. **24 files, 131652 logical bytes**
removed in total. Physical allocation/free-space change was not measured or
attributed during concurrent work. Minimal per-phase manifests/state packets,
all sources, the reusable fixture/scratch module and shared Cargo target remain
for review/reproduction; no unrelated artifacts or accepted prior proofs were
removed. Documentation file links and required source/proof paths were checked.

History retained, not altered: active session path
the agent's local Codex session file.
Report that exact history for separate cleanup after the session ends; no supported
history deletion workflow or consent was supplied here. No history-space recovery
is claimed. Pending coordinator acceptance is distinct from the test results above.


### Skill-window / exact-selection follow-up (2026-10-13)

The NUI and exact-target extensions are code/headless-proof complete for review;
connected-PC/UI rendering remains the explicit owner checklist above. Work reused
the single private module/cache in ~/mglive-events and UDP5126/Redis6392. The demo,
main executables and mg-ui were not operated or rebuilt. The ignored ownership
ledger is target/test-output/live-events/qol-files.json, task live-dm-qol, session
01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a. Source, current module and minimal JSON proofs
are retained. The NUI phase's12 runtime secrets/config/log/cache files were removed
by tools/live/cleanup-events.py pc_skill_nui --ledger qol-files after owned-process
exit verification (132122 logical bytes; no claim of physical free-space change).
The final stop_combat_target phase removed another 12 owned runtime files (131652
logical bytes), for 24 files / 263774 logical bytes in this follow-up. Required
source/module/proof JSON files remain; no physical free-space change is attributed.
Validation: 86 mg-live tests passed, 4 ignored; OpenAPI regenerated/checked, Clippy
all-targets with warnings denied and Rust formatting checked. The operation count
remains 61. Active session history remains untouched; its cleanup is a separate,
unapproved scope. Guarded mg-live tests and OpenAPI generation use the shared claude target,
CARGO_BUILD_JOBS=1, toolchain1.98.0 and nonexistent NWN_ROOT, as described above.


## Shared-tree owner fixes (2026-10-13)

This section supersedes historical worktree/build paths above. Current source is
MAIN on `live-mode`. All Cargo checks use `tools/live/check.ps1`, `-Jobs 1
-NoUiDebug -UiUnoptimized`, toolchain 1.98.0 and its nonexistent NWN_ROOT guard.
No application is built or run. The private harness now compiles through
`mg-live/tests/private_fixture.rs` (explicit opt-in, one compiler worker), using
read-only installed game resources and the existing owned fixture directory.
It no longer needs mg.exe. Run from MAIN:

```powershell
wsl.exe -e python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase pc_skill_target
```

`MGLIVE_PWSH` can override the Windows PowerShell 7 executable path in the
harness. `compile.json` records the actual test compiler binary. Phase manifests
pin module, compiler, scripts, includes, server and plugin hashes. Ports remain
5126/6392, scratch `~/mglive-events`, prefix `mglive-events`; ownership checks,
private launch and identity-bound child cleanup are unchanged. The owner's
running demo and real user directory are outside this test scope.

### Skill check: one PC or a radius (B)

Exact target versus x/y/radius remains exclusive; both player switches and the
living-PC/area protections remain. This earlier extension is now incorporated
in the [interactive skill-check contract](#nearby-player-skill-check), which
supersedes its immediate boolean-roll/NUI result format. A PC selection defaults
to one player; an NPC selection defaults to radius. The shared map aim chooses
the final radius centre. Optional texts still default to empty.

### Runtime placeable and door appearance (E)

Every reported placeable/door now carries `app`, independent of its resref:
`{"id":"3","r":"mget_chest","app":5,"useable":true}`.
For creatures it remains an appearance.2da row. Placeables use **placeables.2da**
from `NWNX_Object_GetAppearance`; doors use **genericdoors.2da** from native
ObjectToJson `GenericType_New.value`, falling back to legacy `GenericType`.
`GetAppearanceType` is a creature API, and reading only legacy GenericType from
current door JSON incorrectly gave zero. Moonglow must resolve the model by
reported `app`, falling back to its blueprint only for old reports without it.
Rust's optional `LiveObject.app` retains backward compatibility.

`instance_appearance` proves an authored placeable blueprint at row4 becomes a
runtime instance at row5 with the **same resref**, and both native NWNX readback
and the report agree. The placed door reports row1, independently corroborated
by its native snapshot. The owner's running demo was not read or changed, so
these are exact fixture IDs in the retained state stream, not identification
of the owner's cabinet/tent pair. NWNX functions in ExecuteScriptChunk are
blocked by this server configuration and can silently return defaults; the
proof correctly uses compiled mget_app/mget_props scripts and builtin chunk
readback of their locals. No plugin bypass was enabled.

### Corpse loot and owned drop flags (D)

`{"id":"loot1","area":"events","op":"set_lootable","target":"3","lootable":false,"no_drop":true}`
(group npc) calls SetLootable on a **living NPC**. PCs/DMs return `protected`,
dead creatures `dead`, other object kinds `invalid`. `lootable` is required
boolean; `no_drop` is optional boolean. Omitted no_drop leaves item flags alone.
Creature reports add optional `loot` (Rust Option<bool>; missing old fields
remain unknown). Result:
`{"id":"loot1","ok":true,"value":{"loot":false,"item_updates":2}}`.

no_drop=true records each item's prior droppable flag in owned MGLIVE_DROP_*
locals and sets it false, across inventory and equipped slots (deduplicated,
maximum256 items; oversize refuses before changing anything). no_drop=false
restores only marked items and removes those locals. It preserves a newer
script-enabled true flag, and never forces an originally nondroppable item to
true merely because ours is cleared. Items gained later are unaffected until
another command; this is not a permanent inventory hook. UI: Lootable checkbox
and an optional prevent-drop checkbox, with an explicit restore action.
`lootable` proof verifies report loot, native [true,false] -> [false,false] ->
[true,false] item flags, newer-script flag ownership and dead/input refusals.

### Placeable appearance setter and PC consent (F)

`set_appearance {target,row}` now accepts placeables as well as creatures/PCs.
The type determines the 2DA: placeables.2da or appearance.2da. Row0..65535 must
exist; missing is `not_found`. A placeable uses NWNX_Object_SetAppearance;
result value.app and the next report show the native readback. PCs require both
player switches as before. Doors are not accepted by this setter.

Proof `placeable_appearance` sets row5, checks a separately compiled native
getter and report, rejects a missing row, then captures and spawn_json clones
the object with row5 preserved. A connected client's immediate mesh refresh is
**unverified**. If that client requires replacement, use capture_object ->
spawn_json at the required location -> verify the clone -> explicit destroy of
the old ID. This tested fallback is caller-controlled, not an automatic deletion;
object identity, references and queued actions are not migrated. UI: placeable
row picker and PC appearance picker, retaining player consent and target name.

### Static placeables (G)

`{"id":"dynamic1","area":"events","op":"set_static","target":"3","static":false}`
(group npc, placeables only) calls NWNX_Object_SetPlaceableIsStatic and returns
`{"id":"dynamic1","ok":true,"value":{"static":false}}`.
The report adds `static` (Rust `LiveObject.is_static: Option<bool>`). Native
`static_flag` proof toggles true/false, compares report and independent getter,
then applies a scale1.5 transform after making the object dynamic.

NWNX documents client updates only on area reentry; native success is not an
immediate client update guarantee ([Object plugin reference](https://nwnxee.github.io/unified/dark/group__object.html)).
Observed side effect: setting true makes useable=false; setting false leaves it
false. Use set_useable explicitly if interaction is desired. UI: Make Dynamic
button beside disabled VFX/transform controls, this reentry explanation, and
separate Useable control. Live static state takes priority over blueprint Static.

### DM force player orders and new player actions (H)

With server MGLIVE_ALLOW_PLAYER_OPS=1 **and** Policy.players=true (publisher adds
players:true), PCs now accept npc_move and npc_attack as DM force orders, alongside
freeze/unfreeze, kill/heal, apply/clear_vfx, set_appearance and pc_teleport.
Skill checks use the B contract above. A forced PC attack never changes the PC's
faction; explicit fighting faction on a PC is invalid. Game/module PvP and
client action behavior remain subject to native rules. No claim of simulated PCs.

All following operations are in group **players**, require both switches, and
target an actual PC (NPC/DM/possessed returns protected). Policy disabled returns
disabled before publication. Shared identity/area guards still apply. They may
be delivered while a PC is dead; movement/combat/heal/kill retain dead refusals,
and pc_raise still refuses a living target. UI must show the selected PC name.
Examples include the usual id/area/players envelope:

| Operation and fields | Native behavior / result / UI |
| --- | --- |
| `pc_message {target,text,mode:"chat"}` | Private SendMessageToPC. text1..200 nonblank characters. mode chat/floating/nui. Floating uses nonbroadcast, no-chat native flags. NUI uses mgliveMessage, OK/30s close, token0 chat fallback; value.nui_token is0 except a successful NUI allocation. Text editor and mode choice. |
| `pc_journal {target,quest:"intro",state:2}` | AddJournalQuestEntry for only this PC, allowing lower states; tag1..64 ASCII alnum/_/-, state0..2147483647. Module must define the quest/state; no native readback confirms a nonexistent quest. Quest tag and state inputs. |
| `pc_take_item {target,item:"abc"}` | Exactly one item ID or resref, existing inventory removal rules/plot protection. Removes the matched item object (entire stack); prefer ID from view_inventory. Inventory selection and confirmation. |
| `pc_boot {target,reason:"Session ended"}` | Optional reason<=200 (empty allowed); BootPC scheduled0.5s after result acknowledgment. **Confirm target/name before submitting**. Positive disconnect proof needs a client. |
| `fade {target,direction:"to_black",seconds:2}` | to_black/from_black, finite seconds>0..60. Native player fade; visual acceptance needs a client. Direction and duration inputs. |
| `play_sound {target,resref:"as_pl_birds1"}` | Valid WAV resref1..16 must exist; PlaySound assigned to PC. This is mono positional world audio; nearby clients can also hear it, not private audio. Sound resref picker. |

Except pc_message's value, these acknowledge `{id,ok:true}`. Standard errors
remain invalid/not_found/other_area/protected/disabled. NUI handler checks our
window IDs and forwards unrelated windows to the preserved module handler.
`pc_force_bounds` proves both-switch and real-PC refusals for all six operations;
Rust tests cover exact payloads, bounds, permission gates and force-order fields.
Positive forced movement/attack, messaging, journal, item removal, boot, fade,
audio and PC appearance need a connected private/owner client. No demo was used.

### Shared-tree validation, retention and handoff

The current catalogue has **70 operations**. `phase5-dm-final-focused.log`
records92 passing mg-live unit tests, four opt-in tests ignored, and the fixture
preparer's safe default test. `MOONGLOW_UPDATE_OPENAPI=1` generated the schema;
the final normal test verified it unchanged. Targeted all-target Clippy passes
with warnings denied for both mg-live and mg-ui; the guarded wrapper now honours
`-Package ... -Lint` and applies NoUiDebug/UiUnoptimized to metadata checks too.
Two single-case refusal tests now assert the exact invalid code instead of using
single-element loops; the five follow-up validation tests pass after that change.
The screenshot module reuses the existing fixture module (avoiding duplicate_mod).
Rust formatting checks pass for the owned Rust sources/tests. Seven Hexxar app checks pass;
reviewed window pictures and reproduction commands are in
[live-windows.md](live-windows.md#inventory-and-owner-follow-up-windows-2026-10-13).

Private phases passed: pc_skill_target, silent_chat, instance_appearance,
lootable, placeable_appearance, static_flag, pc_force_bounds, material_uniform,
and dm_inventory. Each retains `<phase>.json` and `<phase>-state.json` under
`target/test-output/live-events/`. All owned child processes exited; the final
cleanup checked PID/start/executable identity, scratch ownership and private-port
vacancy. No app rebuild, demo command, demo log-tail operation, game install
write or real user-folder write occurred. Positive connected-PC and client
visual checks listed above remain unverified, not waived by these tests.

Commit handoff: a7f4440 freshness; 60519f8 compiler-library fixture route;
3ea3713 player skill target; 68c99ee silent chat; 9fe5c3c runtime appearance;
e0d2f74 loot; 446dfe4 placeable appearance; 9b70354 static; fa1964b PC actions;
d9cb1c1 material uniforms; 463777e Inventory/windows. Shared-index collision in
446dfe4 also included Astra's direct numeric transform UI/test/docs work; it was
preserved and acknowledged rather than rewritten. Subsequent commits use private
indexes constructed from current HEAD and only the intended changes.

Ownership ledger: `target/test-output/live-dm-followup/files.json`, task
hexxar-dm-followup, session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`. Canonical code,
the one current fixture/module, per-phase manifests/state streams, compact
baseline records and four reviewed PNGs are retained. The guarded cleanup command
`tools/live/cleanup-events.py dm_inventory --ledger followup-files` removes only
closed private-run transients, including its generated secret/settings/log/cache
files. Redundant before-skill/before-inventory pictures, superseded intermediate
Rust logs and eight empty event-log arrays were also removed: **28 files,
685187 logical bytes** including the three disposable commit-assembly helpers. Shared target/build caches are not task-owned;
no physical allocation or free-space change is attributed during concurrent work.

The exact active session history remains
the agent's local Codex session file.
It was not altered. Separate history cleanup needs an approved retention workflow
outside this active session; no supported deletion mechanism/consent was supplied.
These are reproducible tests for coordinator review, not owner acceptance of a
rebuilt executable. UI integration names/fields and client-check limits are in
the per-operation sections above; new Inventory Action/Tab names are identical.


## Unified teleport (owner item 24)

`teleport` / `live_teleport` accepts `target` (hex object ID), finite `x`, `y`,
`z`, optional destination `area` (resref, 1..16 characters), and optional facing
`f` (finite, 0 inclusive to 360 exclusive). Omit area to use the target's current
area; omit f to preserve facing. Unlike other commands, area is the destination,
not a source-area constraint. The target can come from any reported area.
The source command, including an omitted area, is retained for idempotent retries.

```json
{"id":"jump-1","op":"teleport","target":"4","area":"events2","x":5,"y":5,"z":0,"f":180}
```

The game resolves the destination across all module areas, including areas not
previously reported. It requires `0 <= x < width*10` and `0 <= y < height*10`;
area dimensions are tile counts. Z must be finite; there is no rectangular
vertical area limit. Creature placement uses the native walkmesh and may adjust
both XY and Z. A valid request is not a promise of exact creature coordinates.
After dispatch, both source and destination are watched for the remainder of
this boot and a full report is requested, even with no player in either area.

- Living creatures: cancel Moonglow patrol/hold, restore any temporary fighting
  faction through the existing hold release, clear actions and JumpToLocation.
  The object ID and native creature state stay intact across areas.
- Placeables and ground items within an area: NWNX_Object_SetPosition and facing,
  preserving the object ID. Inventory/equipped items are refused as protected.
- Doors within an area with unchanged/omitted f: NWNX_Object_SetPosition,
  preserving the ID. Native SetFacing ignores doors. A changed door facing uses
  the same bounded recreation fallback as a cross-area move, also within one area.
- Placeables, doors and ground items across areas: ObjectToJson with saved state,
  bounded to 256 KiB and validated by the existing GFF JSON validator, then
  JsonToObject with saved state at the new location. Verify native type/area
  before DestroyObject removes the original. Failed/oversized captures retain
  the original. This is an internal capture/spawn transaction, not an arbitrary
  JSON injection endpoint.

Successful results always identify the resulting object:

```json
{"id":"jump-1","ok":true,"obj":"4","value":{"previous":"4","recreated":false,"area":"events2"}}
{"id":"jump-2","ok":true,"obj":"8","value":{"previous":"6","recreated":true,"area":"events2"}}
```

Creature movement is dispatched; inspect subsequent reports/native state to
confirm arrival. With recreation, object IDs change, including inventory object
IDs. References held by other scripts/windows to the original are not rewritten.
Do not infer client rendering, pathing around moved doors, or transition-link
integrity merely from a successful server readback. Ordinary library capture
continues to use template state; teleport specifically needs saved runtime state.

Refusals use `invalid` for malformed fields/bounds/oversized snapshots,
`not_found` for missing object/destination or an object without an area, `dead`
for dead creatures, `protected` for inventory items, and `disabled` when player
permissions are missing or native reconstruction cannot produce the requested
object. Neither a refusal nor snapshot-size failure deletes the original.

Player/DM/possessed targets and PC-controlled companions require server
`MGLIVE_ALLOW_PLAYER_OPS=1` and command `players:true`. Moonglow supplies that flag
only when Options Players and the submitting client's Players scope allow it.
`live_teleport` has the npc scope, with Players additionally required for PCs.
The existing `pc_teleport` / `live_pc_teleport` is a compatibility alias through
the same native helper; it keeps the creature-only and unconditional Players
gates, now supports destination area and optional f too. Old xyz-only calls work.

Area reports add optional `size:[width_metres,height_metres]`, e.g.
`{"area":"events2","size":[20,20],"full":true,"cre":[]}`. `live_area_state`
exposes size too. Serde defaults preserve old reports, and deltas retain the last
known dimensions. Rust checks bounds when dimensions are known; the native
handler checks every destination, including previously unreported ones.

UI handoff: use `Op::Teleport { target,x,y,z,f:Option<f32> }`; `Command.area` is
an empty String for omitted destination. PcTeleport also gains f. Inputs are
selected object, destination-area chooser (default current), XYZ and optional
facing. After live_wait, retarget selection to `result.obj` when recreated, and
refresh both areas. Explain changed IDs for cross-area scenery/items and rotated
doors. PCs need the existing Players switch. No new Tab is required.


### Teleport proof and reproduction

The private `teleport` phase proves an NPC, placeable, GIT door and ground item
moving within events and into events2. Native GetArea/GetPosition/GetFacing and
subsequent area reports verify the effect. The NPC keeps its identity; all three
cross-area noncreatures receive new IDs and their originals become invalid.
A same-area door rotation also creates a new ID and reads back 270 degrees.
The recreated chest retains its name, local integer 42, plot flag and inventory.
Both an active patrol and a held movement order are cancelled. The pc_teleport
alias runs through the same path on an NPC with player permissions enabled;
missing players flags are refused. Omitted area/f preserve current destination
and facing, and invalid XY/f, missing areas and inventory-item requests refuse.

Two engine findings determined the implementation:
1. Native JumpToLocation adjusted requested (7,7,0) to approximately
   (6.4,6.4,-0.617) on this fixture's walkmesh, while noncreatures read back exact
   positions. Creature tests allow one metre per horizontal component here;
   this tolerance is a fixture observation, not a universal navigation guarantee.
2. Template ObjectToJson/JsonToObject (FALSE/default state) preserved appearance,
   name, plot and inventory but dropped local MGET_MARK. Passing TRUE to both
   preserves that local value. Teleport therefore uses saved-state snapshots.

Reproduce from the main tree without rebuilding either application:

```powershell
wsl.exe -e python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase teleport
pwsh -NoProfile -File tools/live/check.ps1 -Phase 5 -Step teleport-final -Package mg-live -Jobs 1 -NoUiDebug -UiUnoptimized
```

The harness rebuilds the same tiny authored two-area module through
`crates/mg-live/tests/private_fixture.rs`, using the guarded compiler-library
route. Includes come from the configured NWNX NWScript.zip; exact NSS, compiler,
module, server and plugin hashes are in `target/test-output/live-events/teleport.json`.
It uses only UDP5126, Redis6392 and owned `~/mglive-events`, the patched NWNX root
`~/nwnx-redis-reconnect`, and
`~/server/bin/linux-x86/nwserver-linux`. The server is private, with a
random unprinted admin password. The source fixture adds only one two-by-two
area (events2), not a copy of a demo or installed module.

Connected-player arrival and client rendering/door collision or transition
behaviour remain owner-client checks. On the private server, enable both player
switches, teleport a living PC within/across areas, and confirm arrival/facing;
with either switch off expect disabled, and for a dead creature expect dead.
No player or client success is claimed from this headless run.

The Rust catalogue has **71 operations**. Three targeted teleport tests cover
wire JSON for every reported kind, alias fields, finite/range/area bounds,
idempotency after crossing areas, old reports and named-client restrictions.
The guarded full mg-live suite passes95 unit tests plus the fixture safe-default
test (four opt-in tests ignored). All-target mg-live Clippy passes with warnings
denied. OpenAPI regeneration uses MOONGLOW_UPDATE_OPENAPI=1 through the same
wrapper; normal openapi testing verifies the committed schema. The mg-ui all-target
Clippy metadata check passes too; the UI worker owns the new targeting flow.


The final size-boundary proof creates a 530,969-byte saved-state snapshot,
receives invalid and reads the original still valid in its source area before
removing only the test local. Proof evidence remains in teleport.json and
teleport-state.json. The owner/coordinator has not yet accepted this new item;
these are observed native and automated-test results.

Task ledger: `target/test-output/live-teleport/files.json`, owner-item24-teleport,
session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`. Canonical sources, the current
reusable two-area fixture/module, one passing manifest/state stream and final
check logs are retained. `tools/live/cleanup-events.py teleport --ledger
teleport-files` verified ownership, process identity/exits and vacant private
ports before deleting12 generated private-run secret/settings/log/cache files
and an empty event-log array. Two superseded check logs were also removed:
**14 files, 140446 logical bytes**. The temporary private Git index is removed
after the commit and accounted separately in the ledger. No shared build cache or
other worker's files were removed. Physical allocation/free-space change is not
attributed during concurrent work. Active session history was not altered;
its exact path and separate-retention restriction remain recorded above.


## Gold and XP removal (third acceptance)

These target living creatures in `area`. NPCs need `npc` permission; PCs require
both server `MGLIVE_ALLOW_PLAYER_OPS=1` and Moonglow Players permission (and the
named client's `players` group). Disabled PC requests return `disabled`. Dead,
missing and wrong-area targets return `dead`, `not_found` and `other_area`.

| Op | Required fields | Limits / native action |
|---|---|---|
| `take_gold` | `target`, `amount` | Integer 1..1,000,000; `TakeGoldFromCreature(..., TRUE)` destroys gold, capped at current balance. |
| `set_xp` | `target`, `value` | Integer 0..2,147,483,647; native `SetXP`. |
| `take_xp` | `target`, `amount` | Integer 1..100,000; subtract using `SetXP`, floor zero. |
| `remove_level` | `target` | Actual level 2..40; XP becomes `exptable.2da` `XP` at zero-based row `GetHitDice(target)-2`, the previous level's threshold. Missing table row is `not_found`; no reduction or level one is `invalid`. |

Examples (common envelope `id`, `area` omitted):

```json
{"op":"take_gold","target":"abc","amount":25}
{"op":"set_xp","target":"abc","value":10000}
{"op":"take_xp","target":"abc","amount":500}
{"op":"remove_level","target":"abc"}
```

Every success returns `value:{before,after,level_before,level_after}`. The amounts
are native gold or XP, and levels are native `GetHitDice`, not a calculated XP
eligibility. Creature reports also add optional `level`; old reports parse as
`None`. UI inputs: target, positive removal amount or nonnegative absolute XP;
Remove Level needs confirmation explaining that native level loss can affect
class progression and associated stats/feats. Give XP does not choose a class.

Reproduce with `test-events.py --phase economy` on the private 5126/6392 fixture.
The evidence is `target/test-output/live-events/economy.json` and `economy-state.json`.
The native proof changed gold 100 -> 75 -> 0, XP 10,000 -> 9,999 -> 3,000 -> 0,
and actual class levels 4 -> 3 -> 1. The first level loss reduced base attack bonus
4 -> 3 and maximum HP 48 -> 36. `SetXP` therefore performs real level loss,
not just eligibility changes; normal native recalculation/last-level removal is
used, rather than manually rewriting feats or class records. PC UI/relevel choices
still require an owner client check. Gold initially failed when the caller was
the module (unchanged across 30 seconds). The validated dispatcher now executes
`mglive_gold` synchronously on the creature; the same native call then returns
75 immediately and the next report confirms it. The helper consumes its private
amount local and does not transfer gold to another creature.

`Op::{TakeGold,SetXp,TakeXp,RemoveLevel}` have exact publisher/schema/limit/type and
player-gate tests. The catalogue now has 78 operations. Guarded reproduction:
`tools/live/check.ps1 -Phase 5 -Step economy -Package mg-live -Jobs 1 -NoUiDebug -UiUnoptimized`;
set `MOONGLOW_UPDATE_OPENAPI=1` only when regenerating OpenAPI.


## Creature animation playback (third acceptance)

`play_creature_animation` accepts `target`, `animation` (numeric NWScript
creature `ANIMATION_*` value, or model animation name 1..16 resref characters),
`loop` (default false), `speed` (finite 0.1..10, default 1), and `seconds`
(finite 0.1..300, default 30). Creature constants 0..92 and 100..116 are accepted;
placeable/door constants 200+ are refused. UI shows constant names from the loaded
`nwscript.nss` and sends their numeric values. The operation uses `npc` scope,
with both player switches and named-client `players` scope required for PCs.

```json
{"id":"bow","area":"events","op":"play_creature_animation","target":"abc","animation":105,"loop":false,"speed":1,"seconds":10}
{"id":"custom","area":"events","op":"play_creature_animation","target":"abc","animation":"talknormal","loop":true,"speed":0.5,"seconds":30}
```

Numeric constants retain the engine's animation semantics: native looping
constants play for the requested duration; fire-and-forget constants play once,
or are queued again after completion when `loop:true`. The overall seconds
limit ends the order even if a native animation is still running. A new live
movement/combat/hold order also cancels it. While owned, the creature is held
against automatic AI replacement; completion releases that hold.

NWN1's installed header does **not** provide `PlayCustomAnimation` (the requested
function name is unavailable in this game). Named playback therefore borrows
`custom70`/`ANIMATION_LOOPING_CUSTOM70` for loops or `salute`/
`ANIMATION_FIREFORGET_SALUTE` for a one-shot with `ReplaceObjectAnimation`.
Its previous native mapping is restored on cancellation/timeout, but only if
it still equals our mapping; a later script's different override is preserved.
Native fire-and-forget timing remains engine-defined, so a long custom one-shot
can be cut short. No model/animation asset existence or visual success is claimed
by the native setter. [Beamdog documents native animation timing distinctions](https://nwn.beamdog.net/docs/).

Success `value` contains `animation`, `loop`, `speed`, `seconds`,
`native_constant`, and `replacement_slot` for named animations. While owned,
creature reports include the same `animation` object plus `cycles` (number of
native queue invocations). When released the field is omitted. Separate `anim`
is `NWNX_Object_GetCurrentAnimation`, an internal native state ID, **not** the
input `ANIMATION_*` constant. Both fields remain optional for older reports.

UI handoff: `Op::PlayCreatureAnimation {target, animation:
CreatureAnimation::{Id(i32),Name(String)}, looping:bool, speed:f32, seconds:f32}`.
The `looping` field serializes as `loop`. Inputs fit Astra's shared single/group
Animation menu; refuse dead/wrong-kind targets, finite bounds, bad IDs and player
permission before enqueue. Raw commands receive the same native validation.

Reproduction: `test-events.py --phase creature_animation` on the owned private
server. Evidence: `target/test-output/live-events/creature_animation.json` and
`creature_animation-state.json`. The proof checks changed native animation state,
active report fields (meditation constant 3 changed native state 1 -> 32),
repeated bow queue cycles (two invocations, native state 35), preserved replacement
storage and interruption by movement (native walking state 2). It also verifies
an older timer cannot cancel a newer request and preserves a later external
replacement. Readbacks run on the area: a `run_chunk` targeted at the animated
creature is itself a new order and correctly cancels its animation. A connected client must still verify
custom model animation content and appearance.


## Inventory equipment and grid (third acceptance)

| Op | Required fields | Result / refusal |
|---|---|---|
| `give_item` | `target`, `resref` (1..16 resref characters) | Create one blueprint item; result `obj` is its ID. Missing blueprint is `not_found`. NPCs do not need Players; `pc_give_item` keeps its earlier stricter contract. |
| `equip_item` | `target`, `item` (inventory object ID), `slot` (integer0..13) | `value:{item,slot,native_return}` is native readback after `NWNX_Creature_RunEquip`. Native equipment rules may refuse with `invalid`. |
| `unequip_item` | `target`, `item`, `slot` (current slot0..13) | `value:{item,slot:-1,native_return}` after `NWNX_Creature_RunUnequip`. A stale slot is `invalid`. |
| `drop_item` | `target`, `item` | `obj` and `value.item` are the new ground ID; `value.previous` is the former inventory ID, `queued:false`. Native CopyObject with local state followed by removal of the held instance. Check the new ground-item report. Plot/nondroppable items are `protected`. |

All target a living creature in `area`; the item must belong to that creature,
otherwise `not_found`. NPC scope is `npc`; PCs require both player gates and
named-token `players` scope (`disabled` if missing). Type/ID/slot bounds are
validated by Rust and NWScript. Hidden creature equipment slots14..17 are not
mutation targets. Existing `take_item` remains exact-item destruction with plot
protection. Every UI mutation refreshes native inventory; authoring GFF is unchanged.

```json
{"id":"gift","area":"events","op":"give_item","target":"abc","resref":"nw_it_mring001"}
{"id":"wear","area":"events","op":"equip_item","target":"abc","item":"def","slot":7}
{"id":"remove","area":"events","op":"unequip_item","target":"abc","item":"def","slot":7}
{"id":"drop","area":"events","op":"drop_item","target":"abc","item":"def"}
```

`view_inventory` rows additionally contain `plot` and `droppable` booleans, and
`look`, a compact numeric object retaining native `BaseItem`, model parts,
armour parts (including EE `x` twins) and colour fields. These are actual runtime
item appearances for the game icon resolver; an older row without `look` can use
its blueprint. The whole rows array still stops at128 items or15KB, with
`truncated:true`. Example: `{"id":"def","resref":"nw_it_mring001",
"name":"Gold Ring","stack":1,"slot":7,"look":{"BaseItem":52,"ModelPart1":4,"xModelPart1":4},
"plot":false,"droppable":true}` (module item names/looks can differ).

The installed APIs do not expose a backpack-cell setter. Moving cells in
Moonglow rearranges only its view; exact native pages/footprints, bag transfers
and stack splitting are not covered. Equip, unequip and ground drop use native
operations, not a recreated player/creature inventory.

Proof/reproduction: `tools/live/test-events.py --phase inventory_grid`, private
5126/6392. It creates a ring with `give_item`, checks actual appearance data,
equips into slot7, checks native `GetItemInSlot` and another inventory query,
unequips, refuses wrong holders/stale slots/nondroppable items, then drops and
checks the ground-object report and no possessor. Evidence:
`target/test-output/live-events/inventory_grid.json` and `inventory_grid-state.json`.
Connected-PC native rules/client visuals still need owner acceptance.
UI hook remains `Action::LiveInventory` / `Tab::LiveInventory`.


Native discovery: this installed NWNX `RunUnequip` returned0 **after successfully
removing the ring from its slot**. Trusting that return code alone incorrectly
reported refusal. The implementation now decides success from native slot and
possessor readback, retaining the plugin return as `native_return` for diagnostics.
The proof's subsequent inventory query independently confirms slot-1; it does
not merely accept `ok:true`. No NWNX C++ change was needed.


### Immediate ground drops (fourth acceptance B7)

The old queued `ActionPutDownItem` returned `ok:true` on an uncommandable NPC
without dropping anything. Module routines can also clear action queues; the
owner's read-only demo sources contain repeated `ClearAllActions` calls.
`drop_item` now uses native `CopyObject` at the holder's current location with
`bCopyLocalState=TRUE`, verifies a ground object in the correct area, and only
then destroys the held instance. Copy failure preserves the original. It does
not clear actions or change the creature's commandable flag. Plot/non-droppable
items remain protected; PCs retain both permission checks.

**Identity changes:** external script object references to the old item do not
follow the new item. Consumers must read `obj`/`value.item`, not keep using the
old inventory ID. Runtime item locals (including object-valued locals) survive.
This is a DM force operation, with a native ground copy rather than an animated
inventory action or a promise that a queued action will eventually execute.

```json
{"id":"drop","ok":true,"obj":"6","value":{"previous":"5","item":"6","queued":false}}
```

Private proof `test-events.py --phase drop_routine_probe` reproduces the old
queued failure on an uncommandable NPC, then asserts the new ground report's XY
matches its feet, its holder is invalid, the inventory and old ID are gone,
both integer/object locals survive, and commandability is unchanged. Raw evidence
is `target/test-output/live-events/drop_routine_probe*.json`. This stand-in
establishes the native failure/fix; the running demo and a connected PC were
not manipulated. `inventory_grid` also checks ordinary equip/unequip/protected
drop and ground readback with the returned new ID.


### Restore original appearance (fourth acceptance B8)

Reports now include `app_original` for creatures, placeables and doors: the
native appearance row when first reported in this module run. The initial
`set_appearance` also records the pre-change row if it runs before a report.
The baseline is scoped to boot **and object ID**, so copied/library objects with
inherited locals start a new baseline. This is the observed runtime appearance,
not an assumption that the blueprint still matches. Older reports parse with
`app_original: null` in Rust; row zero is a valid baseline.

```json
{"id":"7","app":5,"app_original":4}
```

Restore uses the existing `set_appearance` command with each selected object's
own original row. It has the same validation and PC permission rules. No new op
is needed. The Live Appearance Gallery supports creatures and placeables and
asks for confirmation naming each target and original row. Missing old-server
baselines disable Restore. Doors report their baseline but have no native
set_appearance operation, so their gallery retains its offline workflow.

Private `test-events.py --phase restore_appearance` checks initial, changed and
restored report rows plus native ObjectToJson readback for a creature and
placeable. Evidence: `target/test-output/live-events/restore_appearance*.json`.
Guarded mg-ui `-Filter appearance` covers both target kinds, confirmation,
legacy absence and offline gallery regression. `-Picture live_appearance_restore_snapshot`
produces inspected `restore-placeable.png`/`restore-confirm.png` in
`target/test-output/live-windows/appearance-restore-picture/`. Client appearance
updates remain the previously documented client-side limitation/owner check.


### Skill NUI layout (fourth acceptance B13)

The window is 520 pixels wide, 300 high (324 with a nonzero DM bonus). Roll has
52 pixels of height. A finished roll hides Roll and shows a compact d20,
modifier, optional DM bonus and total line: green `(115,225,145)` for success,
red `(245,100,100)` for failure. Timeout is neutral grey. Optional narrative
text is a separate control, invisible when empty. OK closes the owned window;
the existing 30-second post-result close and configurable roll timeout remain.

The bonus breakdown uses purchased ranks from `GetSkillRank(TRUE)`, the native
modifier for the skills.2da KeyAbility and the residual native bonuses as
**Other**. It does not invent a decomposition of feat/item/effect contributions.
It is refreshed at the click, so expiring buffs cannot leave the displayed
breakdown inconsistent with the actual roll. The full skill/DC equation still
goes to the player's log and the Live event/result stream.

`test-events.py --phase pc_skill_rolls` compiles and executes the real native
NUI constructors, asserts title, DC visibility, no bonus row for zero, no text
edit or old instruction text, colour/visibility bindings and button height.
The exact bonus breakdown is proven as `Ranks 5 | CON +4 | Other +12 = +21`;
roll success/failure, no-roll timeout, duplicate-click guard and module-event
forwarding continue to pass. Evidence is `target/test-output/live-events/pc_skill_rolls*.json`.
The constructor returns zero without a connected PC: these are layout-JSON and
native calculation proofs, **not a client screenshot or positive NUI-token
proof**. The owner should see a compact named window, one large Roll button,
a colour-coded result after clicking, optional story text and OK.

## Inventory actions and slot rules (fourth acceptance B6)

`view_inventory.value.items[]` now includes `equip_slots` (native baseitems.2da
EquipableSlots bitmask) and `uses:[{property,spell,name}]` (cast-spell item property
indices, spells.2da ID, translated label). Old reports omit these fields: UI falls
back to the loaded baseitems table for equipment, and offers no guessed Use.
Equip validates this mask in both the UI (including drag/drop) and game before
NWNX RunEquip. E.g. ring mask 384 permits slots 7/8, potion mask 0 permits none.

| Operation | Minimal command fields after id/area | Result / checks |
| --- | --- | --- |
| `use_item` | `{"target":"4","item":"6","property":0}` | `value:{"queued":true}`; recipient defaults to target. Optional `recipient` is a same-area living creature. Uses native `ActionUseItemOnObject(item,property,recipient,0,TRUE)`; engine restrictions and charge/stack consumption apply. Property index 0..255 must be a cast ability from `uses`. A noncommandable actor is refused (`disabled`), after releasing our own hold/patrol. Queue acceptance is not completion: query inventory and effect/HP. |
| `transfer_item` | `{"target":"4","item":"6","recipient":"5"}` | `obj`/`value.item` NEW ID, `value.previous` old ID and `value.recipient`. Native CopyObject with locals to the recipient, then removes the original only after verifying ownership. Plot/non-droppable items refused (`protected`); same holder invalid; recipient absent/other area/dead uses corresponding code. Outside references to old ID do not follow. |

Both commands require `npc` scope, and Players permission for **either** endpoint
when a PC/DM/associate is involved; the game independently checks both switches.
They never replace item use with a free spell cast. `use_item` cancels existing
orders before queuing use. It does not manufacture charges, bypass class/feat
restrictions or promise that native AI will complete the queued action.

Private proof: `tools/live/test-events.py --phase inventory_actions` on 5126/6392
proved potion-to-head and ring-to-head refusals, ring slot 8, native healing and
stack decrement 2 -> 1, transfer between NPC inventories with preserved local
integer and explicit changed ID, plus protected-transfer refusal. Native/API
inputs live in `target/test-output/live-events/inventory_actions*.json`.
Rust per-op JSON/scopes/refusal tests and fake-store item menus cover both endpoint
checks. Connected-PC inventory feedback remains an owner-client check.

## VFX transforms and colour (fourth acceptance B10)

`apply_vfx` also accepts optional `scale` (finite 0.01..100, default 1),
`offset:{x,y,z}` (finite native translation, each -1000..1000, default zero),
and `rotation:{x,y,z}` (finite native rotation, each -3600..3600, default zero).
The native `EffectVisualEffect(row,FALSE,scale,offset,rotation)` receives these
unchanged, for an object or location. Refusal is `invalid`; existing requests
without these fields keep their previous behavior. Result `value` adds `scale`
and echoes provided offset/rotation. Native declarations do not specify rotation
units; the readback below proves exact XYZ transmission, not an angle-unit claim.

```json
{"op":"apply_vfx","target":"1","row":2,"duration":"permanent","scale":1.5,"offset":{"x":1.25,"y":-2.5,"z":3.75},"rotation":{"x":10,"y":20,"z":30}}
```

Private `test-events.py --phase vfx_transform` passes on UDP 5126 / Redis 6392:
report contains row 2; `GetEffectFloat(e,0)` is 1.5 and `GetEffectVector(e,0/1)`
return the offset/rotation above. Invalid bounds are refused; clearing removes
only tagged effects. Evidence: `target/test-output/live-events/vfx_transform*.json`.
The production Rust validation/wire test and VFX app test cover both selected
objects and the same transform payload. The browser thumbnail remains the base
asset; it does not preview these runtime transforms.

### VFX colour: removed after native proof

Effects cannot be recoloured through these APIs: `EffectVisualEffect` has no colour argument, and an authored MTR declaring a colour parameter stayed blue after green target uniforms on both creatures and placeables (the object's own model was a successful control). Ground effects expose no object handle for the setter. Native readbacks and inspected baseline/changed/reapplied captures are retained under `target/test-output/live-colour-targets/`; source/reproduction history is Git commit `ec46580`. The owner dropped the helper workaround and extra HAK; Scale/Offset/Rotation remain supported.

| Native VFX target | Uniform result |
|---|---|
| Placeable | Attached VFX unchanged |
| Creature | Attached VFX unchanged |
| Ground location | No object handle for the uniform API |

## Session factions (fourth acceptance B15)

No runtime faction-creation operation is exposed by the installed NWScript or
NWNX plugins. Read-only inspection of `~/unified` at
`3f504d0f1463e49d405639bd3d454ae7416b1a98` found creature get/set faction and
encounter get/set ID, but no create function. `CFactionManager.hpp` exposes
CreateDefaultFactions/load/delete/get/set reputation internally; it does not
provide a general creation API to scripts. NWNX Creature SetFaction looks up an
existing ID before AddMember. This does not prove an engine hook could never
create a faction; such a hook is outside this implementation. Source hashes and
exposed-function inventory: `target/test-output/live-accept4-game/faction-api-research.json`.

Practical workflow: **Tools > Faction Editor > Basic > Add session factions**, 1..32
per click, adds collision-free Session 1..N names to `repute.fac`. Each has
Global Effect on, Commoner parent, neutral 50 in both directions toward others
and friendly 100 to itself. Existing IDs/reputations stay unchanged. The whole
addition is one undoable edit. Save and load this module on the server before
playing; this action never reloads or mutates the running module.

**Factions > Live session** uses the existing native IDs. Choose an acting
faction and optionally give it a session label (up to 64 characters). Labels are
Moonglow-only and last until the server boot or open module path changes; they
do not rename native factions, change repute.fac or survive application restart.
Select one Live NPC and Assign faction, or Read/Set live reputation toward the
selected creature. Match the open module to the server: the list comes from the
loaded module, while the target's faction comes from the live report.

The existing operations now allow NPCs with Players off:

```json
{"op":"get_faction_reputation","area":"events","target":"6","faction":6}
```

```json
{"op":"set_faction_reputation","area":"events","target":"6","faction":6,"reputation":75}
```

Names hostile/commoner/merchant/defender or native IDs are accepted as before;
PC source faction 0 is disabled. Integers are 0..100. Read scope is `read`, write
scope is `npc`; protected targets additionally require the existing Players
policy, token group and server switch. Catalogue entries remain visible when
Players is off, with dispatch refusing protected targets using `disabled`.
Both return asynchronously via live_wait, e.g.
`value:{faction:6,before:10,reputation:75,native_reverse_base_workaround:true}`.
The workaround field is present only when used. If native readback still differs,
`ok:false,code:"disabled"` includes the actual before/reputation value: a native
attempt may have changed it. Do not retry blindly.

Custom factions require a non-plot representative in a loaded area (the existing
256-area/4096-object scan); an empty/limbo-only faction yields `not_found`. Assign
an NPC to it first. Global NPC factions affect all their members; the opposite
direction is separate. Existing temporary/personal reactions can affect native
readings. The pane shows actual command results, and the normal Live Log records
them. It does not save runtime standings back into the module's source FAC file.

### Engine bug and bounded readback correction

The installed engine's NPC AdjustReputation uses the reverse relationship as
its base. Native probe: A -> B 50, B -> A 50; adjustment -40 gives A -> B 10;
then +25 gives 75 (not 35); then -20 gives 30 (not 55). Changing B -> A by -15
at that point gives 15, and adjusting A -> B by zero copies that reverse 15.
ObjectToJson confirms current faction IDs 6/7/6. SetStandardFactionReputation
with custom faction index 5 is a no-op here; its getter returns 50, so it is not
used as a custom-faction setter. Native AdjustReputation accepts three parameters;
there is no whole-faction boolean overload. Those rejected approaches are not
part of production code.

Production first requests desired-minus-before and reads back. On an NPC mismatch
only, it retries once with desired-minus-reverse (measured before the first call)
and reads back again. Fixed engines succeed on the first call. No experimental
plugin is enabled or changed; NWNX's separate AdjustReputationFix source is only
research input. The normal PC route retains one native adjustment and its
verification. Persistent PC/party behavior still needs a connected client.

The private `tools/live/test-events.py --phase session_factions` fixture reserves
Session 1/2 as IDs 6/7 after Agents (5). It proves membership in reports,
multiple members, separate directions, exact set requests and native readback,
plus absent faction and out-of-range refusal with server Players disabled.
Artifacts: `target/test-output/live-events/session_factions*.json`; the independent
bug probe is `target/test-output/live-accept4-game/faction-native-rebase-probe.json`.
Reproduce with WSL/private 5126/6392 and the existing guarded compiler harness.

UI handoff: **existing Tab::Factions / Action::OpenTab(Tab::Factions)**. The
subpane uses `app.faction_view.live_mode=true`; no new top-level window/action.
`LiveFactions::label_for` returns the current display alias; native IDs stay the
wire values. Area/menu colour labels are unchanged. Do not persist these aliases
as module faction names. Runtime reputation controls are explicit commands, not
automatic writes while dragging the offline editor's sliders.

### Fourth-acceptance reproduction and retained files

Current workspace: `C:/Projects/moonglow-toolset-original`, branch live-mode;
use its shared target, not the removed worker worktrees. PowerShell 7:
`tools/live/check.ps1 -Phase 5 -Jobs 1 -NoUiDebug -UiUnoptimized` with
`-Package mg-live`, `-Package mg-module -Filter session_reserve`, or
`-Package mg-ui -Filter session_factions`, and distinct `-Step` names. The script
uses +1.98.0 and a nonexistent NWN_ROOT for ordinary tests. Native proofs use
`wsl.exe -e python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase PHASE`.
This compiles only the mg-live fixture test and owns only private 5126/6392.
No application build or demo lifecycle operation is part of this reproduction.

The task ledger is `target/test-output/live-accept4-game/files.json`, session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`. It identifies source/evidence, shared
files which must not be deleted, exact removed paths, process closure and logical
sizes. Native cleanup ran through `tools/live/cleanup-events.py`; the completed
colour experiment's own markers and recorded process start identities were
checked before removing its runtime/dependency cache. Approximately 43 MiB of
colour-client cache/rejected diagnostics was removed; exact totals including
other closed phases are in the ledger. This measures logical bytes removed,
not allocated blocks or an attributed change in free disk space.

At fourth-acceptance closure, retained: committed source/tests, the one current private event fixture, final
native packets/manifests, inspected window pictures, and the authored colour
sources, scratch tables and client module. The owner subsequently retired the
colour feature; the sixth-acceptance cleanup below supersedes that retention. The two colour
comparison images and final native stdout are retained as evidence; discarded
images/traces/extracted stock shader research and runtime copies were removed.
Game assets remain outside Git. Shared target builds and other workers' files
were not cleaned. The active session history path is recorded in the ledger
for separate supported cleanup after closure; no active history was changed,
and whole-session deletion has not been authorized.

OKF CLI was unavailable; index links and the dated log were maintained by hand
as authorized. Documentation and retained-input links are checked locally.
These are test results awaiting owner review, not a claim of owner acceptance.

## Fifth acceptance: custom module factions (C8)

The game and Rust paths already support **every existing raw `repute.fac` ID**,
including empty custom factions. `NWNX_Creature_SetFaction` is used, followed by
`NWNX_Creature_GetFaction` readback; this is not `ChangeToStandardFaction`.
The menu must send the integer row ID, not the displayed name or the
`STANDARD_FACTION_*` enum. String aliases remain only `hostile`, `commoner`,
`merchant`, `defender`; arbitrary custom names are UI labels, not wire aliases.

```json
{"op":"set_faction","area":"events","target":"80000011","faction":8}
```

Private `test-events.py --phase custom_faction` proved changes to authored
`test` (ID 8), Session 1 (6), Session 2 (7), then Commoner (2), in both replies
and subsequent creature reports. Nonexistent ID 99 returns `invalid` and leaves
membership at 2. Evidence: `target/test-output/live-events/custom_faction.json`.
Astra1 owns the module-backed menu; no new core restriction or fictitious native
runtime faction creation was added. Rust validation covers raw IDs 0, 5, 8, 42,
65535 and rejects 65536; native existence is checked on the server.

## Fifth acceptance: DM inventory Drop (C1)

The reported lamplighter is `C12_ACTOR`, placed creature index 17 in
`demo_area.git` of the read-only `demo.mod` source. Its three
`nw_it_mpotion001` entries have no `Dropable` field. Native inventory readback
reports `droppable:false`, `plot:false`. Both the old context menu and
`MgLiveInventoryOrder` refused that flag; this was unrelated to player permission.

`drop_item` now treats a confirmed DM drop as an override of the ordinary
creature drop restriction. Plot items still return `protected`; PC inventories
still require both player switches. The immediate native ground-copy path keeps
locals, stack and item data, explicitly restores the original droppable flag
(the engine's ground copy otherwise resets it), then destroys the held original.
The result still returns the changed identity and additionally reports
`value.droppable_overridden:true` when applicable:

```json
{"id":"drop-1","ok":true,"obj":"80000024","value":{"previous":"80000020","item":"80000024","queued":false,"droppable_overridden":true}}
```

Private proof `test-events.py --phase drop_flags` reproduced the old `protected`
refusal, then observed the same potion on the ground at the creature's feet,
absent from the inventory, with preserved flag and local value 73. A plot potion
was refused. The optional `MGLIVE_DROP_TEMPLATE` points to the exact extracted
lamplighter JSON; without it, the test authors an equivalent non-droppable NPC
potion. Evidence: `target/test-output/live-accept5-astra2/drop-native-before.json`,
`target/test-output/live-events/drop_flags.json` and `drop_flags-state.json`.
The running demo was read only and its server was untouched.

The default fixture compiler remains the guarded mg-live test. During a shared
Cargo rebuild, `MGLIVE_FIXTURE_MG=/mnt/c/Projects/moonglow-toolset-original/target/release/mg.exe`
uses that existing CLI solely to compile/pack the private fixture (no app build).
The proof records which compiler executable and route were used. Run in WSL with
`PYTHONDONTWRITEBYTECODE=1`; server/Redis remain private ports 5126/6392 and
`~/mglive-events`, with owned-process cleanup in each proof manifest.




## Sixth acceptance: removal and deployment

Owner retired the coloured helper feature. No current source generates a colour HAK or accepts `apply_vfx.colour`; native transforms remain. Offline preparation removes the old script and HAK reference only with the prior pipeline ownership manifest, preserving original HAK order. `-SkipBuild` refuses a cached module that still lists the retired dependency. This does not modify a running server or its module. The owner must perform the next ordinary offline rebuild/restart; that is deliberately outside this acceptance task.

Verification: `pwsh -File tools/live/demo/test-retired.ps1` tests owned/unowned migration, idempotence, startup refusal and an actual packed module readback. Guarded `tools/live/check.ps1 -Phase 5 -Step acceptance6-vfx-removal -Package mg-live -Jobs 1 -NoUiDebug -UiUnoptimized` passes, including the OpenAPI colour-input rejection. `tools/live/test-events.py --phase vfx_transform` on private UDP 5126/Redis 6392 verifies the authored model is absent and native effect scale/offset/rotation still read back exactly. Evidence is retained in `target/test-output/live-accept6-astra2/retired/result.json` and `target/test-output/live-events/vfx_transform.json`.

## Server faction catalogue (sixth acceptance D1)

Every state report carries `factions:[{"id":0,"name":"PC"},{"id":6,"name":"Session 1"},...]`, read with native `TemplateToJson("repute", RESTYPE_FAC)` at module load. The FAC list index is the actual faction ID, including empty session/custom factions. This snapshot follows the server boot, not the editor's open or unsaved module. `live_status` and `live_area_state` expose the same `factions` array; `LiveStore::factions()` returns the typed list. Missing/null means an older server did not advertise it, not an empty list. Names use the existing game-byte text boundary. Repeated/out-of-range IDs and oversized names are refused by the Rust receiver.

Live faction menus must use only these IDs. Editor-only factions are labelled **not on the server: save, rebuild and restart the module to use it**. Runtime creation remains unavailable. Session labels in Moonglow do not create engine factions. Template changes after boot deliberately do not change this catalogue: the server must reload the module to load the corresponding native faction manager.

Private `tools/live/test-events.py --phase server_factions` checks all nine native fixture names/IDs against the report, assigns Session 1 (ID6), verifies the next creature report, and proves missing ID9 is refused without changing membership. Evidence: `target/test-output/live-events/server_factions.json`. Guarded mg-live regression checks status/area output, omitted-list compatibility, invalid IDs and boot reset.

## Object animation replacement (retained after D15)

`replace_object_animation {target, old, new}` maps an existing model animation name; `reset:true` without `new` removes that one mapping. Both names use the 1-16 ASCII resref rules. The operation uses group `npc`; protected player targets additionally require both player permissions. Native `ReplaceObjectAnimation` stores the mapping; result `value:{called,readable,truncated,state}` reads `ObjectToJson.AnimationReplace`, omitted above 15000 bytes. Native storage does not prove the model contains the named animation. Example: `{"op":"replace_object_animation","area":"events","target":"7","old":"walk","new":"run"}`; reset: `{"op":"replace_object_animation","area":"events","target":"7","old":"walk","reset":true}`.

The owner removed the Live Shaders window, material/player uniform operations and unused Live texture replacement in D15. The separate blueprint visual editor and renderer remain unchanged. Private `test-events.py --phase animation_replacement` verifies exact mapping and reset on creatures, placeables, doors and ground items. No demo rebuild or lifecycle action occurred.

## DM private reply (sixth acceptance D10)

`{"op":"pc_message","area":"events","target":"7ffffffe","text":"You notice fresh tracks.","mode":"dm"}` requires both player permissions and a real PC. Native `SendMessageToPC` sends `DM: You notice fresh tracks.` as private server feedback; it is not the game's DM-avatar channel. `value:{native:"SendMessageToPC",recipient:"7ffffffe",text:"DM: You notice fresh tracks.",submitted:true,delivery_verified:false}` records the call. The existing byte-companion boundary decodes the echo. Other modes are unchanged. [DM capture and client checklist](live-events-wire.md#sixth-acceptance-player-to-dm-chat-d10). Native test `pc_dm_chat` passes, with connected delivery explicitly unverified.

## Sixth-acceptance verification and retention

Current source, exact wire contracts and reproduction are in this document,
[live-events-wire.md](live-events-wire.md) and
[live-windows.md](live-windows.md). This task is session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`; the small ledger is
`target/test-output/live-accept6-astra2/ledger.md`. Native checks reuse the single
owned `~/mglive-events` private server workset (UDP5126/Redis6392) through
`tools/live/test-events.py`: phases `vfx_transform`, `server_factions`,
`animation_replacement`, `equipment_look`, `pc_dm_chat` and `drop_flags`.
`tools/live/demo/test-retired.ps1` proves offline HAK-list migration and the
stale-cache startup refusal. No running demo or application binary was changed.

After the owner withdrew the colour/shader feature, provenance-checked derived
colour assets, its private fixture HAK, orphan bytecode, and five obsolete
shader/colour UI fixtures were removed: **24,214,073 logical bytes (23.09 MiB)**.
Records: `colour-cleanup.json` and `ui-retirement-cleanup.json` beside the ledger.
The former sources remain reproducible from Git `ec46580`; the retained native
negative-proof PNGs and verdict/lineage JSON remain under
`target/test-output/live-colour-targets`. Current source, tests, engine result
packets and before/after UI captures remain required evidence. Shared target
builds, other workers' files and the running demo's loaded HAK were untouched.
Physical allocation and free-space change were not attributed on the shared
disk. Active session history was not changed: the agent's local Codex session file
(metadata only: 121,149,292 bytes at closure inspection; still growing). It is
registered for separate retention review; deletion needs supported tooling and
explicit authorization, and is not part of this task.

OKF CLI was unavailable; the index and log were maintained manually. These are
checks of the implementation, not owner acceptance or proof of connected-player
delivery. D10 still needs the connected-client check documented above.

Final guarded checks on this acceptance: mg-live **119 unit tests**, plus two
integration/document example checks (121 passed, four opt-in tests ignored);
mg-ui **668 passed, 18 ignored**; both crates' all-target Clippy pass with
`-D warnings`. Commands use `tools/live/check.ps1 -Phase 5 -Package CRATE
-Jobs 1 -NoUiDebug -UiUnoptimized` (add `-Lint` for Clippy). Logs use
`-Step acceptance6-npc-looks`, `acceptance6-ui-full`, `acceptance6-core-lint`,
and `acceptance6-ui-lint` under `target/test-output/live-mode/`. Ordinary checks
set NWN_ROOT to a nonexistent directory; the explicit real-resource UI checks
and inspected pictures are separate, recorded in live-windows.md.

Astra1 integration `bbb17a5` closes the D8 view part: both matched and extra NPCs
replace their equipment preview on each new hash; inspected lower-angle pictures
show the sword disappearing. Native report/store proof remains `dc31430`.
The real-resource acceptance suite passed all 11 checks, including the D10
To DM tab/private-feedback recipient test (`a01d400`). This proves UI filtering
and queued commands, not delivery to a connected player.

## Reply to player speech (2026-10-15)

Public API/MCP: `live_reply_to_player`; game op: `reply_to_player`.
Input fields are `player` (a reported hexadecimal PC ID), `text` (1..200
nonblank game-codepage characters), and optional `as` (NPC ID). Omit `area`:
the store and game resolve the player's current area. Both the Moonglow Players
policy/client `players` group and server `MGLIVE_ALLOW_PLAYER_OPS=1` are required;
Moonglow, not the caller, inserts `players:true` on the Redis command.

```json
{"player":"7ffffffe","text":"The harbour gate closes at dusk.","as":"3d"}
```

Equivalent wire command:

```json
{"id":"reply-1","op":"reply_to_player","player":"7ffffffe","text":"The harbour gate closes at dusk.","as":"3d","players":true}
```

The native chat hook remembers the player's latest non-silent speech route on
the player object. A DM-directed line selects a private native channel **30**
reply; `as` is ignored for that route. A connected DM avatar supplies the speaker
identity if present, otherwise the module object is used (result name `DM`).
Other speech, or no recorded line yet, selects the supplied NPC or the nearest
living, unprotected NPC within 20 m in the same area. PC/DM/possessed creatures
and their associates are not eligible NPC voices. Native talk channel 1 keeps
normal hearing rules; the operation does not cancel that NPC's patrol/actions.

`live_wait` returns the selected speaker and actual submission outcome:

```json
{"id":"reply-1","ok":true,"text_kind":"reply","value":{"speaker":"3d","name":"Street lamplighter","recipient":"7ffffffe","channel":1,"route":"talk","native":"NWNX_Chat_SendMessage","text":"The harbour gate closes at dusk.","submitted":true,"delivery_verified":false}}
```

Name/text use the same byte-companion decoding boundary as speech. For DM
routing the channel is 30 and route is `dm`; `speaker:"0"` denotes the module
fallback, not a fabricated character. Submission does not prove client display.
A vanished/non-PC recipient is refused; a native send failure returns
`ok:false,code:"invalid"` and the same value with `submitted:false`. Other
refusals: `disabled` for either switch, `forbidden` for client scope, `invalid`
for malformed IDs/text or a speaker beyond 20 m, `not_found` for missing
player/speaker or no eligible nearby NPC, `other_area` for the named speaker,
`protected` for NPC/DM recipients or protected voices, and `dead` for a named
dead NPC. Store freshness, idempotency and boot expiry remain unchanged.

Native basis: the installed read-only `~/unified/Plugins/Chat/Chat.cpp`
`SendMessage` (lines 146..211) resolves a specified target to a native player ID;
for channels PlayerDm/DmDm it calls `SendServerToPlayerChat_DM_Silent_Shout`
directly. Thus this op uses an actual targeted DM channel, unlike the earlier
`pc_message mode:"dm"` private-feedback helper. That existing helper is unchanged.
The targeted path bypasses the ordinary broadcast chat hook: rely on the
command result for the outgoing DM submission, not an invented incoming chat
event or claimed delivery receipt.

Proof: `wsl -e python3 tools/live/test-events.py --phase pc_reply` on the owned
5126/6392 server compiles production helpers plus `mget_reply.nss`. Production
permission/input/PC gates reject the NPC stand-in. The exact shared delivery
helper selects native NPC 5 automatically and NPC 6 explicitly; both produce
real channel-1 chat with the corresponding speaker IDs. Channel 30 is selected
for the DM route and correctly returns false for the non-client stand-in.
Evidence is `target/test-output/live-events/pc_reply.json`, including source,
module/plugin hashes and owned-process exit receipts. Unit tests cover the
Players gate, malformed text/IDs, NPC/DM rejection and exact outgoing JSON;
the catalogue/OpenAPI now includes this 83rd operation.

**Connected-client limitation:** no PC/DM was connected to the private server.
The native source and negative readback establish channel selection and target
validation, not the client's visible DM sender label. Owner check after the
next authorized rebuild: with both switches on, speak to the DM as a PC,
invoke `live_reply_to_player`, and confirm only that PC receives the DM-channel
reply; test with and without a connected DM avatar. Then speak normally and
confirm the selected/nearest NPC talks. Player/possessed `nearby` context is a
latest-report estimate, as specified in [the event contract](live-events-wire.md#immediate-chat-and-agent-watches-2026-10-15).
The running demo was not touched. UI handoff: input player ID, reply text and
optional NPC voice; display `route`, speaker name and submitted status, and keep
client delivery explicitly unverified.

## Atmosphere (audit H6, 2026-10-15)

These `npc`-scope operations require a reported `area`, no target/player switch. API names add `live_` to the wire `op`. All numbers must be finite; native validation is independent. Invalid enum/range/empty change returns `invalid`, absent area `not_found`.

| op | Minimal command fields after `id`/`area` | Constraints / result |
| --- | --- | --- |
| `set_weather` | `{"weather":"rain"}` | `clear/rain/snow/default`; optional `all:false` selects one outdoor area, true all outdoor areas. Indoor single-area requests refused. Default restores area weather settings. |
| `set_fog` | `{"period":"both","color":1193046,"amount":42}` | `day/night/both`; RGB integer0..16777215 and/or amount0..100; optional fade0..30 seconds. |
| `set_area_light` | `{"period":"night","ambient":1122867,"diffuse":4478310}` | `day/night`; at least ambient/diffuse RGB; optional fade0..30 seconds. |
| `set_skybox` | `{"row":1}` | Loaded skyboxes.2da row0..65535. |
| `set_music` | `{"slot":"battle","track":3,"action":"play"}` | day/night/battle; track and/or play/stop; loaded ambientmusic.2da row. Background play uses the current day/night, not a forced time override. |
| `set_ambient_sound` | `{"period":"day","track":1,"volume":90,"action":"play"}` | day/night; at least track, volume0..127 or play/stop; loaded ambientsound.2da row. |
| `sound_at` | `{"resref":"al_pl_whispersm","x":5,"y":5,"z":0}` | Resref1..16 ASCII, finite point inside area. Temporary invisible waypoint executes PlaySound and is destroyed after2s. |

Except sound_at, `value` returns native getters: weather, skybox, fog_day_color/fog_night_color, fog_day_amount/fog_night_amount, light_day_ambient/light_day_diffuse/light_night_ambient/light_night_diffuse, music_day/music_night/music_battle, music_playing/battle_playing, ambient_day/ambient_night, ambient_day_volume/ambient_night_volume. These are engine state, not cached requests. Sound returns `obj` (ephemeral helper) and `value:{"submitted":true}` after its native script executes. It is not an audibility receipt. A dedicated server may not load client sound resources, so existence cannot be inferred from ResManGetAliasFor; only sound name syntax is checked. Native sound and ambient playback have no delivery getter. No new atmosphere area-report field is implied.

Reproduce: `wsl -e env MGLIVE_FIXTURE_MG=/mnt/c/Projects/moonglow-toolset-original/target/release/mg.exe python3 tools/live/test-events.py --phase atmosphere` (uses an existing compiler, never builds apps). Without the env override the normal guarded library fixture compiler is used. Evidence `target/test-output/live-events/atmosphere.json` and `atmosphere-state.json`:15 native readback changes,9 validation refusals, sound submission and helper removal; no TMI, all owned children reaped. Installed declarations SHA256 `41e964b1643d18de65b39f4c6bf520cd18cf0c20408a2f04de517b101a1348a4`; primary lookup helper `tools/live/inspect-native-api.py`. NWNX Area enabled and its include extracted for the next build. Never ran the active demo launcher. Rust wire/validation, catalogue/OpenAPI and full mg-live tests pass. Client rendering/fades and audibility remain unverified.

UI handoff: area selector shared with Live; weather choice/all checkbox, day/night selectors, RGB pickers, fog density/fade, skybox row picker, music/ambient row pickers with play/stop and volume, point sound resref + aim. Native return values can refresh fields after a command.

## Additional spawn kinds (audit H8, 2026-10-15)

`spawn.kind` now also accepts `waypoint`, `sound`, `encounter`, `store`, `trigger`; the existing `r,x,y,z,f,count` fields and count1..10 spread apply. Faction is still creature-only. Native loaded resources are UTW/UTS/UTE/UTM/UTT respectively. `TemplateToJson`/`JsonToObject` loads the non-sound kinds; NWNX Area creates sounds. UTT/UTE templates normally lack instance geometry: the loader supplies a2m square footprint when absent (loading a trigger without geometry stalled the private command path). Use `spawn_trigger` for an explicit shape. No source blueprint is rewritten.

`spawn_trigger` (API `live_spawn_trigger`, group `spawn`) fields: `area,x,y,z,f,shape`, optional `r` (UTT palette blueprint). `shape:"circle"` requires `radius` only; `shape:"rectangle"` requires `width,height` only. Dimensions must be finite, >0 and <=30m; all rotated vertices must remain inside the area. Circle uses16 vertices. With no `r` it creates a script-free generic trigger; with `r` it retains the blueprint's properties/scripts and replaces instance geometry. Example input: `{"area":"events","x":10,"y":10,"z":0,"f":30,"shape":"rectangle","width":6,"height":4,"r":"mget_trigger"}`. Result `obj` is the new ID; `value.geometry` is native NWNX readback and `contains_center` verifies the shape. NWNX's setter places the object's origin at its first vertex; geometry encloses the requested centre.

Reports add optional/default-empty `waypoints`, `sounds`, `encounters`, `stores`, `triggers` collections, with the same object envelope. Rust full/delta merge, limits, text decoding, find and area/status counts include all five. Sound has no public NWScript object-type bit and is skipped by native area iteration: only Live-created sounds are currently reported, through an area-owned registry capped at256 and pruned after removal. Module-authored pre-existing sounds are not enumerated. Other new kinds use ordinary area iteration. No sound audibility/client rendering claim is made.

`destroy {target}` removes waypoints, sounds, stores and triggers. **Encounter removal is blocked**: native DestroyObject leaves a loaded encounter active/valid after20s, including separate SetIsDestroyable and AI-level attempts. Both Rust and native command paths return `disabled` instead of false success. Spawn/report encounter works; a dedicated safe engine hook is still needed for removal.

Private proof: `wsl -e env MGLIVE_FIXTURE_MG=/mnt/c/Projects/moonglow-toolset-original/target/release/mg.exe python3 tools/live/test-events.py --phase spawn_world`; `trigger_probe` records the native geometry format. Evidence `target/test-output/live-events/spawn_world.json`, `spawn_world-state.json`, `trigger_probe.json`. Two instances per kind, distinct positions, native type/area, report appearance/removal, both shapes, loaded UTT and input refusals are checked. Encounter negative proof is retained in the same test. Rust world-kind delta/find/removal and flat schema guards pass. All private processes are reaped. UI handoff: palette categories map to the new SpawnKind variants; rectangle aim sends SpawnTrigger with optional r, centre, dimensions and facing; disable encounter Remove with the native reason.

## Authenticated command author (H15)

Moonglow adds optional `by` to native command JSON from the authenticated client,
never from input fields. Native results echo it and Moonglow verifies it against
the command owner. Local GUI commands omit it. Example native command:
`{"id":"say-1","area":"room","op":"npc_say","target":"4","text":"Welcome","by":"Storyteller"}`;
result: `{"id":"say-1","ok":true,"by":"Storyteller"}` (other ordinary result
fields unchanged). [Event causality, limits and proof](agent-protocol.md#command-authorship-audit-h15).

## Area subscriptions (2026-10-16)

`watch_areas` (API/MCP `live_watch_areas`) is module-wide observation control, in the `read` client group.
It needs no `area`, target, previous area report or player permission. The caller
provides 1?64 distinct (case-insensitive) area resrefs and a required boolean `on`:

```json
{"op":"watch_areas","areas":["demo","demo_area"],"on":true}
```

Redis commands additionally require an `id`; API calls return `queued`, then
`live_wait` returns `{"id":"?","ok":true}`. `on:false` clears the named areas'
`MGLIVE_WATCHED` locals. Invalid types, names, duplicates or counts return
`invalid`; an absent area returns `not_found`. The whole batch is checked before
any mutation. Repeating a watch is harmless. A changed selection forces a full
report on the next tick, including placeables/doors/items and looks, so it also
boots an area that has never reported. A report already being assembled can
precede that tick.

Every native report and `live_status` carries `watched_areas:["demo",?]`, the
actual explicitly watched server areas. Old reports yield `null` (unsupported),
not a guessed list. This field excludes player occupancy, `MGLIVE_AREAS` extras
and the short teleport handoff. Clearing a flag never overrides those independent
reasons to report. Teleport exposes source/destination for up to five report
sequences for verification/follow handoff; it no longer leaves permanent watches.
Unwatched cached store rows age normally; their presence is not a subscription.

Moonglow computes one local union from open Live area views, pinned Chat windows,
the main Chat's selected/current area, Focus members/filter while Focus is open,
and the followed object's latest known area. Closing a window releases its part;
a second window can still hold the same area. Cached closed views/chat history
hold nothing. The store serializes changes, waits for acknowledgements, retries
lost control messages, and replays the full desired set after native boot/module
reload and either Redis connection reconnect. It releases only areas it managed,
leaving unrelated explicit watches alone. Native flags are server-wide, not
per-client leases: separate controllers setting the same flag must coordinate.
At most 64 distinct local areas can be watched at once; excess demand is shown in
the Live panel. `watch_error` in status exposes native subscription refusals.
Older game scripts without `watched_areas` keep working but need a newly prepared
Live module to support automatic subscriptions.

Reproduction: `python3 tools/live/test-events.py --phase watch_areas`, with the
existing marker/lock/identity-guarded private server (`5126/6392`,
`~/mglive-events`, empty startup extras). Store tests cover validation/wire,
boot/reconnect replay and close-during-activation; the app test covers a fresh
empty store, Live map activation, shared pinned Chat ownership and final close.
The real-PC occupancy branch requires a connected private client; headless proof
must not be presented as a connected-PC test.

Native proof: first watch reported its previously unseen placeable in a full
report after **one tick (~1.0 s)**; unwatch stayed absent for three following
ticks. Empty start, six atomic validation refusals, repeated watch, and startup
extras surviving unwatch, and expiry of the teleport handoff passed. Server/module/compiler/include hashes and
owned-process exit identities are retained in
`target/test-output/live-events/watch_areas.json`; the compact report transcript
is `watch_areas-state.json`. The fixture is authored and regenerated in place.

Guarded checks (PowerShell7, repository root, main target; no app build):

```powershell
tools/live/check.ps1 -Phase 8 -Package mg-live -Jobs 1 -NoUiDebug -UiUnoptimized
tools/live/check.ps1 -Phase 8 -UiTest live_watch_areas -Jobs 1 -NoUiDebug -UiUnoptimized
tools/live/check.ps1 -Phase 8 -Package mg-live -Lint -Jobs 1 -NoUiDebug -UiUnoptimized
tools/live/check.ps1 -Phase 8 -Package mg-ui -Lint -Jobs 1 -NoUiDebug -UiUnoptimized
cargo +1.98.0 fmt --all --check
```

Result:176 core tests passed,4 opt-in tests ignored;5 guarded integration test
entrypoints passed (native tests remain guarded), one fake-store app flow passed;
both all-target Clippy checks passed with warnings denied, and formatting passed.
Final private native watch latency:1002.56ms/one tick, no TMI/script errors,
owned processes exited. These checks do not constitute owner acceptance or prove
connected-PC occupancy. The running demo was neither rebuilt nor restarted.
Evidence/cleanup inventory: `target/test-output/live-watch/ledger.md`. The small
regenerable app fixture was retained after automatic approval review blocked its
scoped deletion; no space reclamation is claimed. Shared build/fixture data and
the active conversation history were left intact.
