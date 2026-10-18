---
type: Reference
title: 'Live game events: wire contract and engine proof'
description: Game-side event wire contract, tested private-server transport, exact proof sources and coverage limits. General instant VFX is deferred by coordinator decision.
tags: [live, events, nwnx, vfx, protocol]
generated: { by: codex/astra2, at: 2026-10-14T00:00:00Z }
sources:
  - ../crates/mg-live/src/assistant.rs
  - ../crates/mg-live/src/assistant_tests.rs
  - ../crates/mg-live/src/store.rs
  - https://nwnxee.github.io/unified/nwnx__events_8nss.html
  - https://github.com/nwnxee/unified/blob/3f504d0f1463e49d405639bd3d454ae7416b1a98/Plugins/Events/Events/EffectEvents.cpp
---

# Live game events

Status: **E3.2 chat/buffer and E3.3 combat/death/spells engine checks passed;
player lifecycle compiled, pending client proof; E3.4 log and narrowed E4.3
lasting VFX/projectile engine checks passed**.
Coordinator accepted the source finding below and resumed the task with general
instant VFX deferred. A blocker in one task no longer blocks the others.

Initial E3 worker (historical): `C:\Projects\moonglow-toolset-original-astra2`, `live-mode-events`,
base `a70cd30507d49062ceb8915d1b578c14bf63f72d`, session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`. Rust and `mglive_exec.nss` are outside
that original assignment's scope. Current work is in the main `live-mode` tree;
the Focus additions and their proof limits are recorded below.

## State envelope and event transport

**Current transport (2026-10-16):** game events and followed server-log lines use
the bounded Redis Stream `<prefix>.events`. See [durable event delivery](#durable-event-delivery-2026-10-16)
for the wire, acknowledgement, overflow and crash-recovery contract. The report
batch and immediate pub/sub descriptions below record the earlier transport.
New reports carry `event_stream:true`; their state still travels over pub/sub.

Existing report fields are unchanged. Every `<prefix>.state` report adds
`events` (array, at most 256) and `dropped` (cumulative number of oldest
buffered entries discarded during this boot). Both initialize to empty/zero.
The module retains up to2,048 entries and sends at most256 per report; a short
backlog drains in sequence order over later reports. Overflow discards the oldest
entry and increments `dropped`. The module buffer retains entries while PUBLISH
reports zero subscribers.
A positive subscriber count clears the transmitted entries; pub/sub has no
end-to-end acknowledgement or durable delivery guarantee. Consumers deduplicate
by `(boot, event.seq)`, independently of report `seq`; a new boot resets event
sequence and dropped count. Sequence numbers start at 1 and increase on enqueue,
including entries subsequently dropped. Counters are signed NWScript integers;
indefinite operation past 2,147,483,647 events is not supported.

```json
{"v":1,"boot":"58671500c52f19d1febc4892959fa92e","seq":2,"ack":[],"results":[],"areas":[],"dropped":0,"events":[{"seq":1,"kind":"chat","time":"1791815908.123456","id":"2","name":"Bandit","area":"events","target":null,"channel":1,"text":"Hello"}]}
```

Every event has `seq` (integer), `kind` (string), `time` (Unix UTC seconds
as a decimal **string**, six fractional digits), `id` (source/speaker object
ID or null), `name` (source display name, empty if unavailable), `area`
(source area resref or null), and `target` (object ID or null). Object IDs
are opaque engine hex strings, not integers; do not assume fixed-width padding.
Time comes from `NWNX_Util_GetHighResTimeStamp`; JSON float is deliberately
avoided to preserve epoch precision. Null is unavailable, not an empty object.
All examples describe the wire shape; exact observed values are in proof JSON.

### chat

`kind:"chat"` adds `channel` (raw NWNX integer) and `text` (the complete
message, JSON escaped). The callback does not filter channels or suppress
messages. Channels: 1 talk, 2 shout, 3 whisper, 4 tell, 5 server, 6 party,
14 player-to-DM, 17 DM talk, 18 DM shout, 19 DM whisper, 20 DM tell,
22 DM party, 30 DM-to-DM. Unknown future channel numbers are retained.
`target` is the resolved recipient character when available, otherwise null.
The same hook covers native NPC `SpeakString`; no dispatcher fallback needed.

Deploy the authored `mglive_*.nss` scripts, extract `nwnx_chat.nss` alongside
the existing Redis/Util includes, and enable `NWNX_CHAT_SKIP=n`.
`mglive_load` initializes the buffer and calls `mglive_evload`, which registers
`mglive_chat`. NWNX has one global chat callback: registration replaces any
existing callback, so modules with their own callback must integrate/chains
explicitly. These collectors do not change attack/chat delivery behavior.

### Focus: chat recipients and speaker position (2026-10-14)

`target` has always come from `NWNX_Chat_GetTarget()`. Focus adds `recipient`
(the same object ID/null, explicit chat meaning), `audience`, and `position`.
`position:{x,y,z}` is the speaker's position at callback time, in `area`; null
means the speaker has no valid area. In particular talk/whisper keep the speaker
identity and location even if the next report observes that actor elsewhere.

```json
{"seq":41,"kind":"chat","time":"1791995000.123456","id":"4","name":"Guard","area":"events","target":"7","recipient":"7","channel":4,"audience":"direct","silent":false,"position":{"x":3,"y":5,"z":0},"text":"A private tell"}
```

This tell example defines the wire shape; positive connected-recipient delivery
has not been proven by the headless test. The installed plugin resolves the native
tell **player ID** to its active character object before calling our collector
(`Plugins/Chat/Chat.cpp:31-49`, unified commit
`3f504d0f1463e49d405639bd3d454ae7416b1a98`). Do not substitute the speaker or a
nearby NPC when the recipient is null.

`audience` is a routing classification, **not a list of clients that received it**:

| Value | Meaning |
| --- | --- |
| `direct` | NWNX resolved one recipient; use `recipient`/`target`. Includes tells (4/20) and any other targeted callback. |
| `unresolved` | Tell (4/20), but no recipient could be resolved. |
| `dm` | Channel 14 (non-silent player-to-DM) or 30 (DM-to-DM), with no single recipient. |
| `party` | Channel 6 or 22, no single recipient. |
| `vicinity` | Talk/whisper 1/3/17/19, no single recipient. |
| `server` | Shout/server-message 2/5/18, no single recipient. |
| `ai` | Existing silent AI classification (13, or NPC-only 14). |
| `unknown` | Future/unclassified channel. Raw `channel` is always retained. |

DM talk/shout/whisper are channels 17/18/19; they are not necessarily private
messages. DM tells are channel 20. A player-to-DM broadcast (14) and DM-to-DM
broadcast (30) genuinely have no single recipient in this API. Focus should label
their audience, not invent a conversation target. Other scripts that call native
per-player send functions directly may bypass NWNX Chat's generic send hook;
`Chat.cpp:165-209` explicitly does this for some targeted talk/whisper/DM routes.
This collector cannot recover packets that never invoke that hook.

Proof: `tools/live/test-events.py --phase focus_chat`, private UDP5126/Redis6392
and owned `~/mglive-events`, passes all 13 supported callback channels plus native
NPC talk/whisper. It checks exact source/area/position, raw channel, recipient/null,
audience, and native refusal (return 0, no event) when an NPC is passed as the
recipient of a tell/DM tell. No NPC is misrepresented as a connected player.
`target/test-output/live-events/focus_chat.json` binds compiler, module, script,
NWNX/plugin hashes and exited owned processes; `-state.json` is the wire evidence.
Run with the existing private fixture harness; optional
`MGLIVE_FIXTURE_MG=/mnt/c/Projects/moonglow-toolset-original/target/release/mg.exe`
uses the already-built compiler without building apps. The running demo is untouched.

Still requires a connected PC/DM check: send a normal tell and a DM tell to a
second character, then verify `target==recipient==receiver.id`, correct sender,
channel 4/20 and `audience:direct`. Check a PC-to-DM broadcast separately: channel14,
`silent:false`, `audience:dm`, normally null recipient. Source inspection supports
this behavior; the headless proof does not claim client delivery or a real click.

### Focus: dialogue node events (automatic Live build capture, H11)

**Feasible through conversation action scripts; not a global dialogue tap.**
Installed NWNX unified `3f504d0f1463e49d405639bd3d454ae7416b1a98` has no
`NWNX_ON_DIALOG_*` / conversation-node subscription in its Events plugin.
Its separate Dialog plugin hooks native `SendDialogEntry`, `SendDialogReplies`,
`HandleReply`, `CheckScript` and `RunScript` to expose the current context to
scripts (`Plugins/Dialog/Dialog.cpp:83-195,199-285`). It exports node type, ID and
locstring getters, not a registration API that calls a collector for every node.
The Events `NWNX_ON_DEBUG_RUN_SCRIPT_*` events concern a player's debug command;
they are not general VM script execution hooks.

The shipped `mglive_dialog.nss` include exports:

```c
int MgLiveDialogLine(string dialog, object speaker = OBJECT_INVALID,
    object listener = OBJECT_INVALID, int language = NWNX_DIALOG_LANGUAGE_POLISH);
```

The Live build now installs this call automatically (details below). For manual
integration, call it once at the beginning of each node's existing **Actions Taken** script,
using the actual `.dlg` resref (without extension, 1..16 bytes). Keep the original
action code and parameters. Example:

```c
#include "mglive_dialog"
void main()
{
    MgLiveDialogLine("quest_intro");
    // The existing action body follows unchanged.
}
```

For an otherwise empty Actions Taken slot, use the supplied `mglive_dlgev` script
with script parameter `mglive_dialog=quest_intro`. Do not replace an existing
action with that script, and do not attach the collector to a Starting Conditional.
It returns FALSE and emits nothing outside an entry/reply Actions Taken context,
so offered/filtered reply candidates do not become conversation records. It also
rejects absent/oversized dialogue identity. The true resref is explicit because
the actor's default dialogue can differ from an `ActionStartConversation` override.
Automatic capture modifies only the dedicated Live build output, never the source module.

Enable `NWNX_DIALOG_SKIP=n`, extract `nwnx_dialog.nss`, compile and deploy the
collector and instrumented scripts. The include extractor and next-start private
launcher now include this plugin; neither was run on the active demo.

`kind:"dialog"` has the common event envelope and these fields:

| Field | Type and meaning |
| --- | --- |
| `speaker` | Object ID/null, alias of common `id`; `name` names this object. |
| `listener` | Object ID/null, alias of common `target` for existing event filtering. |
| `owner` | Object ID of the native conversation action-script owner (`OBJECT_SELF`). |
| `speaker_is_pc` | Boolean, or null if the speaker is unknown. |
| `dialog` | Explicit actual DLG resref. |
| `node` | Absolute zero-based node ID, from NWNX Dialog. |
| `node_type` | `entry` or `reply`; these have separate ID namespaces. |
| `language` | Requested NWNX locstring language; default5=Polish, 0=English. Native fallback applies. |
| `text_source` | `node_locstring`: native text at action execution, before client token substitution. |
| `text` / `text_bytes` | Same raw-byte companion and one Rust decoding boundary as speech. |

No chat channel is invented. The event means the engine executed that node's
action; it is not a delivery acknowledgement. Replies can be auto-followed by
the engine as well as chosen by a player. Default entry attribution is owner ->
`GetPCSpeaker()` (fallback `GetLastSpeaker()`); reply is `GetPCSpeaker()` (fallback
`GetLastSpeaker()`) -> owner. Missing identities stay null. If the source is unknown,
`area` falls back to the owner's area. For nodes using a different **Speaker tag**,
pass the actual speaker/listener explicitly: the installed Dialog API does not
expose that tag, so the collector cannot infer it safely from the node text.

Example after Moonglow's byte decoder (IDs illustrative):

```json
{"seq":42,"kind":"dialog","time":"1791995300.123456","area":"events","id":"4","speaker":"4","name":"Guard","target":"7","listener":"7","owner":"4","speaker_is_pc":false,"dialog":"quest_intro","node":0,"node_type":"entry","language":5,"text_source":"node_locstring","text":"Naprawdę: ąęłńóśżź"}
```

On the raw wire the measured CP1250 text carries
`text_bytes:"Naprawd%EA%3A+%B9%EA%B3%F1%F3%9C%BF%9F"`; this preserves the original
bytes even when native `JsonString` produces the wrong glyphs in `text`.
`wire_text::report` already decodes every event's text: valid UTF-8 stays UTF-8,
otherwise the configured legacy codepage (default cp1250). No dialogue-specific
second decoder was added. Named API clients need `read_chat` for dialogue events,
just as for speech; the default token retains its existing full rights.

**Native proof:** `test-events.py --phase focus_dialog` compiles an authored
`mget_focus.dlg` with separate entry0/reply0, custom TLK lines containing
`ąęłńóśżź`, native condition/action scripts and an original action parameter.
Two NPCs on private UDP5126/Redis6392 cause the engine to execute entry and
auto-followed reply actions. Assertions check both node namespaces, actual owner,
null inferred identities when there is no PC, explicit alternate speaker/listener
IDs, original `preserved=original-param`, byte-perfect Polish entry/reply text,
and zero events from conditions or an out-of-dialog invocation. A separate native
one-liner call executes its condition but no action and produces no `dialog` event.
That is a measured coverage limit, not a reason to log conditional evaluations.

Evidence: `target/test-output/live-events/focus_dialog.json`, `-state.json`,
`-log.json`; source/module/server/NWNX hashes and exited owned-process identities
are included. Reproduce with the same harness as `focus_chat`, changing only
`--phase focus_dialog`. The last accepted current fixture replaces earlier
iterations in the same working set. Guarded mg-live tests:117 passed/4ignored,
integration1 and fixture2; all-target Clippy passes. The regression test decodes
the exact measured node byte companions and preserves speaker/listener/node fields;
the API regression denies `dialog` to a read-only token without `read_chat`.

**Automatic capture (H11):** `Start-LiveDemo.ps1 -CaptureDialogue` defaults
on; `-CaptureDialogue:$false` disables it on the next build. `-SkipBuild`
retains the already-built module. Generic `compile.ps1 -CaptureDialogue` is
opt-in. `demo/build.ps1` now calls `mg live-build --capture-dialogue` directly;
`compile.ps1` is only a compatibility delegate. The shared Rust instrumenter
runs before compilation; the Python instrumenter was removed in L1-L4.
Do not restart an active demo merely to change this option.

The pass reads DLG resources from the read-only V1.0 base MOD/ERF. It replaces
each entry/reply Actions Taken resref with a deterministic `mgd_<hash>` wrapper,
including originally empty slots. The wrapper collects the current node, then
runs the original action once. `SetScriptParam(key, GetScriptParam(key))` forwards
each declared ActionParams key: native ExecuteScript alone loses these parameters.
Speaker tags are resolved explicitly; non-ASCII tag/key bytes are constructed
from NWNX's byte table. Locstring payloads, conditions, links and action-parameter
bytes remain unchanged. The output's ownership/input manifest and wrapper hashes
are checked before writes; an edited/unowned collision refuses the build.

The `mg live-build --json` report records node-to-wrapper mappings; the output
module's `mglive_build.txt` records generated-source hashes and original load.
Rebuilding from the original source with capture off leaves original DLG bytes
unchanged. Updating an already instrumented Live copy preserves its wrappers;
omitting capture does not remove instrumentation. See [Live build](live-build.md).

**Native proof (accepted H11, before the Rust port):** `test-events.py --phase
capture_dialog` requires `MGLIVE_DIALOGUE_SOURCE` pointing to
the read-only demo MOD. The measured source is
`<demo-source>/demo.mod`;
its exact SHA256 is in `target/test-output/live-events/dialogue-inputs.json`.
Private UDP5126/Redis6392 exercised entry0/reply0 of `city_chat` and `city_gate`,
plus the authored Polish `mget_focus` conversation. Six node events arrived;
the original action ran twice with `preserved=original-param`. The production
helper passed enable/rebuild/disable with unchanged source bytes and byte-exact
restoration. Evidence: `capture_dialog.json`, `capture_dialog-state.json` and
`dialogue-inputs.json` in that same directory. No running demo was rebuilt.

For a new run, use the guarded Rust fixture compiler (no `MGLIVE_FIXTURE_MG`);
it now calls the production Rust instrumenter. The earlier evidence remains
valid for the unchanged native collector. L1-L4 additionally tests byte preservation
and wrapper forwarding in Rust, and starts its compiled copy in the engine.

**Not covered:** DLGs supplied only by HAKs/standard resources,
one-liners that never run a node action, final client token expansion/rendering,
and a real connected PC choosing a reply. The headless proof uses two NPCs and
auto-followed replies. A connected-PC check must select a nonautomatic reply,
match speaker/listener and node IDs, verify Polish text and confirm the original
quest action executes once. A global NWNX dialogue hook remains a possible route
for content outside the build-time pass. These gaps are not silently claimed as
captured conversations.

### Focus: direct DM command messages are not chat-hook events

The installed native server's `ExecuteCommandSendMessageToPC` at `0x6cc690`
calls `CNWSCreature::SendFeedbackMessage` (`0x462ce0`), which calls
`CNWSMessage::SendServerToPlayerCCMessage` (`0x5b85a0`). Static native-call readback
is retained in `target/test-output/live-focus/sendmessage-calls.txt`, bound to the
server hash in `focus_dialog.json`. This is the feedback/combat-message packet
path, not the generic `SendServerToPlayerChatMessage` hook used by NWNX Chat.
Consequently `pc_message`/`SendMessageToPC` must not be advertised as automatically
captured chat. The UI may echo a successful DM command as an explicit command
record; that still does not prove a client's delivery or display. NPC stand-ins
cannot verify client feedback packets.

No event-time `nearby` listener list or delivery list is supplied in this task.
Speaker position is authoritative at capture time; later object positions cannot
reconstruct exactly who was nearby or heard old speech. Focus should retain that
distinction when showing nearby-conversation coverage.

Focus task closure: session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`, main
`live-mode`, ownership ledger/cleanup receipt in `target/test-output/live-focus/`.
Retained current source, authored private fixture and minimal native/decoder
evidence; removed the temporary research helper and staging source copy
(167,052 logical bytes). Native-call evidence was reduced to the relevant calls.
No shared cache, active demo, other worker's files or session history was removed;
volume free-space gain is not attributed while other work runs. History cleanup
requires a separate approved mechanism outside this active session.

### attack

Source `id`/`name` is the attacker, `target` is the victim. `roll` and
`modifier` are the d20 roll and attack modifier. `result` is the native
result: 1 hit, 2 parried, 3 critical, 4 miss, 5 resisted, 7 automatic hit,
8 concealed, 9 miss chance, 10 devastating critical. Preserve unknown values.
`attack_number` is the 1-based index in the round; `attack_type` is the
feat/special attack ID (65002 opportunity, 65003 riposte). `weapon_attack_type`
is 1 main hand, 2 offhand, 3–5 creature weapon, 6 extra/haste, 7 unarmed,
8 extra unarmed. `ranged` and `killing_blow` are booleans. The latter is the
attack hook's prediction, not a confirmed death.

```json
{"seq":2,"kind":"attack","time":"1791818010.281776","id":"2","name":"Bandit","area":"events","target":"5","roll":17,"modifier":3,"result":1,"attack_number":1,"attack_type":0,"weapon_attack_type":1,"ranged":false,"killing_blow":false}
```

### damage

Source is the damager, `target` the damaged creature; `area` is the victim's
area. `damage` is an object containing positive amounts only; absent keys mean
zero/not present. Keys: `base`, `bludgeoning`, `piercing`, `slashing`,
`magical`, `acid`, `cold`, `divine`, `electrical`, `fire`, `negative`,
`positive`, `sonic`, `custom1` through `custom19`. `amount` sums those values.
`spell` is the native associated spell ID, or -1 if unknown. NWNX Damage
fires before HP application: these are hook amounts, not an independently
measured HP delta or a guarantee that other handlers cannot alter damage.

```json
{"seq":5,"kind":"damage","time":"1791818010.907958","id":"2","name":"Bandit","area":"events","target":"5","damage":{"base":7},"amount":7,"spell":-1}
```

### death

Source is the dead creature, `target` is null, `hp` its observed HP, and
`killer` the last observed NWNX damager's object ID or null if unavailable.
The damage hook schedules a 0.01-second deferred `GetIsDead` check. The tick's
creature scan also observes deaths, including non-damage sources. An object
local deduplicates both paths; observing a living creature after death clears
the marker and old damager. A confirmed death does not depend on the attack's
`killing_blow`. Instant destruction before either observation, an unreported
area with a non-damage death, or death and resurrection between observations
can be missed. `killer` is last-damager attribution, not proof of causality
for unrelated later death effects.

```json
{"seq":8,"kind":"death","time":"1791818010.918328","id":"5","name":"Bartender","area":"events","target":null,"hp":-11,"killer":"2"}
```

### spell

Source is the caster; `spell` is the `spells.2da` row. `target` may be null
for a location cast. `position:{x,y,z}` is the target's observed position for
object casts, otherwise the hook's target location in the caster's `area`.
Native object casts were observed to supply a zero event-data vector, so
reading the actual object position avoids inventing a location at (0,0,0).
`countered` and `instant` are booleans. `projectile_path` is the native path
type. This is `NWNX_ON_CAST_SPELL_AFTER`, not an impact or successful-hit
confirmation; Moonglow maps the spell row to visuals.

```json
{"seq":13,"kind":"spell","time":"1791818021.541839","id":"6","name":"Bandit","area":"events","target":"9","spell":107,"position":{"x":14.0,"y":10.0,"z":-1.3},"countered":false,"instant":true,"projectile_path":0}
```

### player_join and player_leave (awaiting real-client proof)

`player` is the account/player name. No CD keys, IPs or authentication fields
are read or transmitted. CONNECT_AFTER runs at character selection: `id`,
`target`, `area` are null, `name` empty, and `dm` is the connection's DM flag.
This is a connection event, not a character-entered-area event. For leave,
DISCONNECT_BEFORE caches character identity/area on the module without
publishing; DISCONNECT_AFTER emits the event with that snapshot and clears it.
Canceling character selection can produce leave with no character. These
scripts compile; automated proof does not impersonate a connected client.

```json
{"seq":14,"kind":"player_join","time":"1791818022.100000","id":null,"name":"","area":null,"target":null,"player":"ExamplePlayer","dm":false}
{"seq":15,"kind":"player_leave","time":"1791818030.100000","id":"a","name":"Example character","area":"events","target":null,"player":"ExamplePlayer"}
```

Enable `NWNX_DAMAGE_SKIP=n` and `NWNX_EVENTS_SKIP=n` in addition to Chat,
Redis, Util and the Phase 1b Creature plugin. The include extractor supplies
`nwnx_damage.nss`, `nwnx_events.nss` and `nwnx_creature.nss`; `prepare.ps1`
no longer retires `nwnx_events`. Damage/attack callbacks are global and replace
any previously registered callbacks; coordinate existing module integrations.

## Lasting VFX in creature reports

Every creature in every full or intermediate state report carries `vfx`, an
array of up to **16 `visualeffects.2da` row integers**. It is a current snapshot,
not an event or a delta; `[]` clears the previous snapshot. Effect iteration
order and duplicates are preserved. Further visual effects beyond the cap are
omitted; this does not increment the event buffer's `dropped` counter. Other
object kinds do not gain this field. The existing creature fields are unchanged:

```json
{"id":"2","name":"Bartender","r":"nw_bartender","x":3.0,"y":3.0,"z":0.0,"hp":[4,4],"vfx":[411]}
```

The loop uses `GetEffectType(e, TRUE) == EFFECT_TYPE_VISUALEFFECT`, explicitly
asking for the internal type, and excludes `DURATION_TYPE_INSTANT`. The row is
`GetEffectInteger(e, 0)`. This index was measured in the engine, not inferred:
applying permanent `VFX_DUR_GLOW_RED` and reading all eight effect integers
produced **`411 0 0 0 0 0 0 0`**, while the state contained `vfx:[411]`.
The fixture exposes those native values through the creature name so the same
Redis report contains both the independent probe and the production field.

The VFX phase also applies temporary `VFX_DUR_GLOW_BLUE` (row 410) and instant
`VFX_IMP_HEALING_M`; only the lasting blue row appears. Seventeen tagged red
effects yield a native count of 17 and a report with 16 rows. Removing all
visual effects changes the report to `vfx:[]`. This is engine-state proof;
it does not verify client or Moonglow rendering.

### vfx event: projectile

`mglive_vfx` subscribes to `NWNX_ON_BROADCAST_SAFE_PROJECTILE_AFTER` only,
without a whitelist. Each observation enters the common buffer as `kind:"vfx"`,
`subtype:"projectile"`. `id`/`name` is the callback's source object, `target`
the target object or null for a location. `position:{x,y,z}` is the raw target
location from the native callback, in `area`. Additional integer fields:

* `spell`: `spells.2da` row, -1 if the native unsigned invalid sentinel is used.
* `projectile_type`: native projectile category, **not a visualeffects row**.
  Values are 0 weapon/no VFX, 1 acid, 2 cold, 3 electrical, 4 fire, 5 sonic,
  6 default spell, 7 spell using path. Preserve unknown values.
* `projectile_path`: native `PROJECTILE_PATH_TYPE_*` value.
* `delta_ms`: native time in milliseconds before destination, including zero.
* `attack_result`: 0 means not applicable; otherwise the same result numbers
  described for attack events.

```json
{"seq":13,"kind":"vfx","subtype":"projectile","time":"1791818778.692307","id":"3","name":"Bandit","area":"events","target":"7","spell":107,"projectile_type":6,"projectile_path":0,"delta_ms":0,"attack_result":0,"position":{"x":13.67,"y":11.34,"z":-1.30}}
```

This is a broadcast observation, not a confirmed impact or client render.
Native magic missile 107 produces both a projectile observation and a separate
`spell` event. Their order is not guaranteed; in the proof the projectile
arrived first. Zero `delta_ms` is a real engine value at short range and must
not be rejected. A spell event remains `kind:"spell"`; it is not duplicated
as an invented instant-VFX event. General instantaneous VFX is deferred below.
Native fireball (`SPELL_FIREBALL`, row **58**) also proved location casts:
both events had `target:null` and the same nonzero target position, with a
246 ms projectile delay in one measured run. The test initially expected
the wrong fireball row (44); the captured native event corrected that fixture
assumption. The cast fixture's delayed release is token-guarded so an earlier
cast cannot release a later hold.

```json
{"seq":24,"kind":"vfx","subtype":"projectile","time":"1791819027.826514","id":"3","name":"Bandit","area":"events","target":null,"spell":58,"projectile_type":6,"projectile_path":0,"delta_ms":246,"attack_result":0,"position":{"x":13.67,"y":11.34,"z":-1.30}}
{"seq":25,"kind":"spell","time":"1791819027.826597","id":"3","name":"Bandit","area":"events","target":null,"spell":58,"projectile_path":0,"instant":false,"countered":false,"position":{"x":13.67,"y":11.34,"z":-1.30}}
```

## Server log channel

The current follower commits lines to the same durable stream as game events.
The `.log` channel description below is the legacy wire, still accepted from
older followers. File-following coverage and initial-history limits are unchanged.

The separate helper publishes JSON messages to `<prefix>.log`, not to the
module event buffer. Run it with the same prefix, Redis port and boot as the
server; it does not discover or stop other processes:

```sh
MGLIVE_INSTANCE=events MGLIVE_REDIS_PORT=6392 MGLIVE_PREFIX=mglive-events \
  MGLIVE_BOOT=<server-boot> bash tools/live/wsl/log-tail.sh
```

For a production demo, `MGLIVE_INSTANCE=demo` retains the existing `owned.sh`
marker/scratch convention. The new `events` instance exclusively uses
`~/mglive-events` and `.moonglow-live-events` containing
`moonglow-live-events-v1`. Unknown instances, symlinks, unmarked existing
scratch, and a second follower of the same scratch are rejected. The helper
only connects to Redis on 127.0.0.1. Its caller owns shutdown; the proof runner
records and terminates only its exact child PID/start/executable.

```json
{"v":1,"kind":"server_log","boot":"example-server-boot","stream":"fb0ffb97c792025a04f92ba1d54ea922","seq":2,"time":"1791818100.123456","line":"[Mon Oct 12 17:15:00] Script error: example","script_error":true}
```

`v` is protocol version 1, `kind` is `server_log`. `boot` is the supplied
server boot string (null if omitted). `stream` is a random helper-instance
ID; `seq` increases from 1 within that stream, separately from game events.
Deduplicate retries by `(stream,seq)`. `time` uses the common decimal Unix
seconds string, but is **read time**, not the log's original timestamp.
`line` is the complete line without its CR/LF terminator. A non-ASCII line
also carries lossless `line_bytes`; Moonglow decodes it at the shared
[game-text boundary](#module-text-receive-boundary-owner-bug28).
`script_error` is a case-insensitive classifier
for script-error/execution-error, instruction-limit, stack, divide-by-zero
and unknown-opcode messages. It ignores echoed `MGLIVE: chunk` source.

Existing bytes are skipped on initial attachment. A newly created log is
read from its beginning. Newline-complete lines are published; partial lines
wait for completion. On rotation the old descriptor is drained then the new
file is read from the beginning; shrinking files reset the read offset.
One pending message is retried on Redis disconnect or zero subscribers, using
the same stream/sequence. Retention is in the source file and process memory;
no durable delivery acknowledgement or persistent offset exists. Stopping
and restarting the helper skips existing history; rapid multiple rotations,
unobserved truncate-and-regrow, and unterminated final lines are not guaranteed.

`--phase log` passed against the private running server: normal line, a
server-written **classifier fixture** (`Script error: ...`, not a VM-raised
exception), no false positive on echoed source, subscriber-gap retention,
rotation and truncation, ordered sequence, and initial-history skip.
`log.json`/`log-log.json` preserve the observations. An earlier nested
instruction-limit test aborted the command before acknowledgement and did
not put an error in this source file; it was replaced by the explicit
classifier fixture. Other files such as `nwengineLog.txt` and NWNX stdout
are not followed. This helper flags errors present in its input, not every
engine diagnostic on the system.
The successful log phase removes its own rotation copy after all children
exit, checking the embedded boot ID and recording logical bytes in
`cleanup.json`. Failed proof leaves it for diagnosis instead of deleting it.

## Private engine proof and reproduction

Build the CLI with Rust 1.98.0 and two jobs, reusing
the coordinator's target (PowerShell, from this worktree):

```powershell
$env:CARGO_TARGET_DIR = 'C:\Projects\moonglow-toolset-original-claude\target'
$env:CARGO_BUILD_JOBS = '2'
cargo +1.98.0 build -p mg
```

Any Cargo **tests** must go through
`tools/live/check.ps1`; no Rust tests are required for this script-only task.
The common helper now resolves `mg.exe` from `CARGO_TARGET_DIR` too.

Run in WSL from this worktree:

```sh
NWNX_ROOT=~/nwnx-redis-reconnect bash tools/live/test-events.sh --phase chat
NWNX_ROOT=~/nwnx-redis-reconnect bash tools/live/test-events.sh --phase combat
NWNX_ROOT=~/nwnx-redis-reconnect bash tools/live/test-events.sh --phase log
NWNX_ROOT=~/nwnx-redis-reconnect bash tools/live/test-events.sh --phase vfx
```

Python 3 (stdlib only) generates a small authored 2x2 `ttr01` area called
`events`, compiles via the Windows `mg.exe` against the read-only Steam
installation, and packs one current module. No demo, HAK or full asset copy.
The harness starts private Redis 6392 and NWN UDP 5126 under
`~/mglive-events`, ownership marker `.moonglow-live-events`.
Server: `~/server/bin/linux-x86/nwserver-linux`, build
8193.37-17; NWNX 8193.37-17 at the source commit below. `-publicserver 0`,
no player/DM passwords, random unprinted admin password. Exact owned PID,
start tick and executable are checked before graceful shutdown; both children
are reaped. Never launch against occupied ports or an unmarked scratch.

`target/test-output/live-events/chat.json` binds module, script, server and
plugin hashes and test results; `chat-state.json` contains observed reports.
The chat proof passed: native NPC speech including JSON quoting, collector
callbacks for all 13 defined channels, a synchronous 300-message burst retaining
44..299 with exactly 44 drops, monotonically increasing event sequence, and
retention across three seconds without a state subscriber. Commands travel
through Redis `run_script`, `run_chunk`, spawn and speech, not direct VM injection.
Real player tells and recipient resolution still need a connected-client test.

The authorized merge `b04a5d3` integrates `live-mode` at `53584f8`, including
Phase 1b `759684d`, while retaining both sets of plugin flags/includes.
`combat.json` and `combat-state.json` prove native attacks, positive typed
damage, a real dead creature with its damager and one death event, then spell
107 at the expected object/position. Both owned processes exited. Player hooks
remain pending a real client; this is explicitly not owner acceptance.

The initial Tile_ID 0 fixture was a slope: the attacker approached at Z=5
while its victim remained at Z=0. `ttr01.set` identifies tile 50
(`ttr01_g01_01`, four Grass corners of height zero); switching the fixture to
that tile made the same attack produce combat/damage/death. The spell fixture
first establishes native target perception through an attack, then clears it
and holds the cast against routine AI. Without perception the cast was not
queued; after perception it queued but emitted failure reason 0 (canceled)
until held. Raising AI priority alone, changing cheat/metamagic/instant flags,
and adding a special ability did not fix the initial failure. No production
AI or faction rule was changed for these fixture failures. The fixture's
cast diagnostics remain in its small source files.
The final regression also caught intermittent cancellation in the same second
as return damage from the bartender (3 base damage at `1791819388.090975`).
The spell fixture now holds that opponent and plot-protects the caster after
the perception-establishing attack. This isolates native cast/projectile
observation from combat interruption; it does not synthesize callbacks.
The separate lethal combat pair remains unprotected.

Two rejected harness approaches matter for reproduction:

* Default NWNX blocks plugin functions inside `ExecuteScriptChunk`, including
  reentrant callbacks. This caused empty metadata and a contaminated NWNX stack
  in an early test. Plugin test calls now use compiled `run_script` fixtures;
  native `npc_say` tests speech. The launcher does not silently expand chunk
  privileges. Deployments needing plugin calls inside chunks must explicitly
  enable `NWNX_CORE_ALLOW_NWNX_FUNCTIONS_IN_EXECUTE_SCRIPT_CHUNK=1`.
* Sending player-only party/tell channels as an NPC into native delivery without
  clients crashed in `CNWSPlayer::GetGameObject`. The all-channel fixture now
  invokes the real registered hook and production collector, then suppresses
  downstream native delivery. NPC speech still uses normal native delivery.
  This does not prove an actual player's party/tell delivery.

`compile.json` records NWScript compilation. Python syntax and `git diff --check`
are checked. No GUI was touched and no renderer evidence is claimed.

## Not covered

General instant VFX is **deferred by coordinator decision**: the installed
`NWNX_ON_EFFECT_APPLIED_*` excludes visual effects and instant durations.
Lasting VFX comes from the per-creature effect list; spell casts and
projectiles are emitted from their dedicated hooks. Moonglow maps spell
IDs through `spells.2da`. Arbitrary instantaneous object/location effects
need a later verified NWNX hook. No C++ change is authorized in this task.
See [deferred work](deferred.md). Full channel/client lifecycle coverage still
requires a real connected client; simulated hook dispatch is labeled as such.
## Resolved scope decision: effect-applied does not capture instant VFX

The requested route is `NWNX_ON_EFFECT_APPLIED_AFTER`. In the installed
`NWNX_ROOT=~/nwnx-redis-reconnect`:

* `Binaries/NWNX_Events.so` resolves to
  `~/unified/Binaries/NWNX_Events.so`.
* The shipped `Binaries/NWScript.zip`, entry `nwnx_events.nss`, documents
  that this event only fires for temporary/permanent effects and excludes
  visual effects and item-property effects. The same note appears at line
  910 of `~/unified/Plugins/Events/NWScript/nwnx_events.nss`.
* `~/unified/Plugins/Events/Events/EffectEvents.cpp`, lines 38–41,
  returns before dispatch for durations other than Temporary or Permanent.
  Lines 43–50 separately return for VisualEffect, Beam and other types.
  `SignalEvent` is only reached at line 86. Both BEFORE and AFTER use this
  filtered helper.

The local checkout is `3f504d0f1463e49d405639bd3d454ae7416b1a98`;
`git status --short` for those two source files is empty. The official
[event documentation](https://nwnxee.github.io/unified/nwnx__events_8nss.html)
also documents the exclusion. These are **source/include findings**, not a
runtime observation or verified source-to-binary build provenance.

Consequently, subscribing to that event cannot satisfy the proposed
general instant-VFX collector on the documented installation. Spell and
projectile events cannot by themselves identify every arbitrary
`ApplyEffectToObject` / `ApplyEffectAtLocation` visual effect.

This preflight finding narrowed E4.3; it no longer blocks the other tasks.
The engine results above supersede the initial untested status for chat,
combat, death and spells. Real player lifecycle still requires a client.
Spell observations do not prove arbitrary instant-effect capture.

## Reproduce the source check

Run from WSL; all commands are read-only:

```sh
git -C ~/unified rev-parse HEAD
git -C ~/unified status --short -- \
  Plugins/Events/Events/EffectEvents.cpp Plugins/Events/NWScript/nwnx_events.nss
readlink -f ~/nwnx-redis-reconnect/Binaries/NWNX_Events.so
unzip -p ~/nwnx-redis-reconnect/Binaries/NWScript.zip nwnx_events.nss \
  | sed -n '886,910p'
sed -n '33,54p' ~/unified/Plugins/Events/Events/EffectEvents.cpp
sha256sum ~/nwnx-redis-reconnect/Binaries/NWNX_Events.so \
  ~/nwnx-redis-reconnect/Binaries/NWScript.zip \
  ~/unified/Plugins/Events/Events/EffectEvents.cpp
```

Expected findings and hashes are retained in
`target/test-output/live-events/preflight.json` (ignored local evidence).
The matching hashes observed on 2026-10-12 were:

| Resource | SHA-256 |
|---|---|
| `NWNX_Events.so` | `43c8fbbb5ef1e16cb012dd2d1f9f1c0ff503062cab46130079b9f493cca2df70` |
| `NWScript.zip` | `0d57d9549e93ee5bd8cd80371b31efee23b88ff6b6a410d3055b38143e61bd14` |
| `EffectEvents.cpp` | `504daf575b8666e31ccdf993ddd98df5430f93b501e8fd0078de83882308321c` |
| source `nwnx_events.nss` | `23893d8601f45e5adf2c8296d16ad60a51288d9c8574277fda3599cceaa13085` |

## Ownership, evidence and cleanup

All new paths belong to session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`.
Required authored source: `tools/live/nwn/mglive_{evt,chat,attack,damage,spell,player,vfx,evload}.nss`,
edits to inc/load/tick, `tools/live/test-events.{sh,py}`,
`tools/live/fixtures/events/`, `tools/live/wsl/log-tail.{sh,py}`,
and the include extractor/launcher/common/ownership helpers. Required documentation:
this file, `docs/index.md`, `docs/log.md`, `docs/deferred.md`.
`target/test-output/live-events/` is the single working set: `module/`,
`events.mod`, fixture `*.json`, compile cache `app-data/`, isolated `user/`,
and current proof `{chat,combat,log,vfx}{,-state,-log}.json`. Each final phase report
binds its compiled module, compiler binary, production script/plugin hashes,
and authored fixture/helper hashes. `compile.json` records 27 scripts,
zero failures. `vfx.json` preserves the independent native integer probe,
permanent/temporary/removal/cap checks and both object/location projectile
observations. The final chat/combat regressions include the VFX collectors.
One WSL runtime copy lives in `~/mglive-events/modules/events.mod`; it is needed
to run the Linux server against the Windows-authored module and is reused.
The exact MOD sizes are recorded in `owned-files.json`; remove the runtime copy after
review if no further private-server run is needed. No HAK or asset tree copied.
`preflight.json` retains the resolved source-blocker lineage. The requested
main-worktree mailbox is the only authorized write there.

`okf` is absent; manual index maintenance is explicitly accepted. No data from
other tasks is deleted. `owned-files.json` records exact retained task paths and
purposes; `cleanup.json` records exact removals and their measured logical bytes.
The working module and current proof stay for review. Shared Cargo output is
untouched. Phase closure removed **181,849 logical bytes** of own redundant
spell-only diagnostics, Python bytecode, completed rotation copies and
regenerable private-server logs/config/cache. Current authored sources, the
479,141-byte MOD (plus required Linux runtime copy), current proof reports,
include dependencies and minimal ownership records remain. All owned children
exited and UDP 5126/TCP 6392 are free. This is a sum of deleted file lengths;
no host free-space gain is attributed amid concurrent work and WSL virtual-disk
allocation. No file cleanup remains blocked. Active Codex history is preserved:
the agent's local Codex session file.
Separate history cleanup requires explicit retention authorization outside this
active session. No history cleanup has occurred.

Patrol reached/blocked/resumed events and their generation/point/lap/position fields are
defined in [Live operations: NPC patrols](live-ops-wire.md#npc-patrols). They use
the same per-boot sequence and bounded module buffer.

The `tile` event carries native before/after terrain readbacks and flags; its
complete schema is in [Live terrain tiles](live-ops-wire.md#live-terrain-tiles).


## Silent AI chat (2026-10-13, owner fix C)

Chat now includes `silent`, for example
`{"seq":4,"kind":"chat","time":"1791899000.1","id":"3","name":"Bartender","area":"events","target":null,"channel":14,"text":"NW_I_AM_DEAD","silent":true}`.
Native `TALKVOLUME_SILENT_TALK` arrives as channel **13**; silent shout is
**14**. Channel 14 also carries PC-to-DM traffic, so it is classified as silent
only for a valid NPC speaker excluding PC/DM/possessed speakers. Channel 13 is
always silent. Ordinary talk remains channel 1 with `silent:false`.

Reproduced and proven by the compiled `mget_silent` fixture: distinct silent-talk,
silent-shout and audible messages arrive on 13/14/1; killing the stock bartender
also emits `NW_I_AM_DEAD` with silent=true. `silent_chat-before.json` preserves
the missing-field baseline; `silent_chat.json` and `-state.json` record the fix.
The Live Log's default AI-shouts filter consumes this flag (and retains its
legacy NW_* fallback). Publication/storage/agent queries keep every event;
this is display filtering, not event suppression. Older events without the
field still parse. Native source reference: unified `Constants/Messages.hpp`;
the public Chat enum alone omits silent talk and calls 14 PlayerDm.

Current reproduction is MAIN `tools/live/test-events.py --phase silent_chat`
under WSL; see [the current private fixture route](live-ops-wire.md#shared-tree-owner-fixes-2026-10-13).

## Module-text receive boundary (owner BUG28)

New optional fields preserve bytes **before** native `JsonString` applies its
configured game charset. `mglive_text.nss::MgLiveTextField` records the ordinary
legacy string and, only for non-ASCII source bytes, an ASCII URL-encoded
companion: object/inventory `name_bytes` and `tag_bytes`, event `name_bytes` and
chat `text_bytes`. Literal percent and plus are escaped; `+` denotes space.
The log follower adds `line_bytes` from the original file bytes. It no longer
throws those bytes away. A state/log envelope declares `text_codepage` (default
`cp1250`, alternatives `cp1251`/`cp1252` through `MGLIVE_TEXT_CODEPAGE`). Inventory
results additionally declare `text_kind: "inventory"`; only explicitly marked result schemas
(inventory and skill result names) are decoded, never arbitrary `run_chunk.value` or captured object JSON.

```json
{"v":1,"boot":"example","seq":1,"text_codepage":"cp1250","events":[{"kind":"chat","seq":1,"text":"legacy fallback","text_bytes":"%B9%EA%B3%F1%F3%9C%BF%9F"}]}
{"v":1,"kind":"server_log","stream":"example","seq":1,"text_codepage":"cp1250","line":"legacy fallback","line_bytes":"%B9%EA%B3%F1%F3%9C%BF%9F"}
```

The **one receive conversion** is `crates/mg-live/src/wire_text.rs`, called by
`LiveStore::sync` and `LiveStore::server_log`. It decodes the companions to bytes,
accepts strict UTF-8 when valid, otherwise uses the declared legacy codepage,
and replaces the corresponding name/tag/text/line before storage. The API,
Live Log, chat, and object views all receive ordinary Unicode. Old reports
without companions are unchanged; already-mangled old strings cannot be
reliably repaired. Companions are removed after decoding. Malformed/oversized
companions preserve the original field. The decode limit is 196608 ASCII bytes.

UTF-8 preference is deliberate for modules with UTF-8 script literals alongside
legacy GFF/TLK. Ambiguous legacy bytes that also form valid UTF-8 follow that
policy; a single string mixing the two encodings is not recoverable reliably.
Outbound commands still use the separate existing ASCII-escaped JSON publisher;
configure the game's own `game.language.codepage` to represent their characters.

### Findings and reproduction

The source module named by `target/test-output/live-demo/inputs.json` was read
without changing it: `<demo-source>/demo.mod`,
SHA256 `4ff92afb1e650887e355d9f2a9cd8df5a00fc81563c9d07cc2ca51316ada8ddb`.
The owner's example `Naprawd\u0119 sama widzia\u0142am` occurs as **UTF-8** bytes
(`c499`, `c582`) in both NSS and compiled NCS, contrary to the initial all-cp1250
assumption. The running demo's language codepage setting was empty/default.
No demo process, settings, module or application was changed.

The private authored fixture `mget_polish.dlg` references custom TLK row16777216,
whose exact cp1250 bytes are `4e617072617764ea3a20b9eab3f1f39cbf9f`.
`mget_polish.nss` speaks that module line, sets the speaker name and logs it; a
second pass speaks a compiled UTF-8 literal, including literal `+ %C4%85`.
The fixture also contains a cp1250 placeable name/tag. The engine sanitizes its
tag to `Naprawd` on creation (also when native SetTag is used), before reporting;
we preserve the actual engine tag, not the rejected source characters.

Run from the repository, using PowerShell 7:

```powershell
./tools/live/check.ps1 -Phase 5 -Step module-text -ProofExample live_ops_proof -Jobs 1 -NoUiDebug -UiUnoptimized
wsl.exe -e python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase module_polish
./tools/live/check.ps1 -Phase 5 -Step module-text -Package mg-live -Jobs 1 -NoUiDebug -UiUnoptimized
```

Only the mg-live proof helper is built; no application rebuild. The harness uses
UDP5126/Redis6392 and owned `~/mglive-events`, starts the production LiveService
HTTPS/Redis bridge, sends scripts through its API, and checks returned names,
chat and log lines. `module_polish_repro` records the pre-fix native charset/log
failure; raw reports, source hashes and native process identities are retained
in `target/test-output/live-events/module_polish*.json`. Its dialogue line is
read from the actual module TLK headlessly; a player conversation/client render
is not claimed. Regression coverage includes both encodings, punctuation,
malformed companions, legacy reports, inventory and optional-array omission.

`skill_roll` events and their `request`, roll/modifier/bonus/total/passed/text or
no_roll fields follow the [interactive skill contract](live-ops-wire.md#nearby-player-skill-check).


## Creature portraits (third acceptance)

Every creature report adds `portrait`, the exact `GetPortraitResRef` base resref,
without a trailing size letter. No `po_` prefix is inserted: consumers resolve
`<base>s`, `<base>m`, etc. through the module/HAK/game resource stack. An empty
string means no native portrait; omitted legacy fields become Rust `None`.
This is independent of appearance row and preserves custom portrait names.

Native reproduction: `tools/live/test-events.py --phase portrait` uses the owned
UDP5126/Redis6392 fixture, reads a bartender's stock portrait, sets a custom base
with `SetPortraitResRef`, and asserts both native readback and the next report.
Evidence: `target/test-output/live-events/portrait.json` and `portrait-state.json`.
Client assets still must contain the corresponding portrait textures.


## Mixed module speech and native JSON loss (fourth acceptance B17)

The decoder is per string: strict UTF-8 bytes survive unchanged; otherwise the
explicit legacy codepage (normally cp1250) is used. ASCII has the same result in
both. This boundary already existed; re-decoding Unicode or guessing missing
letters cannot repair a string whose bytes the module has lost.

The exact `npc_sounds.nss` source (SHA256
`acde1b26ee225f2a64edab527ec026bcece8f7bc4be4d1207dbf3c010c44f1f6`)
contains UTF-8 `Naprawd\u0119 sama widzia\u0142am` and
`Teraz ju\u017c ka\u017cdy o tym wie`. `CreateSoundDefinition` puts the text
through `JsonString`, and `DispatchSound` reads it with `JsonGetString` before
`SpeakString`. On the private engine with an **empty** game codepage, the first
line goes from `c4 99` / `c5 82` to `c4 3f` / `c5 3f` at `JsonGetString`.
The chat hook already receives literal question marks. The second line's
`c5 bc` pairs survive. This reproduces the owner's asymmetric Live Log result;
it is not evidence that the incoming text is all cp1250.

Setting `[game.language] codepage = "cp1250"` preserves both original UTF-8 byte
sequences through that native JSON round trip, and also preserves actual cp1250
TLK text. `wsl/start-server.sh` now applies the same explicit codepage selected
by `MGLIVE_TEXT_CODEPAGE` before a **future** owned server start.
`configure-text.py` changes only that key in the owned scratch settings,
preserves other settings and refuses unsupported/unrecognized syntax.
The running demo was not stopped, started, rebuilt or reconfigured. Its owner
must use the corrected launcher at their next chosen restart; already damaged
history cannot be repaired. There is no literal-word substitution in Moonglow.

Reproduce with the existing private UDP5126/Redis6392 harness:

```powershell
./tools/live/check.ps1 -Phase 5 -Step mixed-text -ProofExample live_ops_proof -Jobs 1 -NoUiDebug -UiUnoptimized
wsl.exe -e python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase mixed_speech_repro
wsl.exe -e python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase mixed_speech_cp1250
wsl.exe -e python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase mixed_speech
./tools/live/check.ps1 -Phase 5 -Step mixed-text -Package mg-live -Filter wire_text -Jobs 1 -NoUiDebug -UiUnoptimized
```

`mget_mixed.nss` reproduces the module's nested JSON construction with those
exact two lines; the module itself remains read-only. The first two phases
record URL-encoded bytes before/after native JSON and chat. The last phase uses
the production LiveService HTTPS/Redis bridge, waits for every command and
checks the names, speech and server log returned by its API, for both UTF-8
literals (direct and module JSON route) and cp1250 TLK entries through that route.
Evidence is `target/test-output/live-events/mixed_speech*.json`; ownership,
source hashes and cleanup are in `target/test-output/live-accept4-game/files.json`.
The Rust regression test includes both exact strings in both encodings in one
frame and the already-damaged first line. A connected game client's font/render
and the owner's running demo after restart remain unverified.

`app_original` is the first reported creature/placeable/door appearance in a module run, independently of current `app`. See [restore semantics and proof](live-ops-wire.md#restore-original-appearance-fourth-acceptance-b8). Legacy absence becomes `None`, not row zero.

## Sixth acceptance: player-to-DM chat (D10)

Native NWNX channels are **14 = PLAYER_DM (players writing to DMs)** and **30 = DM_DM**; party is 6/22. Chat events preserve the speaker `id`/`name`, `channel`, area and event-time position. New `to_dm:true` explicitly identifies a visible 14/30 DM-audience line; there is no single recipient for that broadcast (`recipient:null`, `target:null`, `audience:"dm"`). A channel-14 NPC silent AI shout has `silent:true,to_dm:false` and remains hidden by default. Example: `{"kind":"chat","id":"7ffffffe","name":"Player","channel":14,"silent":false,"to_dm":true,"recipient":null,"target":null,"audience":"dm","text":"Can I examine the door?"}` (the ordinary seq/time/area fields also apply).

Reply with `pc_message {target,text,mode:"dm"}` under both player switches. It calls native `SendMessageToPC(target,"DM: "+text)`: private **server feedback**, not a fabricated DM-avatar tell or a channel-14 broadcast to DMs. Native feedback bypasses NWNX_Chat; the successful command result is the explicit submission echo. UI label: **DM private feedback (submitted)**. Do not claim recipient delivery. A connected DM avatar can use the game's actual tell channel20 itself; Moonglow does not borrow a user's DM object.

Proof: private `test-events.py --phase pc_dm_chat` observes native channel30 and NPC silent14 with the exact flags/speaker; rejects DM reply without players consent and rejects an NPC through the normal dispatcher. A fixture calls the same native reply helper on a stand-in, verifying exact prefixed text and recipient readback. This is a call/serialization proof only. The owner's client must send a line on DM chat (expected14, visible To DM), receive a targeted reply prefixed DM, and confirm other players do not receive it. Evidence: `target/test-output/live-events/pc_dm_chat.json`. Source: NWNX unified `3f504d0`, `Plugins/Chat/NWScript/nwnx_chat.nss` and `Plugins/Chat/Chat.cpp`.

## NPC equipment looks (sixth acceptance D8)

Creature `look` / `lookHash` now include NPCs. The old reporter restricted them to PCs, so NPC equipment always came from the editor/blueprint. NPC look contains only native appearance/body/colour/tail/wing fields and visible head/chest/right hand/left hand/cloak equipment. `Equip_ItemList:{"type":"list","value":[]}` explicitly means naked/unarmed; never replace an empty native list with blueprint equipment. The view must apply it to both matched module NPCs and extra runtime NPCs, invalidating the parsed/model cache when `lookHash` changes.

A five-slot object-ID signature plus appearance and boot identity caches the NPC snapshot; equip/unequip is observed on the next tick. A 30-tick refresh repairs other native appearance edits. The cached object enters ordinary reports; delta mode omits unchanged objects. Native ObjectToJson is not rerun for every unchanged NPC each second. Full and delta store merges accept the new hash and authoritative empty equipment; older reports without look remain valid.

Private `test-events.py --phase equipment_look` verifies the bandit's original armour/sword, removes both through Live Inventory operations (empty list), equips and unequips a new sword, then repeats through ordinary native action commands outside the Live inventory dispatcher. The final empty-look JSON is 2258 bytes; hashes return to the exact prior empty appearance. The bartender fixture refused weapon equip, so the proof uses an NPC that already natively equips that weapon. Evidence `target/test-output/live-events/equipment_look.json` contains all four looks and hashes. Astra1 consumer commit `bbb17a5` applies the look to matched module and extra runtime NPCs and clears stale caches on boot/source changes. The real-resource `live_accept6_npc_equipment_hash_replaces_the_reported_look` check verifies equipped, empty, re-equipped and empty hashes for both NPC kinds. Inspected `target/test-output/live-accept6/npc-look-equipped-after.png` and `npc-look-unequipped-after.png` show the weapon disappearing without restoring blueprint equipment; the complete explicit real-area suite passes 11 checks.

### Decoder-boundary repair (2026-10-15)

The original NPC compact filter mistakenly discarded `__data_type`. The earlier
native equipment proof checked contents/hashes, while the view tests used valid
hand-authored GFF: neither checked this decoder boundary. Preserve both
`__data_type` and `__struct_id` when present. The **current** native cleaner
(`c51cec4`) removes a fixed list of unused prose keys from the root and each
visible equipped item. It never walks unknown nested GFF fields. The view's
decoder defence (`93dc6bc`) removes remaining free text before GFF decoding;
resource references, numeric appearance fields and metadata remain. Both the
initial unrestricted walk and the later unknown-text fallback could exhaust the
VM budget on large PC snapshots, so neither belongs in the native cleaner.
Root deny-list deletions mutate their private snapshot in place.

`crates/mg-live/tests/look_reports.rs` parses actual private-engine packets with
`mg_gff::from_json` and serializes them in both default and cp1250 codepages.
Reproduction: set `MGLIVE_LOOK_PROOF_RUN=opt2-before` and
`MGLIVE_LOOK_EXPECT_INVALID=1`, then run the guarded `native_look_reports_parse`
test: 283 captured NPC looks reproduce missing `__data_type`. For the repaired
run set `MGLIVE_LOOK_PROOF_RUN=opt2-look-repair` and unset the negative flag:
222 NPC and one uncached PC-path look parse in both codepages. No connected PC
or rendered-client result is claimed.

The native `measure-look.nss` control retains Unicode directly in JSON (extracting
it through `JsonGetString` had already produced question marks). A UTF-8 prose
field fails default-codepage GFF encoding before pruning; native pruning fixes
root, equipment-list and nested-struct text, preserving the resource reference.
Run the private measurement harness with `MGLIVE_LOOK_TEXT_PROBE=1`,
`MGLIVE_MEASURE_STAGES=one_look_surrogate`, `MGLIVE_MEASURE_SECONDS=35` and the
existing owned5126/6392 fixture. Evidence is in
`target/test-output/live-measure/runs/opt2-look-repair/` (`look-parse.json`,
`look-text-probe.json`, packets, source hashes and process cleanup receipt).

The cold-load instruction overflow recorded with that repair is fixed by the
G1 report preflight: cold, changed and expired NPC looks are built by
`mglive_look` in separate VM invocations, four per 0.1-second continuation.
Only after all watched looks are ready does the tick consume a sequence and
publish its complete report. No wire fields change. The private cold-start
proof reached seq1..35 with all 37 NPC looks, including simultaneous expiration,
without raising the 1,048,576-instruction limit or logging a TMI. See
[the reproducible G1 follow-up](live-game-performance.md#g1-cold-start-instruction-budget).
No game renderer proof is implied by successful parsing.

## Immediate chat and agent watches (2026-10-15)

Transport update: the optional immediate `.event` publication now carries
`event_stream:true`. New consumers ignore this observational copy and use the
ordered stream for every game event, including chat. The native flush runs in a
fresh, immediately scheduled VM entry, independently of the next state tick.

Non-silent chat is now also published immediately on `<prefix>.event`:

```json
{"v":1,"boot":"process-epoch","text_codepage":"cp1250","events":[{"seq":42,"kind":"chat","time":"1792060000.123456","id":"7ffffffe","name":"Player","area":"area002","target":null,"recipient":null,"channel":1,"silent":false,"to_dm":false,"pc":true,"possessed":false,"audience":"vicinity","position":{"x":10,"y":20,"z":0},"text":"Hello","text_bytes":"Hello"}]}
```

The hook still queues that same event, including its sequence, in the next
state report. Older consumers therefore retain tick delivery. Current consumers
subscribe to `.event` alongside `.state` and `.log`, decode the same byte
companions, and deduplicate by boot plus individual `game_seq`. A bounded
4,096-sequence receipt window preserves earlier buffered combat events that a
fast chat event overtakes. Packets require one chat event, version 1, a positive
sequence, an established matching boot and at most 64 KiB. A pre-baseline event
is ignored on the fast path and remains available through its tick fallback.
Fast events do **not** refresh state age, advance report `seq`, or acknowledge
commands. Silent AI messages retain their buffered path. Redis pub/sub still
has no durable replay guarantee; `dropped` retains its buffer-overflow meaning.

Native chat adds `pc` and `possessed` booleans at speech time (`GetIsPC`,
`GetIsDMPossessed`). The store enriches non-silent player/possessed speech with
up to five nearest creatures within 20 metres, excluding the speaker, sorted
by 3D distance then ID. `nearby` entries contain `id`, `name`, `distance` in
metres and `pc`; `nearby_age_ms` is the age of the area report used, or null
if unavailable. Speaker coordinates come from the event; neighbours come from
the latest accepted report. This is **reported proximity, not proof of who
heard the line**. Empty/old reports remain explicit; the store does not invent
listeners. Older game hooks lacking speaker flags use the latest report as a
compatibility approximation.

```json
{"nearby":[{"id":"3d","name":"Street lamplighter","distance":2.25,"pc":false}],"nearby_age_ms":485}
```

`live_events` retains its existing filters and adds:

| Input | Meaning |
| --- | --- |
| `wait_ms` | Integer 0..30000, default 0. Wait until a matching event, timeout, reset, loss or shutdown. |
| `from` | Optional `"players"`: chat from PC/DM-possessed speakers, excluding silent AI. |
| `to_dm` | Optional boolean filtering chat's native DM-channel classification. |
| `boot` | Optional previous response boot, to detect a module restart. |

```json
{"since":120,"boot":"process-epoch","wait_ms":25000,"from":"players","to_dm":true,"kinds":["chat"],"limit":100}
```

The result is `{"events":[],"last":120,"boot":"process-epoch","reset":false,"lost":false}`
on an empty timeout. `last` remains the global Moonglow event cursor, not a game
sequence. `reset` means a supplied boot changed or `since` exceeds the current
cursor. `lost` means the requested area's history may have been evicted or more
matching rows exist than `limit`. Old newest-window semantics are preserved:
results contain the newest matching rows, ordered oldest first. On `lost` or
`reset`, stop and re-read state instead of silently advancing a watch. `lost`
is conservative for evicted rows; it is not an additional game-side drop count.

The wait uses a condition variable under the ingestion mutex, with notification
on insertion, boot change and shutdown. Unrelated events re-check filters;
there is no periodic poll. `read_chat` and command-owner visibility are applied
before selection. Describe advertises
`capabilities.eventStream.operation = "live_events"`; MCP discovers this name.
[Reproduction, timing and limitations](live-game-performance.md#chat-response-latency-2026-10-15).

### Stable look identity and sparse delta reuse (audit H1)

Creature `look` contains only appearance/model/part/colour data and visible
`Equip_ItemList` slots; both PC and NPC paths preserve GFF `__data_type` and
`__struct_id`, including EE wide part numbers and per-part armour colours.
Position, orientation, animation clocks, combat state and prose are not look
identity. In a delta, an object can carry `lookHash` without `look`: retain its
previous look **only if the same object has that exact non-null hash**. A new or
different hash never inherits an old look. Full/area-full/resync packets include
looks, including authoritative empty equipment. Example delta creature fields:
`{"id":"109","x":12.5,"y":9.0,"lookHash":-1315966712}` (other ordinary creature
fields omitted from this explanatory fragment). A periodic full report remains
a complete baseline. [Native proof and measured savings](live-game-performance.md#stable-appearance-hashes-audit-h1-2026-10-15).

## Agent attribution (H15)

Optional `command_id` links a native event to its exact originating command;
optional `by` names the authenticated agent client. Example:
`{"kind":"chat","id":"4","text":"Welcome","command_id":"speech-7","by":"Storyteller"}`.
Native `by_bytes` uses the existing raw-byte text boundary; Moonglow presents
UTF-8 `by`. Results and command events also expose `by`; older reports omit it.
Local activity is unmarked. Deferred speech, patrol and skill callbacks retain
explicit provenance; synchronous command effects and direct kill/death are
labelled. Autonomous AI and arbitrary later module callbacks without an explicit
origin are not inferred. [Trust boundary and proof](agent-protocol.md#command-authorship-audit-h15).


### Session stress backlog bound (S3, 2026-10-15)

The original300-message/44-drop proof above describes the initial256-slot
implementation. The current buffer holds2,048 events while the wire cap remains
256. Private `--phase chat` now proves a300-event burst drains without loss, and
2,400 events produced with no state subscriber retain652..2699 (2,048 rows),
discard exactly352 older rows, and drain in <=256-row reports. Enqueue and ring
wrap remain bounded; the4,096-sequence store receipt window covers the backlog.
This is the historical pub/sub assay, before durable delivery. With Streams, the
same updated `--phase chat` guard now receives all2,400 events without state
subscribers and reports zero drops. The2,048-slot local ring remains the bound
while Redis itself is unavailable. Original receipt: `target/test-output/live-stress/backlog/chat-proof.json`.
[Session measurements and limitations](live-game-performance.md#session-stress-preparation-and-scope-2026-10-15).


## Durable event delivery (2026-10-16)

The game keeps its existing 2,048-event local outage ring. Enqueue schedules
`mglive_evflush` in a fresh VM entry; each entry sends at most 256 events and
retries after one second on failure. It clears only the exact prefix accepted
by Redis. State publication/subscriber count never acknowledges event delivery.
The native callback therefore does not perform a network call per damage/chat
row, and a burst cannot spend the caller's VM budget on flushing.

`tools/live/event-stream.lua` is the canonical producer, embedded into
`mglive_stream.nss` by `python tools/live/generate-event-stream.py`. The installed
NWNX generated XADD wrapper has no MAXLEN argument, but its raw
`NWNXPushString` / `NWNXCall("NWNX_Redis","Deferred")` API supports the complete
EVAL command. Both the game and log follower call that script. It appends one
row per event with `XADD <prefix>.events MAXLEN ~ 10000 * ...`; approximate
trimming can retain an extra Redis macro-node. Native JSON text is passed as
an argument, never interpolated into executable Lua.

Each stream entry has these string fields:

| Field | Meaning |
| --- | --- |
| `n` | Monotonic ordinal of every accepted entry in this queue epoch, including log lines. |
| `epoch` | Producer identity plus Redis TIME when metadata was created; a database reset changes it even if the game boot does not. |
| `source` | Game boot, or log follower's stream ID. |
| `type` | `game` or `log`. |
| `codepage` | The existing text decoding hint. |
| `data` | Exact JSON event; game events retain `seq`, kind and all existing fields. Log JSON retains its own boot/stream/seq. |

Metadata lives at `<prefix>.events.meta`. A producer receipt at
`<prefix>.events.producer:game:<boot>` (or `:log:<stream>`) makes retries after a
lost Redis reply idempotent. The receipt and ordinal update happen inside the
same Lua execution as XADD. Receipts expire after 14 days without new activity;
this is a bounded retry horizon, not indefinite archival. Game sequences still
use signed NWScript integers and must not exceed 2,147,483,647 in one boot.

Moonglow uses a separate connection and a stable consumer group per data
folder/server namespace (host, port, prefix). A filesystem lock prevents two
writers from sharing its inbox. `XREADGROUP ... COUNT 128 BLOCK 250` reads new
entries; `XAUTOCLAIM ... 0 ... COUNT 128` drains pending entries on reconnect,
including a process crash. The fixed consumer name does not accumulate a new
Redis consumer record on each reconnect. Groups are independent: one Moonglow
instance never steals another instance's events.

The local `<data>/live/event-inbox/<namespace hash>/` contains the consumer
identity and a bounded append-only inbox. A page is validated, flushed to that
inbox, applied to the store, then XACKed. A crash before XACK reclaims the pending
page; committed ordinals and `(boot,game_seq)` dedupe prevent another application.
A torn final line was not acknowledged and is retried; a malformed complete
record fails closed. Startup restores the retained inbox before reading new
entries. Compaction retains at most 10,000 rows / approximately 32 MiB of payload
and overhead; compaction runs before the next append after 20,000 written rows /
64 MiB. A maintenance failure leaves the next receipt unchanged. These are
bounded recent events, not an unlimited history. Store/Live Log per-area history
and API query limits are unchanged. This transport covers native event callback rows and followed log lines;
state-derived events and command acknowledgements/results retain their report
semantics. The optional JSONL logger drains all retained
rows instead of the API's newest 500-row page and flushes after bridge shutdown.

Stream ingestion does not need a state baseline and never refreshes state age,
advances report sequence or changes the current server boot. This permits an old
boot's queued events to drain after a newer boot's state is received. Game event
`data.boot` and `data.stream_id` preserve that lineage. `queue_epoch`/`queue_n`
are the disk writer receipts; restored events are not appended to JSONL twice.
The writer preserves each queued event's original boot folder, even before a
state baseline arrives. It repairs only an incomplete final JSONL record.
Historical events are not enriched with a newer boot's object/proximity data. Legacy state/event/log
producers remain accepted, with their original pub/sub delivery limits; reports
marked `event_stream:true` cannot overtake or duplicate the durable path.

A gap in queue ordinals records **"N events lost: queue overflow"** in Live Log
and increments `status.redis.events.lost`. This includes pending entries trimmed
before reclaim. `backlog` is the latest producer ordinal minus the committed
ordinal; `connected`, `last_id`, `applied`, `persistent`, `warning` and `error`
make recovery/storage failures visible. Backlog is last-observed while offline.
A missing/recreated Redis queue epoch is explicitly reported as a reset with
unknown possible loss, rather than inventing a count. Malformed entries or local
storage failures remain unacknowledged with a status error.

Redis **6.2+ and AOF persistence** are required for restart survival. Configure
`appendonly yes`; the strict private restart proof uses `appendfsync always`.
`everysec` has its documented power-loss window ([Redis persistence](https://redis.io/docs/latest/operate/oss_and_stack/management/persistence/)). Moonglow reports a warning if
Redis AOF is disabled, and does not change an existing Redis service's configuration.
The owned WSL startup helper provisions new instances with AOF/always in their
marked scratch directory; existing external services need operator configuration.
No queue can recover data deleted by FLUSHDB, lost Redis storage, or a game process
crash before its local ring reaches Redis. Longer Redis outages can still overflow
the native 2,048-event ring, whose cumulative `dropped` count remains separate.
The log follower still skips existing bytes on first attachment and cannot
recover unobserved rotations; durability begins when the follower queues a line.

Primary command references: [XADD](https://redis.io/docs/latest/commands/xadd/),
[XREADGROUP](https://redis.io/docs/latest/commands/xreadgroup/) and
[XAUTOCLAIM](https://redis.io/docs/latest/commands/xautoclaim/).
[Implementation and proof record](live-mode-redis.md#durable-event-queue-2026-10-16).

## Watched-area status (2026-10-16)

State envelopes now include `"watched_areas":["demo","demo_area"]`, the
server's `MGLIVE_WATCHED` areas, including on deltas and empty-area reports.
`live_status` preserves this optional field (`null` for older scripts).
Player-occupied areas and explicit launcher extras can report without being in
this list. See [area subscription control](live-ops-wire.md#area-subscriptions-2026-10-16).

## Action metadata for draft and completion events

Both a pending assistant proposal and a store command result now enter the event
log through `EventLog::command_action`. This retains the command's operation,
area and target object when the result itself has no object. Empty native `obj`
strings are normalized to null only in the event payload, allowing the existing
metadata helper to fall back to the command target; API results are unchanged.
The shared action
log can describe the action using its existing projection; it does not receive
an entire command payload. Uncorrelated native results still use the plain result
path because the store has no command to describe.

Regression: `cargo +1.98.0 test -p mg-live draft_and_completion_events_describe_the_action -j 2`.
It checks operation, target and area for both pending and rejected draft events,
covering the assistant entry point and the common `push_result` completion path.
The test failed with a missing operation before these two calls were connected.
Use `NWN_ROOT=/nonexistent` and `CARGO_INCREMENTAL=0`.

Follow-up verification on the shared tree based on `05d5be9`: the regression
passed after both call sites and empty-object normalization were connected.
The exact workspace gate was run with Rust 1.98.0 and the environment above:

```sh
cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2
cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings
cargo +1.98.0 fmt --check
```

Clippy and format passed. Tests reached **466 passed, one failed, four ignored**
(in `mg-live`: 239 passed, one failed, four ignored). The only failure was
`api::tests::real_tls_status_command_and_stop`: Windows curl/Schannel returned
`SEC_E_NO_CREDENTIALS` in the sandbox. Later workspace binaries were not executed;
repeat the full test command outside that restriction. No TLS test was weakened.
The three reported redundant mutable batch references were already absent on
HEAD; `store_ops_tests.rs` needed no change. This Rust gate does not replace the
[compiled budget probe's native WSL run](live-translations.md#compiled-instruction-budget-measurement).
