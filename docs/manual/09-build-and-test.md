---
type: Manual Page
title: Build, verify and test
description: Build Module, publishing to NWSync, long work, Verify Module, Test Module, private NWNX live mode, reloading haks and 2DAs, and Area Statistics.
tags: [manual, build, verify, test, nwsync, live]
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-16T20:28:07Z }
---

# Build, verify and test

## Build Module

**Build › Build Module…** prepares the module for play and checks it, as
Aurora's Build does:

- **Compile**: the **Scripts** (with their debug files, where **Generate
  Debug Information** is on in Options), recalculated **Creature CR** (challenge
  ratings), **Encounters** (each encounter's creature list updated with
  its creatures' challenge ratings and appearances; creatures that no
  longer exist are removed) and **Palettes** (the custom palettes rebuilt
  from the module's blueprints).
- **Check that resources are available**: **Missing Resources** (scripts,
  blueprints, conversations, models… that something in the module names
  but neither the module, its haks nor the game has) and, if ticked,
  **Unused** resources (in the module but used by nothing).

Compile and Missing Resources are on by default, Unused off. The
**Results** list what was found; double-click a result to open what it is
about, and **Export…** saves the list as text. With **Build module on
save** (Options › General), every save builds first.

## Publish to NWSync

A server's players download its haks and talk table through NWSync:
before joining, the game fetches them from a web server that nwserver
names (`-nwsyncurl ADDRESS`). **Build › Publish to NWSync…** writes what
that web server serves into a folder, as neverwinter.nim's
`nwn_nwsync_write` does (the same manifest, byte for byte):
- **What goes in:** the module's haks (the first-listed hak's copy where
  several have one) and its talk table. Scripts' source and debug files
  and area comments are left out, since players don't need them. Files
  over 15 MB are refused, as `nwn_nwsync_write` refuses them.
- **With the module itself:** adds the module's own resources, under the
  haks', with a name and description players see. Use it for a
  single-player module that players download whole, not for a persistent
  world.
- **Group ID:** for servers sharing one repository.
- **Latest:** **Make it the latest** points the repository's `latest` at
  it; nwserver serves `latest` unless given `-nwsynchash`.

Publishing again writes only the changed files. It runs in the
background; the window shows progress, then the manifest's hash
(**Copy** copies it, for `-nwsynchash`). Upload the folder to the web
server. `mg nwsync` does the same from the command line.

## Long work

Compile All Scripts, Build Module (its Build button) and Verify Module
run in the background: a small window names the work, shows how far it
is, and has **Cancel**. Until it is done the rest of the window takes no
input, so nothing changes under the work; what it made (compiled scripts,
say) then goes in as one step that Undo takes back. Canceled, it changes
nothing. The build before saving (Options › General) and the compile
before a test run wait instead, since saving and testing need their
result.

## Verify Module

**Build › Verify Module** writes to the log, as errors and warnings, what
is missing and what is wrong with the module's custom content.

**Missing resources** are things something names that exist nowhere.
- **Errors:** scripts, conversations and areas named by areas, placed
  objects, conversations or the module, and scripts that were never
  compiled.
- **Warnings:**
  - what a blueprint names, which matters only once something places it;
  - a placed object's blueprint that is gone (the object keeps working);
  - sounds and portraits.

**Custom content** covers the module, the haks in your `hak` folder, and
`override` and `development`. Each problem names the hak or folder, the
file, and the row, column, section or object at fault. Most of them
crash Aurora with an access violation that names nothing:

| Problem | Where it shows |
| --- | --- |
| A tileset whose `[TILES]` or `[GROUPS]` count disagrees with its sections | Aurora's "Range Check Error" when opening an area |
| A tile whose model is missing | Aurora crashes painting it |
| A group whose first tile is `-1` or shared with another group, or names a tile that doesn't exist | the group won't paint, or Aurora crashes |
| A tileset door type that isn't a `doortypes.2da` row | the door has no appearance |
| A material (`.mtr`) naming a texture longer than 16 characters | Aurora's "Pure virtual function called" |
| A 2DA row with more cells than columns (two rows run together) | Aurora's access violation; the game drops the cells |
| `baseitems.2da` over 256 rows, `lightcolor.2da` over 32 | Aurora fails (the game is fine) |
| A 2DA string reference past the end of its talk table | "Bad Strref" |
| A hak's 2DA row naming a row of another table that isn't there (a feat's required feat, a spell's feat, a race's appearance, a package's class; some sixty columns of eleven tables, and the feat, skill and spell lists that a class, a race or a package names) | the feat, spell or class is missing or wrong in the game (a warning) |
| A custom talk table the module names that isn't in its haks, the module or the `tlk` folder (or is named with `.tlk`, or, on Linux, in a different case from the file) | the game won't load the module |
| A hak's 2DA hiding another hak's longer copy | the rows past it are lost (an older copy?) |
| A creature, placeable, door or item naming a 2DA row that doesn't exist or whose model is missing | Aurora's area view crashes on it; in the game it has no appearance, or the game crashes (a creature with a class that doesn't exist) |
| A tile model with over 10,000 faces | can crash Aurora when painting |
| A hak over 2 GiB with files starting past that mark | the game can't read those files and doesn't look for them in lower haks (a 2DA reads as empty, a script doesn't run); nor do Aurora and nwsync |

Moonglow reads a hak over 2 GiB as the game does: files that start past
the mark are listed but can't be opened. Move them to another hak.

A blueprint's problems are warnings, since they matter once it's placed;
a placed object's are errors. A hak that hides the game's own longer
2DA isn't reported, because haks made before Enhanced Edition routinely
do that. When it matters, an object naming one of the lost rows is
reported instead.

The checks of enabled [plugins](15-plugins.md) run with these, and their
findings are listed with the rest: a team's own rules (naming, required
scripts) become part of Verify Module.

`mg verify` does the same from a terminal, and can write its results as
JSON for a build pipeline (`mg --json verify`; see [Command-line
tools](11-command-line.md)). There each problem carries the id of the
check that found it (`set-model`, `2da-row`…); `mg checks` lists the
checks.

**Models kept as text.** A model in a hak, the override or the module
that is text (ASCII, not compiled) is read as the game would read it
(`mdl-text`): what the game refuses or crashes on is an error (a
point-to-point emitter without its reference node, a node without a
type or a name), what it would misread a warning (a node without its `endnode`,
a second node of a name whose children hang from the first, more than
four bones on a vertex; the first five of a model). Keywords the game
skips are not named: old content is full of them.
Each names the line. Compiled models are not read so.

**Build › Compile Models** compiles the module's own models kept as
text, each against its supermodel from the module, its haks or the
game, in the background and as one step to undo. A model with errors,
with no nodes, or whose supermodel isn't found stays as text, and the
log says why. (The hak editor's Compile Models does the same for a
hak, `mg pack --compile-models` for a folder.)

## Live mode with NWNX

Live mode connects Moonglow and the game through **Redis pub/sub only**.
Clients use Moonglow's separate loopback HTTPS API. In **Tools › Options ›
Live**, enable the service and Apply; it is off by default and remembers its
settings. See [Options](10-options.md) and [Areas](04-areas.md)
for the saved controls and live viewer.

**After Redis restarts, restart the game server.** The installed NWNX_Redis
connects once at startup: `Reconfigure()` is called only from its constructor,
and its old cpp_redis has no auto-reconnect. Moonglow reconnects automatically.
Options and `GET /live/v1/status` → `redis.connected` show that connection
separately from the game's report age. A new game boot expires old queued
commands. Repairing the plugin's pool and pub/sub resubscription is a
separate future task recorded in the repository's `docs/deferred.md`.

### Windows/WSL setup

The demo needs PowerShell 7, the installed game, WSL with NWNX Core, Redis and
Util plugins, and a base module with its HAKs. Install Redis inside WSL:

```powershell
wsl -e sudo apt-get install -y redis-server
```

Keep Redis bound to loopback in protected mode. NWNX_Redis has no password
setting. Windows reaches it through WSL's localhost forwarding. WSL can stop
when idle: the launcher keeps an owned foreground WSL process alive throughout
the run, including outages and GUI/server restarts. If Options says **Redis
unreachable — is WSL running?**, check WSL and the configured Redis port.

Put workstation-specific paths in ignored `tools/live/local.ps1`; set
`NWNX_ROOT` (WSL directory containing `Binaries/NWScript.zip`),
`NWN_SERVER` (WSL server executable), `NWN_ROOT` (Windows game directory),
`MGLIVE_DEMO_BASE`, `MGLIVE_HAK_SOURCE`, and optionally `MGLIVE_DISTRO`.
Live views subscribe to areas automatically; `-Areas` is only an optional extra
list, empty by default. The base is read-only.
From the repository root:

```powershell
.\tools\live\check.ps1 -Phase 1b -Next -Build
$live = .\tools\live\Start-LiveDemo.ps1 -PassThru
```

The launcher reuses one working set under `target/test-output/live-demo/`,
compiles and verifies `DEMO-Live-01.mod`, opens it in Moonglow and starts a
**private** WSL server on UDP 5121. By the owner's demo policy, player/DM
passwords are omitted; the admin password stays random and is never printed.
`-Passwords` restores random player/DM passwords. Public launches are refused.
It refuses an occupied
game port. Writable game data stays in the owned scratch user directory;
neither the original module nor the real game/user folders are written.
`start-redis.sh` reuses the system service if PING succeeds, otherwise starts
an owned loopback instance. The normal demo never stops the system service.

Open the reported area in Moonglow and turn **Live** on. `-Base PATH` and
`-Areas resref1,resref2` select another input; preserve any needed result before
explicitly replacing a working set. `-SkipBuild` reuses the current verified
module. `-AllowChunks` opts this private run into both chunk switches. The
launcher uses environment overrides; clear them when testing saved Options
alone. Its returned token stays in `$live.Token`; `-Quiet` suppresses printing it.

```powershell
.\tools\live\Stop-LiveDemo.ps1
```

Stop checks the recorded process identity, stops only owned processes, removes
the owned stale descriptor and releases the WSL keeper.

### Connecting another module

Import `tools/live/nwn/*.nss`, compile using the installed NWNX Redis, Util and Creature
includes from `NWScript.zip`, and call `mglive_load` from OnModuleLoad while
preserving the module's existing load script. Enable NWNX Core, Redis, Util and Creature.
There are no HTTPClient scripts and no `/live/v1/sync` route.

| Process | Setting | Meaning |
|---|---|---|
| Moonglow | Options › Live | Saved enable switch, API token/port, Redis host/port, prefix and script policies |
| Moonglow | `MOONGLOW_LIVE_TOKEN` | Override the bearer token; at least 32 printable ASCII characters (the launcher accepts 32–256 letters, digits, hyphens or underscores) |
| Moonglow | `MOONGLOW_LIVE_BIND` | Override loopback IP:port; default `127.0.0.1:5199` |
| Moonglow | `MOONGLOW_LIVE_REDIS` | Override Redis host:port; default `127.0.0.1:6379` |
| Moonglow | `MOONGLOW_LIVE_PREFIX` | Override channel prefix; default `mglive` |
| Moonglow | `MOONGLOW_LIVE_CHUNKS` | Override its chunk policy; only `1` enables it |
| Server | `NWNX_REDIS_HOST`, `NWNX_REDIS_PORT` | Redis connection; loopback, default port 6379 |
| Server | `NWNX_REDIS_PUBSUB_SCRIPT` | `mglive_ps` |
| Server | `NWNX_REDIS_PUBSUB_CHANNELS` | `mglive.cmd`, or `<prefix>.cmd` |
| Server | `MGLIVE_PREFIX` | Same prefix as Moonglow; state publishes on `<prefix>.state` |
| Server | `MGLIVE_AREAS` | Optional comma-separated area resrefs; player areas are also reported |
| Server | `MGLIVE_BOOT` | Fresh identifier for each start; the launcher generates it |
| Server | `MGLIVE_ALLOW_CHUNK` | Second chunk switch; only `1` allows execution |

An explicitly supplied `MOONGLOW_LIVE_*` override also enables the service for
that process. With no overrides, saved Options control it; no environment token
is required. The game never receives the HTTPS bearer token.

### Client API and command examples

All HTTPS routes require `Authorization: Bearer <token>`:

| Route | Result |
|---|---|
| `GET /live/v1/status` | Redis state, boot/sequence, area ages/counts, queue and recent results |
| `GET /live/v1/area/RESREF` | Reported creatures, placeables, doors and items |
| `POST /live/v1/command` | Queue one command and return `{"queued":"id"}` |
| `GET /v1/describe` | Available operations and flat input schemas |
| `POST /v1/call/NAME` | Call a discovered operation, including `live_wait` |
| `GET /v1/openapi.json` | OpenAPI 3.1 for current policy |

The following Git Bash examples use `TOKEN` copied from Options. Replace
`AREA`, object ids and blueprint resrefs with values from your module/reports.
Each command is asynchronous; inspect status results before relying on its
effect. Do not run the examples against objects you need to preserve.

```bash
api=https://127.0.0.1:5199/live/v1
get() { curl --noproxy '*' -skS -H "Authorization: Bearer $TOKEN" "$api/$1"; }
send() { curl --noproxy '*' -skS -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d "$1" "$api/command"; }
get status
get area/AREA
send '{"area":"AREA","op":"spawn","kind":"creature","r":"nw_bartender","x":20,"y":20,"z":0,"f":90}'
send '{"area":"AREA","op":"npc_move","target":"NPC_ID","x":20,"y":20,"z":0,"run":true}'
send '{"area":"AREA","op":"npc_say","target":"NPC_ID","text":"Good morning!"}'
send '{"area":"AREA","op":"npc_attack","target":"NPC_ID","victim":"VICTIM_ID"}'
send '{"area":"AREA","op":"set_state","target":"DOOR_ID","open":true,"lock":false}'
send '{"area":"AREA","op":"run_script","target":"NPC_ID","script":"mglive_probe"}'
send '{"area":"AREA","op":"run_chunk","target":"NPC_ID","code":"SetName(OBJECT_SELF, \"Chunk ran\"); SpeakString(\"Sent from outside\");"}'
send '{"area":"AREA","op":"run_chunk","code":"this is not nwscript"}'
send '{"area":"AREA","op":"destroy","target":"SPAWNED_ID"}'
get status
```

Spawn also accepts `placeable` and `item`. Script resrefs are 1–16 ASCII
letters, digits or underscores; the script must already be compiled in the
server's resources. Omitting `target` for scripts/chunks runs on the area.
Chunks accept 1–8000 nonblank characters and optional `wrap:false` for a full
script; default wrapping supplies `void main()`. Compilation errors return
`ok:false` with `err`. Both sides log script/chunk execution. Scripts can
intentionally act on PCs/DMs; the ordinary object actions protect them.

Object targets are reported hexadecimal ids, and areas use resrefs, not tags.
An explicit `id` can support retrying the same command. Reusing it for different
content is rejected. Commands expire after 30 seconds without acknowledgement,
or on a new game boot. The game remembers 256 ids; Moonglow remembers up to
65,536 until exit. An action's success means dispatch succeeded: check reports
for the final position, name or object state.

Keep the game private (`-publicserver 0`), Redis on loopback
and the client API on loopback. Its self-signed certificate needs curl `-k`;
the MCP adapter instead verifies the descriptor's certificate pin. Tokens and
descriptors are credentials. `run_chunk` is off by default and requires both
switches because it executes arbitrary server code. It is not a public-server
interface.

### Reproducing acceptance

`tools/live/test-demo.ps1 -Screens` exercises real HTTPS/Redis/NWNX operations,
GUI reconnect/deduplication, the owned Redis 6380 outage, required game restart,
old-command expiry and both independent chunk-off gates. The test explicitly
enables both gates temporarily for its positive cases; it takes no
`-AllowChunks` argument. It never stops the system service on 6379.

`tools/live/morning.ps1 -SkipBuild` reruns that acceptance and then runs curl
and an actual MCP SDK client. Configure `MGLIVE_MCP_CLIENT` in local settings
to the installed `@modelcontextprotocol/client` package. Results and status
JSON go in `target/test-output/live-demo/morning/`. The guarded
`tools/live/check.ps1 -Phase F -Picture live_morning_snapshot` renders its
before/after reports through the actual viewer. All Rust checks on this
workstation use `check.ps1`, which excludes native Windows game-server tests.

The automated run uses an owned temporary bartender for each transport and
requires arrival at (20,20); shutdown discards those runtime objects. A
module's own NPC routine can issue competing actions, so dispatch alone
does not prove arrival. On failure, the harness retains area/status responses
alongside the failed result.

`npc_move` and `npc_attack` accept `hold:true` to keep routines from replacing
the order, with `timeout` from 1–120 seconds (default 30). Arrival/end of action,
death, a replacement movement/attack order or timeout restores commandability.
Different-faction attacks request temporary personal hostility for 180 seconds.
Same-faction attacks temporarily put the attacker in Hostile with a bounded hold:
**it is hostile to everyone, including players**, until action/hold end, victim
death, timeout or a new order restores its original faction. Their command result
includes `faction:"hostile_temporarily"`. A hold's timeout releases routine
control and does not itself stop all combat. The view's
Attack command enables a 30-second hold. `tools/live/test-next.ps1 -Phase 1b`
checks same-faction combat and hold restoration in the private WSL game.

Recorded-report pictures prove the renderer, not that a person watched the
native Moonglow window. Watching Live movement there and joining with a player
remain manual acceptance. The default private demo join needs no player/DM
password. With `MGLIVE_PASSWORDS=1`, `MGLIVE_PRINT_PASSWORD=1` makes the WSL
helper print only its random player password; do not retain that output in logs.


## Test Module

**Build › Test Module** (F9) saves the module and starts the game on it:
your first local character appears at the module's start location. The
game looks for the module by name in your user folder's `modules`
folder, so the module must be saved there (File › Save As…). The game is
started on that same user folder (`-userdirectory`): its characters,
override and settings. To test with another, choose it in Options ›
Folders, or start Moonglow with `--user-dir FOLDER` for one run (see
[Options](10-options.md)). With
**Minimize Toolset on test module** (Options › General), Moonglow's window
is minimized.

A Steam copy of the game is started so that it reaches Steam when Steam
is running, and loads your Steam Workshop content as when you start it
from Steam. With **Start a Steam copy of the game through Steam for a
test** off (Options › General), the game's program is started on its
own instead: no Steam overlay and no Workshop content, which some find
steadier.

- **Test Module, Choose Character** (Shift+F9) opens the game's
  character selection for the module instead, so you can test with any
  of your characters. You no longer need to rename one to sort it first.
- **Test From Here** (an area's right-click menu) starts the game at the
  point you clicked, facing the way the camera looks, using the module as
  it is now, without saving it. It writes the module to `modules` as
  `moonglow-test.mod`, replaced each time, so your module and its start
  location are left as they are.
  **Test From Here, Choose Character**, below it, opens the game's
  character selection first, as Test Module, Choose Character does.

Moonglow starts the game as a separate program (`nwmain +TestNewModule`),
as nwn.wiki recommends, to avoid the problems it lists for Aurora's F9:
lagging combat, AI that overflows its time, and modules damaged when the
game crashes. The module is always saved before
the game starts, and Test From Here plays a copy.

`mg serve` runs a module in the game's headless server from a terminal,
and `mg test` runs a module's test scripts there and reports what they
assert: see [Command-line tools](11-command-line.md).

## Reloading haks and 2DAs

While you work on custom content in another program, Moonglow picks up
these changes without a restart:
- a hak rewritten;
- 2DAs, models, textures and talk tables added, changed or removed in
  `override` or `development`;
- the module's custom talk table.

It checks every few seconds (Options › General, on by default), or when
you choose **Tools › Reload Resources**. Palettes, previews and open areas are
redrawn from the new files, keeping the camera, and the log says what
was reloaded.

## Area Statistics

**Build › Area Statistics** shows, for the area shown last, its tiles,
its objects and the memory its models take.
