---
type: Manual Page
title: Areas
description: Areas - the viewer's toolbar, live game reports and commands, minimaps and camera, selecting, arranging and placing objects, painting placeables, painting terrain, tiles, Area Properties, area sounds and making tilesets.
tags: [manual, areas, terrain, tiles, live]
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-17T21:13:04Z }
---

# Areas

Double-click an area in the module tree (or right-click it › **View
Area**) to open it in the **area viewer**, which draws its tiles and
objects with the area's lighting, fog and sky, by day or night. Areas open
looking straight down at their middle, north up, as in Aurora.

## The toolbar

- **Creatures, Doors, … Waypoints, Start**: show or hide each kind of
  object and the start location marker; **All** and **None**.
- **The start location** (where a player's character enters the module)
  is marked as in Aurora: a blue ring on the ground with a red arrow the
  way the character faces. To set it, right-click the ground where it
  should be and choose **Set Start Location Here**: it faces the way the
  view looks. Click the marker to select it (a yellow box, as round an
  object; the game's invisible object, which has nothing to draw, has
  the box its model holds undrawn, 0.6 m across, as Aurora outlines it). Drag it by its ring or its arrow to move it; selected, the
  arrow's tip has a round handle that turns it (both follow the snapping grid and
  angle; Escape lets go; an object standing on the marker is taken first). **Go to Start Location** brings the view to it; with the start in another area, it opens that area there. **Start** on the main toolbar (View › Go to Start Location) does the same with no area open: move the start location to where you are working, and it is a bookmark.
- **Fade Geometry**: draws the tiles without the parts that fade in the
  game to show a character behind them (roofs, upper walls), to see and
  place what is under them (Aurora's Environment › Fade Geometry):
  **Never**, **Object Mode Only** (while you work on objects: not with
  Select Tiles on or a terrain brush chosen) or **Always**. The choice
  is kept for the areas opened after.
- **Night**, **Fog**, **Grid**: show the area at night, with its fog, and
  the tile grid. The grid is faint from above; as the view comes down to
  look along the ground from the area's edge it turns Aurora's red, to
  count tiles by and line areas up, and under raised ground it is drawn
  at the area's foot as well. The fog is drawn as the game's camera (at most 20 m from
  the player) would see it from where the view looks, so a view from
  farther off isn't lost in it.
- **▶ Animations**: creatures and other placed objects play their
  animations, and those with particles show them (a campfire's flames,
  magic sparks, a portal: placeables that are only an effect show
  nothing otherwise), as do the tiles: a chimney smokes with its
  animation loop on in Tile Properties, and a torch on a wall burns where
  its source light has a color (Tile Properties). Switched off, they hold
  still (tiles' water and flames go on), in every area view; it stays as
  you leave it.
- **💡 Lighting**: the area's lighting. Switched off, everything is evenly
  lit, whatever the area's colors and lights: for working in a dark area.
  Areas opened later open as you left it.
- **🔊 Sounds**, **Ambient**, **Music**: play the placed sound objects, the
  area's ambient sound and its music, heard from where the camera looks.
- **◎ Sound Ranges**: two circles around each placed sound that is heard
  from where it stands: at full volume inside the inner one (its minimum
  distance), not at all outside the outer one (its maximum). The
  selection's are brighter; a sound heard everywhere in the area has none.
  A sound that plays from a random place has a rectangle too: how far
  east and north of itself it may play. Areas opened later open as you
  left it.
- **Walkmesh…**: a click shows the ground's walkmesh over the area
  (Aurora's Render AABB Nodes): walkable faces green, the others red; a
  click again hides what is shown. Hold the button, or right-click it,
  for the two to choose from: **Ground Walkmesh** and **Object
  Walkmeshes**, where placeables (their `.pwk`, orange) and doors (their
  `.dwk`, blue, for the state they're placed in: closed or open) keep
  creatures out. The selection's are brighter.
- **Select Tiles**: select tiles rather than objects (Aurora's Select
  Terrain). Without the toolbar: a double click on the ground selects its
  tile (Select Tiles comes on), and a click on an object while tiles are
  selected selects it (Select Tiles goes off).
- **✏ Paint** (B): paint the placeables chosen in the palette onto the
  ground (see [Painting placeables](#painting-placeables)).
- **Area Properties**, **Reorient Camera** (north up again), **Go to Start
  Location**.
- **To Scratch**: copy the area as it is now (its `.are`, `.git` and
  `.gic`) into the scratch folder (chosen the first time; Tools › Options
  › Folders changes it).

## Live mode

**Live**, beside **Grid**, shows what a running NWNX server reports about this
area. In **Tools › Options › Live**, select **Enable live mode** and **Apply**.
Connect the server to the same Redis host, port and channel prefix as described
in [Build, verify and test](09-build-and-test.md). Live mode starts off; its
settings are saved, and environment variables only override them. Open the same
module and HAKs that the server runs, then open its area and turn Live on.

Turning Live on applies the **Live layout**: Live Chat, Focus, Legend, Live Panel, Selection,
Skill Check, VFX, NPC Library, Scenes and Inventory open as tabs to the right of
the area, beside Palettes. Live Chat is shown first; Live Log opens only from the Live menu. Close tabs with their close
buttons; opening a Live tool again docks it on the right. Turning Live off and
on reapplies the layout. Options > Live wraps narrow controls and explains token
drafts: Generate/Remove change the draft; Apply activates those changes.

The **Live** menu does the same from anywhere: **Start Live** / **Stop Live**
(the option above), **Show Live in This Area** (Ctrl+Shift+L, the area shown
last), **Live Options…** (the Options window at its Live page: status, token,
API and Redis) and **Live Mode Help** (this section).

Right-click one object or a selection for the same organised menu. **Walk Here...**,
**Attack...**, **Say...** and (for exactly one eligible creature) **Inventory...**
are direct shortcuts. Submenus group the other actions: **Move**, **Combat**,
**Character**, **Look and effects**, **Talk**, **Player**, and **Scripts**.
**Attack...** also appears under Combat for a group; one map target orders each
eligible attacker. A disabled Attack explains that no living creature is allowed.
The header counts selected/allowed objects; the footnote explains eligibility.

The separated **DANGER** submenu confirms **Kill**, **Remove from game (no body)**
and **Limbo** before sending. Kill causes a death event and leaves the corpse/loot
according to the creature's settings. Remove from game deletes the object without
a body or loot and cannot be undone. Hover either action for this distinction.
**Inventory...** opens Live Inventory; it never sends an inventory dump to the Log.
A group offers it only when exactly one selected creature is eligible; opening it
focuses that creature while retaining the selection. Inventory explains unavailable
actions on hover. Give Item explains double-clicking, search results and the selected recipient.

Right-click a reported object for the DM controls. NPCs offer **Kill**, **Heal**,
**Force Rest**, **Limbo**, **Invulnerable**, **Immortal**, **AI**, **Faction**,
**Freeze**, **Stop Attack**, **Stop Fighting Here (10 m)**, **Inventory** and **Locals**. **Heal** accepts
an amount or **Full Heal**; **Freeze** accepts seconds (up to 300), with presets
as shortcuts. **Apply VFX...**, **Appearance...**, **Skill Check...** and
**Inventory...** open tools for the clicked target. Skill Check targets the clicked
player; Players in radius arms a circle to position with a map click.
Doors and placeables also
offer **Disable Trap** and **Trap**. On empty ground, **Set Time** accepts hour
and minute fields as well as presets. Fields send nothing until their action
button is clicked.

With **Allow actions on players** in Options > Live and the server's player
switch enabled, player commands use the same submenus: Walk Here, Run Here, Attack, Stop Attack, Freeze, Kill,
Heal, Raise,
Teleport, gold, XP, level and alignment. **Give Gold** and **Give XP** accept
numeric amounts. Teleport uses the same targeting flow as other live objects.
**Message...** chooses chat, floating text or NUI; **Journal...** takes a quest
tag and state; **Fade** fades to or from black. **Boot...** names the player and
requires **Confirm Boot**. Closing or cancelling sends nothing. DM and possessed
creatures remain protected. Every action uses the Live command queue and the
server's permission checks. Possess and Appear/Disappear are the DM client's own.

Live Chat explains an empty conversation and why Send is unavailable; recipient
and volume choices wrap to fit the dock.

The Log explains empty or paused views; hover a truncated event to read it in full.

**Live › Live Log** shows what happens in the running game, newest at the
bottom: chat, attacks, damage, deaths, spells, players joining and leaving,
creatures entering and leaving the area, the server log (script errors in
red) and the game's answers to commands. Its pages (All, Chat, Combat,
Players, Server, Commands), a search and **This area only** narrow it;
**Pause** holds it still, **Copy** copies the lines shown, and a click on a
line selects its object in the area's view. Times are UTC. Agents read the
same events with `live_events` and find objects with `live_find`.

While Live runs, Moonglow also keeps the events on disk, a folder per server
run under its data folder's `live/logs` (a file each for combat, players,
server, commands and other events). Runs older than 14 days are removed, and
the oldest go once all of them pass 500 MB. Chat is kept only with **Keep
chat too** (Options › Live › Logs): it is what players write.

A scene is a **macro: a recorded list of DM commands you can replay**. The **Scenes** window manages these macros. Follow its three
steps: **1 Record** (the same button changes to **Recording... / Stop**),
**2 Review steps** (readable action/target summary with inline action, named target
and delay fields (IDs are on hover; coordinates round to 0.1 m)), then **3 Replay** (**Play scene** / **Stop scene**).
The empty list shows a guard greeting-and-walking example. Expand **Edit step**
for JSON input, the operation schema and wait conditions. Invalid inputs identify
their step and disable Save/Play; disabled buttons explain why on hover.

For a first example, select one living NPC and click **Try it: make an example
scene with the selected NPC**. It creates a draft: the NPC says Hello, then walks
5 m after a one-second delay. The rows show the NPC's name, destination and waits.
Press **Play scene** to execute it; creating the example sends nothing. Save or
discard an existing draft before replacing it. The game still checks navigation.

**Saved macros** lists module macros with a Play button beside each. Click a name
to load it; right-click to rename or delete. **Save** is beside the scene name.
The compact **Scene** menu also holds **New scene**, **Load scene** (choose a saved
name), **Save scene**, **Save As...**, **Import...** and **Export...**.
Save writes beneath the application data folder's `live/scenes/<module basename>/`;
scene names accept ASCII letters, digits, spaces, underscore, hyphen and dot.
**Save As...** chooses a new JSON filename through a file dialog and keeps that
named copy in this module's scene list. **Export...** writes JSON without
changing the current scene. **Import...** validates a JSON file and opens it as
an unsaved draft; save or discard existing edits first. Invalid input or a
cancelled dialog leaves the current scene intact. File actions never play a
scene. Review recorded targets when using a different server session.

Choose **Record**, then issue commands from the Live view. Only
successfully queued commands are appended, with the time since the preceding
one and fresh IDs on replay. Stop recording before editing or saving. **Play
scene** runs the steps in order and shows completion or the first error. Delays
start after the preceding step and its wait finish. **Stop scene** prevents
later steps; commands already sent remain in the game. Closing/changing the
module, disabling Live or a new server boot also stops recording/playback.
Saved object IDs must be reviewed after a server restart. Unsaved drafts remain
in memory while switching modules; save before exiting the application.

The module supplies the tiles and static scenery. Creatures appear at reported
positions, updated about once a second; placeables, doors and ground items
refresh about every five seconds. Missing non-static objects disappear after
a full report. A creature's blueprint supplies its model; a player's reported
appearance supplies their equipped model. A reported placeable or door
appearance overrides its blueprint or module instance (including changes since
spawn). Older reports without that field fall back to the blueprint. An unknown
model keeps its marker. Names and HP bars accompany them. A door or placeable
can have an **open** or **locked** label.

**Colours** chooses **Faction** (default), **Hostility** to the selected player, or
**Kind**. Faction names come from the open module: PC blue, Hostile red, Commoner
orange, Merchant gold and Defender green, with a fixed palette for custom
factions. **Options › Live › Factions** edits those colours; Apply saves them
for this module path and faction index, Cancel discards the draft, and Reset
restores the defaults. The separate **Legend** window has one NPC factions colour/filter list per area view.

In Hostility mode select a player: enemies are red, friends green and neutral
creatures orange. Grey means an older report without relationship data or no
selected player. These are the game's reported relationships, including personal
reputation. Kind uses blue for players, violet for DMs/possession, green for
objects made live and orange for others. Non-creatures keep their Kind colours.

**Labels** in the Live toolbar chooses **All**, **Near and selected** (the
default), **Hover only**, or **None**. Nearby means within 15 metres of the
selected object, or the camera target when nothing is selected. Crowded names
give way to selected objects, players, combatants and objects made live, in
that order; their markers and HP bars remain. Names fade between 40 and 80
metres from the camera, except the selected name.

Creatures in combat have a red ring. A dashed red arrow points from an attacker
to its reported target; hidden, missing or dead targets have no line. Dead
objects have a grey cross and keep their name under the chosen label mode.
A plain left drag on empty ground selects a box of live objects. **Shift** adds
to the selection. **Ctrl+click** toggles one marker; **Ctrl+drag** pans the
camera, including when starting on a marker. Ctrl never draws a selection box. The
selected count opens **Selection**, listing creature portraits, names, HP and
kind. Click a portrait or name to focus the camera while keeping the group.
Selection, Live Chat and Focus use the reported portrait, then the reported appearance
default from `appearance.2da`, then the installed default portrait. The default
picture is not a claim about that creature's chosen likeness. A missing custom
asset still has a placeholder/hint when no fallback image can be loaded.
Portraits use the reported native resref plus the small (`s`) size, falling back
to medium (`m`); unavailable custom assets show a placeholder with a hint. **Ctrl+1..5** stores a group,
**1..5** recalls it and a quick second tap centres the camera. The keys are
configurable under **Options > Keyboard > Live Area**; in Live they take
precedence over camera digits and Ctrl+digit tab switching. A server restart
clears saved groups. The Live Panel also highlights selected rows. Its **Filters
and selection help** explains map visibility; empty results point to the filters.
An empty Selection window explains the map selection gestures.

Right-click keeps a group selected and offers walking in a two-metre formation
or to one point, Attack, Kill, Heal (amount/full), Freeze (seconds)/Unfreeze,
Faction, AI, Invulnerable, Immortal, Force Rest, Limbo, Say, VFX, Appearance,
Animation, Stop Attack, Teleport and Patrol. Inventory opens only for exactly one eligible creature. Each
eligible object gets its own command through the same queue. **Animation** opens the same
chooser for an NPC, a permitted player or a group: ANIMATION_* names from the
loaded game's script constants, a custom animation name, Loop, speed and seconds.
**Play animation** submits the validated command to each eligible creature.
Custom names have 1–16 resource-name characters. Speed is 0.1–10, duration
0.1–300 seconds. Looping constants use the duration; one-shot constants play
once, or repeat with Loop. Editing fields never sends; an invalid name disables
Play with an explanation. Players require both player switches.
**Take Gold** and **Take XP** accept a positive amount to remove, with an explicit
Take button; the Give actions remain separate. **Remove Level** removes one
actual level and is disabled for a reported level-one creature. These actions
also work on eligible members of a selection, once each; players require both
player switches. Gold floors at zero. XP reduction can remove earned levels.
VFX and appearance tools keep the selection and act on eligible objects of the
appropriate kind. Live VFX explains unavailable Apply/Clear actions on hover;
**Aim on the ground** enables Apply without a selection and waits for a map click.
Long effect names have full hover text; no-match results explain how to reset the search.

**Stop Attack** targets only the selected living
creatures, including players when both player switches are enabled; it never
uses a radius. The single-NPC menu separately keeps **Stop Fighting Here (10 m)**
for nearby NPCs. Player Stop Attack is disabled with the player-switch hint when
player actions are off. It clears the player's orders without changing factions
or reputation. DMs and possessed creatures are excluded. Permitted players
also receive Walk/Run, Attack, Kill, Heal, Freeze, rest, AI, invulnerability,
immortality, VFX, appearance and inventory commands. Faction, Patrol and Limbo
require NPCs; Say speaks through NPCs or placeables.

Choose **Patrol...** on an NPC or group, then click up to 16 points. **Loop** repeats
the route and **Run** changes the pace. Double-click the final point to send one
order per NPC. Enter or **Finish on next click** arms a final map click; neither
sends by itself. Right-click, Escape or **Cancel Patrol**
cancels. The title-bar close button also cancels the draft. **Stop Patrol** clears the route; new orders also replace it. Selected
NPCs show known routes and reported arrival progress. **Observed patrol points**
means only reached points were reported, not a complete route; stale paths expire. Opening the view or restarting the server starts a fresh
observation window; unlabelled old event history is not replayed.

Right-click a live placeable or door and choose **Visual Transform...** to open
the numeric editor; **Live Properties** in the toolbar opens the same window.
It includes the name,
useability and numeric visual transform (scale, rotation in degrees and translation
in metres). **Apply Transform** sends the values; **Reset Transform** restores
identity; **Reload Report** discards unsent edits without a server command, including
for static objects. Fields show their units. A selected placeable turns with
Q/E, its orange rotation ring or Alt+right drag; drag its marker to move it.
Releasing sends one command. Escape cancels a gesture. Doors support movement
and visual transforms; physical door rotation remains unavailable in the game.
A reported static placeable explains why VFX and visual transforms are disabled.
**Make Dynamic** sends the static flag change; game clients must re-enter the
area before the engine updates that flag on their side.

A live object's own model and emitters (for example Green Lightshaft) stay
visible in every Effects mode. The Effects selector controls attached spell
models and event cues; Animations still controls emitter simulation. Full
is the default for new area views.

The **Legend** toolbar button opens the separate **Live > Legend** window.
Its shared Area tabs choose which view it controls; no key covers the map. **NPC factions** is the creature-only faction colour list;
there is no duplicate Colour key section. Its checkboxes hide markers for players, DMs,
NPCs, NPC factions, placeables, doors, items and dead objects. Combat rings,
attack lines and labels have separate switches. Each view remembers these
choices; they affect our overlays and leave the reported models visible.
A selected combatant also has a white selection ring.

In **Colours: Faction**, only creatures use faction colours. Placeables, doors
and ground items use muted kind colours matching their legend entries, even
when the game reports them as Commoner. NPC faction filters never hide them.

The view reuses report-derived models and lists between updates, while movement
and gesture previews continue smoothly. Live models are posed on animation
redraws; static scenes retain their GPU picture.

**Effects** in the Live toolbar chooses **Off**, **Markers**
or **Full** (the default) for this area view. Markers show coloured rings and effect names;
glows and unknown effects keep a marker even when no model is available.
Full adds attached effect models and particles to both NPCs and player looks,
using the creature's appearance size. When a visible creature has reported
effects in Markers mode, a hint explains that models are hidden and **Show
Full effects** switches them on. It keeps at most 64 attached models and 20,000 live particle quads or
trail segments, nearest objects first; rings remain beyond the budget.
Animations Off pauses particles. These display controls do not change the
running game's effects.

Location-targeted spells show a short circle when the loaded spell table gives
a spherical radius; its label gives the localized spell name and metres.
Unknown/non-circular areas keep a named point flash without an inferred radius.
In Full, spell events also draw the loaded spell table's cast/projectile models
and particles. The stock Fireball impact is resolved from its shipped script;
unknown scripted impacts keep their marker. **Apply VFX → Aim on the ground**
draws the chosen effect at the clicked point after a successful command reply.
Ground effects from other clients require a position/VFX event from the server;
without one they cannot be reconstructed. These ground cues belong to this view
and are cleared by Effects Off or a server restart.
Spell and projectile events draw brief arcs with a moving marker and an arrival
flash; damage events flash at the victim with the reported amount. These are
event cues, not a simulation of spell success. Object targets use their reported
position; location casts use the event position. Off hides them. Opening Live or
a server restart starts at the current log cursor, so old spells are not replayed.

Chat appears above a reported speaker for four seconds after receipt. A new
message replaces that speaker's bubble. Bubbles follow the speaker and share
the view's filters; crowded bubbles give priority to the selected speaker and
avoid markers, other bubbles and the legend. They remain visible with Effects
Off. The bubble shows up to 240 characters; Live Log keeps the complete message.

Every place action uses a targeting cursor and a ghost before it sends:
**Walk/Run**, **Attack**, **Teleport** (including its destination-area map),
**Patrol**, palette and **NPC Library Spawn**, **Skill Check in radius**,
**Stop Fighting in Radius**, **Apply VFX** with **Aim on the ground**, and
**Jump All Players**. A map click executes; Escape or right-click cancels.
Live NPC Library explains Capture > select saved entry > Spawn and aim; hover
the module label for its storage folder and disabled actions for the reason.

Library spawn shows the saved NPC model. Radius fields update the following
circle while aiming. Skill Check states the next step above Send and explains
disabled controls on hover; **How player rolls work** holds the detailed rules. A selected-player skill check and effects applied directly
to selected objects need no ground point. Jump All Players needs both player
switches; Stop Fighting's radius still affects NPCs only.

**Live Panel** opens the reported-object table for the current area. Search
names, tags or resrefs; the kind chips combine, while state chips narrow the
results together. **Hostile** means hostile to the selected player, or any
player when none is selected. **Wounded** means below half maximum HP.
The same filters hide live markers and their extra models in the view.
When these filters or the toolbar kind toggles hide reported objects, the map
shows **N of M hidden by filters** below its Live report age. **Show all** clears
the panel chips and search and enables creatures, placeables, doors and items
in the toolbar. It does not change the separate legend marker preferences.
**Hide module markers** hides authoring flags and arrows while Live is on.
Click a name to select it and centre the camera; double-click to follow it.
Following moves to another area only if its view is already open; otherwise
it stops and names the area to open. **Stop following**, switching Live off,
closing the area view or restarting the game ends following.

The panel initially sorts by distance from the camera target (**Near camera**).
Click **Name**, **HP**, or **Distance** to sort that column; click again to reverse
the order. Distance is from the selected object, or the camera target when none
is selected. Names in the viewport also give way to nearby object markers.

The corner shows the report's age. It turns red after three seconds without
fresh data, or says **Live: no report for this area**. The last report stays
visible during an outage; it is not a guarantee that the game is still connected.

**Teleport…** is available for NPCs, placeables, doors and ground items, and in
**Player** when player actions are allowed. It also works on a mixed selection:
click a destination in the view and the green landing ghosts keep the selected
objects' relative offsets. The usual turn keys (Q/E, Shift for 90°) adjust facing;
Escape or right-click cancels without sending. Dead, DM/possessed and disabled
player targets are excluded. Placeables can still be dragged to move them.
**Teleport to Area…** lists every area in the open module, together with reported
server areas. Choose an area and click its tile minimap to send. The map shows a
10 m grid and coordinates, blue current positions when in that area, and green
landing ghosts following the cursor. Z and the facing offset are editable. The
default is the area's centre; unreported areas use the module's dimensions and
map. Runtime tile edits may differ from that map. Every landing point must fit inside
the area. The server checks whether each object can move (including doors and
static placeables); the Live Log shows any refusal. These commands never edit
the module. Cross-area scenery/items and rotated doors may be recreated with
new IDs; selection follows the reported replacement (in the destination view
when already open). Creature landing positions can be adjusted by navigation.

Live positions and turns move smoothly over the measured report interval, so
the picture trails the newest report by at most that interval. Teleports over
ten metres and death snap into place. Following and picking use the displayed
position too. **Trail** shows the selected object's last five seconds of travel.
Moving creatures use an available walk, run or combat-walk animation; **Animations**
still controls model animation. Static views reuse their rendered
image until something changes, while movement and animated scenery keep updating.

- Choose or drag a creature, placeable or item blueprint from the palette and
  then click the map to spawn it. Dragging a blueprint arms the same preview;
  dropping it does not send. Shift-click keeps spawning.
- Click near a live marker to select it; Delete sends **Remove from game (no body)**.
- Group menus offer **Run in Formation…** and **Run to One Point…** beside
  their Walk commands. Shift-click also runs when aiming a group Walk command.
- Right-click an NPC for **Walk Here…**, **Run Here…**, **Say…**, **Attack…** or **Remove from game (no body)**.
  Walk Here and Run Here take the next ground click. Shift-click is also a run
  shortcut while aiming Walk Here; Attack takes the next
  creature marker. Escape cancels either choice.
- A door or placeable offers **Open**, **Close**, **Lock**, **Unlock** under Look and effects > Object state, and
  **Remove from game (no body)**. DMs and possessed creatures remain protected; player orders
  require both player-action switches.
- **Run Script…** runs a compiled module script on the selected object. Enter
  its resref (1–16 ASCII letters, digits or underscores, without `.ncs`).
- **Run NWScript…** sends 1–8000 nonblank characters of source to the game.
  Wrapping into `void main()` is on by default. This menu requires **Allow
  NWScript chunks** in Options and the server must have `MGLIVE_ALLOW_CHUNK=1`.
  Chunks are off by default because they can run arbitrary code.

The script dialogs show their target; a ground-menu invocation runs on the
area. Script commands may intentionally target a player or DM. The module
script menu is controlled by **Allow module scripts** (on by default).
Queued commands and their results, including compiler errors, appear in the
Log even when the Live view has since been switched off. Sending a script
does not change the module's source files.

With Live on, open **Palettes > Tiles**, pick a tile thumbnail and click or drag
on the area. **Q/E** rotates the tile by a quarter turn (the normal Turn key
bindings, configurable in Options). Each cell keeps its current height. The
translucent preview shows the replacement before sending it. The yellow
**Live: painting the running server** banner identifies the active brush.
**Stop painting**, right-click or Escape leaves tile mode and clears the palette
selection and unsent confirmation; subsequent map clicks cannot paint. The first change in each server session asks
for confirmation with the exact area, cell and tile values.

Live Tile Properties opens below the painting banner; hover Restore for why it
is unavailable when no previous server tile was observed.

**Select Tiles** opens **Live Tile Properties** for inspecting a cell. Numeric
**Tile ID**, **Orientation** and **Tile height**, plus **Apply Live Tile** and
painting with explicit height, are under **Advanced**. Close the window with its
title-bar close button to leave tile painting and discard an unsent change.

Changes appear after the server reports them. **Restore First Observed Tile**
sends the first reported previous values for that cell. The module is unchanged;
leaving Live displays its original terrain again. Only changes observed by this
open view are known: there is no full live terrain snapshot, and opening the
view or restarting the server discards ambiguous event history. Terrain
transitions, tile groups and adjacent doors/heights are not adjusted; existing
terrain brushes direct you to single-tile replacement in Live.

These viewport actions change the running game and are not module Undo steps.
Normal GIT placement, dragging, scaling, pasting and terrain edits are bypassed
while Live is on. An hourglass beside a target means its command is pending;
commands expire after 30 seconds without acknowledgement. Turn Live off to
resume ordinary area editing.

The view uses idle creature poses and labels for door/placeable state; it does not
animate their open/closed poses. Particles belonging to hidden module objects
can remain visible. Overlapping objects at the same position and resref may
not be distinguishable.

## Minimaps

**Export Minimap…** (an area's right-click menu in the module tree) saves
the area's map as a PNG, as the game's map draws it: each tile's picture
(the tileset's `ImageMap2D`) turned as the tile is, north up, unsaved
tiles included. A tile without a picture is black, as in the game. `mg
minimap` does the same from the command line. Each picture keeps its own
size unless `--size` sets one; the game's are mostly 16 pixels.

## The camera

Aurora's bindings, plus a few of Moonglow's (change the keys in Tools ›
Options › Keyboard):

| Do | To |
| --- | --- |
| Ctrl + drag | move the camera over the area, the ground following the pointer (a closed hand shows it held; a tileset brush paints nothing with it), including in Live; no box selection or tile painting |
| Right drag, or middle drag | turn the camera (W A S D move it meanwhile) |
| Alt + middle drag, or Alt + drag begun off the selection | move the camera (Alt + drag on the selection raises or lowers it) |
| Wheel | zoom (slowly with Shift, or with Ctrl when nothing selected takes a scale; when painting tiles, Shift is the brush's) |
| Sideways scroll (a tilted wheel, a second wheel, two fingers sideways on a touchpad) | move the camera left and right, as the screen has it |
| Arrow keys, W A S D, or numpad 4, 6, 8, 2 | move the camera |
| Numpad 7, 9 | turn |
| Numpad 1, 3 | tilt |
| Z, C | move the camera up, down |
| Ctrl + Shift + wheel, or Ctrl + Shift + middle drag | move the camera up and down, finely |
| Numpad 5 | look straight down at the whole area (and back to the ground) |
| F10 | select tiles or objects (Aurora's) |
| Double-click an object in Find Instance | go to it |

The keys drive the area view the pointer was in last, also while the
pointer is over the palette or the module tree (not while you type in a
field).

## Selecting and arranging objects

| Do | To |
| --- | --- |
| Click | select an object (the one whose model is under the pointer: a click through an arch reaches what is behind it) |
| Ctrl + click | add it to the selection, or take it out |
| Drag on the ground | select the objects in the box |
| Drag the selection | move it over the ground |
| Alt + right drag | turn the selected objects |
| Drag the ring around them | turn them as the pointer goes round |
| Shift + drag a tilt ring | tilt their models about X (red) or Y (green) |
| Shift + drag the square handle | scale their models: away from the object larger, toward it smaller |
| Ctrl + wheel | scale their models, as in Aurora: up larger, down smaller |
| Shift + drag an arrow | move them along one axis alone: east (red), north (green) or up (blue) |
| Q, E | turn them 15° left or right (or by the snapping angle); with Shift, 90° |
| G | drop them to the ground |
| Shift+G | drop them onto the placeable below |
| Alt + drag | raise or lower them (not creatures, which stand on the ground) |
| Delete | delete them |
| Escape | select nothing (it drops a drag, a brush or a paste first) |
| Ctrl+C, Ctrl+X, Ctrl+V | copy, cut, paste (in this area or another) |
| Double-click | the object's Properties |

Every move, turn and deletion is one undoable step. The view's corner
shows the pointer's position in the area, to the centimeter, and the
tile there: its model's name (`ttr01_a01_01`), its column and row, and its
height and turn when it has them. With one tile selected (Select Tiles),
it names that tile too.

**The turning ring**: an orange ring lies around the
selected object, with a mark on it the way the object faces. Take the
ring anywhere along it (near it is enough) and lead it round: the object
turns with the pointer, its facing shown in degrees; with several
selected, each has its ring (and with Shift its tilt rings and arrows),
any of which leads them all: each turns about itself by the same angle. With
**Together** on (beside Turn on the toolbar), they turn about their
middle as one instead, keeping their places among themselves, like a
prefab turned whole; Q and E turn them so too. A trigger or an
encounter turns by its outline: alone about the outline's middle (an
encounter's spawn points go round with it), and with the others when
they turn together. **Turn** on the
toolbar snaps it. **Tools › Options › Area** switches the ring off.

**The tilt rings**: hold **Shift** and two upright rings take the turning
ring's place, red and green. Lead the red one round to tilt the model
about its X axis, the green one about its Y axis; the angle shows in
degrees, and **Turn** snaps it. This is the rotation of EE's visual
transform (the one **Adjust Location…** has as numbers), so it is the
model that tilts, not the object: what it blocks stays as it was.
Creatures, items, doors and placeables that aren't **Static** have the
rings; a static placeable has no visual transform in the game, and its
right-click menu has **Make Dynamic** to clear Static. A dynamic one's
has **Make Static**, which also takes its visual transform away, as
Aurora does. (The game draws static placeables, and finds paths around
them, more cheaply: keep scenery that needn't tilt static.) A ring
seen edge-on (from straight above, both are lines over the object) is
led along its line.

**The scale handle**: with **Shift** held, a white square stands on a
stalk up and to the right of the selected object. Pull it away from the
object to make the model larger and push it toward the object to make it
smaller: twice as far from the object's feet, twice the size. The size
shows beside the pointer as you drag; with **Snap** on it goes in steps
of 0.05. The model grows from its feet, so it stays on the ground, and
every way alike (Adjust Location's Scale is the same number). Like the
tilt, it is the model that changes, for the objects a tilt works on;
several selected objects each scale about their own feet. Escape drops
the drag.

**Ctrl + wheel** scales the selected objects too, as Aurora's does: with
the pointer over the view, turn the wheel up to make their models
larger and down to make them smaller, about a twentieth a notch. The
size shows beside the pointer and in the status line; it becomes one
step to undo when Ctrl is let go or the wheel rests (Escape, or Undo
meanwhile, drops it). It works on what a scale works on: creatures,
items, doors and placeables that are not static. A static placeable
takes no scale (the game ignores one): clear **Static** in its
properties first; the status line says so, and the wheel zooms the view
slowly, as it does with nothing selected that takes a scale.

**The arrows**: with **Shift** held, three arrows stand out from the
selected object: red to the east, green to the north and blue upward.
Take an arrow's head and lead it along: the object moves along that axis
alone, however the pointer strays, with its place on the axis shown in
meters. East and north keep it as high above the ground as it was, and
**Snap** snaps along the axis; up raises or lowers it, as Alt + drag
does. They move the object itself, so they work for static placeables
too; creatures stand on the ground and have no blue arrow. An arrow
pointing at the camera (the blue one, from straight above) is led up and
down the screen.

A click on a ring or an arrow selects the object under it, if there is
one; the selection stays otherwise.

**Lights**: right-click the ground and choose **Add Light Here**, then
a color. Moonglow puts one of the game's invisible light placeables
there ("Light, White" and the six others of placeables.2da), a little
above the ground: it lights what is around it in the game and needs no
custom content. It shows as a small sun in its color; select, move and
raise it as any placeable. **Light Color** on its right-click menu gives
it another of the colors. Selected, a ring in its color shows how far
it reaches: where the game's light ends, 20 m out, the light fading
all the way there. These are the game's fixed lights: seven colors, one
reach. A hak that adds rows of the same kind (a `LightColor`
on the model `dag_invisible`) adds to the list.

**An encounter's spawn points**: with the encounter selected, the tip of
each spawn point's arrow is a handle. Lead it round the point and what
spawns there faces that way (**Turn** snaps it). The square at the foot
of its post is another: lead it over the ground and the point moves, the
encounter staying where it is.

What you are dragging stays in hand while another button turns the
camera (hold the middle button too and swing the view), until you let go
of the button that holds it. **Escape** drops the drag: nothing moves.

**Snapping** (the toolbar's **Snap** and **Turn**):
- **Snap** moves objects to a grid of 0.25 to 5 m as you drag, place or
  paste them.
- **Turn** turns them in steps of 5° to 90°, or of any number of degrees
  typed beside **Other** in its list.

With several selected, the first selected snaps and the others keep their
places and turns around it.

**Right-click** the selection for its menu:

- **Properties**: the object's properties as placed in the area (an
  instance can differ from its blueprint). With several objects of one
  kind selected, one window edits them together: a field changed there
  changes in all of them. It shows the first one's values, with **≠**
  before a field where the others have another (so too for blueprints
  edited together from the palette).
- **Adjust Location…**: an exact position and bearing, and the visual
  transform that EE added (scale, rotation about each axis and offset)
  for creatures, doors, items and placeables that aren't Static (a static
  placeable has none, as in Aurora).
- **Random Facing**: turns each selected object to face a way of its
  own, at random (by the snap angle, if one is set): for trees and rocks
  placed in numbers.
- **Drop to Ground** (G): puts raised objects back on the ground under
  them. A trigger or an encounter comes down (or up) as a whole, until
  its lowest corner is on the ground.
- **Drop onto Object** (Shift+G): sets each selected object on the
  placeable or door below its center (a skull on an altar, a book on a
  table), by that object's model, up to 2 m above it; with nothing there,
  on the ground. Not creatures, which the game stands on the ground.
- **Arrange** (several selected; the first selected leads):
  - **Line Up West–East** or **South–North**: on a line through the first.
  - **Space Evenly**: evenly spaced between the two farthest apart.
  - **Face Alike**: facing as the first does.
  - **Mirror West–East** or **South–North**: mirrored about the
    selection's middle, facings too.

  Objects keep their height above the ground.
- **Lock**: locked objects can't be selected by a click or a box, so you
  can work around them. **Unlock All** (on any part of the area) frees
  them. Moonglow marks a locked object with a field of its own
  (`MG_Locked`), which the game ignores. Aurora drops it when it saves,
  unlocking the object.
- **Save as Prefab…**: the selected objects, kept under a name to place
  again (see below): a group of placeables to copy around, in this area
  or another.
- Doors: **Reverse Door**, **Initial State** (open or closed, locked).
  Placeables: **Initial State** (open, active…). Sounds: **Mute** or
  **Turn On**. Triggers and encounters: **Redraw Polygon**; encounters:
  **Add Spawn Point** (at the point you right-clicked).
- **Conversation**, **Inventory** (creatures, placeables with an
  inventory, merchants), **Variables…**.
- **Add to Palette**: the instance as a new custom blueprint.
- **Test From Here**: starts the game at the point you right-clicked,
  facing the way the camera looks, with the module as it is now (see
  [Build, verify and test](09-build-and-test.md)). **Test From Here,
  Choose Character** does the same through the game's character
  selection.
- **Create Waypoint**: a waypoint where the object stands. **Create Set…**
  (waypoints): name the selected waypoints `<name>_01`, `<name>_02`…
- **Levelup Wizard…** (creatures), **Setup Store…** (a creature or
  placeable as a shopkeeper: its conversation, a store and the script that
  opens it), **Add Popup Text…** (a placeable that speaks a line when
  clicked).

## Placing objects

Choose a blueprint in the palette and click in the area to place it, or
drag it onto the area from the palette or the module tree. Until it is
placed, a see-through copy in a blue box follows the pointer where it
would go (on the snapping grid; a door on the nearest hook). **Q** and
**E** turn it as they turn a selection (Shift + Q and E by 90°), and it is
placed facing that way; a door faces as its hook does. Shift + click
places it and keeps it chosen for another; a right click or Escape lets
it go. Doors go on a tile's door hook when you click near one. Triggers
and encounters are drawn point by point: click each corner, double-click
to close the outline. For a rectangle, click one corner, press Alt (the
rectangle to the pointer shows), then click the corner opposite: its
sides run east and north, and it is placed at once (the same in
**Redraw Polygon**). Alt may be held for the click or let go before it:
on Linux, many window managers keep a click with Alt held for
themselves, to move the window, and there it is let go first. Escape
drops the outline, to begin again point by point. Turn the rectangle
after, by its ring, if it is to lie another way.

**Edit › Find Instance…** lists the placed objects across the module by
kind, area, blueprint and tag; double-click one to go to it.

**Prefabs** are groups of placed objects saved under a name, such as a
camp, a market stall or a furnished room: the way to copy a group of
placeables around.
- **Saving one:** select the objects (a click, Ctrl+click for more, or a
  box dragged around them), then **Save N as Prefab…** on the area's
  toolbar (it shows when several are selected), **Save as Prefab…** on
  the selection's right-click menu, or **Save Selection as Prefab…** in
  the palette's **Prefabs**.
- **Placing one:** choose it in the palette's **🗐 Prefabs** (beside the
  blueprint types), or under **Edit › Prefabs**. It follows the pointer
  like a paste. A click places it, with the objects in their places
  around each other and at their heights above the ground.
- **What one holds** shows while the pointer rests on it in the
  palette's Prefabs: how many objects of each kind, and their tags.
- **Renaming one:** right-click it there and choose **Rename…**.
- **Deleting one:** right-click it and choose **Delete**. A prefab isn't
  the module's, so Edit › Undo doesn't bring it back: **Undo Delete**,
  above the list, does (for the one deleted last, until Moonglow is
  closed).
- **Copy and paste** do the same without a name: Ctrl+C on a selection,
  Ctrl+V in this area or another. (Copying leaves a line of text on the
  system's clipboard, "Moonglow Toolset: 3 objects": Windows gives Ctrl+V
  nothing to do with an empty clipboard.)
- **Any area, any module:** prefabs are kept in Moonglow's data folder, in
  `prefabs` (see [Troubleshooting](13-troubleshooting.md)). Copy the
  `.prefab.json` files to share them.
- **Blueprints:** an object keeps everything it had when it was saved,
  and works without the blueprint it came from.

## Painting placeables

Trees, rocks, bushes, rubble: **✏ Paint** on the area's toolbar (or **B**
with the pointer over the area) turns the placeable brush on, and the
placeables chosen in the palette are painted onto the ground rather than
placed one a click. Choose them in the **Placeables** palette, the
game's or the module's: a click on one, **Ctrl + click** on more (Ctrl +
click again takes one out). The brush picks among them at random. A strip
of its settings shows above the view while it is on; they are kept for
the next time.

- **Scatter** paints in a green circle on the ground. Press to lay a dab,
  drag to go on laying them. Hold the button still and it keeps laying
  until the circle is full. **Shift + wheel** sizes the circle (the
  **Radius**).
- **Density** is how many placeables a tile's ground (10 by 10 m) holds
  once painted. The brush adds placeables until the circle has its share,
  counting those of the chosen blueprints already there. So going over a
  painted place again fills its gaps and adds no more.
- **Spacing** is the least distance between two placeables: those the
  brush puts down, and any placeable already in the area.
- **Flow** is how much of what the circle lacks each dab adds: at 100% a
  click fills the circle at once; lower, it fills as you go over it.
- **Turn**: each placeable faces the way **Q** and **E** turned the brush,
  give or take half this (360°, the default, is any way at all; 0° all
  alike). With the snapping angle on, facings snap to it.
- **Scale**: each placeable's scale is picked between the two. Static
  placeables take none: the game draws them at their own size.
- **Height** raises them above the ground (or sinks them, below 0): roots
  into a slope, rocks half buried.
- Placeables go only on **walkable ground** (the walkmesh's walkable
  faces, as **Walkmesh…** shows them green), at the level nearest the
  pointer's: not into water, onto roofs or inside walls.
- **Row** lays pieces end to end instead: drag from where the row
  begins to where it ends (floor tiles, a fence, a line of benches). Each
  piece faces along the row; **Q** and **E** turn the pieces on it
  (Shift + Q by 90°, for a piece that is long the other way). A piece
  takes up its model's length along the row. **Gap** adds room between
  pieces, or overlaps them when below 0: a model's box takes in whatever
  sticks out of it, so a little overlap may be what makes floors meet.
  The row begins on the snapping grid, and its direction snaps to the
  snapping angle. It follows the ground (walkable or not, so a walkway
  can cross water), raised by **Height**. Of several placeables chosen,
  each piece is a random pick.
- **Fill** covers a rectangle instead, in rows (a floor): drag from one
  corner to the opposite one, whichever way. **Q** and **E** turn the
  grid, and the rectangle's sides with it (by the snapping angle, if on;
  Shift + Q by 90°), each piece facing along it. A row lays its pieces end
  to end, each by its own length and **Gap**; the next row begins past the
  deepest piece of the one before, and Gap. So pieces of different sizes
  pack as they are. The first piece's corner is where you pressed (on the
  snapping grid), and pieces go on until the rectangle is covered, so the
  last row and piece may reach a little past it. It follows the ground as
  Row does, raised by **Height**.

Before a press, the dab a click would lay (with Row or Fill, the first piece)
shows see-through under the pointer, and a click lays exactly that. What
a stroke lays shows see-through until you let go; then it goes into the area as one step,
**Paint Placeables**, which one Undo takes away. Escape or a right click
during a stroke drops it. Otherwise it turns Paint off, and the
placeables stay chosen.

## Painting terrain

Choose **Tiles** in the palette pane to paint the area with its tileset,
as Aurora's Terrain tab does:

- **Terrain brushes** (grass, water, cliffs… whatever the tileset has)
  and **Raise/Lower** act on the tile corner nearest the pointer; the
  right button lowers. Drag to mark every corner the pointer passes
  (yellow; running the drag back unmarks them), or Shift + drag to mark
  the whole rectangle from where the drag began. Letting go paints them
  all, and one undo takes the whole drag back. The cursor shows the four
  tiles a stroke changes.
- **Crossers** (roads, streams, walls) are dragged: they follow the
  pointer through the tiles it passes. A drag straight across a tile may
  wander up to 2.5 m off its middle. To turn within a tile, head for the
  side it should leave by. Running the drag back over its path lets go of
  what it passed. Shift + drag lays it round the outline of the rectangle
  from the tile where the drag began (straight along a rectangle one tile
  wide). Right-click a quarter the crosser already crosses (the
  cursor is blue there) to take that crosser off the tile, as the Eraser
  would, leaving other crossers (a road goes, the stream it crosses
  stays) and the brush chosen.
- **Groups** (buildings, big features) are placed whole; right-click to
  turn one before placing it. It stays chosen, to place another. A group
  placed over part of another takes the other away whole (Aurora refuses
  to place it there; Undo brings the other back).
- The **Eraser** takes the crossers off a tile, and on a tile of a group
  or a feature takes the whole of it away (its tiles become ground that
  fits there); Shift + click steps the
  tile through the other tiles that fit there. Dragged, it marks the
  tiles it passes, as a terrain brush marks corners (run back, Shift for
  a rectangle), and erases them when you let go.
- **Refine Tile** (Moonglow's own) steps the tile you click through the
  other tiles that fit there, whatever it holds, and never paints.
- The Eraser, Refine Tile and Raise/Lower head the Terrain list, whatever
  the tileset's order.

Under the pointer, the area shows the tiles a click would make, slightly
see-through, in place of those they replace: a feature or group, a
terrain's or a raised corner's tiles, the Eraser's or Refine Tile's
choice. While you drag a terrain, a crosser or the Eraser, it shows what letting go
now would paint. The click or drag puts down exactly the tiles shown:
where several fit, the one shown.

The cursor is green where a click paints and red where the tileset
refuses it. Click there all the same and the tiles in the way flash red
for a moment, as in Aurora: those that have no tile fitting what the
stroke would make of them, or belong to a group it would break. It is blue where a click only chooses tiles again: a crosser
over a quarter it already crosses, the Eraser with Shift, Refine Tile, and
a corner of the brush's own terrain (with Shift, the next tiles that fit).

Each stroke is one undoable step. Moonglow paints as Aurora does: the
same strokes give the same tiles, heights and crossers.

## Tiles

With **Select Tiles** on, a click selects a tile, Ctrl + click adds one,
and a drag selects a box of them. **Delete** takes the selected tiles'
crossers away. **Shift + right click** steps the tile under the pointer
through the tiles that fit, as **Next Variant** on the tile menu does;
**Previous Variant** there steps back. Ctrl+C and Ctrl+V copy and paste tiles.

A right click opens the tile menu with **Tile Properties**. With one
tile chosen it shows the **variants**: every tile that fits there, as
pictures, the one there now framed. Click another and it is put there
at once (one step to undo), the window staying open on it. Tile
Properties is a window like the editors': resize it, maximize it or dock
it beside the area, and the pictures fill it. The same
model turned another way is listed again, its picture turned so (seen
from the south-west) and the turn written under it. And:
- the tiles' main and source light colors and their animation loops, as
  the tile's model has them (**Defaults** puts back the lighting
  scheme's);
- the **Replacement Texture**: what a texture named `replace_tex` in the
  tile's model is drawn with (a `replacetexture.2da` row; the game keeps
  it, though Aurora has no field for it).

**Edit › Resize Area…** grows or shrinks the area at its north and east
edges, as Aurora does, or at the two you choose there (east and south,
south and west, west and north): what is in the area keeps its place
against the two edges that stay; **Edit › Rotate Area…** turns it by 90° steps, objects and all.
**Build › Area Statistics** counts the area's tiles and objects and the
memory its models take.

## Area Properties

Right-click the area in the module tree › **Properties**, or **Area
Properties** on the viewer's toolbar:

- **Basic**: name, the tileset (fixed once made) and the size.
- **Visual**: the lighting scheme, day and night (or always one), the sun and moon colors,
  fog colors and amounts, shadows, the skybox, lightning, rain and snow
  chances, as Aurora's Customize Environment sets them.
- **Audio**: day and night ambient sounds and their volumes, music, battle
  music, the environment's sound effects.
- **Events**, **Advanced** (the tag, the loading screen, Variables; flags
  such as interior, underground, natural, no rest, PvP; **Shader Flags**, the area flags past those three, which
  custom shaders read from their `areaFlags` uniform), **Comments**.

### Several areas together

**Edit › Edit Areas Together…** (or an area's right-click menu in the
module tree) lists the module's areas to tick. Narrow the list by a name
or ResRef, by tileset, and by kind (interior or exterior, above or under
ground, natural or artificial); **Tick Shown** ticks what the filters
leave. **Edit N Together** opens one Area Properties for the areas
ticked, shown as the first of them:

- A change is set on each area, as one undoable step: the lighting scheme
  (each area's tiles get its lights) and environment, fog, weather, day
  and night, the ambient sounds and music, the event scripts, the check
  modifiers, player vs. player, the loading screen and No Rest.
- **Give Every Area This Page's Settings** sets what the page shows (the
  first area's) on all the others, whether changed here or not: to make
  the areas alike in what they already have.
- **Loading Screen** (Advanced, and a door's or trigger's Area
  Transition page) shows the chosen screen's picture under the list, and
  each screen's when the pointer rests on its name in the list.
- Interior, natural, underground and the shader flags change alone: each
  area keeps its other flags.
- **Variables**: the variables you add or change are set on each area,
  those you delete are deleted from each, and each keeps its others.
- Names, tags and comments are an area's own, and aren't offered.

To give every underground area the same music: Underground in the
chooser, Tick Shown, Edit Together, then Audio. From the command line,
`mg areas` does the same (see [Command-line tools](11-command-line.md)).

## Static placeables, many at once

The game merges an area's **static** placeables into its tiles when the
area loads, so they cost less to load and draw than dynamic ones. An
area's right-click menu in the module tree has **Make Placeables
Static**, and the **Areas** group's has **Make Placeables Static in Every
Area**: each sets Static on the placeables that lose nothing by it, as
one undoable step, and the log says how many it made static and how many
it left. Left dynamic are placeables that:

- are **Useable**;
- are tilted, scaled or moved by a visual transform (a static placeable
  has none);
- have a script, a conversation, a trap or an inventory, or an animation
  switched on (a lit torch): a static placeable plays no animations and
  can't be destroyed.

One a script destroys or animates by its tag can't be told from the
rest: make it dynamic again from its menu in the area view. **Make
Placeables Dynamic**, on the same menus, clears Static on every
placeable of the area or the module. From the command line, `mg areas
--static-placeables` and `--dynamic-placeables` do the same.

## Area sounds

With **🔊 Sounds** on, the placed sound objects play as you move around:
full volume within a sound's minimum distance, fading to nothing at its
maximum, each repeating, looping or playing once as set. **Ambient** and
**Music** play the area's ambient sound and music (by day or night, as the
view shows). Options › Sounds sets which play when an area opens, and the
music's volume.

## Making tilesets

**Tools › Tilesets** edits a tileset's `.set` file: **New Tileset…**
starts one, **Open Tileset…** opens one (a copy of a game tileset, or one
being made beside its models). An edit changes only the lines it is
about, so comments, order and spacing stay. **Undo**, **Redo** and
**Save** are in the editor's toolbar; saving the module saves open
tilesets too.
- **General:** the name, the name players see, the height step, the
  border, default and floor terrains, interior, height transitions, and
  grass.
- **Terrains and Crossers:** the lists, and **Add Terrain** and **Add
  Crosser** by name.
- **Tiles:** every tile with its corners and edges; **Find** by model or
  terrain. Per tile: model, walkmesh, minimap picture, path node,
  orientation, corner terrains and heights, edge crossers, lights and
  animation loops. **Add Tile** and **Duplicate Tile** add at the end, and
  **Remove Last Tile** takes the last away: areas store tiles by number,
  so tiles in the middle stay where they are. **Preview** shows the
  model when the game data has it.
- **Groups:** each group's name, size and tiles, the bottom row first.

The toolbar also has:
- **Make Palette** writes `<tileset>palstd.itp` beside the `.set`, the
  palette the painter offers: one-tile groups as Features, the others as
  Groups, the terrains and crossers, the eraser and (with height
  transitions) raise and lower. For the game's own tilesets it holds
  every group their palettes place. `mg tileset-palette` does the same.
- **Check** runs Verify's tileset checks: counts that disagree with the
  sections, tiles whose models are missing, groups that won't paint.
- **Render Minimap Pictures** draws each tile from above with Moonglow's
  renderer and saves it as its minimap picture (32 pixels, a TGA named
  by the tile's `ImageMap2D`, which tiles without one get as
  `mi_<model>`) beside the `.set`. Models and textures beside the `.set`
  are used before the game's.

To use the tileset, put the `.set`, palette, models, walkmeshes, textures
and minimap pictures in a hak (Tools › Haks › Build Hak from Folder…).



Live **Character → Faction** lists only the running server's reported factions
and session names, with their IDs. Editor-only factions are disabled until the
module is saved, rebuilt and restarted. It applies the selected ID to each eligible
NPC; faction 0 (the engine's player faction) is excluded.

For shared area tabs, private DM chat, Focus drilldown, follow across areas,
Live copy/cut/paste and counted group removal, see [Live mode](19-live-mode.md).
