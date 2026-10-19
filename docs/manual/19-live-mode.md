---
type: Manual Page
title: Live mode
description: Live mode - watching and directing a running NWNX server from Moonglow through a Live gateway; connecting (Options › Live, Discord sign-in, certificate fingerprint trust, the resource download window), the Live menu, the Live view and its windows, AI agents, drafts, shared notes and presence chips.
tags: [manual, live, gateway, nwnx, redis, dm, agents]
generated: { by: claude, at: 2026-10-19T09:00:00Z }
---

# Live mode

Live mode shows a running server's area in the area viewer, as the game
reports it, and sends the DM's commands to it. **Moonglow talks to a game
server only through a Live gateway**: a service run by the server's
administrator, next to the game. The gateway reads the game through Redis
(NWNX_Redis on the server), decides who may do what, serves the server's
resources and shared notes, and lets several DMs and their AIs work on the same
game. Moonglow itself opens no local Live service, port or token, and never
talks to Redis or to the game directly. The module file is never changed by
Live: what you do goes to the running server only.

Areas subscribe automatically when their view's **Live** is on, when Chat/Focus
shows them, or while an object there is followed. Closing the last view releases
the watch; areas containing players still report. Watches return after a server
restart or reconnect. There is no startup area list to maintain (the server's optional
`MGLIVE_AREAS` list is only for extra areas). Rebuild an older Live module to
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
Use a private Redis instance and give the gateway the same host, port and prefix. Copy
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

In short: add your server's gateway under **Options › Live** and sign in,
connect with **Live › Connect to Gateway**, then open an area and turn **Live** on.
The next sections give the details; the Live menu, the Live view and its windows
follow.

## Connecting to a gateway

Ask the server's administrator for the gateway's address (an `https://` URL).

1. Open **Tools › Options › Live** (or **Live › Live Options…**). Under **Live
   servers** choose **Add server**, give it a **Name** and the **Gateway URL**.
   **Remove server** also deletes its saved session from this computer.
2. **Sign in with Discord.** Moonglow opens your browser; after you approve,
   the gateway issues a session that Moonglow keeps in the operating system's
   credential store (never in the settings file). **Sign out** erases it.
3. If the gateway uses a certificate your computer does not already trust,
   Moonglow shows its **SHA-256 fingerprint** before anything is sent: compare
   it with the one the administrator gives you, then **Trust certificate and
   sign in** (or **Reject certificate**). The fingerprint is saved with that
   server and checked on every later connection; editing the URL forgets it.
4. Tick **Enable live mode** and **Apply**, or use **Live › Connect to
   Gateway**. **Status** shows connecting, connected or reconnecting, and
   **Live › Disconnect from Gateway** (or unticking and Apply) ends the
   connection. A fresh profile has no server selected; with none selected,
   connecting only tells you to choose one.
5. Open the area the server runs and choose **Live › Show Live in This Area**
   (Ctrl+Shift+L), or the **Live** button in the area view's toolbar.

Settings written by older versions that ran a local Live service still load:
the local server entry, its token, the local API and Redis fields are ignored
and dropped the next time settings are saved, and no server is selected until
you choose one.

For a development gateway the environment variables `MOONGLOW_GATEWAY_URL` and
`MOONGLOW_GATEWAY_TOKEN` select the server and supply a session for that process
only; neither is saved, and Options shows that an override is active.

Areas, objects and commands behave the same whichever server you are on; what
you may do is decided by the gateway from your account. Commands the server
does not allow (module scripts, NWScript chunks, actions on players) are
greyed out, and results that could not be confirmed are shown as **Unknown
results**: the command may have run, so inspect the game before sending it again.

### Downloading the server's resources

The first time you connect, and when the server's haks or module change, the
gateway announces a new set of resources. Moonglow shows **Download Live
resources** (a progress window with **Download details**, **Cancel download**
and, after a failure, **Resume download**), checks every file against the
gateway's manifest and only then uses it, so the area view can draw the same
models, tiles and portraits the players see. Large downloads wait for you; small
ones run in the background. Nothing is written to the game installation.

**HAK / module cache** in Options › Live shows how much is stored under
Moonglow's data folder, the **Limit (MiB)** (2048 by default) and **Clear cache**
(disconnect first).

## Options › Live

* **Live servers**: the list on the left, and for the selected server its
  **Name**, **Gateway URL**, **Status**, **Account** (**Sign in with Discord** /
  **Sign out**), the **HAK / module cache** and **My AI** (**Manage AI**, which
  opens the AI agents window while connected).
* **Enable live mode**: Apply connects or disconnects.
* **Focus window**: enabled by default. Keeps per-server watch lists and unread
  alerts; switching it off pauses Focus until you enable it again.
* **Factions**: the colours used for factions in the Live views.
* **NWNX folder**: the folder with `Binaries/NWScript.zip`, used by **Live ›
  Prepare Module for Live?** (see above). It is a build setting only; Moonglow
  never starts or stops a server.

## The Live menu

**Connect to Gateway / Disconnect from Gateway**, **Show Live in This Area**,
**Live Log**, **Live Chat**, **Focus…**, **GM card…**, **Atmosphere…**, **Legend**,
**Scenes…**, **Visual Effects…**, **NPC Library…**, **Skill Check…**,
**Prepare Module for Live?**, **Live Options…** and **Live Mode Help** (this
chapter). While connected the menu also shows the gateway's state, **AI agents**
and **Reconnect gateway**.

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

## AI agents

An AI is a separate identity on the gateway that an assistant (a language model
client) uses to read the game and propose or perform actions with the
permissions the gateway gives AIs. Open **Live › AI agents** (or **My AI** in
the area view's toolbar, or **Manage AI** in Options) while connected.

* **Create AI** with a name. Moonglow shows its **token once**: **Copy AI token**
  and keep it; **I saved the token** closes the box. Anyone with the token acts
  as that AI.
* Each AI has an **owner** (you; administrators see and manage all AIs), a
  **name**, an optional **avatar URL** (HTTPS image, PNG or JPEG; the gateway
  fetches and serves it, so the picture only appears after **Save AI**), and
  **Allow LLM for this AI**, which is **off for a new AI**. Turn it off to
  stop an assistant at once without deleting it; **Revoke AI** removes it.
* Connect an assistant with **moonglow-mcp** (see
  [Command-line tools](11-command-line.md#agents-and-mcp)): it takes the
  gateway's address and the AI's token (`MOONGLOW_AI_TOKEN`), and offers the
  same operations as MCP tools or as shell commands. It has no local mode: it
  reaches the game only through the gateway.

Anything an AI does is subject to the gateway's rules for AIs, the server's
policy and the AI's rate limit; the gateway keeps the audit trail.

Presence: while someone has an object in **Focus**, a chip such as **Guide · DM**
or **Scribe · AI** (with their avatar when they have one) appears beside it in the
Focus window and over the object's marker on the map. The chip says who is
watching; it does not lock the object.

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

Watch names and member identities are saved automatically per gateway server
under Moonglow's data folder, `live/focus/`. Missing objects stay grey. The same
boot/id is matched first, then a unique matching tag; ambiguous tags remain
missing. Another server has its own list. Each entry keeps at most 500 captured rows in memory; transcripts
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

On a standard gateway an AI's speech (NPC speech, replies to players and
messages to players) does not go straight to the game: it waits as a **draft**
for a DM. Other actions follow the AI's permissions.

Open **Live Chat** or **Focus** for the conversation. The **LLM drafts** panel
shows the proposing AI, intended speaker/recipient and text. **Send draft**
approves it; **Edit draft** changes the text before sending; **Reject draft**
tells the AI that the proposal was declined. **Send all drafts** approves only
the drafts shown by that window's area/member filters. Point at the panel and
press **Enter** to send the top draft when no text field or button has keyboard
focus. Enter inside a text editor keeps its normal editing behaviour.

You see the drafts of your own AIs; administrators see everyone's. Approval
checks the AI's current permissions and the target again. A dead, missing or
moved target can require a fresh proposal. A server restart expires pending
drafts. The AI follows the original command ID with `live_wait` or command
events to learn whether it was sent, edited, rejected, expired or failed. A game
acknowledgement confirms submission, not that a connected player read the message.


## NPC cards, the GM card and shared notes

Open **Live › GM card...** to write private campaign guidance: tone, house rules,
what never to reveal, how to answer on the DM channel, narration style and
language. The assistant reads this card first.

Select an NPC and click **NPC card** in **Selection** or **Focus**. Describe who
they are, personality, what they know, what they hide, how they speak and their
language. Cards are keyed by creature tag, falling back to blueprint resref;
creatures sharing that key share a card.

These are **shared books**: they are stored on the gateway and shared with the
server's authorized DMs, not saved on your computer. Saving checks the revision
you started from; if someone else saved first Moonglow shows your version and
the newer one side by side, and you choose **Keep mine** or **Take newer**
(**Reload saved version** discards your edit). Cards and notes do not modify the
module or the running server.

Conversation history for an NPC is not available in the gateway views yet.


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

Selection has a **DM note** editor for each selected object; Focus has the same editor under **DM note** for an individual member. Press **Save note**; an empty saved note removes it. Notes are shared books on the gateway (visible to the server's authorized DMs) and never change the module or game. An **N** badge on the map shows the note on hover. Live Panel search includes saved notes. Unique object tags survive restarts; players use tag plus character name. Objects without a unique tag use the current boot and ID, so their notes are not guessed onto a different object after restart.

### Macro hotkeys

In **Scenes**, use **Assign key...** beside a saved macro. Four slots default to **Ctrl+6, Ctrl+7, Ctrl+8, Ctrl+9** while the pointer is over a Live map; outside it, numbered tab navigation stays available. Change each **Play Live Macro Slot** binding in **Options > Keyboard > Live area**. F1, F10, F11 and F12 retain their existing actions. Assignments are saved per module; rename updates them and delete removes them. A key plays the saved macro without replacing an unsaved scene draft. Stop recording/playback first. Holding a key does not repeatedly run the macro.

### Atmosphere on the running server

Open **Live > Atmosphere...** and choose a reported area using the same area tabs as Chat. Expand Weather, Fog, Lighting, Skybox, Music or Ambient sound, then Apply. Weather can explicitly affect **All outdoor areas**; indoor weather may be refused. Music/ambient have separate Apply, Play and Stop buttons. Resource lists use the game load order; Advanced exposes a numeric row for custom content. Values are proposals, not an initial server snapshot. After a command, **Server settings from last reply** shows the native readback; a refusal is displayed. Changes affect Live only. Hear audio and verify weather/lighting with a connected client.

### Spawn world objects from the palette

With Live on, select a waypoint, sound, encounter, store or trigger blueprint in the palette. A ghost/cursor marks the destination; click the map to place the first four kinds. Q/E turns the usual placement ghost; Shift keeps spawning. For a trigger, **drag a rectangle**, 0.1 to 30 m on each side, then release: the selected UTT and its scripts are retained, with the new geometry. Esc/right-click cancels without a command. The module stays unchanged. Reported objects can be selected and removed with the usual confirmed menu/Delete command. **Encounter removal is unavailable on this server** and encounters are excluded from mixed removal. Only sounds created through Live are reported as Live sound markers.
