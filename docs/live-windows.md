---
type: Reference
title: 'Hexxar Live windows: effects, library, skills and inventory'
description: Window implementation and menu handoff, selection and file contracts, passing fake-store and renderer checks, inspected pictures, retained findings and cleanup.
tags: [live, ui, vfx, library, skills, inventory, testing]
generated: { by: codex/astra2, at: 2026-10-13T00:00:00Z }
sources: [live-ops-wire.md, live-mode-next.md, live-mode-roadmap.md]
---

# Hexxar Live windows

Current status: these windows are integrated on `live-mode`, including the
third-acceptance inventory grid (`f4c4fc9`), interactive player Roll (`0b1afa0`)
and shared map aiming (`a569ace`). The older branch names, check counts and
blockers below are dated implementation history. Native and fake-store proofs
are complete for the recorded cases; owner desktop and connected-client
acceptance remain separate. See the current
[acceptance 4 click checklist and integration gate](live-mode-next.md#fourth-acceptance-integration-2026-10-13).

## Current window acceptance coverage

- VFX and NPC Library: arm placement, inspect the map ghost/circle, cancel or
  click to send; Library capture preserves the selected creature look.
- Skill Check: PC recipient or NPC/radius centre, explicit player Roll, bonus,
  optional hidden DC, timeout/no-roll, and final results in the window/Live Log.
- Inventory: current native item icons, equipment slots, give/equip/unequip,
  confirmed drop/take and gold; verify each operation in the client. Backpack
  arrangement is local, with native cells/footprints/pages/stack edits deferred.

- Text (owner 28): module-origin Polish name/speech/log text reaches these
  windows without replacement characters; see the
  [receive-boundary proof](live-events-wire.md#module-text-receive-boundary-owner-bug28).

View items 1-9 and their concrete clicks are in the linked acceptance 4 checklist.
The original 10-14 numbering was not present in this window record; mapping it
awaits the coordinator's brief. This explicitly separates implemented features
from a claim that a missing numbered requirement has been audited.

## Menu and ownership handoff

| Menu action | Tab | Required context |
| --- | --- | --- |
| `Action::LiveVfx` | `Tab::LiveVfx` | An open area with Live enabled and a fresh report to apply effects |
| `Action::LiveLibrary` | `Tab::LiveLibrary` | Saved module for its library; a Live area to capture or spawn |
| `Action::LiveSkill` | `Tab::LiveSkill` | Selected live object as centre; player operations enabled |
| Existing Appearance Gallery action | `Tab::PlaceableGallery`, Creatures | A live creature selection activates the named-target confirmation |

The first three actions only open their tab. They can be registered in the Live
menu and object context menus without extra arguments. Context menu callers must
keep `palette.area` and the live selection on the intended area/object.
The existing `view.live.selection_ids()` helper supplies group selections;
`view.live.selected` is the primary object for the library and skill centre.

Implementation is in new `live_vfx.rs`, `live_library.rs` and `live_skill.rs`.
Small registration/state/dispatch hooks are in `lib.rs` and `tabs.rs`. Small
hooks in `appearance_gallery.rs`, `model_view.rs` and `live_log.rs` reuse the
existing gallery, animated renderer and log. `commands.rs`, `keys.rs`,
`area_view.rs`, `live_panel.rs` and `options.rs` are unchanged. The only changes
to `live_view.rs`/`live_group.rs` are the coordinator-supplied integration fixes
and exact-target Stop Group Fighting, committed first in `50c5a7b`.
All sends use `live_scene::enqueue`, preserving normal command validation,
player policy, command results, log events and scene recording.

## VFX and appearance

The VFX list reads the configured game's/module's `visualeffects.2da`, retains
row numbers, searches labels or row IDs, and filters `Type_FD` D/F/B/P. D effects
allow instant, temporary seconds (0.1 through 86400) or permanent duration;
other types select instant. Apply sends `apply_vfx` for each selected object,
or one location command at the last primary ground click in that live area and
server boot. The ground hook uses the area's existing walkmesh-aware
`ground_at` method and excludes clicks on floating windows. Clear sends
`clear_vfx` without a row for each selected object: the game removes only the
tagged effects owned by Moonglow.

Preview uses `mg_preview::vfx::Vfx`, `with_models`, the existing creature look
builder (appearance row 6 mannequin) and `model_view`. One model-view key is
reused across effects. Preview resource data is cached per selected row,
game-data instance and workspace revision. Duration/impact models use the
renderer already used by model previews. A small model-view hook delegates
attached-emitter simulation and animated mesh alpha to the new module. Particle
bounds join the camera's Frame calculation; the preview fits them once after a
short initial simulation, leaving subsequent user camera control intact. The
renderer hook is limited to `Source::LiveVfx`; this does not simulate a beam between
two actors or a projectile trajectory. ProgFX rows additionally show a
representative colour swatch from the existing live-effect colour resolver;
this is labelled as representative, not an exact procedural shader preview.
Rows with no models can show the mannequin and the swatch only.

In the creature Appearance Gallery, clicking a row with a live selection opens
a confirmation listing each target's name and engine ID. Confirm sends
`set_appearance` for the captured target set. The confirmation is discarded if
the module, server boot or live availability changes, including a stale report.
An unavailable live selection does not fall through to offline editing.
Without a live selection the gallery follows its previous editor behavior.
Neither path in the new live handler writes the module's GIT.

## NPC library

The library contains native creature ObjectToJson values, without a wrapper, at
`<data_dir>/live/library/<module filename stem>/<entry name>.json`.
Module folder and entry names use `mg_live::scene::valid_name` plus Windows
reserved-name and trailing-dot/space rejection. Names are 1-64 ASCII characters
from letters, digits, spaces, `_`, `-`, `.` and cannot start with a dot or space.
Folders/files that are symlinks are refused. Existing entries are not
overwritten; Capture requires a new name. Rename saves the validated value to a
new exclusive file, then removes the old entry. Delete requires a named
confirmation in the window.

Capture requires exactly one NPC and sends `capture_object`. Its result must
be successful and contain `object_json`; no file is saved just because the
command was queued. The captured name/module/boot are held until completion;
changing module, disconnecting, rebooting the server or timing out cancels the
pending save. A file is at most 262144 bytes and must have native type `UTC `.
Both save and load call the same typed `spawn_json` command validation used at
the API boundary, including GFF structure/depth, encoded JSON size and PC/DM
restrictions. Reads are bounded even if the file grows after its size check.

Spawn reloads and validates the selected file, then sends `spawn_json` at the
open area's camera target or the primary selected object's reported position.
Facing defaults to zero. The optional thumbnail shows the saved appearance and
gender as a plain body; full equipment appearance is not reconstructed for the
thumbnail. The native snapshot still carries the object's inventory, scripts
and other captured state. Re-running its native OnSpawn behavior is governed by
the existing [wire contract](live-ops-wire.md), not by this window.

## Skill check

Skills use skills.2da Name/TLK with label fallback. The window sends a request
for one selected PC or arms the shared player-radius aim. Inputs: DC0..255,
radius0.1..30m, signed DM bonus -100..100, Show DC to player, timeout1..600s
(default120), and optional success/failure texts up to200 characters. Both
player gates still apply. The player must click Roll in the native NUI; sending
from Moonglow does not roll. The bonus is locked in the player window.

The DM table follows pending command-result snapshots until every row is rolled,
no_roll or busy. It shows the game's d20, full skill modifier, DM bonus, total,
DC and success/failure, or Waiting/No roll. The same formatter supplies Live Log.
Pending checks cancel on module/boot/live changes. Native roll details and the
connected-client checklist are in the [wire contract](live-ops-wire.md#nearby-player-skill-check).

Current proof: tests/live_skill_roll.rs exercises a fresh fake store, exact bonus/
DC-display/timeout wire input, pending-disabled Send, final roll/no-roll rows and
Live Log text. Existing skill-window and exact-target tests still pass. Picture
`target/test-output/live-windows/skill-roll-picture/skill-roll.png` uses the real
installed skill labels and a fake report; the earlier accepted baseline is
`target/test-output/live-windows/dm-pictures/after-skill.png`. Inspection caught
and fixed an equation overlapping subsequent rows: labels now wrap using the
grid's natural row height. No app binary, demo server or game client was touched.

Reproduce with check.ps1 -Phase5 (PowerShell 7; separate arguments):

```powershell
./tools/live/check.ps1 -Phase 5 -Step skill-roll-ui -Package mg-ui -Filter skill -Jobs 1 -NoUiDebug -UiUnoptimized
./tools/live/check.ps1 -Phase 5 -Step skill-roll-picture -Picture live_skill_roll_snapshot -Jobs 1 -NoUiDebug -UiUnoptimized
```

## Verification and reproduction

Source tests are `crates/mg-ui/tests/support/live_windows.rs`, four `hexxar_`
entries in `tests/app.rs`, and ignored `hexxar_windows` in `tests/screens.rs`.
Fixtures author their own small MOD and 2DA data under
`target/test-output/live-windows/<case>/`, marked `.hexxar-windows`. The current
fixture is written directly with `to_archive_bytes`, avoiding save-as backups on
each run. They use a
fake `LiveStore` and do not start a Live service, Redis, game client or server.
The renderer test additionally reads an explicitly supplied game installation;
it does not write to it. The running owner demo is outside this test scope.

Renderer inputs: Steam NWN:EE build **20277208**, read from
`C:\Program Files (x86)\Steam\steamapps\common\Neverwinter Nights`, English
resources and no user directory. The build ID comes from local
`steamapps/appmanifest_704450.acf`. The test reads the installed 2DAs/models and
talk table through `GameInstall`/`GameData`; no game assets are committed. The
mannequin is appearance 6; pictured effects are entangle 2, fire aura 268 and
red ProgFX glow 411. App tests instead use authored two-/four-row fixtures.
The screenshot fake store receives a fresh report each UI frame, independently
of rendering speed, and no real Live service is created.

Use PowerShell in `C:\Projects\moonglow-toolset-original-astra2`:

```powershell
$env:CARGO_TARGET_DIR='C:\Projects\moonglow-toolset-original-claude\target'
$env:CARGO_BUILD_JOBS='1'
$env:NWN_ROOT=Join-Path $PWD 'target/test-output/live-windows/no-native-game'
$uiFlags=@('--config','profile.dev.package.mg-ui.debug=0',
  '--config','profile.dev.package.mg-ui.opt-level=0',
  '--config','profile.test.package.mg-ui.debug=0',
  '--config','profile.test.package.mg-ui.opt-level=0')
cargo +1.98.0 test -p mg-ui @uiFlags --test app hexxar_ -- --test-threads=1
cargo +1.98.0 test -p mg-ui @uiFlags --lib live_library::tests
# Set MGLIVE_UI_GAME_ROOT to the read-only installed NWN resource root.
# Keep NWN_ROOT pointing to the non-existent guard directory above.
cargo +1.98.0 test -p mg-ui @uiFlags --test screens hexxar_windows -- --ignored --test-threads=1
```

Verified on 2026-10-13:

- `mg-ui --lib`: 110 passed, one existing performance test ignored.
- Four `hexxar_` app tests: group VFX and ground command fields, owner-only
  clear, named appearance confirmation before enqueue, unchanged module,
  capture result before file write, rename/delete/path rejection,
  camera/selection spawn, player-policy disablement, skill results and full
  Live Log formatting. All passed.
- Existing offline gallery and Live Log app regressions also passed (six app
  tests in the final combined run).
- `hexxar_windows`: GPU renderer test passed. It asserts entangle parts,
  advancing animation time, nonempty fire-aura particle geometry, actual area
  ground picking and isolation from floating-window clicks. Seven PNGs are
  kept in `target/test-output/live-windows/pictures`: `vfx-models.png`,
  `vfx-animated.png`, `vfx-particles.png`, `vfx-glow.png`,
  `appearance-confirmation.png`, `library.png`, `skill-results.png`.
- Clippy for `--lib --test app --test screens`, with `-D warnings`, passed.
- `cargo fmt -p mg-ui --check` and `git diff --check` passed.

Additional reproduction commands use the same environment and `$uiFlags`:

```powershell
cargo +1.98.0 test -p mg-ui @uiFlags --lib -- --test-threads=1
cargo +1.98.0 test -p mg-ui @uiFlags --test app -- hexxar_ the_placeable_gallery_gives_the_selection_an_appearance live_log_shows_the_game_s_events_by_page --test-threads=1
cargo +1.98.0 clippy -p mg-ui @uiFlags --lib --test app --test screens -- -D warnings
```

The real client's rendering and player interaction still require owner review.
No app executable, demo, client or game server was rebuilt or run for this UI
batch. The native operation behavior is covered by the already accepted
[game-side proofs](live-ops-wire.md).

### Findings retained from proof

The initial model-view reuse showed the mannequin without the VFX. Entangle is
mesh-based, not a particle effect: its default mesh alpha hides it until its
own animation controllers are evaluated. Composed previews supplied the node
poses but not that animated mesh state. Separately, the existing model viewer
simulated only base-model particles, omitting effect emitters on attached
parts. The new module supplies both through narrow model-view hooks.

Fire-aura particle geometry existed but initially lay outside the body's camera
frame: its measured horizontal bounds were about -4 to +4 m around a roughly
2 m mannequin. Framing includes particle bounds now. The inspected final
pictures show entangling vines, orbiting fire points, and a red procedural
swatch. These are resource/renderer previews, not a claim of exact game shader
parity. The existing gallery retains its scroll offset after filtering; the
screenshot harness scrolls to the top before choosing Human rather than
changing unrelated gallery behavior.

The original integration blocker was three old `NpcAttack`/`StopCombat`
initializers. The coordinator supplied their fixes and Stop Group Fighting;
`50c5a7b` records those changes. No separate permission is outstanding.

## Working files and retention

Task `hexxar-windows`, session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`.
The small exact-path ledger is
`target/test-output/live-windows/files.json`. Keep source/tests, this canonical
document, seven reviewed PNGs, authored fixtures and the one test-library entry.
The latter are tiny required inputs for reviewing/reproducing the current
result, not per-attempt copies. Reuse their paths on every run. The integration
patch (1418 bytes) and old generated fixture backup (984 bytes) were removed
after the integration commit and fixture-writer correction: 2402 logical bytes.
The backup differed only in the ERF build-day header; resource bytes matched.
Exact removed paths/hashes and retained-picture hashes are in the ledger. Shared Cargo outputs
and other workers' evidence stay outside this cleanup scope. No change in free
disk space is attributed to this task while other work is running.

Active session history has not been altered. Session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a` is identified here for a separate,
explicitly authorized history cleanup outside this session; no approved
history-deletion mechanism was used or assumed.

## Owner follow-up: runtime freshness (2026-10-13)

The demo launcher enables Live via `MOONGLOW_LIVE_*`; `apply_live` intentionally
leaves saved `settings.live.enabled` unchanged. The windows incorrectly used
that saved value to reject a fresh report and cancel pending results. They now
use `live_service.config().enabled` when a service exists, falling back to saved
options in a fake-store/offline harness. The area report still expires after
10 seconds. Appearance confirmation also has an explicit close button.

Reproduction: `tests/live_windows.rs::hexxar_runtime_enabled_overrides_saved_option`
starts only its own ephemeral API service and reserved dummy Redis endpoint,
with keep_wsl/logs off, saved enabled=false, runtime enabled=true, and a fresh
fake report. Before the fix Apply VFX was disabled (assertion failure); after,
VFX Apply, an empty-text skill check including result delivery, and library Spawn
all queue their expected commands. A 20-second-old report still disables Apply.
No game or demo process is touched. This is app behavior proof, not client visual
acceptance. Both logs are under `target/test-output/live-mode/`, prefixes
`phase5-dm-freshness-before` and `phase5-dm-freshness-after`.

Current shared-tree test command (no app rebuild, main target):

```powershell
& '<PowerShell 7>/pwsh.exe' -NoProfile -File tools/live/check.ps1 -Phase 5 `
  -Step dm-freshness-after -Package mg-ui -Filter hexxar_runtime `
  -Jobs 1 -NoUiDebug -UiUnoptimized
```

Task `hexxar-dm-followup` owns a small file ledger at
`target/test-output/live-dm-followup/files.json`. The runtime-enabled fixture is
`target/test-output/live-windows/runtime-enabled/`, under its existing fixture
ownership marker. It is overwritten in place; no full module copies per attempt.
Earlier worktree paths above describe historical proof; these commands supersede
them for current work. Shared target/cache files belong to the whole project.


## Inventory and owner follow-up windows (2026-10-13)

`Action::LiveInventory` opens `Tab::LiveInventory` (kind live-inventory). The
implementation is `crates/mg-ui/src/live_inventory.rs`; only module/state/action
and tab registration hooks are required elsewhere. It reads the active live
selection and resets cached rows, pending results and confirmation when the
module, boot, area or target changes. Refresh issues view_inventory; no game
inventory is written into the authoring module. Reports must still be fresh.

The window now shows a paperdoll equipment layout and a backpack icon grid.
Icons use numeric appearance fields from the **actual native item**, resolved
through `baseitems.2da`, layered PLT/TGA textures, and the module/HAK resource
stack. Old reports fall back to their item blueprints. Hover shows name, resref,
stack and protection flags; click selects an item. Drag onto a body slot to
equip, onto the backpack to unequip/reorder, or onto the ground strip to request
a confirmed drop. Buttons provide the same actions without dragging. Take
requires named confirmation and destroys the whole item/stack; plot items are
protected. Equipment slots 14..17 (creature weapons/skin) are read-only.

Backpack arrangement is **local to this view**, not the game's inventory cell
coordinates. The installed NWScript and NWNX Creature/Item/Object APIs expose no
cell-position setter. The grid deliberately does not claim to reproduce native
item footprints/pages or stack splitting/merging. These need a separate engine
hook/API. Equip/unequip/drop are real native actions, checked through the next
inventory query or ground-item report. The native result stays bounded at128
rows/15KB and explicitly marks truncation, including appearance data.

Give Item accepts a resref, current item-palette selection, or palette search.
It uses `give_item`: NPCs need no Players switch; PCs require both permissions.
The legacy `pc_give_item` remains strict for compatibility. The gold field shows
the live balance and supports numeric Give/Take; Give Gold keeps the existing
`pc_give_gold` permission, while Take Gold permits NPCs normally. Successful
mutations queue a fresh inventory query. PC inspection and mutation require
player permission; the server independently enforces its switch.

`live_inventory_grid_snapshot` renders the current panel using installed game
icons and an isolated fake store, without a game/app launch. Compare
`target/test-output/live-windows/inventory-grid-picture/inventory-grid.png` with
`inventory-grid-reference/nwn-manual-p17.png` in the same parent directory.
The latter is the **original manual's** inventory image (printed p17, PDF index10),
not a capture of the owner's current client. The [BioWare manual hosted by
Beamdog](https://forums.beamdog.com/uploads/editor/xs/eovsvce00axn.pdf) supplies the
layout reference: equipment above inventory grid, drag equipment, gold display.
Its source URL, hash and page are in `inventory-grid-reference/source.json`.
Current native PC rendering is still a client acceptance step; this machine lacks
the configured offscreen client's `Xvfb` and `bwrap`, and the live demo is untouched.

Core proof: `test-events.py --phase inventory_grid`; UI proof:
`tools/live/check.ps1 -Phase 5 -Step inventory-grid-ui -Package mg-ui -Filter inventory -Jobs 1 -NoUiDebug -UiUnoptimized`;
picture uses the same guard with `-Picture live_inventory_grid_snapshot`.

Final inventory gate: 104 mg-live unit tests plus the private-fixture test,
six inventory-focused UI tests, the 83-operation OpenAPI snapshot, and all-target
Clippy for mg-live and mg-ui passed. The native proof and the current panel
picture were inspected independently; these do not substitute for connected-PC
acceptance. The library already uses the shared `live_aim` placement helper
(integration commit `a569ace`), so no second placement path was added here.

Third-acceptance task ownership and cleanup are recorded in
`target/test-output/live-accept3-game/files.json`. The six final native phases
retain their proof manifests/state, source inputs and required pictures. Closed
private-server logs/settings/caches, superseded unequip diagnostics, temporary
Git indexes and four reproducible inventory UI modules were removed; shared
Cargo caches and the owner's demo were preserved. The ledger records logical
bytes removed; concurrent work prevents attributing a free-disk-space change.
Session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a` is registered there for separate,
approved history cleanup after this conversation closes. Its active history was
not modified; no supported history-retention procedure has been approved here.

Skill Check now automatically offers a selected PC or player-radius mode using
the selected object as centre. Both messages are optional. In one-player mode
the radius control is disabled. Returned rows and Live Log still use the same
result formatter. See [the exact wire forms and client limits](live-ops-wire.md#skill-check-one-pc-or-a-radius-b).

The shared right-pane docking exposed Library capture controls outside the
visible pane. `before-library.png` reproduces that clipping; bounded entry width
and wrapping action rows make Capture/Refresh visible in `after-library.png`.
Inventory actions/rows also wrap. The fixture now supports both floating and
docked tools and explicitly reselects its requested tab after Live activation
queues the default tools; it no longer unwraps a nonexistent floating surface.

Proof, without application builds or demo control:

```powershell
& '<PowerShell 7>/pwsh.exe' -NoProfile -File tools/live/check.ps1 -Phase 5 `
  -Step dm-windows -Package mg-ui -Filter hexxar_ -Jobs 1 -NoUiDebug -UiUnoptimized
& '<PowerShell 7>/pwsh.exe' -NoProfile -File tools/live/check.ps1 -Phase 5 `
  -Step dm-after -Picture hexxar_followup -Jobs 1 -NoUiDebug -UiUnoptimized
```

Seven fake-store app checks pass: four existing VFX/gallery/library/skill checks
and three owner regressions for runtime freshness, exact PC/optional-text skill
checks, and inventory read/give/confirm-take/target-change/player protection.
The tests assert exact queued JSON, delivered results, and no unsaved module edits.
Screens use a fake fresh store, an authored module and read-only installed icon/
skill resources; one GPU device at a time, no game/client/service process.
Reviewed PNGs in `target/test-output/live-windows/dm-pictures/`: before-library,
after-library, after-skill and after-inventory. They prove window layout and the
potion icon, not the black fixture area's scene or any native-client visuals.
Native inventory behavior is checked separately by `test-events.py --phase dm_inventory`.

The owner's running executable has deliberately not been rebuilt. The coordinator
owns its menu wiring; the available Action/Tab names above and PC inputs in the
[operations contract](live-ops-wire.md#dm-force-player-orders-and-new-player-actions-h)
are the handoff. Owner acceptance of a rebuilt app remains separate from these
tests. Main target is shared, not an artifact owned by this task.


## Fourth acceptance UI audit preparation

The named view items are implemented separately: Live Log default visibility
9575255; common single/group context menus 880c0b1; Scenes workflow 7e88576;
world-kind colours c31504c. Details and guard commands are in
[the fourth UI follow-up](live-mode-next.md#fourth-acceptance-ui-follow-up-2026-10-13).
These are synthetic UI checks, not a new owner acceptance or a demo restart.

The common audit fixture opens each tool in the same right dock at 1600x1000,
with a selected injured NPC and a player. It does not modify the module or send
to a real server. Inventory's automatic read remains pending in the fake store.
Run guarded Phase8/Next/Step accept4-window-baseline/UiTest live_accept4_windows,
Jobs1/NoUiDebug/UiUnoptimized, with MGLIVE_POLISH_PICTURE=before (after for the
final pictures). No picture environment means a non-writing window smoke test.
Pictures are live-next/visual/polish-<tab>-before.png; one run passed and all ten
were inspected. This is the baseline, not completion of the requested polish.

| Window | Concrete audit finding / remaining polish |
| --- | --- |
| Live Panel | Dense controls; help text extends horizontally. Group hints into a compact help section and add an explicit empty/filter state. |
| Selection | Selected rows work; an empty selection needs map selection instructions instead of only click-to-focus guidance. |
| Live Chat | Empty conversation is completely blank and Send remains an unexplained action. Add a title, empty hint and send prerequisite feedback. |
| Live Log | Empty filtered view has only a zero count; title and empty-state/filter guidance should identify the tool even when its tab is scrolled out. |
| Scenes | New Record/Review/Replay flow is already covered by populated, empty and file-menu pictures; include it in the spacing review. |
| Inventory | Automatic-loading feedback is present after B4/B5. Recheck filled native-icon and item-menu states after Astra2's remaining inventory work. |
| Skill Check | Current radius/selected-player controls are readable; coordinate final send/aim wording and disabled reasons with Astra2's pending skill changes. |
| VFX | Long constant names wrap heavily in the docked list; preview controls overflow horizontally. Recheck after Astra2's VFX/appearance changes. |
| NPC Library | Empty library has no next-step explanation or window heading. Storage path takes the first line; move detail into a tooltip. |

Final per-window edits are ordered after Astra2's relevant changes land, to avoid
editing the same windows concurrently. The guard's appearance-picture filter is
Astra2-owned; audit UiTest entries are Astra-owned. Options and floating Live
properties/tile dialogs remain to include in that final pass. The ownership
ledger is target/test-output/live-next/accept4-ui-owned-files.json.

The B8 gallery is now reachable for a selected placeable through Look and effects >
Appearance. The shared menu chooses Creature/Placeable from the primary eligible
selection. `check.ps1 -Phase 8 -Next -Step accept4-appearance -UiTest
live_accept4_placeable -Jobs 1 -NoUiDebug -UiUnoptimized` passed; the paired
`accept4-appearance-entry-{before,after}.png` pictures were inspected. Opening the
gallery queues no command and leaves the module unchanged.

### Selection window polish

Selection has a persistent title, selected count and an empty-state map gesture
hint. Populated rows retain portraits and click-to-focus. The common audit checks
the empty state and unchanged module; set MGLIVE_POLISH_WINDOW=liveselection to
limit capture to this tool. Paired standard-dock pictures and an empty-state
picture are under live-next/visual/polish-liveselection-*.png.

### Live Panel polish

Live Panel keeps its title visible, wraps the area/report toolbar, and places
filter/selection help in a compact disclosure. Empty results explain both chips
and toolbar kind visibility. The common audit clicks Wounded (zero matches),
then Clear filters (two rows); paired panel pictures are inspected at the same
dock width. Run the audit with MGLIVE_POLISH_WINDOW=livepanel.

### Searchable Give Item picker (fourth acceptance B9)

Live Inventory > Give item opens a searchable palette with item names, resrefs,
category paths, source (Module/Standard), and native item icons. All search words
must match somewhere in those fields. Module entries win duplicate resrefs, as
resource lookup does; the picker uses the loaded module and its resource stack,
which must match the server. Double-click an item, or select it and press Give.
Advanced: item resref retains a fallback for server-only resources. Closing or
changing the live holder invalidates the picker context. This queues the existing
`give_item` command, waits for its result, then reloads the inventory.

Reproduce with `tools/live/check.ps1 -Phase 5 -Step inventory-picker -Package
mg-ui -Filter inventory -Jobs 1 -NoUiDebug -UiUnoptimized`; the authored-palette
app tests cover category/name filtering, double-click, exact target/resref, Give,
and selection changes. Render with the same guard and `-Picture
live_inventory_grid_snapshot` (omit Package/Filter). The installed standard-palette
picture is `target/test-output/live-windows/inventory-grid-picture/inventory-picker.png`.
Native Give remains covered by `test-events.py --phase inventory_grid` (B7),
including the item returned in the queried inventory; no new game operation is
introduced by this picker.

### Live Chat polish

Live Chat has a title and conversation empty state; its recipient/volume choices
wrap inside the dock. Send explains missing text, selection or player permission
and Enter uses the same readiness guard. The queue still validates commands.
The common livechat audit checks empty Send disabled and an NPC message enabled
without sending; paired pictures use MGLIVE_POLISH_WINDOW=livechat.

The existing speech/broadcast app regression also passes (UiTest live_chat).

### Live Log polish

Live Log now identifies itself with a title, explains an empty filtered or paused
view, disables Copy with a reason when nothing is visible, and exposes truncated
rows in a hover tooltip. The common livelog audit checks the empty state and Copy;
paired pictures use MGLIVE_POLISH_WINDOW=livelog. Event caching is unchanged.

### NPC Library polish

NPC Library presents the capture/select/aim flow, an empty-state next step and
a compact module label (storage path on hover). Actions explain their prerequisites;
Capture requires one NPC and a valid name. Thumbnail size fits its column.
The library audit uses its own empty data path and checks Capture/Spawn eligibility.
Paired pictures use MGLIVE_POLISH_WINDOW=livelibrary; no capture is sent.

### Skill Check polish

Skill Check wraps its skill/DC/radius controls, labels the selected player as a
recipient, and explains disabled actions on hover. Detailed Roll rules collapse
behind help; the main action states whether it invites one player or starts map
aiming. The common liveskill audit checks NPC radius mode and disabled player mode;
paired pictures use MGLIVE_POLISH_WINDOW=liveskill. No game/NUI behavior changed.

### Valid inventory item actions (fourth acceptance B6)

Item right-click menus show only allowed equipment slots; dragging a potion onto
Head sends no equip. Cast abilities appear under Use with a self button and
same-area recipient choices. Give to transfers the selected existing item (not
a new blueprint) to another eligible creature; player recipients require Players.
Take retains its destructive confirmation, Drop its ground confirmation, and
Unequip appears only for ordinary equipped slots. Unknown use metadata is not
guessed. Native validation remains authoritative for class/feat restrictions.
The `inventory_actions` engine phase proves consumption/healing/transfer; app
`-Filter inventory` tests exact commands and menus. The standard-icon
`inventory-use.png` picture is produced by `live_inventory_grid_snapshot`.

### Live Properties polish

Transform fields display units and disabled static-placeable controls explain
the restriction. Apply/Reset explain their immediate server effect; Reload Report
is available separately, including for static objects, and explicitly discards
only the unsent draft. The dialog audit opens/closes it without queuing a command.
Paired pictures: live-next/visual/polish-properties-{before,after}.png.

### Live Tile Properties polish

The initial Tile Properties position sits below the painting banner, keeping its
title and close button visible. Apply and Restore explain their effects; unavailable
Restore explains that no earlier tile was observed. The dialog audit opens the
Advanced fields and closes painting without sending or modifying the module.
Paired pictures: live-next/visual/polish-tiles-{before,after}.png.

### Live Options polish

Connection and named-client controls wrap at narrow widths. Empty clients explain
Add; token generation/removal describes draft-versus-Apply behavior. Copy token,
Add at the client limit, and chat logging expose disabled reasons. The dialog
audit captures Options and expanded Named clients without applying anything.
Paired pictures: live-next/visual/polish-{options,clients}-{before,after}.png.

### Inventory polish

After B6 release, Inventory gains a consistent header/divider, an empty-state next
step and pending/permission/amount explanations for disabled controls. Confirm Take
clarifies destruction. Existing inventory action app tests guard command semantics.
The picture fixture retains B6 baselines in live-windows/inventory-grid-picture;
MGLIVE_POLISH_PICTURE=after writes the current pictures to
live-next/visual/inventory-after instead. No native action logic changed.

### Item picker polish

The Give Item window explains double-click versus select/Give, empty palette and
no-match states, and disabled Give. Category paths truncate with full hover text,
and the selected-item footer wraps. Reuses the B9 search/target tests and the
inventory-after picture destination; the accepted B9 picture is the before image.

Inventory picture review caught unnecessary footer reservation clipping the second
backpack row after the header divider. The scroll area now reserves only its
visible footer/confirmation space; the revised standard-dock picture shows both
rows in full. The same picture check passes after this correction.

The final real-resource Panel regression exposed an older selector collision:
cf6ec05b used a unique Players label; da1288c later added the same label to the
map legend. The test now selects the Panel button by role, retaining the actual
filter/follow/picking assertions. No UI behavior is bypassed.

Final all-target UI lint found B6 9d65229 added an eighth cell argument. A private
CellContext now carries holder/recipients/readiness together; the same item
actions are retained, with no lint suppression. Inventory app tests rerun for it.

### Fourth-acceptance polish verification and handoff

Run from the repository in PowerShell 7. Each command uses
`tools/live/check.ps1 -Phase 8 -Next -Jobs 1 -NoUiDebug -UiUnoptimized`
plus the arguments below; no application build or demo lifecycle action is needed.

| Check | Additional arguments | Verified result |
| --- | --- | --- |
| View/menu regression | `-Step polish-review -UiTest live_review` | 24 passed |
| Menu, layout and window audits | `-Step polish-accept4 -UiTest live_accept4` | 7 passed |
| Panel selection/follow | `-Step polish-controls -UiTest live_panel` | 2 passed |
| Named clients | `-Step polish-controls -UiTest live_clients` | 1 passed |
| Chat delivery | `-Step polish-chat-send -UiTest live_chat` | 1 passed |
| Inventory actions and picker | `-Step polish-inventory-final -Package mg-ui -Filter inventory` | 8 authored-fixture checks passed; install-dependent cases return early under this guard |
| UI lint | `-Step polish-lint -Package mg-ui -Lint` | All targets, warnings denied, passed |

For dock captures, set `MGLIVE_POLISH_PICTURE=after` and run the window audit;
`MGLIVE_POLISH_WINDOW=livechat` (or another lowercase Tab debug name) limits it
to one window. The dialogs audit captures Properties, Tiles, Options and clients.
Inventory uses the same environment variable with
`-Step polish-inventory-fit -Picture live_inventory_grid_snapshot`.
Remove those process environment variables when finished. Pictures and logs are
under `target/test-output/live-next`; the exact owned paths and retained evidence
are in `accept4-ui-owned-files.json`. Existing area and authored inventory inputs
are reused; no module/resource copy per attempt is needed.

These checks prove UI layout, eligibility, queue payloads and draft/file behavior
with fake reports. Connected-client effects, animations and the owner's comfort
using the new menus still need owner acceptance. The running demo was untouched;
both normal application binaries retained their recorded hashes and timestamps.
VFX was initially reserved for Astra2, then released as 863ef0d and polished as
described below. Its connected-client limitations remain separate from UI proof.

Cleanup closed five owned, superseded outputs: the animation popup diagnostic,
two early successful audit logs and duplicate after-captures of unchanged inventory
context/use menus. Their exact paths are marked removed in the ledger; the original
B5/B6 menu pictures and final inventory grid/picker/loading pictures remain.
Removed size is 1,502,762 logical bytes; no on-disk allocation or free-space delta
is attributed to this work. Required before/after pictures await review, and shared
builds, inputs and other workers' files remain untouched. Session
`01a12fe8-1458-7551-99fc-b277832def84` and its exact active history path are recorded
in the ledger for separate authorized cleanup; the active history was not edited.

### Live VFX polish after B10 release

The released window has a title and a short select/apply/aim flow, no-match guidance,
compact effect rows with full hover text, and a preview animation selector that
fits its column. Its compact mannequin toolbar omits resource statistics while
retaining preview controls; regular model viewers keep their detail. Apply/Clear
explain unavailable targets on hover and use the same
eligibility helper as command submission. Ground Apply remains available without
selected objects; Clear still requires an eligible selection. Selection names are
on the count's tooltip. B10's effect transform controls and proven colour limitation
remain intact.

The common window audit covers selected-object availability, empty selection,
ground mode without a selection and empty search results. Reproduce with the
common guard arguments and `-Step polish-vfx -UiTest live_accept4_windows`, setting
`MGLIVE_POLISH_WINDOW=livevfx` and `MGLIVE_POLISH_PICTURE=after` for the capture.
The released baseline is `visual/polish-livevfx-b10-before.png`; the final picture
is `visual/polish-livevfx-after.png`. The older pre-B10 baseline remains a separate
historical capture until this final window review is complete. The real-resource
window audit and authored fake-store VFX apply/clear/transform payload test pass;
the paired pictures were inspected after the final compact-toolbar correction.

### VFX transforms (fourth acceptance B10)

Scale, native Offset XYZ and Rotation XYZ are sent with every object/ground
application. Values are bounded in Rust and the game. The stock-colour tooltip
records the negative attached-material propagation test with a working object
control; no unsupported colour slider is shown. Native proof and retained
client inputs are in [the wire contract](live-ops-wire.md#vfx-transforms-and-colour-fourth-acceptance-b10).
Guarded `-Package mg-ui -Filter vfx` passes; `-Picture live_vfx_transform_snapshot`
writes the inspected dock picture under
`target/test-output/live-windows/vfx-transform-picture/vfx-transform.png`.

### Session faction pane (fourth acceptance B15)

The existing Factions editor now offers Basic/Advanced/Live session. Basic can
reserve neutral global Session 1..N factions in one undoable FAC edit. The Live
pane stores boot/module-bound aliases in memory and sends explicit assign/read/
set commands for the selected creature. Its native direction and empty-faction
limitations are explained in the pane. No new Action/Tab is needed; set
`faction_view.live_mode` when opening `Tab::Factions` directly to the live pane.
See [native contract and proof](live-ops-wire.md#session-factions-fourth-acceptance-b15).
Authored app checks cover reserve/undo, labels without FAC changes, exact command
payloads, asynchronous result, and label reset on boot change. The screens test
`live_session_factions_snapshot` captures both panes under
`target/test-output/live-windows/session-factions-picture/`.

Fourth-acceptance B15 checks: guarded mg-live 111 passed (4 opt-in external
checks ignored), authored session pane app check passed. The private engine
verified changes 10 -> 75 -> 30 and reverse 50 -> 20, then forward 80 for both
members. No connected-player persistence claim is made. Inventory and VFX were
released to Astra1; her follow-up polish is separately committed as 53756b7 for
VFX. No application or owner demo was rebuilt or controlled by this work.

Final B15 module test, updated app test (including native-error values), and
mg-live/mg-ui all-target Clippy with warnings denied pass. The final offline/live
pictures were inspected; wrapping the faction sidebar fixed a clipped help line
and button row seen in the first capture. Each picture was replaced in the same
owned output location. Real installed resources were read only for this capture;
commands still used the fake store.

## Fifth acceptance GUI (2026-10-14)

C2: Selection and Chat now fall back from an unset/missing portrait to the
reported `app` row's `appearance.2da PORTRAIT`. Exact custom bases stay first;
stock defaults such as `po_poly` need an underscore before the size suffix.
The loader tries s, m, t, l, h and retains the normal decoded-image cache.
Chat's cached row now carries appearance even when the portrait field is absent.
Missing images explain both sources on hover. Stock player-body rows 0–6 have
no default portrait; they cannot supply an appearance fallback.

The reported beholder failure is **not reproduced on this head**: `po_behold_`
already displays in both baseline pictures. Stock `po_behold_s/m/t/l/h.tga`
all exist in `key:nwn_base` and the s/m images decode. No guessed replacement
portrait or resource-cache invalidation was added. A read-only request to the
running demo refused the saved viewer token, so its current payload was not
inspected; the owner's observations remain distinct from this resource proof.

Reproduce with `MGLIVE_ACCEPT5=after` and
`tools/live/check.ps1 -Phase 4 -Next -UiTest live_accept5_portraits -Jobs 1 -NoUiDebug -UiUnoptimized -Step c2-after`.
The fake-store app test passed and sent no commands. Inspected pictures are
`target/test-output/live-accept5/{selection,chat}-portraits-{before,after}.png`:
stock parrot (appearance 7, unset `po_`) and beholder (appearance 401).
Inputs are the installed game resources, an authored 4×4 rural module, and
`tests/support/live_accept5.rs`. No demo lifecycle/app binary operation occurred.
Working-file ownership and protected executable hashes are in
`target/test-output/live-accept5/owned-files.json`; session
`01a12fe8-1458-7551-99fc-b277832def84`. Final cleanup is recorded there when this
acceptance batch closes; active conversation history is outside cleanup scope.

## Fifth acceptance C1: confirmed DM Drop

The potion's `droppable:false` flag no longer disables explicit DM Drop. Plot
items remain protected. Disabled item-menu headers and actions explain pending
inventory requests, player permission, or plot protection on hover. Drag/drop,
slot masks, native Use and Give-to retain their existing behavior.

The fake-store regression `inventory_nondroppable_potion_has_a_confirmed_dm_drop`
reproduces the disabled menu, then proves the enabled action sends nothing until
confirmation and sends the exact holder/item IDs after it. Six inventory-grid
checks pass, including drag equip, native Use and transfer. Before/after pictures
were inspected at `target/test-output/live-accept5-astra2/drop-before.png` and
`drop-after.png`. [Native potion/flag proof](live-ops-wire.md#fifth-acceptance-dm-inventory-drop-c1).

## Fifth acceptance C3: the actual item palette tree

Give Item now displays collapsible **Standard** and **Module** roots and the
`Palette.nodes` hierarchy returned by the same cached palette loader as the
Palettes window. Category names, nesting and order come from the palette; no
resref/category inference or alphabetical category reordering is performed.
Module overrides retain priority over the corresponding standard resrefs.

Search matches name, resref, source and the full category path. Matching branches
open while searching, without overwriting the normal collapsed state. Item icons,
select/Give, double-click Give and the advanced resref fallback remain. Closed
branches do not resolve icons, and offscreen item rows reserve layout without
loading blueprints.

Three picker app checks pass: exact item commands on double-click and Give,
selection-change safety, and real authored palette parent/child expansion plus
search through a collapsed branch. Pictures were inspected:
`target/test-output/live-accept5-astra2/picker-before.png` and `picker-after.png`.
These use an authored palette/fake store; no native operation or demo data was
changed by the grouping. The existing Give Item game operation is unchanged.

## Fifth acceptance C8: module faction menu

Faction moved from Combat to Character. The list comes from the current module's
faction document (including unsaved edits), overlaid with the boot-scoped session
names used by the faction window and legend. Numeric IDs are sent unchanged,
excluding row 0 (the player faction). No fixed Hostile/Commoner/Merchant/Defender
list remains. The module must match the server, as for appearance resources.

`live_accept5_factions` passes: the authored module contains custom row 5 `test`
and reserve row 6, the session alias displays `Session 2`, and clicking it queues
`set_faction` with numeric ID 6. Inspected `factions-{before,after}.png` in
`target/test-output/live-accept5`. [Astra2's separate native and Rust proof](live-ops-wire.md#fifth-acceptance-custom-module-factions-c8)
landed as `18fad55`; this GUI test uses a fake store, not the running demo.

## Fifth acceptance C5b: playable scene example

Try it creates an unsaved two-step scene for the one selected living NPC: say
Hello and wait for success (5 s limit), then after 1 s walk 5 m and wait within
1 m of the destination (15 s limit). Near the area's east boundary it chooses
5 m west. Players, dead NPCs, ambiguous selections and dirty/playing/recording
drafts disable the example with a reason. Creation sends nothing; Play uses the
existing Scene Player and normal command queue. Rows display names, spoken text,
coordinates, delay and wait. Native navigation may still refuse the walk.

`live_accept5_scene` passes through example creation, Play, the exact Say command,
a fake success reply, the exact Walk command and a reported arrival; Player ends
successfully with two steps. The picture review caught a decimal timeout rejected
by the integer schema before commit; the example now uses integer 15, and the test
plays it rather than checking labels alone. Inspected
`target/test-output/live-accept5/scene-example-{before,after}.png`.
Run `check.ps1 -Phase 4 -Next -UiTest live_accept5_scene -Jobs 1 -NoUiDebug -UiUnoptimized`.
This is fake-store UI/Player proof, not an owner or native-game acceptance.

## Fifth acceptance C6/C7: complete area picker and tile destination map

Teleport to Area unions reported server areas with every ARE resource in the open
module, matching the module tree. It reads dimensions from the module when the
area is unreported, and starts at its centre. Looking only at Mod_Area_list could
miss newly edited resources while the workspace's module-info document was cached;
the regression fixture caught this. Resource enumeration now matches what is open.

The map uses the existing `mg_module::minimap` tileset renderer, cached by area and
workspace revision. It overlays 10 m grid lines, corner coordinates, blue current
positions (in the source area), green landing ghosts and source-area coordinates.
Only clicking the map sends; hovering, switching areas and editing values do not.
The map describes the open module, so runtime tile edits may differ; the UI says so.
Remote reported areas absent from the module retain the coordinate grid fallback.

`live_accept5_teleport_map` passes with one reported area and a different unreported
module area: both are selectable, a real tile map appears, no command is queued
until the map click, and the command uses the unreported destination's resref.
Inspected `teleport-map-{before,after}.png`, `teleport-areas-after.png` and
`teleport-unreported-after.png` under `target/test-output/live-accept5`.
Run `check.ps1 -Phase 4 -Next -UiTest live_accept5_teleport -Jobs 1 -NoUiDebug -UiUnoptimized`.
No module edits are made by the teleport UI; this is queue/map proof, not a native
cross-area landing test or owner acceptance.

## Fifth acceptance C4: Full effects and event particles

Full is now the per-view default. Existing attachments on NPC and player looks
remain size-aware. Transient spell events resolve Cast/Conj visuals and ProjModel
from the loaded spells.2da. A verified stock Fireball ImpactScript containing
`EffectVisualEffect(VFX_FNF_FIREBALL)` resolves its impact through visualeffects.2da;
other scripted impacts are not guessed from their name. Explicit VFX rows resolve
through the same table. Each cast/projectile/impact has its own simulation and
local time; the existing 64-model/20,000-quad caps and 40 ms repaint bound remain.
Hidden/disabled Live views do not animate stale cues.

Two renderer causes were fixed: stock `vff_explfire` uses animation-level
`event ... detonate` entries, whereas Explosion emitters only listened to node
controllers; and steady-state base-emitter warm-up aged a new impact 20 seconds
before it could be drawn. Animation detonation now includes time zero, handles
loop crossings without replay on redraw, and transient models skip warm-up.
Persistent ambient placeables keep their warm-up. Unit coverage proves time-zero,
future, repeated-time, non-detonation and next-loop behavior.

Ground Apply VFX commands from this view are correlated by command ID. Only a
successful reply creates the cue, with the exact submitted row/location/duration/
transform; refusal creates none. The native ApplyVfx path currently emits no
location-bearing visual event and its result omits location/duration. That gap is
recorded in both mailboxes for Astra2. Consequently remote ground commands and
already-existing permanent ground effects cannot be reconstructed after reopening
the view. No synthetic server event or inferred spell success is claimed.

Proof uses the installed stock resources and the fake-store area harness:
`live_accept5_effects` shows Freedom of Movement's green particle with NPC/module
markers hidden (Off comparison has zero live particles), a Fireball cast glow and
flaming projectile, its impact burst, and ground row 22 after clicking the map and
receiving a success reply. Inspected pictures in `target/test-output/live-accept5`:
`lasting-effects-{before,after}.png`, `lasting-effects-off-after.png`,
`spell-{projectile,impact}-{before,after}.png`, and
`ground-vfx-{markers-after,after}.png`. The ground Markers comparison is a control
on the final tree, not a claimed pre-change native screenshot. All are GUI resource
proof, not an owner two-monitor or native timing/particle-parity acceptance.

Guarded checks (`-Jobs 1 -NoUiDebug -UiUnoptimized`, no app builds):
- `-UiTest live_accept5`: 6 passed (all fifth-acceptance GUI checks).
- `-UiTest live_effects`: 4 passed, including player-look attachments and ambient
  warm-up versus effect arrival; `-UiTest live_scene`: 1 passed.
- `-Package mg-ui -Filter live_cues`: 4 passed (ack/refusal/expiry/reboot included).
- `-Package mg-render -Filter particles`: 11 passed.
- `-Package mg-ui -Lint -CheckFmt`: all-targets clippy with `-D warnings` passed.
  This package-specific guard does not run fmt; direct rustfmt `--check` passed
  for every GUI/renderer/test file changed in this batch.
Logs are `target/test-output/live-next/phase4-{c5-final,c4-regression,c5-scene-regression,c4-cues,c4-particles,c5-lint}-*.log`.

Reproduce pictures with `MGLIVE_ACCEPT5=after` and
`tools/live/check.ps1 -Phase 4 -Next -UiTest live_accept5 -Jobs 1 -NoUiDebug -UiUnoptimized`.
The authored rural fixture is regenerated in `target/test-output/ui-area-live-accept5`.
The original baseline was captured before production changes; `MGLIVE_ACCEPT5=before`
now selects comparison controls and must not be mistaken for rebuilding that baseline.


## Fifth acceptance GUI handoff and retained evidence

GUI commits: `f324de7` (C2 portraits), `b8cd4e1` (C5a shaders), `24b7ea7`
(C8 faction menu), `2e08439` (C5b scene example), `c8f9100` (C6/C7 maps),
`c0c3645` (C4 Full/event effects). Guarded mg-render all-targets clippy with
`-D warnings` also passed (`phase4-c4-render-lint-clippy.log`). The six app checks,
four effects regressions, scene regression, four cue units, eleven particle units
and owned-file formatting checks passed; no full workspace gate was claimed.
Both protected release executable SHA-256 hashes match the task-start ledger.
No running-demo lifecycle operation or application build occurred.

The baseline/final logs, review PNGs, stock game inputs and committed test sources
are retained. Removal of 11 owned temporary paths (160,302 logical bytes: prepared
edit JSON, superseded combined test, regenerated module and intermediate logs)
was rejected before execution by automatic approval review: `blocked by policy`.
Nothing was removed, and no disk-space recovery is claimed. The exact absolute
paths, sizes and ownership are recorded in
`target/test-output/live-accept5/owned-files.json` and the coordinator handoff.
The deletion was not retried through another mechanism. Sources and minimal
review evidence remain usable without those temporary paths.

The owner still needs to verify the new build on two monitors. The reported
beholder portrait failure was not reproduced; appearance rows without a default
portrait still have no invented face. Remote/permanent ground-effect reconstruction
awaits the native location event; non-Fireball scripted impacts remain marker
fallbacks unless an explicit VFX row is reported. The destination map is the open
module's map, not a claim that runtime tile changes match it. Session
`01a12fe8-1458-7551-99fc-b277832def84` history was left untouched and identified
for separate, explicitly authorized history cleanup in the mailbox.

Fifth-acceptance artifact closure (Astra2 session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`): retained source, current generated HAK/client
module, before/after pictures, native proof manifests and exact compressed client
dependency packages. Removed own draft copies, discarded blueprint JSON, expanded
rootless package cache and duplicate closed-client/server scratch assets:
36,498,854 logical bytes (34.81 MiB). Receipts and ownership ledger are in
`target/test-output/live-accept5-astra2/`. No shared build cache, running demo,
other worker's data or session history was removed. Allocated/free-space gains
are not attributed while other tasks build. Session-history cleanup needs a
separate supported/approved process; no active history is modified.


## Focus window implementation (2026-10-14)

`Tab::LiveFocus`, commands `Id::LiveFocus` (`live-focus`, Live > Focus…), and
`Settings.no_live_focus` (false by default) integrate with the right Live pane.
Options uses its normal draft/Apply/Cancel flow. Single/group context menus add
all selected members without applying mutation eligibility to observation.
`live_focus.rs` owns bounded watch state; `live_focus/window.rs` draws the two
columns and uses the shared `live_chat::target_order` path plus the existing scene
queue for replies. No new key binding or native server lifecycle action.

Watch files use the saved module path, a readable stem and stable FNV-1a path
hash to separate modules with the same basename. JSON is written aside then
renamed. A malformed/oversized existing file is preserved and cannot be silently
overwritten. Boot/id matching verifies kind, PC flag and tag; fallback tags must
be unique, and PC names must agree. Runtime portraits/HP/area update per report,
not by cloning every area each rendered frame. Event rows are bounded to 500 per
entry (up to 100 entries, 256 members each), queried since the previous cursor.
No new repaint timer is introduced: yellow tab/count alerts use report wakes.

Native contract `226f329` reports source position, recipient and audience, but
no event-time listener snapshot. Exact proximity is therefore unavailable. The
UI uses an explicitly labelled 20 m estimate from source event position and the
latest watcher report, freezes it at ingestion, and does not retroactively infer
nearby history before adding/restoring a watch. This is a UI filter, not proof
of hearing or delivery. The coordinator was offered exact-snapshot versus
labelled-estimate choices asynchronously; the recommended estimate is the stated
implementation assumption unless superseded. Direct tells and opt-in `dialog`
source/target events do not use the proximity estimate. Dialogue text remains
node text, without a claim about client token substitution.

Astra2's native call-path finding puts `SendMessageToPC` outside the NWNX Chat
hook. Focus converts successful `pc_message` results into explicitly labelled
DM command echoes, including mode-independent feedback; it does not fabricate a
native chat event in the store. Successful give/take/equip/unequip/use/drop/
transfer inventory commands similarly appear as acknowledged inventory actions.
Ordinary game looting is not reported and is stated as a capture limit.

Reproduce with PowerShell 7, all Cargo through `tools/live/check.ps1`, Phase 8,
Next, Jobs 1, NoUiDebug, UiUnoptimized. `-UiTest live_focus` runs the fake-store
corpus workflow/pictures; `-Package mg-ui -LibraryOnly -Filter live_focus::tests` checks event
routing, frozen history, ambiguous/reused IDs and reply policy; `-UiTest live_chat`
checks the shared composer regression; `-Package mg-ui -Lint` checks all targets.
Current evidence stays in `target/test-output/live-focus/`; the single reusable
corpus fixture is `target/test-output/ui-area-live-focus/`. Baseline Chat/menu
captures precede code edits; the existing `live-next/visual/polish-options-after.png`
is the Options baseline. Tests/pictures are UI proof, not two-monitor acceptance.


Focus verification findings: editable menu fields need
`PopupCloseBehavior::CloseOnClickOutside`; otherwise clicking Rename's text
field closes the menu before it can be edited. The app test now types the new
name, presses Rename, restores a fresh window instance from disk and removes a
watch through its actual menu. Test fixture fixes use strictly increasing report
sequences and distinguish the Focus menu button from the window heading.
The identity regression also rejects speech from a reused id while its original
watch is unresolved; final death/leave events for a disappeared same-boot actor
remain eligible. Existing corrupt watch files are never overwritten on load
failure. No actor command is issued by adding, renaming, removing or centring.

The guard now accepts `-LibraryOnly` with `-Package` for focused unit tests,
avoiding unrelated integration binaries while preserving the NWN_ROOT guard.


Focus UI implementation commit: `0678393`. Final guarded checks pass: **6 unit,
3 corpus app and 1 Live Chat regression**, plus all-target mg-ui Clippy and
owned-file formatting. Current app pictures were inspected against the retained
baseline. `phase8-focus-final*`, `phase8-focus-chat-live_chat.log` and the baseline
log under `live-next` are the retained checks. No full workspace/native-client
acceptance is claimed by the UI task. Native contracts/proofs belong to Astra2's
`226f329` / `29d4463`; the active demo still runs its previous scripts/build.

The exact source/evidence ledger is `target/test-output/live-focus/owned-files.json`.
One current app fixture and seven before/after pictures remain for owner review;
no whole game/module resource copy was created. Release moonglow.exe and mg.exe
hashes match the protected fifth-acceptance baseline. Session
`01a12fe8-1458-7551-99fc-b277832def84` remains active; its history is untouched and
registered for separate supported, approved cleanup after closure.

Cleanup removed three identified superseded UI check logs, **4,069 logical
bytes**. No disk-allocation/free-space gain is claimed. Peer native evidence,
current fixture/data, seven review pictures and final checks remain. Earlier
fifth-acceptance cleanup blocked by automatic review is not retried here.


## Sixth acceptance UI (2026-10-14)

Owner dropped coloured effects following the native propagation proof.
The GUI removes the shader filter, row metadata and all colour controls; ordinary
VFX selection/Apply/Clear, transforms, duration and preview remain. Legacy
MGLIVE_COLOUR sentinel rows are hidden from older resource sets. The shared
resource-reload invalidation remains useful independently of the removed filter.
Removed obsolete filter/colour tests and replaced them with an app regression
checking absence and ordinary command submission. Baseline is the retained
live-vfx-parameters/after.png; current picture is live-accept6/vfx-after.png.
Guarded `-UiTest live_accept6` passes (1 test at this checkpoint); picture inspected.
Astra2 owns native/helper packaging removal. No demo or app binary operation.


E7 portraits: a read-only Redis subscription captured current demo full/delta
reports in `live-accept6/demo-reports.jsonl`: 33 creatures have `po_` and appearance
6, one has `po_ha_m_07_`/appearance 3. The installed appearance rows 3 and 6 have
no PORTRAIT default; row 7 (the earlier test) has po_poly. The Beholder row 401
has po_behold and po_behold_s loads correctly: its question-mark symptom does
not reproduce with these installed resources. When both reported and appearance
portraits are absent, use the installed game's po_unknown_m fallback. This is
a default image, not an inferred likeness. Explicit/custom portraits and the
appearance default still take precedence. DDS/TGA order and the shared image
cache are unchanged.

Reproduction: guarded check.ps1 -Phase 8 -Next -Step accept6-portraits-after
-UiTest live_accept6 -Jobs 1 -NoUiDebug -UiUnoptimized (2 passed). The app test
projects the actual unset-human report and the owner's po_behold_ regression;
synthetic chat avoids retaining private conversation in the fixture. Inspected
portraits-{chat,focus,selection}-{before,after}.png under live-accept6. Native
po_unknown_m is the game's illustrated fallback. No demo writes/restart/build;
this is resource/UI verification, not fresh owner acceptance.

## D15 removal

The owner removed Live Shaders and its material/player uniform controls. The menu, context entries, window, support fixtures and pictures code are removed. Native animation replacement remains available to agents. Effects cannot be recoloured; native Scale/Offset/Rotation remain.


E13 and multi-area scope: `live_areas.rs` supplies shared Area tabs. Chat/Log
histories offer All and named areas; Chat prefixes All lines with the area and
opens a pinned `Tab::LiveChatArea` via the area tab's context menu. Pinned replies
reject a selection in another area; all-player sends respect the chosen area.
Panel, Selection, Skill, Inventory, VFX and Library share the active-map tabs at
the Tab dispatch boundary (Inventory picker inherits that scope). Two visible
maps no longer assign active scope by rendering order: clicks/tab choices and
explicit opens choose it. Scene steps retain explicit area inputs; Options are
server-wide. Focus and Legend adopt the same control in following commits.
Guarded accept6-areas-final app check passes5 at this checkpoint; inspected
chat-areas-after, chat-area-window-after, area-tools-after. The two-pane regression
holds the selected area across redraws and switches it explicitly.

E14 Focus uses the same All/area tabs for conversations and events. Members
are clickable; Back to group restores the group history and player-only replies.
Each row retains the member IDs matched on receipt, so drilling down after an
area transfer never reclassifies nearby history. Unread clears only for rows
actually shown under the selected member/area/page. Scoped group replies still
exclude NPCs even if only one member remains in that area. Guarded Focus unit
checks6 and acceptance app checks7 pass; focus-areas-before/after and
focus-member-after inspected. The app sends the member reply only to NPC a.

## Inventory context confirmation (sixth acceptance D2)

Drop and Take now open a small confirmation submenu directly beside the clicked item. Confirm Drop sends `drop_item` immediately; Confirm Take sends the destructive `take_item`. The bottom Drop button and detached confirmation rows are gone. Escape/dismissing the menu cancels. Equipment/backpack dragging and native eligibility/disabled explanations remain.

The fake-store app tests verify the exact item/holder command, no command before confirmation, both backpack Drop and equipment Unequip, Take/refresh, and drag-to-equip. Guarded `check.ps1 -Phase 5 -Step acceptance6-drop-after -Package mg-ui -Filter inventory -Jobs 1 -NoUiDebug -UiUnoptimized` passes 14 matching tests across the binaries. Inspected before/after captures: `target/test-output/live-accept6-astra2/drop-before.png` (detached bottom row) and `drop-after.png` (adjacent submenu). Private `test-events.py --phase drop_flags` again proves the non-droppable NPC potion is on the ground at its holder's feet and absent from the inventory; plot items remain refused. UI captures use a fake store and do not claim a connected client check.

E12 moves marker controls to Tab::LiveLegend, opened by Live → Legend or the
area toolbar and included in the default Live tool layout. No legend is painted
over the map. Shared Area tabs identify the controlled view; category/faction
settings survive Live off/on and remain per view. Before reference: retained
focus-areas-before (map overlay); after: legend-window-after. Replaced the old
collapse test with the window/persistence regression; retained the independent
label/colour test. Guarded acceptance app suite7 passes and pictures inspected.

D1 consumes LiveStore::factions() from8a728fc: only server-advertised IDs are
selectable in Character → Faction. Editor-only rows stay disabled with the
save/rebuild/restart explanation. A missing catalogue on an older server never
falls back to editable IDs. Session display labels decorate server names only.
Acceptance fixture7 and the existing fifth-acceptance faction regression1 pass;
factions-server-after inspected (baseline: fifth-acceptance factions picture).
Native catalogue/assignment proof is Astra2's separate engine check.

## Server factions in the session pane (D1)

The Live session pane now uses `LiveStore::factions()` for acting factions, labels and reputation/assignment eligibility. Editor-only or differently named IDs explicitly show **not on the server: save, rebuild and restart the module to use it**. An older server without a catalogue cannot enable these actions. Session labels still do not edit repute.fac. Native proof is `server_factions`; the fake-store regression creates unsaved Session 1, verifies refusal, publishes a server list containing it, then verifies the exact ID in reputation and assignment commands. The guarded session_factions test and live_session_factions_snapshot pass. Inspected captures: `target/test-output/live-accept6-astra2/factions-{before,after}.png`.

E11 following opens a destination map, enables its Live view before drawing,
centres on the reported object and moves shared tool/history scope there. Focus
Show on map starts the same follow path. A ten-second report gap is tolerated;
closed/off views and a changed boot still end follow. Detached Chat stays pinned.
The app fixture moves a watched NPC through a missing-report interval into an
unopened second map; following, camera position and tool scopes remain correct,
with no commands queued. Guarded8-test checkpoint passes; follow-before/after
inspected. Tile geometry still requires that destination in the open module.

E10: Live Chat has Conversation / To DM pages, sharing the area tabs. To DM
keeps PC channel14 and DM channel30 metadata and excludes party and silent AI.
Reply explicitly chooses the PC on that line, clears on reboot and obeys player
policy. Mode dm uses the owner-approved private DM-prefixed server feedback
(Astra2 1f41b84), not an avatar chat packet; receipt/delivery is not claimed.
Guarded accept6-dm-look-delete app suite passes11/11; chat-to-dm-after.png
was inspected against the original portraits-chat-before.png UI baseline.

E4: Delete and DANGER > Remove from game share a counted confirmation of the
eligible selection. IDs and boot are snapshotted; changed/missing targets refuse
confirmation. One queued destroy per non-player target; no module edits.
The before fixture reproduced one immediate removal; after captures a two-object
confirmation and asserts two exact targets. Guarded accept6-dm-look-delete11/11.

E11 follow-up: opening the followed destination now preserves the active tool
tab instead of reapplying the initial Live layout. The same follow test asserts
Focus stays visible through the report gap and transfer (11/11 app suite).

D8: authoritative reported look/hash now rebuilds both matched and extra NPC
previews, including empty or missing Equip_ItemList. Reboot/store replacement
clears the look cache. The native producer/proof is Astra2 dc31430; the UI test
uses installed nw_wswls001.uti (invented model variants initially yielded the
item bag, fixed in the fixture). Equipped -> empty -> equipped -> empty asserts
weapon models for both NPC paths. Guarded accept6-equipment-picture11/11;
inspected npc-look-equipped/unequipped-after.png. The original before is a
single-extra old-consumer baseline; the final comparison uses two NPCs and a
closer oblique camera to expose the weapon. No new client acceptance claimed.

E3: Live Ctrl+C captures all eligible NPC/placeable snapshots through capture_object;
Ctrl+V aims each captured model, preserving relative positions and facing while
Q/E rotates the group. Esc/right click cancels. Ctrl+X confirms the count once,
then removes originals only after every capture and typed spawn snapshot validate.
A failed/missing capture retains originals and the previous clipboard. Clipboard
and pending cuts are module/boot scoped; scripts policy is required for paste/cut,
as spawn_json includes scripts. No editor clipboard or module resources change.
The real-resource fake-store regression covers NPC+placeable ghosts, rotation/
offsets, cancel, failed cut and successful two-target cut. accept6-final13/13;
clipboard-before/ghosts-after/cut-after.png inspected. Native capture/spawn support
comes from Astra2; this is UI/queue proof, not another live-client clone test.

E9/E9b: Scenes calls a scene a macro, lists saved module macros with per-row Play,
click-to-load and Rename/Delete context actions, and keeps Save beside the name.
Save As/Import/Export remain in Scene. Targets use names in a reported-object
picker (IDs on hover/advanced JSON); coordinates round to one decimal. Rename
refuses collisions, delete confirms once, both validate the source is a regular
file inside the module scene folder. Dirty/playback guards stay in force.
Guarded accept6-macros-play13/13 covers row Play/Stop, Save, rename and delete;
accept6-scene-regression passes the existing record/file/edit/play/stop workflow.
Its target query now uses the name picker, and its JSON edit scrolls into view
after the saved list moved it below the fold. The Play/Stop test settles layout
after playback adds its status line. Scenes before/after pictures inspected.

Sixth-acceptance proof retention: one current working set under
`target/test-output/live-accept6/`, authored app fixtures under
`target/test-output/ui-area-live-accept6*`, and guarded logs in `live-next`.
The ownership ledger is `live-accept6/owned-files.json` (session
01a12fe8-1458-7551-99fc-b277832def84). The original portrait before PNGs were
recovered byte-for-byte from this session's tool image payloads after an
accidental baseline overwrite; the helper now preserves existing before files.
The incorrect chat-areas-before duplicate is retired; the original
portraits-chat-before is the Chat UI baseline. No image content was edited.
The staging-only shader-hooks.patch is obsolete after 1d04f4a. Required source,
read-only demo report, before/current pictures and final checks stay for review.

### Sixth acceptance final checks and handoff

All named UI items are implemented; owner/client acceptance remains separate.
Shared area tabs cover Chat (including pinned windows), Log, Focus, Legend and
map-scoped Panel/Selection/Skill/Inventory/VFX/Library. Scenes is module-wide
with an explicit area in each step; Options is server-wide; properties, painting
and teleport drafts retain their originating/destination area.

Guarded commands use PowerShell7, `tools/live/check.ps1 -Phase 8 -Next -Jobs 1
-NoUiDebug -UiUnoptimized`, with no Build/GuiBuild or demo lifecycle commands:

- `-Step accept6-macros-play -UiTest live_accept6`: 13/13 real-resource app cases.
- `-Step accept6-ui-full -Package mg-ui`: 670 passed, zero failed, 18 ignored
  across the full package at c199055. Native/resource cases use the guard.
- `-Step accept6-ui-final -Package mg-ui -LibraryOnly`: final 243 passed,
  zero failed, one ignored, including manual links and unreported macro targets.
- `-Step accept6-regression -UiTest live_review`: 23/23; with live_accept4:7/7,
  live_accept5_scene:1/1, live_chat:1/1, live_focus:3/3.
- `-Step accept6-scene-regression -UiTest live_scene`: record/files/edit/play/stop1/1.
- `-Step accept6-ui-final -Lint -CheckFmt`: workspace all-target Clippy with
  warnings denied and fmt check pass. Local Markdown file targets also exist;
  this is not a claim of an unavailable OKF executable's lint.

The broader fixtures were brought up to date with the accepted changes:
73191b1 requires the server faction catalogue and numeric label; ec3ed7e uses
counted removal; 666be06 moves faction checkboxes to the Legend tab; c33ed75
leaves unseen event-page alerts unread. No production compatibility fallback was
added to satisfy old expectations. Macro summaries no longer expose an unreported
ID in the visible row (advanced JSON/hover retain it). The Areas chapter's old
legend, portrait, effect-default, faction and retired Shader descriptions now
match chapter19 and the current implementation.

Protected release Moonglow, release mg and debug mg hashes match the ledger's
start values. No application executable was rebuilt or launched. The live demo
was neither restarted nor modified. Remaining external checks: owner acceptance
on an authorized fresh build, connected-PC DM-channel capture and delivery of
private DM-prefixed feedback (the queue tests do not prove network delivery).
Astra2's native proofs and their limits are recorded in live-ops-wire.md.

Cleanup removes only this task's redundant staging patch, mislabeled duplicate
baseline and two superseded compile-check logs. Sources, original/current proof,
read-only report and final gates remain. The old untracked mixed shader/scene
fixture remains outside the compiled suite: earlier automatic approval review
rejected its fifth-acceptance cleanup as "blocked by policy", with no more
specific reason; that deletion was not retried. Active history at
the agent's local Codex session file
is untouched and registered for separate supported retention after closure.

Final cleanup total: **1,003,235 logical bytes** removed from four identified
owned files; allocated/free-volume space is not attributed while other work runs.
The retained ledger validates required paths and records the active history for
separate retention; no shared cache or accepted artifact was deleted.

## Visual look decoding defence (2026-10-15)

The post-sixth-acceptance NPC equipment report omitted the GFF file tag, and a
PC Description contained Unicode outside the module codepage. Either error
prevented the whole creature preview from loading. Astra2 repairs report
production separately in 0e60415. The viewport also normalizes its own cloned
visual input: insert the canonical four-byte `UTC ` tag only when absent and
recursively omit fields typed `cexostring` or `cexolocstring`. Numeric appearance
fields, EE wide twins, equipment lists, resrefs and struct metadata remain. The
stored report stays intact; malformed numeric/structural data still errors.

Reproduce through PowerShell7 with `tools/live/check.ps1 -Phase 8 -Next -Jobs 1
-NoUiDebug -UiUnoptimized`: `-Step look-hotfix -Package mg-ui -LibraryOnly
-Filter live_look` checks missing metadata, both codepages, recursive text,
wide parts and equipment. `-Step look-hotfix-after -UiTest live_accept6_broken_looks`
uses installed game resources and asserts both NPC and PC previews exist. Set
`MGLIVE_ACCEPT6_PICTURES` to a task-specific output folder. The recorded before
run used `MGLIVE_LOOK_HOTFIX_BEFORE=1` against the unnormalized reader and proved
both previews absent. Preserve the before/after PNGs under
`target/test-output/live-opt-ui/after/look-hotfix-{before,after}.png` pending
owner review. This is an offline GPU harness proof; the running demo is untouched.

### Focus member portraits (F1)

Expanded group member rows now use the same portrait resolver as watched entries
and Selection; either the portrait or name opens that member. Guarded
`-Step focus-members-after -UiTest live_accept6_focus` passes the area/member/reply
flow. Inspected `live-opt-ui/after/focus-members-{before,after}.png` show the
missing row portraits restored with the installed-game appearance/default fallback.

### Focus own-speech default (F2)

Conversation defaults to speech by the watched object, or the selected group's
members. Nearby/incoming speech and server/acknowledged DM messages are separate
opt-in filters, initially off. Receipt-time audience matching and retained
history are unchanged; hidden rows keep their unread state until actually shown.
The dialogue speaker field takes precedence over its recipient/context identity.

Guarded `live_accept6_focus` covers both independent filters, group/member area
selection and replies. `live_focus` retains proximity/private-audience coverage
with the incoming filter explicitly enabled. Unit coverage separates speaker,
recipient, dialogue speaker and server/DM cases. Inspected
`live-opt-ui/after/focus-filters-{before,after}.png` record the same three-line
fixture before the change and its own-speech default afterwards.

## Allow LLM consent and presence UI (2026-10-15)

Owner request: one explicit, persisted consent switch (default off) for external
MCP/agent API clients, with a recent-client indicator. The toolbar beside Live,
Live menu (`Id::LiveAllowAgents`) and Options Live page share
`live_agents::set_allowed`. It calls `LiveService::set_allow_agents` in place,
copies the resulting gate state into settings and any open Options draft, and
reports descriptor failures. The API listener, game link, report store and local
DM controls remain running. Options labels this switch as immediate and separate
from Apply/Cancel. Other Live settings retain their existing Apply semantics.

Astra2 owns `LiveConfig.allow_agents`, gate/discovery, presence and the MCP
heartbeat. Agreed service contract: `agents_revision() -> u64` is cheap;
`agents(now_ms) -> Vec<AgentPresence>` contains authenticated clients active for
strictly less than 60 seconds, with `client`, `last_ms` and canonical `op`.
The API wakes on a new/returning client or changed operation. A heartbeat updates
the revision/time while retaining the last real operation and does not wake an
unchanged indicator. The GUI reads the revision once per UI pass and clones the
snapshot only on a changed revision or expiry. It schedules the earliest
`last_ms + PRESENCE_MS` deadline through `live_repaint::after`; no one-second
polling timer or per-frame report/status query is added. An old deadline simply
reschedules if a silent heartbeat extended presence. Empty/off state has no timer.
Options uses the separate `connection_status()` diagnostics accessor so its
existing API/Redis/WSL status display does not clone the new agent presence list.

Red means no recent client or the service/access is off; green means an
authenticated request in the last 60 seconds. Hover lists all recent client
names, operations and ages, without credentials or request arguments. The menu
also spells out connection status. Settings serialization supplies normal desktop
persistence; changing consent does not edit the module. Connection instructions
are in [Allow LLM](manual/19-live-mode.md#allow-llm).

Reproduce with PowerShell 7 and guarded `tools/live/check.ps1 -Phase 8 -Next
-Jobs 1 -NoUiDebug -UiUnoptimized`: `-Step llm-unit -Package mg-ui -LibraryOnly
-Filter live_agents` for fake-presence UI transitions, exact expiry/deadline and
snapshot-read checks; `-Step llm-controls -UiTest live_agents` for the actual
toolbar/menu/Options flow, local HTTPS consent and unchanged listener certificate,
settings roundtrip, and disabled controls with Live stopped. This test uses an
ephemeral loopback API and disconnected Redis port 1, never the running demo.
It reuses the existing `ui-area-live-effects/area.mod` resource fixture.

Evidence: `target/test-output/live-llm-ui/` contains toolbar/menu before and
after, on/red and Options captures plus the ownership ledger. Before captures
were taken before the UI change with `MGLIVE_LLM_BEFORE=1`; use the current normal
test for after captures. Guarded logs have `live-next/phase8-llm-*` names.
These prove offline UI/API behavior; connected-model and owner acceptance remain
separate. No demo start/stop or application build is needed.

Verification: three unit/fake-presence cases, one real-resource controls case,
and the existing Options HTTPS and Live-menu regressions all pass. All-target
mg-ui Clippy (`-D warnings`), scoped Rust formatting and local manual links pass.
The current six pictures were inspected; the first icon choice was unsupported
by the bundled font and was replaced with the viewer's existing lightning glyph.
No disposable standalone build, temporary patch or extra module copy was made.
The one current proof set and guarded logs remain for owner acceptance (ownership
ledger in `live-llm-ui/owned-files.json`); no reclaimed space is claimed. Shared
Cargo outputs and active session history are untouched. The index/log entries
were captured unchanged with Astra2's concurrent `e862457` documentation staging;
this is accounted for without rewriting shared history.


## Audit 7: local review of agent speech (H12, 2026-10-15)

Named clients now have an `auto` / `drafts` delivery mode. The default remains
`auto` for existing saved settings. In `drafts`, `npc_say`, `reply_to_player`
and `pc_message` pass the normal schema, scope, target and player checks, then
wait in a bounded local review queue (128 proposals plus approved deliveries).
No native command is published before local approval. The local DM can send,
edit or reject; the approval method is not an agent operation. Approved speech
rechecks current client permissions, global player consent, target area/death
and game boot. A boot or Access replacement between decision and enqueue is
checked again under the enqueue lock. Removal/reconfiguration cannot silently
turn a held message into an unrestricted local command.

`live_wait` on the original ID returns `pending:true` with `value.draft.outcome`
`pending` or `sending`; it eventually reports `sent`, `edited`, `rejected`,
`send_failed` or `expired`. The corresponding scoped command event carries the
same result. Native acknowledgement is not a connected-player delivery receipt.
Identical client retries and repeated local Send do not duplicate speech.
Review time is not the game's 30-second command TTL; a new game boot expires
unapproved drafts. Pending drafts are session-local, not persisted across app
restarts.

Core reproduction: `tools/live/check.ps1 -Phase 8 -Next -Step audit7-drafts
-Package mg-live -Filter assistant -Jobs 1 -NoUiDebug -UiUnoptimized` (one line).
Six tests pass, including NPC/PC speech, original-ID results, edits, rejection,
duplicate requests, current permissions/policy, dead targets and approval races.
All-target mg-live Clippy passes. UI proof is recorded with its follow-up commit.
Implementation: [assistant.rs](../crates/mg-live/src/assistant.rs),
[tests](../crates/mg-live/src/assistant_tests.rs).


H12 UI follow-up: one revision-cached draft panel is shared by Chat and Focus,
filtered by area, focused member and DM conversation. Options exposes Auto and
Drafts per client. The authenticated fake-agent app test stages speech over
TLS, verifies an empty native queue, edits/sends in Focus, waits on the original
ID, rejects, sends all and exercises Enter. Area/member/DM filtering has a unit
test. Before/after evidence is in `target/test-output/live-audit7`:
`chat-before.png` / `chat-drafts-after.png`, `focus-before.png` /
`focus-drafts-after.png`, and `clients-list-before.png` /
`clients-drafts-after.png`. Pictures were inspected; owner connected-client
acceptance remains pending. The tiny fixture is reused under
`target/test-output/ui-area-live-audit7`, not a copy of the demo.


## Audit 7 assistant cards and memory (H13/H14)

The Live menu opens a GM card; Selection/Focus open the NPC card by its stable
`tag:` or `resref:` key. Fields save atomically per module path. Focus Memory
shows retained exchanges with each player and a summary. The operation table
adds `live_gm_card`, `live_npc_card`, `live_npc_memory` and
`live_npc_memory_note`; descriptors, validation and dispatch stay in one table.
The note handler requires read_chat, validates 4000 characters and waits for
its disk write before reporting saved. Card writes remain local UI actions.

Sources are `crates/mg-live/src/assistant_{book,cards,memory,ops}.rs` and
`crates/mg-ui/src/live_assistant.rs`. Core hooks are coordinated with Astra2;
no NSS or native operation is required. Accepted event capture uses a cursor,
not a per-frame scan. A bounded writer notification coalesces memory snapshots
off the store lock. Limits: 1000 conversations, 200 turns per pair, 10,000 turns
in total, 2048 characters per retained turn, 32 MiB maximum file. Cards and
notes are small explicit Save writes. Nearby capture is qualified, private
unresolved chat is excluded; no connected-client delivery claim is made.

Reproduce with guarded `-Phase 8 -Next -Step audit7-cards -Package mg-live
-LibraryOnly -Filter assistant -Jobs 1 -NoUiDebug -UiUnoptimized`, and the app
variant `-UiTest live_audit7` instead of package/filter. The app proof reads
cards and memory over authenticated TLS, edits/saves both card types, updates a
summary, shows Focus Memory and reloads the files. Inspected pictures:
`target/test-output/live-audit7/{gm-card,npc-card,focus-memory}-after.png`;
Selection/Focus before pictures are in the same evidence folder. The harness
reuses its tiny area and isolates assistant JSON and service journals below
`target/test-output/ui-area-live-audit7`. Successful core persistence tests
remove their own temporary JSON folders; corrupt-original preservation is tested.

H13/H14 final gates: 152 mg-live tests passed (4 opt-in ignored), three integration guards, four audit app tests (baseline capture opt-in), and all-target core/UI Clippy clean. First-report conversation and repeated-client draft wake regressions pass. The Focus Memory page uses the same All/area tabs as other histories. No demo lifecycle or application binary build occurred.


## Audit 7 drag-to-move (H7)

`live_drag.rs` adds release-only moves for creatures, ground items and groups.
It shares eligibility with existing tools, preserves the selected formation,
validates boot/identity/area bounds again on release and keeps Ctrl camera pan
and Shift box selection. Single selected placeable/door manipulation retains
`live_props`; group placeables/doors use set_position, creatures/items use
unified teleport, with the existing replacement-ID follow-up. Draft ghosts are
numbered to avoid overlapping names. Starting another aim clears the drag.

Fake-store app proof (`-UiTest live_audit7`, standard guarded flags) checks zero
commands during the gesture, exactly three on release, +5/+5 metre offsets,
Escape cancellation, no repeated queueing, unchanged module and Ctrl pan.
Inspected `live-audit7/drag-before.png` and `drag-after.png`. No demo/native
client was touched; server command execution relies on existing operation gates.

### Audit 7 H9: private object notes (2026-10-15)

Selection and Focus share `live_notes::editor`; the store keeps notes beside NPC memory under the module/server assistant directory. Stable identity is unique kind/tag or player tag/name, otherwise boot/id. The area snapshot caches notes per report and monotonic assistant metadata revision; search and marker hover use that snapshot. A note edit invalidates visibility even without a server report. No note is exposed through an agent operation.

Reproduce with `tools/live/check.ps1 -Phase 8 -Next -Step audit7-notes -UiTest live_audit7 -Jobs 1 -NoUiDebug -UiUnoptimized`. Fake-store app proof covers shared editing, NPC/player/placeable persistence, server isolation, search refresh and an unchanged module. Before/after images: `target/test-output/live-audit7/notes-{selection,focus}-{before,after}.png`, plus `notes-panel-after.png`. Pictures inspected; connected owner acceptance remains separate.

### Audit 7 H10: saved macro hotkeys (2026-10-15)

Four configurable `Cmd::LiveMacro1..4` entries use Ctrl+6..9 only over a Live map and outside text input. They take precedence over global numbered-tab navigation in that scope, as the existing control-group keys do. F-key defaults are preserved. Assignments live at `<data>/live/scene-keys/<module>.json`, outside the saved-scene directory; bounded JSON loading and atomic replacement. Rename/delete maintain assignments. Playback uses the normal Player and policy validation, refuses overlap with recording/playback, and ignores key-repeat events.

Audit7 fake-store app proof saves an assignment, reloads UI state, remaps to Alt+F6 and proves exactly one queued speech command even with a repeat event. `scenes-before.png` / `scenes-hotkeys-after.png` inspected. Guarded Audit7 app suite: eight passed (two opt-in baseline guards), UI Clippy clean before subsequent atmosphere integration.

### Audit 7 H6: Atmosphere window (2026-10-15)

`Tab::LiveAtmosphere` / `Id::LiveAtmosphere` opens a closable right-hand tool. Shared area tabs enumerate reported areas, cached by store source/boot/report revision; proposed settings and pending results are independent per area. Resource row labels cache by module/resource context. Commands use native typed operations from Astra2 6034a80 via the normal recorder/queue. No automatic mutation and no assumed initial atmosphere state: only the last native command readback is labelled as server state.

Fake-store Audit7 app proof selects the second area, checks weather/fog payloads and readback, then sends to the first area and asserts the module is unchanged. UI all-target Clippy passed. New-window comparison uses the pre-feature `chat-before.png` baseline and `atmosphere-after.png`; the latter inspected. Native proof belongs to Astra2; connected-client appearance/audio acceptance remains due.

### Audit 7 H15: conversation profile and attribution (2026-10-15)

Options uses Astra2's `ClientToken::conversation` (685057d): Read/ReadChat/Speech, Drafts,60/minute, a fresh token and collision-free name. Speech is also visible as a permission checkbox. The shared Log formatter adds the LLM icon/client name to Chat, Focus and Log only when event.data.by or result.by identifies a non-local source. It never infers a client from message text. Native/store provenance and delayed causal limits are owned by Astra2; fake UI proof establishes rendering, not client delivery.

Audit7 fake-store tests verify profile defaults and all three windows; `clients-list-before.png`/`conversation-client-after.png`, `chat-before.png`/`chat-agent-after.png`, `focus-before.png`/`focus-agent-after.png`, and `log-agent-after.png` inspected. UI Clippy passes. Area harness Focus/Scenes persistence is now isolated under its own assistant-data directory: a stale fixture watch previously suppressed a different-tag NPC in a later test. No live user watch is loaded by the harness.

### Audit 7 H8: world palette aiming and native removal limit (2026-10-15)

`live_spawn` maps UTW/UTS/UTE/UTM/UTT alongside existing creature/placeable/item types. Point kinds use the existing spawn aim and queue; trigger drag snapshots ground points, displays dimensions/rectangle, validates >0.1..30 m, and sends one typed SpawnTrigger with the selected optional r on release. Cancellation and boot changes clear the drag. Non-model world objects receive Live markers from AreaLive::all and kind names in Selection/Panel. Show all restores all reported kinds.

Native limitation reported by Astra2: DestroyObject on a spawned encounter still leaves it valid/in-area after20s, including SetIsDestroyable/high-AI attempts. No approved safe native destroy hook exists. The UI disables encounter removal, excludes it from mixed confirmations and reports the reason for encounter-only Delete. This is an explicit H8 blocker, not completed encounter removal. Only Live-created sounds are reported because ordinary native area enumeration omits sound objects.

Fake-store app proof checks four point-spawn kinds, selected-UTT retention,3x4m drag dimensions, release-only dispatch, Escape, all five reported markers and four eligible removals from a five-kind selection. Before/after `world-palette-before.png`/`world-palette-after.png`, `world-trigger-after.png`, `world-reports-after.png`, `world-encounter-limitation-after.png` inspected in target/test-output/live-audit7. Fake template names establish UI payloads, not native blueprint availability; real sound/store/trigger/encounter behaviour remains connected-client acceptance. UI all-target Clippy and eleven Audit7 tests passed (three opt-in baseline guards). Native operations/proof belong to Astra2; no native hook invented here.

### Audit 7 acceptance and retention

The connected-client checklist is `target/test-output/agent-mailbox/acceptance-7.md`; it covers skill NUI, inventory, DM routing, instrumented dialogue, draft delivery, cards/LLM behaviour, persisted memory, attribution, atmosphere and world objects. Automated checks are not owner acceptance. All evidence uses the single `ui-area-live-audit7` fixture plus `live-audit7` pictures; no GUI/demo build or lifecycle operation.

The pre-isolation Focus test wrote one synthetic Watch captain entry in the normal app-data Focus namespace, keyed only to the Audit7 fixture module path. Its exact path/hash/content ownership is recorded in `live-audit7/owned-files.json`; the obsolete watch file was removed after that verification. Together with two clipped/duplicate pictures this removed 1,596,570 logical bytes. Automatic approval review blocked removal of its now-empty app-data directory ("blocked by policy"); the directory remains and the action was not retried. Superseded owned check logs are removed at closeout after outcomes are recorded here; final images and final gates remain for review. Shared Cargo artifacts, peer outputs and active session history are outside this cleanup.


### Audit 7 final guarded integration (2026-10-15)

On the tree including H8 core `e4abc8b`, H11 `ee4d189` and H15 attribution `1e75d9e`: workspace **1,698 passed / 0 failed / 27 ignored**, workspace all-target Clippy `-D warnings` and `fmt --check` clean. The separate mg-live gate passed 156 unit tests plus 3 integration tests (4 opt-in tests ignored). Assistant-focused tests passed 12; the real-resource Audit7 app gate passed 11 (8 feature tests, 3 baseline guards). Before/after pictures were inspected. These are automated checks, not connected-client owner acceptance.

The integration run exposed three old UI fixtures that started a real service but skipped its durable command-identity initialization, now required by H4 `79c462f`. `c9f2f9e` gives the Options/Chat tests isolated data roots and finishes descriptor setup in the runtime-window test. The gate also caught a developer-local source fallback in Astra2's dialogue proof; H11 now requires an explicit source path. No production gate was bypassed to make tests pass.

Reproduce with the existing guarded runner (no app build or demo lifecycle):

```powershell
tools/live/check.ps1 -Phase 8 -Next -Step audit7-integration -CheckFmt -Jobs 1 -NoUiDebug -UiUnoptimized
tools/live/check.ps1 -Phase 8 -Next -Step audit7-world -UiTest live_audit7 -Jobs 1 -NoUiDebug -UiUnoptimized
tools/live/check.ps1 -Phase 8 -Next -Step audit7-assistant-final -Package mg-live -LibraryOnly -Filter assistant -Jobs 1 -NoUiDebug -UiUnoptimized
```

The UI gate reads the installed resources selected by ignored `tools/live/local.ps1` and uses `mg_testkit::gpu::hold()`. It creates one authored `target/test-output/ui-area-live-audit7/area.mod`; isolated assistant/focus/scenes data sit alongside it. `target/test-output/live-audit7/fixture.json` records exact current paths. Final gate logs remain in `target/test-output/live-next/phase8-audit7-integration-{clippy,live,workspace,fmt-check}.log`, with `phase8-audit7-world-live_audit7.log` and `phase8-audit7-assistant-final-focused.log`. Local Markdown validation found 754 existing link paths; `okf` is unavailable, so no OKF lint result is claimed.

Cleanup: removed 22 superseded own check logs (26,527 logical bytes) after the final gate, in addition to the earlier 3 files (1,596,570 bytes): **1,623,097 logical bytes removed**. Retained the referenced draft-test log, one authored fixture, final pictures/gates and the small ownership receipts for acceptance. No allocated-size or free-space gain is attributed; other workers remain active. The blocked empty app-data directory remains. Shared Cargo outputs, peer work and active session history were retained. Ownership/session/paths are in `target/test-output/live-audit7/owned-files.json`; history cleanup is a separate supported/authorized retention task, not a JSONL rewrite.

Remaining: the one-page connected-client checklist `target/test-output/agent-mailbox/acceptance-7.md`; accept encounter-removal unavailability or authorize a separately proven native hook. Only Live-created sounds are enumerated. Card guidance is private DM/agent context, not a guarantee of an LLM's choices; inferred nearby memory is labelled, and exact PC identity cannot exceed the report's tag/name information. No GUI or demo binary was rebuilt and no demo lifecycle command ran.
