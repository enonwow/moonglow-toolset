---
type: Manual Page
title: Live mode
description: Live mode - watching and directing a running NWNX server from Moonglow; Options › Live (API, token, named clients, Redis, WSL, commands, players, logs), the Live menu, the Live view and its windows, agents and MCP.
tags: [manual, live, nwnx, redis, dm, agents]
generated: { by: claude, at: 2026-10-13T11:00:00Z }
---

# Live mode

Live mode shows a running server's area in the area viewer, as the game
reports it, and sends the DM's commands to it. Moonglow talks to the game
through Redis (NWNX_Redis on the server); agents, scripts and tools talk to
Moonglow through its local HTTPS API. The module file is never changed by
Live: what you do goes to the running server only.


Areas subscribe automatically when their view's **Live** is on, when Chat/Focus
shows them, or while an object there is followed. Closing the last view releases
the watch; areas containing players still report. Watches return after a server
restart or reconnect. There is no startup area list to maintain (the optional
`-Areas` launcher list is only for extra areas). Rebuild an older Live module to
get this support; Moonglow never reports every module area by default.

## Preparing your own module

Choose **Live > Prepare Module for Live?**. Set **Options > Live > NWNX folder**
to the folder containing `Binaries/NWScript.zip`, select the saved module (or a
module folder/nasher project), choose a new output name, optionally enable
**Capture dialogue**, and click **Build Live copy**. Unsaved editor changes are
not included. The source and the open editor workspace are never modified.
Compilation runs in the background; the report lists additions, replacements,
the original load handler, instrumented dialogue nodes, errors and warnings.
**Open folder** shows the resulting archive and server files.

The same builder is available without the GUI:

```sh
mg live-build campaign.mod --nwnx /home/dm/nwnx --capture-dialogue --json
```

Use the usual `mg --root GAME --user-dir NWN_USER` options to locate game
resources, HAKs and a custom TLK for compilation/verification. The output defaults
to `campaign-live.mod` beside the input; `--out FILE.mod` chooses another path.
Existing output or sidecar files are refused: choose a new name. All Live sources
are embedded in the executable; a released installation needs no source checkout,
PowerShell, Python instrumenter or external NWScript compiler to build the copy.

The generated `campaign-live.live.env` explains each plugin, Redis and Live setting.
Review it before running `bash campaign-live.start.sh` on Linux/WSL. It starts
only a private NWNX server, with a new random admin password, on the configured
free UDP port (default 5126); it does not start Redis or stop other processes.
Use a private Redis instance and set the same host/port/prefix in Moonglow. Copy
any required original HAKs/TLK into the dedicated `campaign-live-server/hak` and
`tlk` folders after the first run creates them; clients still need those assets.
The launcher refuses an existing unowned runtime directory. Commands/chunks and
player force operations remain subject to their usual explicit switches.

On Windows, browse to `\\wsl.localhost\Ubuntu\home\dm\nwnx` or a Windows
NWNX distribution folder. Select the real directory containing the zip: Windows
cannot follow an absolute Linux symlink exposed through WSL's UNC provider.
The sidecar's Linux server and NWNX paths can be edited for the actual host.
`MGLIVE_TEXT_CODEPAGE` and the dedicated server's `settings.tml` must agree.

Capture dialogue changes only action-script references; original byte strings,
conditions, replies and action parameters are preserved. The generated action
first reports the node and then calls its original action. The module-load wrapper
calls the original handler before starting Live. Preparing a Live copy again
updates its wrapper rather than nesting it; its embedded ownership manifest keeps
this reliable. Omitting Capture dialogue when updating a previously instrumented
copy preserves existing instrumentation; start from the original module to remove it.
Unrecognized pre-existing `mglive_start` code is refused rather than guessed.
Script name clashes are reported and replaced only in the new copy. Compilation
or verification errors produce a report but no output module.

Implementation and reproducible checks: `docs/live-build.md` in the source repository.

## Live mode

1. Start a server with the Live scripts (`tools/live/Start-LiveDemo.ps1`
   builds and starts a private demo; see [Build, verify and test](09-build-and-test.md)).
2. **Live › Start Live** (or Options › Live › **Enable live mode** and Apply).
3. Open the area the server runs and **Live › Show Live in This Area**
   (Ctrl+Shift+L).

The demo launcher's optional `-Areas` adds areas to report regardless of open views;
the default adds none. `-AllowChunks` lets agents run NWScript chunks, `-AllowPlayerOps` lets the
server accept actions on players.

## Options › Live

* **Enable live mode**: Apply starts or stops Live. `MOONGLOW_LIVE_*`
  environment variables override these options and turn Live on when
  Moonglow starts; **Live › Stop Live** still stops it.
* **API for clients**: the address (loopback only) and **Port** of the HTTPS
  API (0 chooses a free port); **Token** with **Generate** and **Copy token**;
  **Copy curl command** puts a status request for a terminal on the
  clipboard. The token is kept in plain text in Moonglow's settings.
* **Focus window**: enabled by default. Keeps per-module watch lists and unread
  alerts; switching it off pauses Focus until you enable it again.
* **Named clients**: a token per tool or agent with its groups (read,
  read_chat, npc, spawn, scripts, chunks, players, variables) and a limit of
  calls per minute; **Open audit folder** shows the log of what each client
  did.
* **Game link (Redis)**: **Host**, **Port** and **Channel prefix**. The
  server publishes to `<prefix>.state` and listens on `<prefix>.cmd`.
* **Keep WSL running** (Windows): keeps WSL, where the server and Redis run,
  from going idle while Live runs.
* **Commands**: **Allow module scripts** (run_script), **Allow NWScript
  chunks** (run_chunk; the server needs `MGLIVE_ALLOW_CHUNK=1` too) and
  **Allow actions on players** (heal, raise, teleport, gold, XP, items…; the
  server needs `MGLIVE_ALLOW_PLAYER_OPS=1` too). Off, players and their
  companions are protected.
* **Logs**: **Keep the live events on disk** (`live/logs` under Moonglow's
  data folder, 14 days, 500 MB) and **Keep chat too** (what players write;
  off unless the server's players agree).
* **Status**: the API address, the Redis link and how old the last report is.

## The Live menu

**Start Live / Stop Live**, **Allow LLM**, **Show Live in This Area**, **Live Log**,
**Live Chat**, **Focus…**, **Legend**, **Scenes…**, **Visual Effects…**, **NPC Library…**, **Skill Check…**,
**Live Options…** and **Live Mode Help** (this chapter).

## The Live view

What the area viewer shows and does with Live on (labels, colours, the Live
Panel, combat, effects, selection and groups, patrols, the DM's controls on
the right-click menu, placeables' properties and live tile painting) is
described with the viewer in [Areas](04-areas.md#live-mode).

## The Live Log

Chat, combat, players, the server log and the game's answers to commands,
newest at the bottom. Its pages, a search, the area (every area the server
reports or an event names), **Pause**, **Clear** (hides what is shown;
agents still read it), **AI shouts** (the creatures' silent shouts such as
`NW_I_WAS_ATTACKED`, hidden unless switched on) and **Colours** (chat by who
speaks: player, DM, NPC, server). A click on a line selects its object.

## Player skill checks

Skill Check sends a request to the selected player or players in a chosen radius.
Players see a window titled with the skill, the DC if shown, their bonus
breakdown and a large **Roll** button. A nonzero DM bonus appears as text;
players cannot change it. Only clicking rolls. The result shows d20 and total
in green for success or red for failure, followed by any DM outcome text.
**OK** closes it. An unanswered request times out as **no roll**.

## Agents and MCP

### Allow LLM

LLM access is **off by default**. Start Live, then enable **LLM** beside the
area view's Live toggle, **Live › Allow LLM**, or **Options › Live › Allow LLM
connections (MCP / agent API)**. These are one global setting for all areas.
It applies immediately, is saved with Moonglow's settings, and does not restart
Live or the game. Options' Apply/Cancel do not undo this immediate consent
choice. The controls are disabled while the Live service is stopped.

Add **moonglow-mcp** as a stdio server in your assistant's MCP configuration;
use the installed executable's full path if it is not on PATH. The adapter
discovers the running Moonglow session after access is enabled. See
[MCP setup and client examples](11-command-line.md#agents-and-mcp), including
`--sessions` for a demo using a separate data folder. Assistants that can run
commands can also use `moonglow-mcp tools` and `moonglow-mcp call`.

The status dot is **green** when an authenticated client has made a request
within the last 60 seconds; **red** means no recent client or access is off.
Hover to see the client name, last operation and seconds since its last request.
This indicates recent API activity, not whether the model is thinking. The MCP
adapter sends a heartbeat every 20 seconds while connected; direct API clients
can use authenticated `GET /v1/ping`. A heartbeat retains the last real operation
in the hover text. No requests means the dot turns red at the 60-second deadline.

Turn **Allow LLM** off to revoke agent access immediately and hide the session
from discovery. Local DM controls keep working. A token is still required while
access is on, and named-client groups, rate limits and player/script policy still
apply. Configure a named client in Options to identify and limit each assistant;
clients sharing the default token appear as **default**.
Disabling access blocks new requests; commands already accepted are not canceled.

Agents discover the operations at `/v1/describe` and call them at
`/v1/call/<operation>`: reading (`live_status`, `live_area_state`,
`live_events`, `live_find`), commanding NPCs and placeables, the DM's
actions and scripts. `moonglow-mcp` offers the same operations to any
MCP client; see [Command-line tools](11-command-line.md).

In the source repository, `docs/live-campaign.md` gives checked JSON inputs
for narration, player Roll prompts, encounters, movement, rewards and effects,
with human approval rules. Scene save/play/stop is
currently a Live Scenes window workflow; those operations are not yet exposed
through MCP or the shell adapter.


## VFX transforms and colour

In Live VFX, expand **Scale / Offset / Rotation** before Apply. Scale defaults
to 1; offset and rotation are native XYZ effect parameters and default to zero.
They apply to the game effect, while the thumbnail shows its base form.
**Clear VFX (ours)** still removes only Moonglow's tagged effects.

VFX colour controls and the shader-parameter filter were removed after owner
acceptance. Target material overrides do not recolour native attached VFX;
effects cannot be recoloured. Ordinary Apply/Clear VFX, duration and transforms remain available.

## Session factions

Before the session, open **Tools > Faction Editor > Basic** and use **Add session
factions** to reserve 1..32 spare factions. This is an undoable module edit;
save and load that module on the server before playing. A running module cannot
create new faction IDs through the available scripting interface.

During play, use **Factions > Live session**. Choose an acting faction, name it
for this session, and assign the selected NPC to it. Labels exist only in
Moonglow and reset on a server restart or module change. They do not rename the
game's factions. The open module must match the running server.

Read or set how the acting faction regards the selected creature, from 0 to 100.
For global NPC factions this changes the faction; the reverse relationship is
separate. A custom faction needs a non-plot NPC representative in a loaded area.
Players require both permission switches. Results show the native before/after
values and appear in Live Log. Runtime changes are not saved back to repute.fac.


## Focus: watched conversations and events

Select a player, NPC, other live object or group, then right-click **Focus**.
**Live › Focus…** opens the window again. The left column keeps your watched
entries with portraits, HP, area, combat state and unread counts. A group keeps
its members even when the map selection changes. **Manage › Rename** changes
its initial “Group 1” name; **Remove from focus** removes just this watch.
**Show on map** opens the first reported member's area and follows that member.
Use Members to open one member's conversation before following them.

Select an entry to read **Conversation**, newest at the bottom. By default it
shows only what that object or the group members say. **Nearby / incoming speech**
and **Server messages** are optional filters, both off initially; hiding them
keeps their captured history and unread alerts. Use **Events**
for combat, damage, death, entering/leaving areas and acknowledged DM inventory
commands. A yellow **Focus (N)** tab and entry counters show unread speech or
combat alerts even while another tab is open. Viewing rows reads only their alerts; hidden areas or event pages stay unread. AI shouts are hidden; speaker colours are the Live Chat colours.

The reply bar speaks **as the NPC**, sends **to the player**, or sends to every
reported player in the selected group. It uses the usual command queue and scene
recorder. Player replies require the player-operation switches. A successful DM
reply is labelled **command acknowledged**: it is not proof of client delivery.

Nearby talk/whisper uses an **approximate 20 m** filter: the speaker's event-time
position and the watched object's latest report in the same area. Area shouts
use the watcher's reported area. These matches are fixed when received, so moving
later does not change old rows. They do not prove who heard a line. Nearby speech
from before a watch was added cannot be reconstructed; old events without a
position show only for their speaker or resolved recipient. Tells go only to
entries containing their source or resolved recipient, not everyone nearby.

The running server must supply the events. Dialogue lines appear only where the
server's opt-in dialogue collector is installed; offered replies are not chosen
lines. Unresolved tell recipients, ordinary item looting and uncaptured module
speech cannot be inferred. **Capture limits** explains these gaps in the window.

Watch names and member identities are saved automatically per module under
Moonglow's data folder, `live/focus/<module>-<path-hash>/watch.json`. Missing
objects stay grey. The same boot/id is matched first, then a unique matching tag;
ambiguous tags remain missing. Moving the module to a different path creates a
separate list. Each entry keeps at most 500 captured rows in memory; transcripts
and unread counts are not saved across application restarts. Focus is a watch
list, not a permanent chat archive.

Unset Live portraits use the appearance default, then the game’s default portrait when that appearance has none. A default picture is not the creature’s chosen likeness.

Live tools use **Area** tabs. Chat and Log offer **All**; Chat labels each All
line with its area. Right-click a Chat area tab → **Open in new window** for a
chat pinned to that area. Replies and “To all players” respect its area. Panel,
Selection, Skill Check, Inventory, VFX and NPC Library use the chosen map's area;
choose its Area tab or click its map. Multiple visible maps keep separate
selections. Painting, properties and teleport aiming stay bound to their map.

In Focus, expand **Members** to see each member's portrait and click a name or portrait to read that member's conversation
and events or reply to them. **Back to group** returns to the group. The shared
**Area** tabs split a group's history across maps; **All** labels each line.
Group replies go to its players in the chosen area, while a member view can
speak as its selected NPC. Unread events in other areas/pages stay unread.

**Live → Legend** opens marker visibility controls beside the map. Its **Area**
tabs choose which open view it controls; each view remembers its own choices.
The toolbar's **Legend** button opens the controls for that map. The map itself
has no legend overlay. Legend opens with the other Live tools.

**Character → Faction** uses the running server's faction list. Factions added
only in the editor are disabled: save, rebuild and restart the module before
assigning them in Live. An older server without a catalogue needs an update.

**Follow** in Live Panel and **Show on map** in Focus keep tracking an object
across areas. Moonglow opens its destination map and moves the shared Live tools
to that area. Detached Chat windows stay pinned. A brief reporting gap does not
cancel follow; closing the followed map, switching Live off or a server restart does.

Live Chat **To DM** shows DM-channel reports separately from ordinary talk and
party chat. Click **Reply to name**, then Send DM reply. The reply is private
DM-prefixed server feedback, not an avatar speaking; player actions must be on.
The same All/area tabs limit either page.

**Delete** or **DANGER > Remove from game (no body)** confirms the number of
eligible selected objects once, then removes each. Players and DMs are excluded.
Cancelling leaves the entire selection untouched.

NPC equipment follows the reported look hash just like player equipment;
unequipping removes the displayed item instead of restoring blueprint gear.

In a Live map, **Ctrl+C** copies selected NPCs and placeables from the running
game. **Ctrl+V** shows their model ghosts; click to paste, use Q/E to turn the
group, or Esc/right click to cancel. Relative spacing and facing are preserved.
**Ctrl+X** confirms the eligible count once, captures all, then removes the
originals only if every capture succeeds. Players/DMs are excluded. Paste and
Cut need module scripts enabled; the Live clipboard lasts for this module and
server session only. No module objects are edited.

A scene is a **macro: a recorded list of DM commands you can replay**. Scenes
shows the module's **Saved macros** at the top: click one to load it or use its
**Play** button. Right-click a saved macro to rename or delete it. **Save** sits
beside the name; Save As, Import and Export stay in Scene. Save/discard changes
and stop playback before switching macros. Step targets show names; hover for
IDs, or use Edit step JSON for an unreported target. Coordinates display to 0.1 m.


## Agent drafts: the LLM proposes, the DM approves

In **Options > Live > Named clients**, set **Agent speech** to **Drafts** and
Apply. That client's NPC speech, replies to players and player messages now wait
for review. **Auto** sends permitted speech immediately. Existing clients keep
Auto until you change it; this setting does not grant extra permissions.

Open **Live Chat** or **Focus** for the conversation. The **LLM drafts** panel
shows the proposing client, intended speaker/recipient and text. **Send draft**
approves it; **Edit draft** changes the text before sending; **Reject draft**
tells the agent that the proposal was declined. **Send all drafts** approves only
the drafts shown by that window's area/member filters. Point at the panel and
press **Enter** to send the top draft when no text field or button has keyboard
focus. Enter inside a text editor keeps its normal editing behaviour.

Approval checks the current client permissions and target again. A dead, missing
or moved target can require a fresh proposal. A server restart expires pending
drafts; closing Moonglow does not save them. The agent follows the original
command ID with `live_wait` or command events to learn whether it was sent,
edited, rejected, expired or failed. A game acknowledgement confirms submission,
not that a connected player read the message.


## NPC cards, the GM card and saved conversation memory

Open **Live > GM card...** to write private campaign guidance: tone, house rules,
what never to reveal, how to answer on the DM channel, narration style and
language. Click **Save card** after editing. The assistant reads this card first.

Select an NPC and click **NPC card** in **Selection** or **Focus**. Describe who
they are, personality, what they know, what they hide, how they speak and their
language. Cards are keyed by creature tag, falling back to blueprint resref;
creatures sharing that key share a card. The card editor keeps this stable key,
so a recycled game object id cannot redirect a saved card. Unsaved fields stay
in the open app's editor; **Reload card** discards them. Save before closing
Moonglow. Cards do not modify the module or running server.

Focus a single NPC and select **Memory** to read saved conversations grouped
by player, with timestamps and a short summary. The assistant can read the same
history and update the summary. Exact reported recipients take precedence;
nearby talk uses the event position and a recent report within 20 m and is
labelled as an estimate. It does not prove who heard a line. Unresolved private
messages, AI shouts and server feedback are omitted. Player identity uses the
reported tag and character name because no vault identifier is reported;
simultaneous duplicates are refused. History starts when this module is open
in Moonglow and is bounded to 200 turns per pair, 10,000 turns per server.

Cards live under Moonglow's data folder at
`live/assistant/<module-path-key>/cards.json`. Memory is additionally separated
by server endpoint/channel prefix, under `server-<key>/memory.json`. Keep these
files to preserve guidance and history across restarts. A parse failure leaves
the original file untouched and reports the error; it does not load another
module's guidance. The module path is part of the key, so moving a module does
not automatically migrate its cards.


### Drag objects in Live

Drag a live creature, placeable or dropped item to move it. Dragging a selected
member moves the eligible selection together, keeping its relative positions.
Numbered ghosts and guide lines show the landing positions; release sends one
teleport/move command per object. **Escape** or **right click** cancels. Player
moves require the player switches; DM/possessed and dead creatures are excluded.
Every landing must remain inside the area. Placeable rotation handles and Q/E
keep their existing behaviour. **Ctrl+drag** still moves the camera; **Shift+drag**
still adds to the selection box. These commands change the running game, not
the module file.

### Private DM notes

Selection has a **DM note** editor for each selected object; Focus has the same editor under **DM note** for an individual member. Press **Save note**; an empty saved note removes it. Notes stay in Moonglow, scoped to the open module and server, and never change the module or game. An **N** badge on the map shows the note on hover. Live Panel search includes saved notes. Unique object tags survive restarts; players use tag plus character name. Objects without a unique tag use the current boot and ID, so their notes are not guessed onto a different object after restart.

### Macro hotkeys

In **Scenes**, use **Assign key...** beside a saved macro. Four slots default to **Ctrl+6, Ctrl+7, Ctrl+8, Ctrl+9** while the pointer is over a Live map; outside it, numbered tab navigation stays available. Change each **Play Live Macro Slot** binding in **Options > Keyboard > Live area**. F1, F10, F11 and F12 retain their existing actions. Assignments are saved per module; rename updates them and delete removes them. A key plays the saved macro without replacing an unsaved scene draft. Stop recording/playback first. Holding a key does not repeatedly run the macro.

### Atmosphere on the running server

Open **Live > Atmosphere...** and choose a reported area using the same area tabs as Chat. Expand Weather, Fog, Lighting, Skybox, Music or Ambient sound, then Apply. Weather can explicitly affect **All outdoor areas**; indoor weather may be refused. Music/ambient have separate Apply, Play and Stop buttons. Resource lists use the game load order; Advanced exposes a numeric row for custom content. Values are proposals, not an initial server snapshot. After a command, **Server settings from last reply** shows the native readback; a refusal is displayed. Changes affect Live only. Hear audio and verify weather/lighting with a connected client.

### A conversation-only LLM client

**Options > Live > Named clients > Create conversation LLM client** creates a separate token with read, read_chat and speech access, 60 commands/minute and **Drafts** by default. Apply activates the credentials; copy the named token separately and enable Allow LLM. The default discovery token has broader access: do not substitute it for the named token when you intend conversation-only access. Speech permits NPC speech, player replies/messages and NPC-memory summaries; it does not grant combat, inventory or scripts. Both player switches still apply. Chat, Focus and Live Log mark events with the authenticated client attribution when supplied by the server. An absent mark does not prove an asynchronous game effect had no agent cause.

### Spawn world objects from the palette

With Live on, select a waypoint, sound, encounter, store or trigger blueprint in the palette. A ghost/cursor marks the destination; click the map to place the first four kinds. Q/E turns the usual placement ghost; Shift keeps spawning. For a trigger, **drag a rectangle**, 0.1 to 30 m on each side, then release: the selected UTT and its scripts are retained, with the new geometry. Esc/right-click cancels without a command. The module stays unchanged. Reported objects can be selected and removed with the usual confirmed menu/Delete command. **Encounter removal is unavailable on this server** and encounters are excluded from mixed removal. Only sounds created through Live are reported as Live sound markers.
