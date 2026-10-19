---
type: Proposal
title: 'Live mode, next: implementation plan'
description: The implementation plan for live mode after its first real session (2026-10-12) - a targeting cursor for Walk Here and Attack, the player's model, clear command errors, a readable view (labels, a live object list with filters, combat shown, smooth movement), an event feed, the controls a DM missed (holding NPCs to orders, stopping a fight, factions, player actions behind a switch), Polish text, tools for LLMs (events, search, summaries, scoped tokens) and the audit's follow-ups.
tags: [proposal, live, dm, llm, area-viewer, nwnx]
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-12T14:30:00Z }
---

# Live mode, next: implementation plan

> **Superseded in part (2026-10-19).** The desktop no longer has a local Live mode: Moonglow reaches a game server only through a Live gateway. The local HTTPS API, its token and named clients, the session descriptors, `Allow LLM` consent, the Options › Live API/Redis/WSL/Logs controls and `moonglow-mcp`'s local discovery (`--descriptor`, `--sessions`) described here were removed; read it as the history of that design. Current behaviour: [Live mode](manual/19-live-mode.md), [desktop contract](live-desktop-link.md), [gateway protocol](live-gateway-protocol.md).

Status: accepted by the owner, 2026-10-12. Builds on branch `live-mode` as
it is after [live mode over Redis](live-mode-redis.md) (commit `8cb36f3`):
the store, the Redis link, Options › Live, the client API, `moonglow-mcp`
and the WSL demo all exist and pass. Self-contained for whoever builds it.
Sizes: S (an hour or two), M (half a day), L (a day or more).

## Implementation record

**Phase 1b committed as `759684d`; Claude completed the final picture check
after the usage-limit interruption.** His worktree merge `53584f8` adds the
Live menu, VFX reader, scenes core, reconnect tooling and MCP line cleanup.
E2.1–E2.6 are implemented in separate checked commits as instructed.
The tree is handed back for Claude's pending E3 merge, merged-tree checks and
the combined E2+E3 two-monitor acceptance; automated checks are not that acceptance.
The owner-approved mailbox plan (17:00) extends the order through the
[master roadmap](live-mode-roadmap.md): acceptance stops are after **E2+E3**
(view, menu, logs), **E4** (effects), **E5+E6** (controls and placeables),
and at the end. This supersedes the older checkpoint text below. Phase 1 is committed as `e6d9a03`; its acceptance findings are
in [Owner acceptance of Phase 1](#owner-acceptance-of-phase-1-2026-10-12-and-plan-changes).
The owner authorizes `Stop-LiveDemo.ps1` on Claude's current acceptance
session when engine tests require UDP 5121. Ship-demo PID 437 / UDP 5122
stays outside the task. The private demo's password change is explicitly
authorized; repository engine runners keep their existing password policy.

Phase 1b reuses `target/test-output/live-next/` and the existing demo working
set. New own evidence: `phase1b-models.json` (required, compact animation
names/source hashes), `visual/{dead,spawn}-{before,after}.png` and
`phase1b-*.log` (planned proof). Authored changes will stay in the existing
live store/UI/scripts/runners plus `mg-preview/src/compose.rs` and
`mg-area/src/scene.rs` for death-pose drawing. No full asset extraction or
extra module copy is needed. The original accepted recovery MOD remains
protected while the acceptance fixes are unresolved.

Preflight read the full revised plan and mailbox. Base-game `pmh0` and
`c_bodak` inherit `a_ba`: its death poses are **`deadfnt`/`deadbck`**, and
falling animations **`kdfntdie`/`kdbckdie`**; `walkdead` is not a death pose.
The names and supermodel-chain hashes were measured with `mg cat --text`,
without writing game data. The installed `nwscript.nss` confirms the
proposed `SetIsTemporaryEnemy(target, source, decays, seconds)` argument
order, and that `SetCommandable` accepts an explicit target object.

### E2 working record and file ownership

The full master roadmap was read after Phase 1b's commit. E2.1 starts from
clean `53584f8`; the same session and `live-next/` working directory continue.
Own planned evidence: `visual/labels-{before,after}.png`, reusing the recorded
crowd in `live-demo/morning/curl-after.json` and `live-demo/DEMO-Live-01.mod`,
and `phase2-labels-*.log` plus a small check summary. No new whole-module or
resource copy is required. Label implementation stays in `live_view.rs`,
its app/screen tests and the guarded runner. Later E2 work adds `live_panel.rs`
and integrates tabs/options/area drawing; each component is checked and
committed as requested. Minimal proof remains needed until E2+E3 acceptance.

Claude owns commands/keys, scenes core integration and renderer VFX support;
Astra2 owns game-side E3.2–E3.4/E4.3 in its separate worktree. Instant VFX via
effect events are no longer assumed: those events exclude visuals; use the
spell/projectile event path per the coordinator's correction. `scene.rs` that
appeared untracked during E1 is now included by Claude's merge, not an Astra
cleanup candidate. No additional worker or shared-file edit is started here.

E2.1 implements the four label modes in each area's Live toolbar. Near and
selected is the default (15 m from selection, otherwise camera target).
Names are placed in stable selected/player/combat/own/other priority order,
with a three-pixel collision margin and no labels outside the visible view.
Dropped names keep their markers and HP bars. Non-selected names fade from
40 to 80 m from the camera; selection stays readable. Names wrap at 180 pixels.
The morning crowd before/after uses All mode and the same recorded positions,
selection and camera, so it isolates overlap handling. This picture is replay
evidence, not a new engine or owner acceptance.

Reproduce through PowerShell 7 after loading ignored `tools/live/local.ps1`:
`tools/live/check.ps1 -Phase 2 -Next -Step labels -Package mg-ui -Filter live_view::tests`,
then `-UiTest live_labels`, `-Picture live_labels_snapshot`, and the full
`-Phase 2 -Next -Step labels` gate. Picture stage defaults to `after`; the
retained `before` was captured against the unchanged merged overlay.
The merged baseline guard completed before these edits: mg-live 39 passed,
workspace 1,027 passed / 85 ignored, clippy and fmt clean, recorded in
`live-next/phase2-merge-checks.json`.

Component verification: workspace 1,031 passed / 86 ignored, no failures;
mg-live 39 passed / 2 ignored; clippy and fmt clean. The explicit corpus-backed
`live_labels` app test passes all four modes, selection with names hidden, and
no command being sent. The crowd picture was inspected: the overlapping rain
shelter names become one readable name, while markers, HP bars and the selected
name remain. The initial app query needed `get_by_value` for egui's combo text;
this was a harness correction, with no production change. The focused app and
picture results each pass one test. `phase2-labels-checks.json` records counts
and input/source/picture hashes. No artifacts have been removed; these inputs
and pictures are still needed for the E2+E3 owner acceptance. E2.2 follows.

E2.1 is committed as `061e2a7`; the coordinator mailbox was read afterwards.
E2.2 work adds `live_panel.rs`, `Tab::LivePanel`, per-area `LiveFilter`, shared
marker/model/picking filters, module-marker hiding, and follow state. Action
names use the loaded game's `nwscript.nss` ACTION constants rather than
assuming numeric IDs. Category chips combine with OR, state chips with AND;
search is case-insensitive over name/tag/resref. A selected player narrows the
Hostile filter to that player; otherwise any reported hostile player qualifies.
Following checks the newest area report, only switches to an open area, and
stops on a closed/off view, missing object or a changed game boot.

E2.2 file ledger (same session): required source `crates/mg-ui/src/live_panel.rs`;
reproducible tiny app fixtures `target/test-output/ui-area-live-panel/area.mod`
and `ui-area-live-panel-follow/area.mod`; `phase2-panel-*.log`,
`phase2-panel-checks.json` and `visual/panel-after.png` under `live-next`.
The already retained `visual/labels-after.png` is the before picture (same
morning input and camera, without the panel); no duplicate baseline is made.
Current picture/fixture/log outputs remain pending the E2+E3 owner acceptance.

E2.2 verification findings: tab bodies must use `panes_tabs` to check open
areas because `Moonglow::ui` temporarily moves `dock` out while drawing them.
The initial blank panel picture exposed that error and was overwritten after
the fix, preserving one current result. Following updates before dock drawing,
so it can check `dock` directly. A manual viewport selection also ends following.
The installed `nwscript.nss` was read with `mg --root $env:NWN_ROOT --no-user-dir
cat nwscript.nss`: `ACTION_MODE_*` is a separate enum whose values collide with
the action IDs. It must be excluded from action names; a regression fixture
puts `ACTION_MODE_DETECT` after an action with the same value. Test harness
corrections use valid hexadecimal object IDs, explicitly focus the search field,
and separate single/double-click gestures from preceding filter clicks.

The final E2.2 workspace rebuild initially hit Windows error 1455 while mapping
the UI rlib for `world_perf` (insufficient paging capacity, not a Rust diagnostic
about the implementation). No build files or other processes were removed.
The guarded runner now accepts `-Jobs 1` (default remains 2) to reduce concurrent
compiler memory while preserving the NWN_ROOT guard and restoration of the
caller environment. The final E2.2 gate uses `-Phase 2 -Next -Step panel -Jobs 1`.

Final E2.2 checks pass: workspace 1,036 passed / 87 ignored; mg-live 40 passed /
2 ignored; clippy and fmt clean. The explicit `-UiTest live_panel` run passes
both corpus-backed app tests (all ten chips, search, selection/centre, follow,
area transitions, reboot, filtered picking, no commands or module edits).
`-Picture live_panel_snapshot` passes; the current picture was inspected with
all columns, selected row, filters and correct action labels readable.
`phase2-panel-checks.json` retains source/input/picture and base-game spec hashes.
Two 23,443-byte test modules and the current minimal proof remain for acceptance;
no module/resource copies beyond those isolated fixtures, no reclaimed bytes.
The next component is E2.3 combat rings, targets, corpses and the legend.

E2.2 is committed as `cf6ec05`. The mailbox audit was read afterwards: Claude
now owns Rust-side E3/events/logs, and his E4.2 renderer work is awaiting a clean
merge window. Before E2.3, the two E2 audit corrections reserve visible marker
rectangles during label placement and order the panel by camera-target distance.
Name/HP/Distance headers also sort, with another click reversing order. Rows stay
in place for half a second after a click so centring does not disrupt a follow
double-click. Column distance remains relative to selection, falling back to
camera target when none is selected; Near camera explicitly resets the default.
Audit evidence owns `visual/labels-audit-after.png`, `visual/panel-audit-after.png`,
`phase2-audit-*.log` and `phase2-audit-checks.json`. Existing labels-after/panel-after
are the necessary before pictures; no duplicate copies are made. The intermediate
pictures may be removed after the E2+E3 acceptance, keeping original/final proof.

The audit UI build also exhausted LLVM memory with one job while the other
worktree was compiling. After waiting for that UI build, the coordinator supplied
the measured workaround: `-Jobs 1 -NoUiDebug`. This guarded-runner option adds
`--config profile.test.package.mg-ui.debug=0 --config profile.dev.package.mg-ui.debug=0`
to test/build commands only, leaving dependency profiles and clippy unchanged.
No owner applications, paging settings or existing build files are modified.

Audit checks pass with the workaround: workspace 1,038 passed / 87 ignored,
mg-live 40 passed / 2 ignored, clippy and fmt clean, both corpus panel tests
passed, and each audit picture test passed. Both images were inspected: the
Patrol follower 1 name no longer crosses follower 2's dot, both markers remain,
and the panel starts with nearby objects and shows the sorting controls.
`phase2-audit-checks.json` records hashes, profiles and results. The active
session history was measured by metadata only at 18:12 (64,270,144 logical bytes);
it remains untouched and is not closed or eligible for cleanup during this task.
No bytes have been reclaimed; required before/after proof is retained until
E2+E3 acceptance. E2.3–E2.6 remain next; E3 Rust work belongs to Claude.

E2.3 starts from `f716e0a`. Own evidence: `visual/combat-{before,after}.png`,
`phase2-combat-*.log` and `phase2-combat-checks.json` in the existing `live-next`
directory. The picture harness reuses the morning module/report/camera with a
controlled attacker, target and corpse; it proves the overlay, not fresh game
combat. E1b retains the engine proof. No additional module/resource copy is made.

Combatants now keep a red ring around their marker in addition to the white
selection ring. Dashed arrows point at reported targets; absent, dead or filtered
ends suppress the arrow, as do stale targets after combat ends. Dead objects
use a grey cross with their normal label eligibility. Ring extents are reserved
in label placement, HP bars sit below them, and the corner legend reserves its
own label space. The Legend toolbar switch defaults on. Faction/hostility
colours follow in E2.4; this component retains the existing Kind colours.

The E2.3 guarded gate (`-Phase 2 -Next -Step combat -Jobs 1 -NoUiDebug`)
passes 1,039 workspace tests / 88 ignored and 40 mg-live tests / 2 ignored,
with clippy and fmt clean. The combat-link test covers hidden or dead ends,
missing/self targets and combat ending; the corpus label test also exercises
the legend switch. Reproduce it with `-UiTest live_labels`, and the controlled
picture with `-Picture live_combat_snapshot` (`MGLIVE_PICTURE_STAGE=after`).
Both explicit runs pass (one corpus UI test and one picture). The inspected
after image shows the selected attacker's separate white/red rings, a dashed
arrow to the other combatant, the grey corpse cross/name and the corner key.
A missing arrow glyph in the legend text was removed, then the picture was
rebuilt and inspected again; the geometric arrowhead remains on the actual
target line. This final text-only correction does not change the tested logic.
`phase2-combat-checks.json` records source/input/picture hashes. No new engine
run or owner acceptance is claimed, and no retained artifact is removed yet.

E2.3 is committed as `28d8e53`. E2.4 owns the new `crates/mg-ui/src/live_colors.rs`,
the faction report/Options integration, and `phase2-factions-*.log`,
`phase2-factions-checks.json`, `engine-factions.json` plus current faction/Options
pictures in `live-next`. The combat-after image is the view baseline; the same
controlled crowd supplies the faction picture. Existing demo working files are
reused. Hostility needs explicit `friendly` PC IDs as well as `hostile`: both
are now queried for every reported creature in the PC's area, including outside
combat. Missing friendship data in older reports means unknown, not friendly.
The installed `nwscript.nss` confirms both relationship functions take target
first and source second; the view describes hostility **to the selected PC**.

Faction IDs map through the same edited `repute.fac` reader as the faction
editor. Colour overrides live in the ordinary settings keyed by module path
and faction index; unsaved modules must be saved before customising. Options
keeps its existing draft/Apply/Cancel behaviour. Unknown IDs or absent report
fields stay grey. Combat rings, dead crosses and selection remain independent
of colour mode. The small legend caps its faction rows and points to Options
for the complete mapping. Non-creatures retain Kind colours.

The Aurora operate/prove skills (contract 1.0.15) were read for their ownership
and evidence rules. Their native Toolset/client capture route is outside this
headless gate: the owner's explicit `tools/live/test-next.ps1` WSL workflow is
the execution authority here, and no Toolset/model-visibility proof is claimed.
Read-only preflight found no Moonglow or UDP 5121 listener; owner client PID
32352 and ship server PID 437 remain untouched. The existing demo was rebuilt
in place, with zero compilation failures and zero module verification warnings.

The first E2.4 corpus run found that the Legend click after colour-mode changes
did not toggle at the narrow 1100-pixel test width. A temporary diagnostic
capture `visual/factions-controls-debug.png` belongs to this task (absent before
the diagnostic run). It proved clipping behind the palette, and is now retained
as the narrow-layout before picture until E2+E3 acceptance. The temporary failure
capture hook is removed; `MGLIVE_CAPTURE_CONTROLS=1` on the corpus label test
captures its corrected counterpart `visual/factions-controls-after.png`.
Live controls now start their own row, and Hostility has a shorter selector
title with an explanatory tooltip. The corpus colour/legend/Options test passes
at the original 1100-pixel width with the palette docked. The earlier full workspace gate passed
1,041 tests, but corpus interaction/pictures/engine are separate remaining gates.

Inspected `visual/factions-after.png` and `visual/factions-options-after.png`:
the defender attacker is green, its hostile target red, commoners orange,
corpse grey, with combat rings still visible. The faction list and colour
controls are readable in Options. Baselines are `visual/combat-after.png`
and the retained `live-redis/visual/options-live-after.png`; no duplicate is
made. The full gate is rerun after the narrow-layout correction. Initial build
diagnostics (LiveObject's module path and Palette method visibility) were fixed
before these checks; they were implementation errors, not changed engine facts.

Final E2.4 checks pass: 1,041 workspace tests / 89 ignored; mg-live 41 / 2;
clippy and fmt clean; the corpus label/colour/legend/Options test passes,
including Reset/Cancel and settings round-trip; the faction picture test passes.
The narrow corrected controls image was inspected too. Build passes with
`-Jobs 1 -NoUiDebug`. `test-next.ps1 -Phase 2 -SkipBuild` passes against the
new executable and rebuilt demo: initial faction 2, then Hostile 1, Commoner 2,
Merchant 3 and Defender 4 all agree with independent `ObjectToJson` GFF readback.
All four states report `friendly`/`hostile` outside combat. This headless run
has no player client, so real selected-PC relationships remain an explicit
owner-acceptance check rather than a claimed engine result.

`engine-factions.json` binds the session/boot/area, module/executable/scripts and
dependency manifest hashes, comparisons and closure readback. Its owned actor,
Moonglow, WSL server and keeper are gone; UDP 5121 is free. The ship server
PID 437 (same 13:00:10 Linux start) and owner client PID 32352 (same 15:29:48
Windows start) were read back unchanged. System Redis was not stopped or
reconfigured. `phase2-factions-checks.json` is the compact check/picture index.
No retained artifact was removed; all listed pictures currently serve E2+E3
acceptance, with zero bytes claimed reclaimed. E2.5 movement and E2.6 Grid follow.

E2.4 is committed as `91ee84d`. E2.5 owns `crates/mg-ui/src/live_motion.rs`,
view/follow/renderer timing integration, and `visual/motion-{before,after}.png`,
`phase2-motion-*.log` and `phase2-motion-checks.json` in the existing working
directory. Its controlled pair of recorded-crowd reports is one second apart,
with a four-metre move; the picture samples roughly halfway through that interval.
The existing module and GPU guard are reused. Real movement and frame pacing
remain part of the E2+E3 owner acceptance, separately from deterministic tests.

The E2.5 corpus app test also owns `target/test-output/ui-area-live-motion/area.mod`
(absent before this test), a tiny isolated fixture retained until acceptance.
Interpolation uses the measured interval and a monotonic UI clock; new reports
start at the currently displayed pose, turns take the shortest arc, and death,
new boots and jumps over ten metres snap. Picking and following use the same
displayed position. Trails retain five seconds, bounded to 128 points per object.
Creature previews choose available walk/run/cwalk animations (run at 3 m/s or
faster, combat prefers cwalk), with idle fallback; this speed threshold is a UI
heuristic, not a claim about the engine's movement-rate enum.

Static views keep their GPU image. Reports wake the UI through the existing
service callback; interpolation, keyed animations, emitters, ripple and flowing
materials keep timed redraws. A one-second UI-only refresh keeps report age
visible during outages. The initial motion picture overwhelmed headless GPU
submission with thirty unpaced steps; six 100 ms harness steps sample the same
half-second interval and pass. The scratch output is overwritten in place.

The first E2.5 workspace run passes 1,046 tests / 90 ignored and mg-live
41 / 2. Its final fmt check found one indentation issue in the added water
condition (fixed). The following corpus app build exhausted LLVM memory despite
`-Jobs 1 -NoUiDebug` while the other worktrees were compiling. Other processes
and build directories are left intact; wait for compiler contention to clear
before retrying. This is not a passing corpus result or an owner acceptance.

The memory workaround is now an explicit runner option, `-UiUnoptimized`,
which sets only mg-ui's test/dev optimization level to zero. Together with
`-Jobs 1 -NoUiDebug`, the corpus motion/picking test passes, both panel/follow
tests pass, and the existing static-area image-cache test passes. Their logs
are `phase2-motion-{live_motion,live_panel,an_area_s_picture}.log`. This profile
is for functional verification, not a frame-time benchmark; other packages
keep their existing profiles. The motion picture test passes and the pair was
inspected: the new view is halfway along the four-metre move with its blue trail,
where the baseline had already jumped to the destination. No fresh engine or
owner-acceptance result is implied.

Final E2.5 gate passes: workspace 1,046 passed / 90 ignored, mg-live 41 / 2,
clippy and fmt clean. Reproduce with `check.ps1 -Phase 2 -Next -Step motion
-Jobs 1 -NoUiDebug -UiUnoptimized`; add `-UiTest live_motion`, `-UiTest live_panel`,
`-UiTest an_area_s_picture`, or `-Picture live_motion_snapshot` for the corpus
proof. `phase2-motion-checks.json` indexes source/input/picture hashes and results.
The existing `ui-area-picture-kept` fixture is reused, not claimed for exclusive
cleanup. No artifacts were removed (zero bytes reclaimed); current evidence and
the isolated motion fixture remain required until E2+E3 acceptance. E2.6 follows.

E2.5 is committed as `0c82e43`; the mailbox is unchanged. E2.6 reuses the
morning module and owns `visual/grid-{before,after}.png`, `phase2-grid-*.log`
and `phase2-grid-checks.json`. It checks the toolbar at 1280 pixels with Live
off. The earlier `077a365` layout placed Grid directly after Animations; the
current baseline places it beside Live in the final row. No extra MOD is made.

E2.6 restores Grid after Animations. The first restored picture exposed a small
overflow from the Turn ComboBox, adding a horizontal scrollbar. The final layout
reserves the combo's width with egui's `available_size_before_wrap()` before
placing it; ordinary `available_width()` reports the wrapped row's full width
and was insufficient. The pinned egui 0.36.2 source confirmed the distinction.
The final 1280-pixel Live-off picture is inspected: Grid is clickable in its
old position, Turn wraps cleanly, and no horizontal scrollbar remains. Rejected
after pictures were overwritten in place; only the baseline and final remain.
Reproduce with `-Picture live_grid_snapshot` and `MGLIVE_PICTURE_STAGE=after`.

Final E2.6 verification passes: workspace 1,046 / 91 ignored; mg-live 41 / 2;
clippy, fmt and the guarded mg/Moonglow/MCP build are clean. The Grid picture
check and the existing 1100-pixel Live labels/colours/legend/Options test each
pass. All use `-Jobs 1 -NoUiDebug -UiUnoptimized`; the compact proof index is
`phase2-grid-checks.json`. This closes E2.1–E2.6, with the coordinator's E3
merge and combined two-monitor acceptance still required before E4 work.
The mailbox handoff includes commands and observations for that acceptance.

Retention at this checkpoint: source, one current demo working set, the existing
accepted recovery MOD and the minimal before/after evidence remain needed.
No separate rejected Grid pictures remain; the current after file was replaced
in place. No files were deleted and no reclaimed space is claimed. Shared build
outputs and other agents' files are not cleanup candidates. The active session
`01a12fe8-1458-7551-99fc-b277832def84` history at
the agent's local Codex session file
was measured by metadata only (108,658,182 logical bytes during E2.6); it is
untouched. Eventual history cleanup remains a separate, explicitly authorized
task after the session closes; no supported cleanup mechanism is established here.

### Phase 1b implementation and verification

Both movement and attack accept optional `hold` (false) and `timeout`
(30 seconds, bounded 1–120) in the store, catalogue and generated OpenAPI.
The game validates both fields independently, clears prior actions, and
holds routine control after queueing the action and its restore callback.
A module-owned watchdog releases holds on death or an ended action; a
hard timeout remains, and generations prevent an old timer releasing a
new order. New movement/attack commands and destruction release an old hold.
Different-faction attacks set temporary personal hostility in both directions
for 180 seconds. Same-faction attacks temporarily switch the attacker to
Hostile with a bounded hold, as approved below; the view requests 30 seconds.
Releasing a hold is not a general stop-combat command.

Dead drawing chooses the measured resting/falling animations and holds the
last frame; when none exists it rotates the composed model onto the ground.
A robe without its own matching death animation follows the base skeleton.
Spawning now requires an arming click, then places once (Shift keeps aiming),
with right-click/Escape cancellation and unchanged drag/drop placement.
Inspection caught a clipped long hint; placement now uses the painter's
visible clip rectangle rather than an overflowing area's full rectangle.

The original HTTP command route uses the catalogue's schema limits before
queueing. Its first workspace regression exposed a lost Options-specific
chunk refusal; validating against all known command schemas, then applying
the store's policy, preserves that useful error. Both routes accept speech
at 200 characters and reject 201 with the same schema error. The generated
OpenAPI and focused mg-live checks passed (35 passed, 2 ignored). The final
workspace attempt failed that one Options-message assertion; the correction
is implemented but not rerun because the subsequent engine blocker requires
a stop. The last clippy run passed before that correction and the hint fix.
The explicit spawning test and four inspected dead/spawn pictures passed
before the hint fix; its final after image still needs regeneration.

The approved acceptance demo was closed through `Stop-LiveDemo.ps1` after
matching GUI PID 14148 (start `2026-10-12T13:27:49.0532708Z`), WSL server 3552
(start tick 976388) and keeper 3522 (976170). Their WSL identities disappeared;
ship server 437 retained start tick 5711 and UDP 5122. The Phase 1b engine
runner uses its own `mglive-p1b` prefix and the same scratch working set.
Default launches omit player/DM passwords; `-Passwords` restores them,
admin remains random/unprinted, and every public request is refused.

Additional ledger entries for this session: `engine-1b.json`,
`engine-1b-look.json`, `phase1b-checks.json` (planned final evidence),
`phase1b-*.log` and the four dead/spawn pictures under the existing
`live-next/`; `target/test-output/ui-area-live-spawning/area.mod` is the
regenerated UI fixture. They remain needed pending the next owner acceptance.
The source, scripts, manual 09, deferred list and this cumulative record are
required results. No redundant whole-module copy or full model dump was made.

### Phase 1b blocker: prescribed hostility did not produce combat

**Resolved by Claude's explicit decision to resume E1.** Within one faction
the engine keeps reputation at 100. The approved correction remembers the
attacker's faction id with `NWNX_Creature_GetFaction`, switches it with
`ChangeToStandardFaction(..., STANDARD_FACTION_HOSTILE)`, and restores the
original through `NWNX_Creature_SetFaction` when its hold ends, the timeout
expires, the victim dies, or a new order arrives. Same-faction attacks always
have this bounded hold even if the optional flag was omitted. The command
result preserves `faction:"hostile_temporarily"` through the store and API.
**During the switch the attacker is hostile to everyone, including players.**
The Creature plugin and include are now enabled/extracted. Fresh module boots
need no restoration. Different-faction attacks keep personal hostility.
If actions still disappear after this switch, Claude authorizes probing
heartbeat/ambient routines and strengthening the hold before asking again.
The failure record below remains historical evidence, not the current decision.

`tools/live/test-next.ps1 -Phase 1b` built through the guarded runner,
compiled **57 scripts with 0 failures**, verified the module with **0 warnings**,
and ran the private WSL game. The two temporary owned `nw_bartender` creatures
passed `GetFactionEqual == 1`. The dispatcher contains the prescribed two
`SetIsTemporaryEnemy(..., TRUE, 180.0)` calls, then clears actions and queues
attack and its hold. The command returned `ok`, but no required combat/target
report appeared within six seconds. The repeated diagnostic probe found:

- attacker `203`, victim `206`; both alive, owned, non-PC/non-DM, 3/3 HP;
- attacker `combat:false`, `target:null`, current action **65535**;
- `GetIsEnemy(victim, attacker) == 0`, `GetReputation(attacker, victim) == 100`;
- `GetCommandable(attacker) == 1`, consistent with the ended-action watchdog.

The first probes protected both creatures with `SetPlotFlag(TRUE)`. The last
probe used `SetImmortal(TRUE)` instead, with the same failed combat result;
this avoids claiming that plot protection alone caused it. **The cause is
unconfirmed**: the evidence does not distinguish hostility never taking effect
from a game/AI routine resetting it before observation. The exact prescribed
sequence has not met its promised gate in this fixture. Under the task's
stop-on-wrong-fact rule, implementation stops here for Claude's corrected
attack strategy or authorization to investigate that distinction. No faction
change or alternate combat behavior has been substituted, and no phase commit
was made. E2 and later implementation have not started.

Required diagnostic evidence: `target/test-output/live-next/engine-1b.json`,
`engine-1b-look.json`, `phase1b-engine.log` and `phase1b-checks.json`.
The engine JSON records source/executable/module hashes; candidate MOD SHA-256
is `caaed1da781f9866af197ab6b0c8b6a9faf5505b9f5ebd2f58a9d7356cf9797c`.
Movement hold timeout and arrival restoration **passed** before this failure;
the look remained 6,795 bytes, player/DM password flags were absent and the
admin flag present. Attack timeout/replacement/victim-death restoration and
the later death-refusal regression were not reached in this run. Earlier
Phase 1 proof remains separate.

Each diagnostic run attempted destruction only of its verified owned actors,
then `Stop-LiveDemo.ps1` closed its owned GUI/server/keeper. Final inventory:
no Moonglow, server PID 5946 and keeper 5916 absent, UDP 5121 free; ship PID
437/start tick 5711 and UDP 5122 unchanged. Ownership records remain as inert
metadata. The source module, working candidate, recovery MOD and current
proof remain needed for the unresolved diagnosis; **0 artifact bytes reclaimed**.
No physical allocation/free-space saving is claimed. No history was edited;
this session's history was 49,356,707 logical bytes at 16:22 Europe/Warsaw.
Its later cleanup remains a separate retention task through a supported
application mechanism, not manual JSONL editing.

### Phase 1b resumed engine result

The approved correction passes the full private engine gate: same-faction
bartenders start combat with the intended target in **1.203 seconds**.
The native GFF faction id changes **2 → 1 → 2**; the result carries
`faction:"hostile_temporarily"`. Restoration passes after timeout, a new
movement order and victim death. An old timeout leaves a newer held order
intact. Movement timeout/arrival recovery, trimmed look (6,795 bytes), private
password flags, actor death, both independent `code:dead` refusals, and final
owned-actor removal all pass. Compilation is now **58 scripts, 0 failures**;
module verification **0 warnings**. This run uses the explicitly configured
stock NWNX root; the separate reconnect build is preferred only when
`NWNX_ROOT` is otherwise unset, preserving explicit local choices.

The initial resumed probe incorrectly read faction 0 from
`NWNX_Creature_GetFaction` executed inside a dynamic chunk, even while native
`ObjectToJson` recorded faction 2. The final harness reads the native GFF
`FactionID` instead, independently of the production getter/setter. Why the
NWNX call in a dynamic chunk returned 0 is **unconfirmed**, consistent with
the earlier diagnostic Redis-call limitation; no causal explanation is claimed.
Compiled game commands do read and restore the correct faction, proven by
native serialization. This correction changed the probe, not the restore logic.

New required source in the same ledger: `tools/live/wsl/extract-includes.py`
(extract Creature include), `mglive_exec.nss` (Creature calls), and
`CommandResult.faction` (bounded optional result metadata). Shared include/
launcher edits must be preserved when the separate event worker is merged.
The game-side E3.2–3.4/E4.3 task is owned by Astra2; this thread continues
with Rust/UI work and does not duplicate its scripts or log-tail helper.
Final verification passed (recorded by Claude after Astra's Codex usage limit
ended her turn at 16:45): guarded workspace run with no failures, clippy and
fmt clean (`phase1b-*.log`), and the `live_next_phase1b` pictures re-rendered
after the last `live_view.rs` change and inspected (dead player and NPC lie flat;
the spawn ghost and its left/right-click hint show). Phase 1b committed.

**Phase 1 checkpoint record (superseded by the acceptance findings below):**
implementation and engine checks passed, then work stopped for the owner's
two-monitor acceptance. The source-preflight
blocker below was resolved by Claude's correction. Work started on clean
branch `live-mode` at `6ac7c7a`, session
`01a12fe8-1458-7551-99fc-b277832def84`. The requested order is 1–6 with
one local commit per passing phase, no push, and owner acceptance stops
after 1, 2 and 4. The full plan and coordinating mailbox were read; this
plan supersedes the older mailbox task.

The Phase 1.2 statement that its allowlist contains the fields the preview
reads is incomplete:

* [Creature preview](../crates/mg-preview/src/creature.rs), `PARTS`, maps
  `footr` to **`ArmorPart_RFoot` on the creature itself** (line 18 at the
  starting commit). It is not a `BodyPart_*` field and is absent from the
  proposed allowlist. The loop at lines 281–284 skips a part when both its
  own and worn part numbers are zero; a missing field defaults to zero.
  Thus the specified copy loses the unarmoured creature's right foot.
* [Part-number reader](../crates/mg-rules/src/items.rs), `wide_label` and
  `part_number` (lines 233–241), first reads the EE wide twin: `x` plus
  the field label, truncated to 16 characters. The stated `BodyPart_*`
  allowlist also omits these `xBodyPart_*` fields, and does not preserve
  the wide right-foot/head counterparts.

These are verified source facts, not a rendered or game-client result.
Reproduce by reading `PARTS`, `number` and the body-part loop in the
preview, then `wide_label`/`part_number` in the rules crate, at `6ac7c7a`.
At that initial stop, no GPU, Cargo, NWScript compile or server test had run;
no visual change had been made. Game JSON shape, appearance sizes and real
player refresh were then unverified. The resumed checks are recorded below.
The inline Python source assertions and the new record's relative-file links
passed, as did `git diff --check`. `okf` is unavailable on PATH, so the
existing index was updated directly; this was not a full OKF lint.

**Original decision request (resolved):** correct Phase 1.2 to retain `ArmorPart_RFoot` and
the present EE wide twins of the copied model/body-part/head fields,
using the same `wide_label` convention as the preview. Then resume Phase 1
with fixture coverage for an unarmoured right foot and a wide part number.
The task explicitly says “stop on a wrong fact”; neither silently
implementing the incomplete allowlist nor silently amending its contract
was accepted as a workaround. The rest of the six-phase plan is unchanged.

Claude confirmed both source findings and authorized resumption in the thread.
As the plan author within the owner's delegation, he replaced the allowlist
with a deny-list: retain the whole GFF JSON struct except irrelevant/heavy
fields, filter equipment slots and item internals, then measure and remove
largest remaining lists above 32 KiB. Phase 1.2 below incorporates that
decision. Fixtures must cover the bare right foot and a wide-only part >255.

### Phase 1 implementation and offline checks

The current working tree implements targeting gestures and feedback, the
destination flag, player-look parsing/caching, optional creature state, and
stable command refusal/result codes. Right-click while aiming takes an early
return before selection, palette placement and context-menu handling. Both
the pointer hint and status line use the same text. Successful move queueing
creates a flag; failure, arrival within one metre, or 30 seconds removes it.
Player looks use the public `mg_gff::from_json` re-export of the function in
`json.rs`, then `AreaObject::read`; cache entries change with the look hash
(or value when no hash is supplied), and a bad look logs once per object ID.
Quick reports retain the last look/hash; a new boot clears it. The store
refuses >64 KiB looks atomically and handles truncated JSON without mutation.

The game copies the native GFF JSON with the approved deny-list, filters
equipment slots and item internals, and preserves all remaining fields.
Unknown non-equipment lists >4 KiB are removed; if necessary the largest
remaining lists are removed until the look fits 32 KiB. A scalar-only excess
omits the look rather than making the whole area report fail. Size and
fallback list names are logged once per minute per PC. These limits are
implemented and compiled. The engine probe now measures **6,795 bytes** for
its owned bartender, with no additional lists dropped. Actual joined-PC
sizes and equipment refresh still require the owner's acceptance run.

Reproduce the completed checks with Rust 1.98.0, two Cargo build jobs, the
existing local configuration and PowerShell 7. The path to this machine's
bundled PowerShell is `MGLIVE_PWSH` in ignored `tools/live/local.ps1`.
Windows PowerShell 5.1 misdecoded a UTF-8 punctuation character in the older
shared helper; use PowerShell 7, now required explicitly by the check runner.

```powershell
. ./tools/live/local.ps1
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 1 -Next
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 1 -Next -UiTest live_targeting
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 1 -Next -UiTest live_player_look
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 1 -Next -UiTest live_viewer
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 1 -Next -Picture live_next_phase1
& $env:MGLIVE_PWSH -NoProfile -File tools/live/compile.ps1
```

Results: workspace **1,013 passed, 0 failed, 83 ignored**; mg-live **33
passed, 2 ignored**; all-target clippy `-D warnings` and formatting passed.
Each of the three explicit UI tests passed with game resources, as did the
GPU picture harness; tests opening a GPU hold `mg_testkit::gpu::hold()`.
NWScript compilation: **57 scripts, 0 failures**. Ordinary workspace UI tests
can skip under the native-game guard, so the explicit UI runs matter.
`target/test-output/live-next/phase1-checks.json` summarizes the accompanying
`phase1-*.log` files. OpenAPI was regenerated through the guarded
`MOONGLOW_UPDATE_OPENAPI=1` mg-live test and checked by the workspace suite.

Before changing the viewer, the harness rendered the existing authored
`live-mode/viewer/fixture.mod` with its old behavior. The retained evidence
is `live-next/visual/{move,attack,player}-{before,after}.png` and
`player-close-after.png` under `target/test-output/`; all seven were inspected.
They show the missing targeting feedback before, green ground ring/dashed
line and red attack ring/line after, and a rendered bare player model with
its blue marker. The wide shot was too distant for detailed model inspection,
so the close-up supplements it. The harness paints its own triangle pointer;
the app test separately asserts actual `CursorIcon::Crosshair` output.
These are GPU fixture renders, **not evidence of a real joined PC**.

### Phase 1 engine gate and owner acceptance

Read-only process inventory found the owner's existing demo: Moonglow PID
54784 (started `2026-10-12T11:04:57.1653599Z`) and the matching private WSL
`DEMO-Live-01` server PID 825, start tick 232295, scratch `~/mglive-home`,
UDP 5121. The session/ownership records match; neither process was created
by this phase. A separate private `DEMO-Statek` server PID 437 uses UDP 5122.
These identities were rechecked after the owner explicitly approved closing
Moonglow 54784, server 825 and their keeper, with no unsaved editor changes.
`tools/live/Stop-LiveDemo.ps1` matched and stopped the owned session; no
fallback PID kill was needed. Ship server 437 retained start tick 5711 and
UDP 5122 throughout. The former accepted module is 975,129 bytes with SHA-256
`2986bc30be5b3568c7364864009ec434dcac03656598c5bfbc38fda0dd9bebeb`.
It is preserved once as `live-next/accepted-before.mod`, with `recovery.json`,
until this phase's owner acceptance. The build reuses the Windows working
module, HAKs and single Linux runtime copy; there are no per-attempt archives.

The new `tools/live/test-next.ps1` refuses an existing Moonglow, a listener
on UDP 5121, or another run's keeper before any build/start/stop. Its actual
preflight refusal is saved in `live-next/engine-preflight.log`. The Python
and PowerShell harness syntax checks passed. The complete runner now passes:

```powershell
& $env:MGLIVE_PWSH -NoProfile -File tools/live/test-next.ps1
```

It builds through `check.ps1`, compiles 57 scripts, verifies the module with
zero warnings, and starts the private WSL demo with Redis prefix `mglive-p1`.
The test verifies ownership and non-PC/non-DM status before acting on its
temporary bartender. `MgLiveLook` retains native GFF JSON metadata, satisfies
the deny-list and equipment checks, and fits 32 KiB. After `EffectDeath`,
the report marks the actor dead and both the API and independently addressed
game dispatcher return `code: dead`. The actor is destroyed, its absence
checked, and the runner stops its own GUI/server/keeper. System Redis and
the ship demo are preserved. `live-next/engine.json` records the exact
module, executable and script hashes; `engine-look.json` retains the probe.

The first probe did not receive its extra diagnostic Redis publication from
inside a `run_chunk`. Reading the request result still reported zero recipients
despite a confirmed subscriber; the cause is unconfirmed. This was a test
transport failure, not a demonstrated GFF-shape failure. The final probe uses
600-character timestamped server-log lines, as the existing engine field
probes do. Its reader excludes the echoed chunk source by matching the log
prefix `] MGLIVE_LOOK `. It then performs the same structural/size checks.
The independent dispatcher refusal is still observed on Redis. All retries
reused the same working set and stopped their owned session. No production
change was needed after the offline gates. A non-PC probe does not establish
the appearance of a real joined player.

**Two-monitor acceptance is due; do not start Phase 2 yet.** From the repository
root in PowerShell 7, using the machine values in ignored `local.ps1`:

```powershell
. ./tools/live/local.ps1
& $env:MGLIVE_PWSH -NoProfile -File tools/live/Start-LiveDemo.ps1 -SkipBuild -AllowChunks
```

Bring Moonglow to the left monitor, open `demo_area`, and turn **Live** on.
Use the owner's existing isolated client setup on the right monitor to join
the private WSL server on UDP 5121. The random player password belongs only
to that new session; do not put it in the mailbox or retained logs.

* **Walk Here:** crosshair, green ground ring/dashed line, pointer/status hint;
  Shift confirms a run. Right-click cancels without opening a menu or changing
  selection; Escape cancels. A confirmed order leaves a destination flag that
  clears on arrival within 1 m, failure, or 30 seconds.
* **Attack:** a red marker and line on a valid other creature; clicking empty
  ground, the attacker or a non-creature keeps aiming. Check right-click/Escape.
* **Player:** a full model with its blue marker; change armour in the game and
  see the model update within five seconds. Record missing parts or equipment,
  and retain the once-per-minute `MGLIVE: look` size/dropped-list log if relevant.
* A dead target's refused move/script has a clear error; the API supplies
  `code: dead`. Crowd label cleanup, combat/event views, hold, stopping fights
  and raising players belong to later phases and are not Phase 1 acceptance.

Stop the acceptance session with `tools/live/Stop-LiveDemo.ps1` when finished.
Record the owner's result in the mailbox. Phases 2–6 and their assigned audit
follow-ups have not started. Passing tests is distinct from owner acceptance.

Owned-file ledger for this session (repository-relative paths):

| Path | Purpose | Status |
|---|---|---|
| `docs/live-mode-next.md` | Canonical cumulative implementation/blocker record | Required |
| `docs/index.md`, `docs/log.md` | Existing knowledge index and change log | Required |
| `target/test-output/agent-mailbox/from-astra.md` | Requested report and decision for Claude | Required, ignored |
| `target/test-output/live-next/` | Reused Phase 1 checks, before/after pictures and engine proof | Working; retain minimal final proof |
| `crates/mg-live/src/`, `crates/mg-ui/src/`, `crates/mg-ui/tests/` | Phase 1 implementation and regression fixtures | Required authored source |
| `tools/live/check.ps1`, `tools/live/nwn/` | Guarded checks and game implementation | Required authored source |
| `tools/live/test-next.ps1`, `tools/live/test-next.py` | Reproducible owned Phase 1 engine checks and preflight | Required authored source; passed |
| `docs/live-api.openapi.json` | Generated API response contract with refusal codes | Required result |
| `tools/live/local.ps1` | Existing ignored local inputs plus PowerShell runtime path | Required local configuration |
| `target/test-output/ui-area-live-targeting/area.mod` | One regenerated UI-test fixture | Working; keep until the phase is resolved |
| `target/test-output/live-demo/{module,scripts,live-scripts.erf,compile.json,inputs.json,DEMO-Live-01.mod}` | Reused pipeline and current Phase 1 candidate | Required working set; awaiting owner acceptance |
| `target/test-output/live-next/accepted-before.mod`, `recovery.json` | One 975,129-byte previous accepted module and its hash | Recovery point; remove after Phase 1 acceptance when no longer needed |

At the **initial source-preflight closure**, no temporary artifact, module copy, build output or process was created;
nothing was deleted, and **0 bytes were reclaimed**. Original inputs,
existing evidence and ignored `tools/live/local.ps1` were unchanged then. On
resumption, new source/tests and retained checks/pictures were added as above;
reproducible check logs and compile outputs were updated in place. No cleanup
deletion of retained artifacts has been performed while owner acceptance is
pending; **0 artifact bytes reclaimed**. The owned stop helper removes its
stale credentials as designed; no free-space saving is attributed to that.
No physical-allocation or free-space claim is made. Original inputs, the
accepted recovery archive and minimal current proof remain protected. Active
session history is preserved at
the agent's local Codex session file.
Its later cleanup is a separate task requiring the owner's retention
decision and a supported application mechanism; none is established here.
The active history was 23,462,011 logical bytes at 14:09 Europe/Warsaw and
continues to grow; it was not edited or trimmed.

## Why: what the first real session showed

On 2026-10-12 the owner ran the demo on two monitors (Moonglow left, the
game client right), joined with a character and recorded both screens.
What was seen (recordings `Nagrywanie ekranu 2026-10-12 131845/131846.mp4`
on the owner's machine):

1. **Live works end to end**: the age counter ticks (`Live: 0.7 s`), 38
   NPCs and the player's blue marker move with the game.
2. **The view is hard to read in a crowd**: names of nearby NPCs overlap
   into one orange blot; static module flags and arrows clutter it; nothing
   says who is fighting whom.
3. **A fight happened and the DM could not act**: the player attacked
   "19 - Street argument"; every demo NPC shares one faction, so a dozen of
   them attacked the player ("18 - First aid", "13 - Meeting", "07 - Group
   patrol"…) and killed them. Moonglow showed a cluster of markers; Live has
   no way to stop a fight, change a faction, heal or raise a player.
4. **Walk Here gives no feedback**: after choosing it, nothing shows that
   the next click picks a destination; a right click opens the menu instead
   of cancelling. The owner: *choosing Walk Here should turn on a cursor
   that shows where to click; left click accepts, right click turns it off.*
5. **The player has no model in Live**, only a blue dot (the owner: "na live
   nie widzę modelu gracza").
6. **An unexplained refusal**: `npc_move`, `run_script` and `run_chunk` on
   an owned bartender (id `20f`) returned `target is not in this area`
   seconds after `npc_say` on the same id succeeded, while the report still
   listed it (probably killed in that fight; unconfirmed).
7. **Polish text**: a line with `ł` was refused by the API as an invalid
   code point when sent from Git Bash (likely the shell's encoding, not
   Moonglow; the path to the game's CP1250 text is untested).
8. Routine NPCs override live orders (found in the morning run: "Patrol
   follower 2" acknowledged `npc_move`, then its routine resumed).

Plus the final audit's follow-ups: overlapping labels, a `hold` option in
`deferred.md`, and long one-line statements in
`apps/moonglow-mcp/src/discovery.rs`.

## Principles (unchanged)

* Additive protocol changes keep `mg-live.v1` (new optional fields, new
  ops); old games and clients keep working.
* Every new op is in the closed list, validated in Moonglow **and** the
  game, listed by `/v1/describe` (so `moonglow-mcp` gains it with no MCP
  change), in the OpenAPI file, and switched off by default when it can
  harm a player or run code.
* Visual changes: a picture before and after (CLAUDE.md); GPU tests hold
  `mg_testkit::gpu::hold()`; all cargo through `tools/live/check.ps1`
  (the `NWN_ROOT` guard); the server only in WSL, private.

## Protocol additions (all optional fields)

Report (`<prefix>.state`), per creature:

| Field | Source in the game | Meaning |
|---|---|---|
| `dead` | `GetIsDead` | dead (corpse still there) |
| `combat` | `GetIsInCombat` | in combat |
| `target` | `GetAttackTarget` (id) | whom it attacks |
| `hostile`, `friendly` | `GetIsEnemy(o, pc)` / `GetIsFriend(o, pc)` for each PC in the area | relationship IDs, now including creatures outside combat; absent `friendly` means unknown in old reports |
| `faction` | `NWNX_Creature_GetFaction(o)` | module faction index, named through `repute.fac` |
| `look`, `lookHash` | PCs only, full reports only: a trimmed `ObjectToJson(pc)` and `HashString` of it | the player's appearance and equipment |

Report, per area: `events` — speech heard since the last post (see
Phase 3), at most 64, each `{t, kind:"speech", id, name, text, volume}`.

Results: a stable `code` beside `err`: `not_found`, `other_area`, `dead`,
`protected`, `invalid`, `disabled`, `unknown_script`, `compile_error`,
`expired` (Moonglow's own refusals use the same codes).

New ops (each with policy and validation; see Phase 4): `hold` flag on
`npc_move`/`npc_attack`; `stop_combat`, `set_faction`, `freeze`,
`unfreeze`; `npc_say` gains `volume`; `spawn` gains `faction` and `count`;
player ops `pc_heal`, `pc_raise`, `pc_teleport`, `pc_give_gold`,
`pc_give_xp`, `pc_give_item` behind a switch.

## Phase 1 — targeting cursor, player model, clear errors (M)

**1.1 Walk Here / Attack targeting mode** (`crates/mg-ui/src/live_view.rs`,
`input()` and `overlay()`):

* While `view.live.aim` is set and the pointer is over the view: pointer
  icon `CursorIcon::Crosshair`; Walk Here draws a ring on the ground at
  `view.ground_at(pos, 0.)` and a dashed line from the NPC's marker; Attack
  highlights the creature marker under the pointer in red with a line from
  the attacker (non-creatures and the attacker itself are not targets). A
  label by the pointer: "Walk here — left click: go (Shift: run) · right
  click / Esc: cancel", "Attack <name> — left click: attack · right click /
  Esc: cancel"; the status line says the same.
* **Left click** confirms and ends the mode. **Right click** while aiming
  cancels only: no context menu that frame, no selection change (call
  `response.context_menu` only when no aim was active and nothing was
  cancelled this frame). Escape cancels.
* After a Walk Here: a destination flag stays until the NPC is within 1 m,
  the result fails, or 30 s pass.

**1.2 The player's model** (game + store + view):

* Game (`tools/live/nwn/mglive_inc.nss`): in full reports, for each PC
  (not DMs), copy the whole top-level `ObjectToJson(pc)` struct using a
  **deny-list**, preserving `__data_type` and the GFF JSON structure.
  Remove `ItemList`, `FeatList`, `SkillList`, `ClassList`, `LvlStatList`,
  `SpecAbilityList`, `VarTable`, `Effects`/`EffectList`, `PerceptionList`,
  `CombatInfo`, `ActionList`, `ExpressionList`, `PersonalRepList`,
  `ReputationList`, `QBList`, and `ExploreList`/explored-area data.
  In `Equip_ItemList` keep head (0x1), chest (0x2), right (0x10) and left
  hand (0x20), cloak (0x40); copy each item except
  `PropertiesList`/`ItemList`/`VarTable`. This preserves `ArmorPart_RFoot`,
  every EE `x*` wide twin, `Appearance_Head` and future preview fields.
  Measure other lists and drop irrelevant ones over a few KiB. If the
  result is over 32 KiB, drop the largest remaining lists and record which.
  `lookHash = HashString(JsonDump(look))`; log size and dropped lists once
  a minute per PC. Add bare-right-foot and wide-only-part (>255) fixtures.
* Store (`crates/mg-live/src/store.rs`): `LiveObject.look:
  Option<Value>`, `look_hash: Option<i32>`; a look over 64 KB refuses the
  post; the last look per id is kept across quick reports.
* View (`live_view.rs` `frame()`): today PCs are skipped (`if o.pc ||
  o.dm { continue; }`). For a PC with a look: `mg_gff::json::from_json`
  (`crates/mg-gff/src/json.rs:122`) → root struct →
  `AreaObject::read(game, ObjectKind::Creature, usize::MAX, &s)`; the
  preview already draws equipment embedded in `Equip_ItemList`
  (`crates/mg-preview/src/creature.rs` `equipped()`, "the item struct
  itself (area instances and characters)"). Cache by `(id, look_hash)`;
  keep the blue marker and name; a parse failure keeps the dot and is
  logged once per id.

**1.3 Clear errors and `dead`**: the game reports `dead`, `combat`,
`target` (and `hostile` for creatures in combat); `MgLiveRun` returns
`code` per the table above, checking in this order: id form, object
exists, same area, dead (for move/attack/say/scripts on a dead target),
protected. Moonglow refuses commands on a target its last report shows
`dead` (`code: dead`), except `destroy`.

Tests: app tests for the targeting mode (right click → nothing queued,
aim cleared, no menu, selection unchanged; left click → `npc_move` at the
point; Attack on empty ground → still aiming; on a creature →
`npc_attack`); store tests (look kept, oversize and truncated refused,
`code` values); a fixture-look unit test giving an `AreaObject` with a
preview; screens pictures of aiming and of a PC model; WSL end to end:
an owned creature killed (`run_chunk` with `EffectDeath`) then `npc_move`
→ `code: dead`; the owner joins and sees their model, changes armour and
sees it within 5 s.

## Owner acceptance of Phase 1 (2026-10-12) and plan changes

The owner ran Phase 1 on two monitors (Moonglow left, client right). Findings, with what the live state
showed at the time (`/live/v1/status`, `/live/v1/area/demo_area`):

1. **Attack does nothing visible.** Two `npc_attack` commands were queued and the game answered `ok`,
   yet the attacker did not fight. Cause (likely): the demo NPCs share one faction, and NWN's combat AI
   drops an attack on a target it does not see as an enemy; routines also clear actions. → Phase 1b.1.
2. **Nothing shows who is fighting.** 15 creatures reported `combat: true` with `target`s; the view
   draws none of it. This is Phase 2.3 (not yet built); the owner confirmed it is needed.
3. **Colours should follow factions**, with an option to map factions to colours. → Phase 2.3a.
4. **A dead player stands upright.** The report had `dead: true`, HP −11 and the look; the model plays
   its idle animation. → Phase 1b.2.
5. **Placeables cannot be selected or managed in Live** (rotation, visual transform…). Matched module
   placeables have no marker, and there are no placeable operations. → new Phase 4b.
6. **"Can a script be sent over Redis and run?"** Yes — `run_script` (a compiled module script) and
   `run_chunk` (any NWScript, compiled by the game) go client → Moonglow API → Redis → game; the
   morning run proved both. Asked as important for LLMs → Phase 5 adds return values and an LLM recipe.
7. **No passwords on a hidden server.** The owner: do not set player and DM passwords while the server
   is private (`-publicserver 0`); it is reachable only from this machine (WSL NAT). → Phase 1b.3.
8. Several clicks spawned blueprints while a palette entry was still chosen (28 spawns in the session):
   spawning on every click is too easy to trigger by accident. → Phase 1b.4.

### Phase 1b — fixes from the owner's acceptance (M, before Phase 2)

1. **Attack that attacks** (`tools/live/nwn/mglive_exec.nss`; amended after
   the same-faction engine finding): different factions keep the two
   `SetIsTemporaryEnemy(..., TRUE, 180.0)` calls. For `GetFactionEqual`, save
   the attacker's faction with `NWNX_Creature_GetFaction`, then
   `ChangeToStandardFaction(attacker, STANDARD_FACTION_HOSTILE)`. Clear actions,
   queue `ActionAttack` and a bounded hold; restore the saved faction with
   `NWNX_Creature_SetFaction` at hold/action end, timeout, victim death or any
   new accepted order on the attacker. Fresh boots need no restoration.
   **While switched, the attacker is hostile to everyone, players included.**
   Return `"faction":"hostile_temporarily"` for this path. Enable the Creature
   plugin/include. Both `npc_move` and `npc_attack` expose `hold`/`timeout` in
   the store, schemas and OpenAPI; same-faction attacks always enable the hold.
   The engine must show `GetAttackTarget == victim` and `combat:true` within
   6 seconds for a same-faction pair, then restoration after timeout. If the
   switched attack is still dropped, investigate heartbeat/ambient AI and
   ensure the hold retains the action before asking for another decision.
2. **Dead creatures lie down** (`crates/mg-ui/src/live_view.rs` + preview): a reported `dead` creature
   (PC or NPC) is drawn with its death pose — the model's `dead` animation if it has one, else the last
   frame of `die`, else lying (rotated) as a last resort; the choice comes from the model's animation
   list. Check the animation names against the game's creature supermodels before coding. Picture
   before/after (a dead PC and a dead NPC).
3. **Private demo without passwords** (`tools/live/wsl/start-server.sh`, `Start-LiveDemo.ps1`): with
   `-publicserver 0`, start without `-playerpassword` and `-dmpassword` (empty: anyone on this machine
   joins as player or DM); keep a random `-adminpassword` and never print it. A switch `-Passwords`
   restores random player/DM passwords. If a public server is ever requested, passwords are mandatory
   and the script refuses otherwise. (Owner's decision 2026-10-12, for this demo tooling; the
   repository's engine tests keep their private-with-random-passwords rule.)
4. **Spawning on purpose**: in Live, a click with a palette entry chosen does not spawn at once; it shows
   the spawn ghost and a hint "Spawn <resref> — left click: place · right click / Esc: cancel" (the same
   targeting mode as Walk Here), and the mode ends after one spawn unless Shift is held (Shift: keep
   spawning). Dragging from the palette still spawns at the drop point.

5. **One validation path** (mailbox addition, 17:00): the legacy
   `POST /live/v1/command` runs the same `operations::validate` schema limits
   as `/v1/call`; test the 200/201-character speech boundary on both routes.

Tests: engine — same-faction attack really fights; dead PC/NPC pose picture; demo starts and the client
joins without a password (manual, owner); app tests for the spawn targeting mode (left places once,
Shift keeps it, right cancels with nothing queued).

### Additions to Phase 2

**2.3a Faction colours**: the game reports `faction` per creature (`NWNX_Creature_GetFaction`, the
module's faction index; add `NWNX_CREATURE_SKIP=n` to `start-server.sh`). Moonglow names it from the
module's `repute.fac` (the same data the faction editor reads, `crates/mg-ui/src/faction_view.rs`).
A **colour mode** in the Live toolbar: *Faction* (default), *Hostility to the selected player*, *Kind*.
Faction colours: a default palette (players blue, hostile red, merchant gold, commoner orange,
defender green, other module factions from a fixed distinct palette), editable in **Options › Live ›
Factions** (a list of the open module's factions with a colour each, saved per module). The legend
shows the current mapping. Combat (2.3) stays on top of any mode: red ring when in combat, a line to
the attack target.

### Phase 4b — placeables and doors in Live (M)

* **Select them**: every reported placeable and door can be selected in Live — by clicking its model
  (the area view's existing ray pick on the model, mapped to the reported id through `reconcile`'s
  `matched`) or its marker; a small marker is shown for reported placeables in the *Near and selected*
  label mode and when the Live panel's Placeables filter is on.
* **Operations** (closed list, validated on both sides, not on objects a player possesses):
  `set_facing {target, f}` (`SetFacing`); `set_visual_transform {target, scale?, rotate_x?, rotate_y?,
  rotate_z?, translate_x?, translate_y?, translate_z?, lerp_seconds?}` (`SetObjectVisualTransform`
  with `OBJECT_VISUAL_TRANSFORM_*`, `ClearObjectVisualTransform` for reset) — creatures too;
  `set_position {target, x, y, z}` (`NWNX_Object_SetPosition`, add `NWNX_OBJECT_SKIP=n`; refused for
  doors); `play_animation {target, animation: activate|deactivate|open|close}`; `set_useable {target,
  useable}`; `set_name {target, name}`; `set_locked` stays in `set_state`.
* **Report**: placeables and doors add `vt` (current visual transform: scale, rotate, translate) and
  `useable`; Moonglow applies `vt` to the drawn model (`AreaObject::visual`) and position/facing from
  the report, so a moved or rotated placeable shows where it is.
* **UI**: a Live properties panel for the selected placeable/door: facing dial, scale and rotate
  sliders (sent on release), position X/Y/Z, Useable, Name, buttons Open/Close/Activate/Deactivate,
  Lock/Unlock, Reset transform; the editor's Q/E turn keys and drag move the selected live object as
  commands (drag sends one `set_position` on release, not per frame).
* Tests: ops validation; engine — rotate, scale and move a placeable and read them back from the report;
  pictures of the panel and of a transformed placeable.

### Additions to Phase 5 (scripts for LLMs)

* **`run_chunk` with a return value**: optional `return: true`; the game wraps the code so it can set a
  module string `MGLIVE_RETURN` (a helper `void MgReturn(string s)` included in the chunk's prelude),
  reads it after `ExecuteScriptChunk`, clears it, and returns it as `value` (≤ 16 KB) in the result.
  `live_wait` passes `value` through. This lets an LLM query the game (e.g. a creature's feats or
  local variables) without new ops.
* **An LLM recipe** in `docs/agent-protocol.md`: discover with `live_summary`/`live_find`, act with the
  typed ops, use `run_chunk` for the rest, read results with `live_wait`; the two switches chunks need.

## Phase 2 — a readable view (L)

**2.1 Labels** (`live_view.rs` `overlay()`): a label mode in the Live
toolbar — *All*, *Near and selected* (default: within 15 m of the
selected object or the camera target, plus the selected one), *Hover
only*, *None*; collision avoidance (labels placed greedily in screen space
by priority — selected, players, in combat, own, others — and a label that
would overlap an already placed one is dropped, its marker kept); labels
fade out beyond a camera distance.

**2.2 The Live panel** (new dock tab, `crates/mg-ui/src/live_panel.rs`):
a table of the reported objects of the open area — name, tag, kind, HP,
action (named from `GetCurrentAction` values), state (dead, in combat,
own), distance to the selected object; search box (name, tag, resref);
filter chips: Players, NPCs, Own, Hostile, In combat, Dead, Wounded (<50%
HP), Placeables, Doors, Items; a "hide module markers" switch that hides
the module's static flags/arrows while Live is on. Click selects and
centres the camera; double click **follows** it (the camera target moves
with the object every frame; leaving the area follows to the area it
enters when that area is open, else stops with a notice). The same
filters apply to the markers in the view.

**2.3 Combat shown**: a red ring on creatures `combat`; a line from each
attacker to its `target`; a marker hostile to the selected player drawn
red, neutral orange, friendly green; dead as a grey ✕ (name kept); a
small legend (toggle) in the view's corner.

**2.4 Smooth movement**: positions and facing interpolated between the
last two reports (linear, over the measured report interval; snap when the
jump is over 10 m); an optional trail (last 5 s) for the selected object.

**2.5 Audit follow-up**: `apps/moonglow-mcp/src/discovery.rs` long
one-line statements split (no behaviour change).

Tests: label placement unit tests (no two placed labels overlap; priority
order kept); interpolation unit tests; panel app tests (search, each
filter, click centres, follow moves the camera); screens pictures: the
crowd from the recording before/after labels; combat lines and colours.

## Phase 3 — events (M)

**3.1 Derived in Moonglow** (`store.rs`, from consecutive reports, no game
change): `entered`/`left` (an object appears or disappears; players
named), `damaged` (HP drop, amount), `healed`, `died` (`dead` false→true),
`combat_start`/`combat_end`, `target_changed`; plus command results
(`queued`, `ok`, `failed` with code). Kept in a ring of 1,000 per area with
increasing sequence numbers.

**3.2 Speech from the game**: `NWNX_Chat_RegisterChatScript("mglive_chat")`
(plugin `NWNX_Chat`, built in WSL; add `NWNX_CHAT_SKIP=n` to
`start-server.sh`); `mglive_chat.nss` appends `{kind:"speech", id, name,
text, volume}` to a module JSON buffer (≤ 64 entries, oldest dropped) sent
with the next report and then cleared. Verify on first run whether NPC
`SpeakString` passes through NWNX_Chat; if not, `mglive_exec`'s `npc_say`
adds its own speech event.

**3.3 Event panel** (dock tab, beside the Live panel): newest first,
filter by kind and by object (the selected one), click selects the object;
speech also as a bubble over the marker for 4 s.

**3.4 For LLMs** (`crates/mg-live/src/operations.rs`, so `/v1/describe`
and `moonglow-mcp` get them): `live_events {area?, since?, kinds?,
limit≤200}` → events after a sequence number (and the next `since`);
`live_find {area, text?, kind?, pc?, combat?, dead?, near_x?, near_y?,
radius?}` → matching objects only; `live_summary {area}` → counts by kind
and state, players with HP and position, fights in progress (attacker →
target), last 10 events — a few hundred bytes instead of the full report.

Tests: derivation unit tests (each event from a pair of reports);
ring bounds; operations' schemas in the portable subset; MCP discovery
test lists the three new tools from a fake describe; WSL: a fight produces
`combat_start`, `damaged`, `died` events; a spoken line appears.

## Phase 4 — the DM's controls (L)

Each op: Moonglow validation, game validation, `code` on refusal, a
context-menu entry and an API/MCP tool.

* **`hold: true` on `npc_move`/`npc_attack`**: the game runs
  `SetCommandable(FALSE)` after queuing the action, appends
  `ActionDoCommand(SetCommandable(TRUE))`, and a `DelayCommand(timeout)`
  safety restore (default 30 s, ≤ 120 s). The view's Walk Here gains
  "Walk Here (hold)" and holding Ctrl while confirming sets it. This fixes
  routine NPCs overriding orders (item 8).
* **`stop_combat {area, x, y, radius ≤ 50}`**: for creatures in the
  radius (not players): `ClearAllActions(TRUE)`, `ClearPersonalReputation`
  towards every PC in the area, and `SurrenderToEnemies()` once on a
  creature of each faction involved. View: context menu on the ground
  "Stop fighting here (radius…)".
* **`set_faction {target, faction: hostile|commoner|merchant|defender}`**
  → `ChangeToStandardFaction`; not on players.
* **`freeze {target, seconds ≤ 300}`** → `EffectCutsceneParalyze` for the
  duration, tagged so **`unfreeze`** removes it; not on players unless the
  player switch is on.
* **`npc_say` `volume: talk|whisper|shout`** → `SpeakString(text,
  TALKVOLUME_*)`.
* **`spawn` `faction` and `count ≤ 10`** (placed in a small circle);
  the view gets "Spawn at player" in a player's context menu and a palette
  search box in Live.
* **Player ops, behind a new switch** — Options › Live › *Allow actions on
  players* (off) **and** the server's `MGLIVE_ALLOW_PLAYER_OPS=1` (off):
  `pc_heal {target, amount?}` (`EffectHeal`, full if no amount),
  `pc_raise {target}` (`EffectResurrection` + full heal),
  `pc_teleport {target, x, y, z}` (`AssignCommand(JumpToLocation)`, same
  area only), `pc_give_gold {target, amount ≤ 1,000,000}`,
  `pc_give_xp {target, amount ≤ 100,000}`, `pc_give_item {target, r}`
  (`CreateItemOnObject`). Every one logged on both sides. `destroy` and
  `run_*` stay as they are for players (`run_*` already allowed by the
  owner's design; `destroy` never).
* **Multi-select** in the view (Shift+drag box, Ctrl+click) and group
  commands: walk (formation offsets kept), stop, freeze, destroy own,
  set faction. One command per object, one undo-free batch.

Tests: store validation per op and per switch; game: each op in the WSL
end-to-end test (a spawned guard made hostile then `stop_combat` ends the
fight; `hold` keeps a routine NPC on its order where it failed before;
player ops refused without the switches and working with both — the test
player is a creature the test owns, or the owner's manual join); app
tests for multi-select and the new menu entries; pictures of the menus.

## Phase 5 — text, Options and tokens (M)

* **Polish text end to end**: test `npc_say` with `ąćęłńóśźż ĄĆĘŁŃÓŚŹŻ`
  sent as UTF-8 by curl.exe from PowerShell and by `moonglow-mcp`; read
  what the game shows (client bubble, server log). If it arrives wrong,
  fix in one place: Moonglow converts text fields of commands from UTF-8
  to the module's codepage (`mg_core` `Codepage`, the module's language)
  before publishing, and converts report strings back; document it.
  Also make the Git Bash examples in the manual use `--data-binary @file`
  with UTF-8 files.
* **Options › Live › Keep WSL running while Live is on** (Windows only;
  default on when the Redis host is loopback): Moonglow starts one owned
  `wsl.exe -e sleep infinity` child and stops it with the service.
* **Scoped tokens**: Options › Live › *Clients* — named tokens (e.g. "DM",
  "LLM") each with allowed groups (`read`, `npc`, `spawn`, `scripts`,
  `chunks`, `players`) and a rate limit (commands per minute, default 60);
  the game's single token stays for the Redis side only. Every API/MCP
  command is appended to `<data_dir>/live/audit.jsonl` (time, client
  name, op, target, result code; never the token). `moonglow-mcp`'s
  descriptor carries only the default client's token; others are given to
  their clients by the owner.

Tests: codepage round trip unit tests; API tests per scope and rate limit
(429 with `Retry-After`); audit lines written; WSL: the Polish line shows
correctly (picture of the client or server log line).

## Phase 6 — documentation (S)

Manual `04-areas.md` (targeting, labels, the Live panel, filters, combat
colours, events, multi-select, the new menu entries), `09-build-and-test.md`
(new server variables `MGLIVE_ALLOW_PLAYER_OPS`, `NWNX_CHAT_SKIP`),
`10-options.md` (players switch, keep WSL, client tokens),
`11-command-line.md` and `docs/agent-protocol.md` (new ops and fields,
`code`s, `live_events`/`live_find`/`live_summary`), the OpenAPI file,
`deferred.md` (what moved out of it: hold, labels), `log.md`, this
document's status.

## E4.4 working record and file ownership

Session `01a12fe8-1458-7551-99fc-b277832def84` continues on `live-mode`,
base `5536e66`. Initial work protected the running owner acceptance: Moonglow
38104, WSL server 17185 and log-tail 17324; no demo lifecycle or executable
builds were performed then. The coordinator's resume message says acceptance
has ended (WSL restarted) and releases normal guarded app builds. This task
has no server session to close. `commands.rs` remains Claude's. Checks use
`tools/live/check.ps1 -Jobs 1 -NoUiDebug -UiUnoptimized`, focused `mg-ui`
tests/pictures, `-Lint`, and the final full workspace/build gate.
All GPU harnesses hold `mg_testkit::gpu::hold()`.

The current ownership ledger is
`target/test-output/live-next/phase4-owned-files.json`: protected input hashes,
session, and new proof paths. New source is `crates/mg-ui/src/live_effects.rs`;
edits belong in live_view/area_view, app/screens tests, check.ps1 and existing
documentation. The one tiny app fixture is
`target/test-output/ui-area-live-effects/area.mod`; screenshots and check logs
use the existing `live-next` working set, no MOD/HAK/resource copies. Required
before/after evidence stays pending review. The existing demo module and
morning report are read-only picture inputs; synthetic effect/event additions
are explicitly UI fixtures, not a new engine observation.

Implementation items: durable marker fallback and Effects control; Full models
and particles with budgets; event arcs/flashes; four-second speech bubbles.
Each gets a local checked commit and a mailbox entry. Default **Markers** is
the owner's updated instruction and supersedes the roadmap's former Off.
No native client render or owner acceptance is inferred from app tests.

Item 1 implements the per-view `live.effects: live_effects::EffectsMode`
control (Off / Markers / Full, default Markers), marker rings coloured from
the effect label, names, and unknown-row fallback. The frame key includes
the mode; filters and existing label collision/priority rules still apply.
Item 2 below supplies Full models and particles. No procedural shader emulation is
claimed for ProgFX glows. Claude has the public field/type for his Live menu.

Reproduction (after sourcing `tools/live/local.ps1`, invoking its PS7):
`check.ps1 -Phase 4 -Next -Step markers -Jobs 1 -NoUiDebug -UiUnoptimized`
with each of `-Lint`, `-UiTest live_effects`,
`-Picture live_effects_snapshot`, and `-Package mg-ui -Filter live_effects`.
`MGLIVE_PICTURE_STAGE=before|after` names the captured PNG. One pure unit
check and one real-install app control check pass; the picture pair verifies
red/blue glows and the unknown row without models. A fixture query needed
`get_by_value` for the ComboBox; an unsupported star glyph was replaced by
an ASCII star. Existing 1100-pixel app harness remains usable. Evidence is
`live-next/phase4-markers-checks.json` and `visual/effects-{before,after}.png`.
Initial lint visibility and query mistakes were corrected before closure.

Item 1 is committed as **3cf49a8**. Item 2 (Full models/particles) resumed after the coordinator explicitly asked
to continue and released the acceptance binaries. The source finding below
is retained; explicit emitter identity replaces the incorrect heuristic.
It wires size from the source UTC/player look's `Appearance_Type` and
`appearance.2da SIZECATEGORY`, uses `with_models` on cloned previews, limits
attachments to 64 nearest-first, and calls `ghost_particles` with each extra's
ID and a separate simulation map. Live particle output is capped at 20,000
six-vertex quads/segments nearest-first; marker fallback remains visible.
Mode changes, reboot and disappearing objects retire particle state. Animations
Off suppresses particles. The display counter now divides vertices by six,
matching the particle renderer (previously four).

**Wrong-fact stop, 2026-10-12 20:47:** the supplied E4.2 contract says only
base emitters are warmed 20 seconds and effect parts start on arrival. Source
inspection disproves this in the general root-effect case:
`mg-preview/src/compose.rs::all_emitters` omits a non-emitting base and emits
only emitting parts; a root part with scale 1 has identity `place`.
`mg-area/src/scene.rs:696` tests `new && n == 0 && e.place == Mat4::IDENTITY`
to warm up. Such a first root effect therefore receives the base warm-up.
This is a source-level counterexample, not a new engine observation.

Proposed correction for Claude's decision: add explicit base/part identity
to `Emitting` (true for the base, false for every part), and use that identity
for warm-up in `ghost_particles`. Add a root-effect regression proving a new
effect does not run ahead while a genuine base emitter still does. The initial turn stopped under the standing wrong-fact rule. After the
coordinator resumed the task, `Emitting::is_base` was added and the API
uses it for warm-up; a real-install app regression compares a root-attached continuous flame
with an unadvanced simulator, and the same model as a warmed base.
Darkness proved unsuitable for that timing comparison: it has a Single
emitter whose initial burst is valid at dt=0. The first Full picture was weak (Darkness behind a tree). The retained
pair replaces that rejected trial: same unobstructed paving/camera, Markers
before versus Full after. No extra module or resource copy was needed.
Read-only `mg cat --no-user-dir ... --text` inspection of `vdr_darkness.mdl`
also found transparent rest state and impact/duration/cessation animations.
`Part::animation`, set to duration by `with_models`, now selects an effect
part's own duration animation (impact fallback) in both poses and emitters,
independently of the creature's gait. This fixes the invisible lasting effect.

The Full picture pair passed visual review: Darkness is visibly drawn in
Full, while red glow and unknown row keep rings. Full item checks: workspace clippy/fmt clean, two pure
`live_effects` tests, three real-install app checks (mode control, emitter warm-up identity,
and 64-model/particle caps, repaint, mode/removal cleanup), and the Full screenshot harness
passed, including the added arrival-time regression. Commands
are the item 1 commands with `-Step full` and
`-Picture live_full_effects_snapshot`. Current results/hashes are recorded in `live-next/phase4-full-checks.json`.
At this item checkpoint executable and demo hashes remained unchanged; no
demo process was stopped, app executable built or server command sent.
Transient projectiles/flashes and speech are completed in the items below.

Current sources, the tiny UI fixture and minimal before/after PNGs/check logs
are retained for audit; no new full module or asset copy was made.
No files were deleted and **0 bytes** are claimed reclaimed. Exact file paths,
logical lengths and purposes are in the small ownership ledger. The active
history remains at
the agent's local Codex session file;
it is not closed or eligible for in-session trimming. Any eventual history
cleanup needs the separately authorized retention workflow; none was run.

Item 2 is committed as **2e99c72**. The next item adds `live_cues.rs`, consuming
`LiveStore::events` since its last `n` in the current area. Opening/re-enabling
Live and server boot changes establish a cursor without replaying history.
Effects Off advances that cursor while discarding cues. Up to 128 cues live
for 0.5–1 second of projectile travel plus a 0.4-second flash (damage and instant
casts flash immediately); receipt age reduces remaining lifetime. The overlay
uses monotonic UI time and does not invalidate the static 3D cache. Filters
apply to known participants. Spell/projectile notifications with matching
source/target/spell/position within 250 ms of receipt coalesce, whichever arrives
first; the explicit projectile wins. This bounded heuristic is necessary because
the wire has no cast-instance ID. Damage shows the reported amount, not inferred
HP loss. No native instant effect models or impact success are invented.

Transient proof commands use `-Step events`, `-Package mg-ui -Filter live_cues`,
`-UiTest live_events_view`, `-Picture live_event_effects_snapshot`, and `-Lint`,
with the same guard/profile arguments. The synthetic picture sends a spell and
its matching projectile plus a separate damage event after the view's cursor is
initialized. Its before/after paths are `visual/event-effects-{before,after}.png`.
The two unit tests and the real-install app check pass; the latter proves
that overlay animation does not redraw a static 3D scene. The inspected picture
shows one coalesced projectile and a damage ring/amount. Flash brightness is
held until its last 100 ms, then fades. These cues are used in Markers and Full;
general native instantaneous VFX remains deferred under the event-wire decision.

Item 3 is committed as **4ece26b**. Item 4 adds four-second speech bubbles
from new `chat` events, independent of the Effects mode. One bubble per speaker
replaces its predecessor; at most 128 remain. Receipt age shortens the lifetime,
server boot changes clear it, and missing/filtered speakers are not drawn.
Text is capped at 240 Unicode characters plus an ellipsis and whitespace is
collapsed; the complete event remains in Live Log. Bubbles follow interpolated
positions, favour the selected speaker and reserve rectangles against markers,
the legend, other bubbles and names. Expiry schedules an overlay repaint without
invalidating the static GPU image.

Speech proof reuses the same module, camera and app fixture. New owned evidence:
`visual/speech-{before,after}.png`, `phase4-speech-*.log` and
`phase4-speech-checks.json`. Reproduce with the standard guard/profile arguments,
`-Step speech -Package mg-ui -Filter live_cues`, `-UiTest live_speech`,
`-Picture live_speech_snapshot` and `-Lint`. The unit fixture covers Polish
characters, replacement, receipt age, the display cap, expiry and reboot;
the real-install app fixture checks expiry and static GPU-image reuse.
All three event/speech unit tests and the speech app check pass; before/after
pictures were inspected and show readable Polish bubbles above both speakers
without covering their names or markers. Lint/fmt passes. The final E4 gate
uses `-Step final` without a selector, followed by `-Build`; results are recorded
below and in `phase4-speech-checks.json`. The coordinator adds the Effects menu
using `AreaView.live.effects: mg_ui::live_effects::EffectsMode` (`ALL`, `name()`).
This task stops for that merge and the owner's E4 acceptance; it does not start
the demo or proceed into E5/E6.

Final E4 gate: **1,078 workspace tests passed / 0 failed / 97 ignored**;
mg-live **58 passed / 3 ignored**; workspace clippy/fmt clean; `mg`, `moonglow`
and `moonglow-mcp` built. The final real-install transient app regression also
passes after the shared cursor gained chat. All Cargo used the guard and one
job; the UI's unoptimized functional-test profile is not a performance result.
The demo MOD remains byte-identical. No game server/client was started or
stopped by this E4 task. `phase4-{markers,full,events,speech}-checks.json` binds
each item to its committed Git source and retained evidence; source hashes use
Git's LF blobs, evidence hashes use the retained files' bytes.

**E4 owner acceptance is due after Claude's menu merge and merged-tree gate.**
Use `Start-LiveDemo.ps1 -Areas demo_area -AllowChunks -Quiet` from the
configured PS7 shell and the existing isolated client setup. On two monitors,
compare Markers/Full/Off with temporary red/blue glows and Darkness on owned
test creatures; Full adds models/particles while model-less glows retain rings.
Check particles while moving, removal at effect expiry, projectiles and damage
flashes during a real cast, and Polish NPC speech that disappears after four
seconds while Live Log retains it. Off hides VFX but keeps speech. Re-enable
Live without replaying old events. The coordinator's mailbox has the detailed
launch/check instructions. General instant VFX and exact native appearance are
not claimed; this stop is for the planned merge and acceptance, not a blocker.

Cleanup accounting: eight current before/after PNGs, the one small app fixture,
current check logs/manifests and required sources remain for review. Rejected
iterations reused these same paths. No redundant own artifact remains identified
for deletion, and **0 logical bytes reclaimed** are claimed; disk free-space
changes are not attributed to this task. Shared build outputs are needed for
acceptance and other tasks. The ownership ledger lists exact paths and logical
sizes. The active session history named above is untouched; separate history
cleanup remains outside this session without an approved retention mechanism.

## E7/E8 UI batch after E4 acceptance

The coordinator accepted E4.4 pictures and merged Astra2 at `9ef5692`.
Owner E2/E3/E4 acceptance now follows the DM-actions batch. This UI-only batch
implements Live Options, Scenes, then the E7.5 agent recipe, one checked local
commit per item. mg-live, game scripts and commands/keys remain with their owners.
Same session, guard and one-job UI profile as E4. Working-file ledger:
`target/test-output/live-next/phase7-ui-owned-files.json`; one current set of
pictures/logs, no demo lifecycle or extra game-data copies.

Item 1 adds `live_clients.rs`: Windows Keep WSL running reflects the config's
automatic setting until changed; an explicit click stores Some(bool). Named
clients expose name, eight scope groups, per-minute quota, token generation/copy
and removal inside the Options draft. Tokens are never displayed. Apply/OK are
disabled on invalid client names/credentials/limits; service enforcement remains
in mg-live. Cancel discards edits; Apply restarts Live through its existing path.
Open audit folder links to the data directory's live folder (`audit.jsonl` is
created when the service publishes its descriptor).

Checks: workspace clippy/fmt clean; `-UiTest live_clients` passes draft isolation,
reserved-name refusal, rotation/copy, scopes, quota/settings round-trip, Apply,
Cancel and no listener/WSL child with Live disabled. The inspected before/after
pictures show the checkbox, all scopes and audit link without clipping or secrets.
Reproduce with `check.ps1 -Phase 7 -Next -Step clients -Jobs 1 -NoUiDebug
-UiUnoptimized`, separately `-Lint`, `-UiTest live_clients` and
`-Picture live_clients_snapshot`. `MGLIVE_PICTURE_STAGE=before|after` names
`visual/clients-*.png`; before was rendered before the production edit.
An initial collapsible-if lint diagnostic was fixed before the passing gate.

Item 2 adds the module-scoped Scene window, opened with
`Action::OpenTab(Tab::Scene)` (the coordinator owns the Live menu entry).
`live_scene.rs` lists, loads and saves through `mg_live::scene`, offers the
current `operations::describe` catalogue, and edits JSON, delays and result/near
waits. Every step is checked against the discovered schema and typed command
constraints before Save/Play; disabled operations stay unavailable. The core's
schema validator is private, so the UI performs a read-only preflight and the
core validates again at dispatch. No validation command is enqueued.

The shared manual submission hook is `mg_ui::live_scene::enqueue(app, command)`;
Live view commands use it and future DM controls should too. Recording appends
only successfully queued commands, strips idempotency IDs and measures the gap
since the preceding command (first delay zero). Script commands with no target
omit that field in the agent input. Playback calls the core Player directly and
does not record itself. A gap over ten minutes or a 201st step stops recording
with a visible notice. Stop, disabling Live, closing/changing the module and a
changed server boot stop playback/recording. Stop does not undo commands already
sent. In-memory drafts survive module switching; explicit Save persists them.

Scenes live at `<data_dir>/live/scenes/<module filename without extension>/`.
Module/scene names use the existing core's bounded ASCII filename rules;
modules with the same basename share this folder. Object IDs remain bound to the
running server boot: review/refresh targets before replay after a restart. This
UI does not add durable object aliases or persist unsaved drafts across app exit.

Reproduction uses the same guard/profile with `-Phase 8 -Next -Step scene`,
`-Package mg-ui -Filter live_scene`, `-Lint` and
`-Picture live_scene_snapshot`. The fake-store app test records manual commands,
rejects malformed JSON, saves/lists/loads, plays, stops and retains a module's
draft. No listener or game process is used. The unit check covers recording gaps,
ID removal and whole-scene preflight. `visual/scene-{before,after}.png` reuse the
existing area fixture; the after picture shows two complete steps, Polish text,
result/near waits and controls without clipping. The tiny owned app-test module
and saved scene are retained in `target/test-output/ui-live-scene/`; no game
assets were copied. Evidence is bound in `phase8-scene-checks.json`.

Item 3 adds the [LLM recipe](agent-protocol.md#llm-recipe-discover-observe-act-and-verify-e75):
select/discover the intended session, check boot and report freshness, find and
read objects, consume `live_events` with independent cursors, submit/wait and
verify the observed effect. It distinguishes outer HTTP success from native
`ok`/`err`, queued typed queries, chunk returns, policy/quota refusals, scope
isolation and the bounded history window. The seven marked JSON inputs were
parsed and checked against `docs/live-api.openapi.json` (types, required/unknown
fields, unions, enums, lengths and numeric bounds); relative file links exist.
`phase7-recipe-checks.json` records the inputs and document/schema hashes. This
is source/schema validation, not a claim that example actions ran in the game.
The OKF index and log were maintained directly because `okf` is unavailable.

A core follow-up was handed to its owner without editing mg-live: `scene::play`
reads a failed result's `error`, while `CommandResult` serializes `err`. Playback
stops correctly, but its status currently reports generic "failed"; Live Log
retains the actual native reason. The coordinator also has the Live menu entry
and a reported-position arrival fix in his separate branch. This batch leaves
both merge-owned areas alone; no additional permission is required for its UI.

**Earlier batch stop (resolved by `57f7859`): MCP schema contract mismatch.**
UI commits are `ca87d6a`
(Options) and `17aec44` (Scenes). The latter replaces local `89a772f`: the full
source guard caught the Scene cache's field name `.module =`, despite it being
only a `PathBuf`. Renaming it to `module_path` keeps the mutation guard unchanged;
the rerun passes that guard, all 276 active app tests and the Scene unit test.
The picture was taken before this private-field rename; its visual code is
unchanged, and its exact source commit is retained in the evidence manifest.

The earlier rerun stopped at `schema::live_schema_tests::every_live_schema_is_discoverable`:
`/v1/call/live_set_variable: Unsupported schema type`. `operations.rs` and the
checked-in OpenAPI include `{"type":"null"}` for typed local values, while
`apps/moonglow-mcp/src/schema.rs` then neither accepted nor validated null.
Discovery omitted that operation with an invalid-schema diagnostic. This existed in base
`9ef5692`; no mg-live, OpenAPI or adapter file was changed in this UI batch.
Those files belong to the coordinating workers, so implementation stopped under
the explicit wrong-fact/ownership rules. Claude corrected schema check/validation
and extended the regression in `57f7859`, then authorized completion. The recipe,
index/log and this cumulative record now include the resolved contract.

Earlier partial gate: **1,089 passed / 1 failed / 99 ignored before abort**;
mg-live **73 passed / 3 ignored**; workspace clippy/fmt clean. The final build
and end-of-gate fmt-check were not reached. No native game/client action was
performed. That failure was captured through `check.ps1 -Phase 8 -Next -Step ui-final
-Jobs 1 -NoUiDebug -UiUnoptimized`; after the adapter/schema correction, repeat
that gate and `-Build`. Current results follow below. The coordinator adds
`Action::OpenTab(Tab::Scene)` and retains the shared manual enqueue hook during
his DM-actions merge; owner E2/E3/E4 acceptance remains after that merge/batch.

Knowledge/evidence remains in this record, the recipe, the small ownership
ledger and current proof JSONs/logs. Four PNGs and the 638-byte fake module plus
535-byte saved scene remain for review/reproduction. No redundant own files
were identified; **0 logical bytes reclaimed**, with no disk/free-space savings
claimed. Shared build outputs remain required by other workers and acceptance.
Session `01a12fe8-1458-7551-99fc-b277832def84` history at
the agent's local Codex session file
was 157,057,735 logical bytes at the closing inventory; it is active and untouched.
Separate history cleanup has no approved retention mechanism in this task and
must occur outside this session. No required input, source or review evidence
was deleted.

The follow-up error-key trace is explicit: `CommandResult.err` in
`crates/mg-live/src/store.rs` is serialized unchanged under each status result,
and `events::EventLog::command` copies it into command-event data. Current
`crates/mg-ui/src/live_log.rs` already reads `data["err"]` correctly. The wrong
lookup is in `crates/mg-live/src/scene.rs` inside `Wait::Result`: it reads
`result["error"]` from that native command result. `error` is a legitimate key
for HTTP API refusal envelopes (`api.rs`) and for `Player::status().last` after
converting a Rust error to JSON; neither changes the native result to that shape.
The Scene UI's `last["error"]` lookup is therefore correct. Claude owns any core
or Live Log branch correction; no such file or branch is changed by this batch.

**UI/recipe batch complete after coordinator fix `57f7859`.** The final guarded
`-Phase 8 -Next -Step ui-final -Jobs 1 -NoUiDebug -UiUnoptimized` run passes
**1,096 workspace tests / 0 failed / 99 ignored**; mg-live passes **73 / 3 ignored**.
Workspace clippy, fmt and final fmt-check are clean. The same command with
`-Build` builds `mg`, `moonglow` and `moonglow-mcp`. The previously failing
`every_live_schema_is_discoverable` passes. Seven recipe inputs and relative
links validate. These are functional checks with the NWN_ROOT guard and the
unoptimized UI profile; native acceptance and performance are separate.

The recipe/index/log and closure record form one local item commit; no push or
coordinator-branch merge was performed. `phase8-ui-final-checks.json` now records
the passing run, its source reference and current evidence hashes. Existing
pictures/fixtures remain sufficient because this last step changes documentation
only. No redundant own artifacts were introduced or deleted; the ledger remains
the exact retained-file inventory, with zero claimed reclaimed bytes. Stop here
for Claude's merge and the owner's combined acceptance. The core error-key
follow-up above remains with Claude; it does not block these completed checks.

## Owner view review after the combined acceptance

The coordinator reports owner acceptance of the merged E2/E3/E4/DM batch and
requests six view corrections: collapsible legend, RTS selection and commands,
placeable manipulation/properties, live placeable emitters, named area spell
cues, and a measured 218-object frame improvement. The running demo and its
executables are protected: no Start/Stop-LiveDemo and no `-Build`. All checks
use `tools/live/check.ps1 -Phase 8 -Next -Jobs 1 -NoUiDebug -UiUnoptimized`.

Item 1 uses the existing per-view `hide_legend` state for an in-view `+ Legend`
/ `- Legend` header. The toolbar switch and header stay synchronized; hiding
Live and showing it again retains that view's choice. The app check confirms
collapse/reopen, preserved selection, an independent default for another view,
and no queued command or module edit. The first after picture exposed missing
triangle glyphs in the loaded font; ASCII toggles correct that visual defect.

Reproduce with `-Step review-legend -Lint`, `-UiTest live_review` and
`-Picture live_review_legend_snapshot` (set `MGLIVE_PICTURE_STAGE=before` or
`after`). The harness replays the existing morning report and demo module,
without connecting to the owner's server. Guarded lint and the real-install
app check pass. The inspected before/after pictures are retained at
`target/test-output/live-next/visual/review-legend-{before,after}.png`.
These checks do not replace the owner's next native acceptance.

The current ownership ledger is
`target/test-output/live-next/view-review-owned-files.json`, session
`01a12fe8-1458-7551-99fc-b277832def84`. It records the reused inputs and retained
minimal proof. No new asset tree or module copy was introduced, no shared
artifact was deleted, and no reclaimed space is claimed. Further items follow
as separate commits; Claude's Live Log/menu and Astra2's core/game work remain
with their owners.

**Earlier item 2 blocker (coordinator resolved the scope): selected-only Stop Fighting has no
matching operation.** `Op::StopCombat` in `crates/mg-live/src/store.rs` accepts
only `x`, `y` and `radius`; `operations.rs` exposes that same contract as
`live_stop_combat`. The native branch in `tools/live/nwn/mglive_exec.nss`
snapshots every eligible creature within the radius before cancelling holds,
clearing actions and changing personal reputation. A small positive radius
cannot make that an object-targeted operation: overlapping or moving unselected
NPCs may be affected, while a selected NPC can leave the sampled position.
`radius=0` is invalid. This is also documented in
[live-ops-wire.md](live-ops-wire.md#stop_combat).

The requested group action, one command per selected object, therefore cannot
be implemented faithfully using this existing operation. No substitute command
or game action was sent. Core/API/NWScript changes belong to Astra2. The next
decision is to provide a target-ID stop operation (or a compatible optional
`target` on `stop_combat`, with core and native tests), or explicitly approve
and label an area-based action that can affect unselected neighbours. Under
the standing stop-on-wrong-fact rule, items 2–6 remain pending. Resume with the
before picture for RTS selection once that contract is settled. Item 1 is
complete and independently reviewable; the running acceptance demo and both
protected app binaries remain untouched.

The coordinator subsequently confirmed the mismatch and authorized continuing
items 2–6: omit group Stop Fighting until Astra2 supplies `stop_combat {target}`.
The single-object menu retains its clearly labelled radius command. No core,
API or native changes are needed for the other group actions.

Item 2 adds Ctrl-click marker toggling and Shift-drag box selection (Ctrl adds),
shared highlights/counts in the view and Live Panel, and one queue entry per
eligible object for formation/one-point walking, attack, faction, freeze/unfreeze,
kill and destroy. Formation destinations use a stable two-metre grid; native
pathfinding can adjust actual arrival. Player/DM markers remain selectable but
are excluded from group orders. Attack targeting rejects members of the squad.
Escape/right-click cancels targeting; report removal and a new server boot prune
selection. These commands use the existing scene-aware enqueue hook and leave
the module's GIT and editor selection untouched. A single selection made by
Live Log replaces a previous group without changing the coordinator-owned log.

Reproduce the group fixture with the same guard flags and `-Step review-group`,
`-UiTest live_review`, `-Picture live_review_group_snapshot`. The baseline replays
three controlled NPCs on the existing morning module, then Ctrl-clicks all three;
it shows only the last selected. The inspected after picture uses the identical
gestures and shows three selection rings and the count. Guarded Clippy/fmt,
two review app checks, the existing targeting check and two panel checks pass.
The group app check proves box/toggle selection, all listed group operation
families, unique formation positions, common-point destinations, player exclusion,
selection counts, boot reset and no module edits. Tests queue only into a fake
store; they do not run commands on the protected demo.

Item 3 adds the Live Properties window for one selected placeable/door: name,
useability, seven numeric Visual Transform components, Apply, Reset and Reload
Report. Numeric edits remain drafts until Apply. The reported transform now
reaches the drawn model. Reported props matched to GIT instances use that exact
instance's appearance, replace its original drawing, and suppress the original
emitter; other live extras still resolve their blueprints. This avoids stale
facing/transforms and duplicate emitters without editing the GIT.

Placeables use Q/E (including editor snap/90-degree bindings), the editor's
rotation-ring geometry and Alt+right drag. Dragging a selected marker moves it.
A gesture previews locally and sends one `set_facing` or `set_position` when
released; pending preview clears on reported arrival, refusal, boot change or
30-second expiry. Escape/right-click cancels. Doors can move and use visual
transforms, but no physical rotation control is offered because native
`set_facing` on doors remains explicitly disabled in the wire contract.

The properties fixture reuses the morning module's `city_crate` blueprint with
a controlled report. `-Step review-props -Picture live_review_props_snapshot`
(`MGLIVE_PICTURE_STAGE=before/after`) shows the prior fixed presets versus the
numeric panel, transformed chest and rotation ring. The after image was
inspected. The real-install fake-store app test exercises Q/E, ring rotation,
drag release, edits all seven numbers before Apply, resets, targets a door and
asserts no module changes. The existing Effects check caught toolbar overflow
at narrow widths; using `available_size_before_wrap` fixes that row overflow.
Guarded Clippy/fmt, three review app tests and all three existing Effects tests
pass. No native commands ran, and neither protected application was rebuilt.

Item 4 reproduces the missing beam using the installed `x3_plc_slightg` UTP
(Lightshaft, Green). Its own emitter was incorrectly gated on Effects: Full.
Intrinsic model emitters now run for every live extra in Off, Markers and Full;
those modes control reported effect attachments/cues. The app check proves all
three modes, continuing animation, removal, and suppression of an original GIT
emitter when its Live model replaces it. Four review checks, existing Effects
checks, Clippy/fmt and inspected `review-lightshaft-{before,after}.png` pass.
The after picture shows the green beam with Effects still set to Markers.

**Full default decision:** keep Markers for now. A replay of a 218-object crowd (plus the existing off-camera Shop door) at
1600×1000, under the required unoptimized UI profile and uncontrolled concurrent load,
measured CPU UI+tessellation median/p95 **13.79/91.87 ms** in Markers and
**13.33/65.18 ms** in Full. Neither establishes headroom for the existing 6.944 ms
frame target; the difference between sequential runs is not attributed to the
mode. GPU fence p95 was 769/680 ms; other GPU workloads were not isolated. This is a headless 3D
submission/fence and CPU UI/tessellation profile, not native app FPS; final egui
paint/font uploads are excluded. The protected demo cannot be stopped for an
isolated GPU benchmark. The same fixture will be measured around item 6's view
optimizations before reconsidering the default.

Reproduction: `-Step review-lightshaft -UiTest live_review`, `-Picture
live_review_lightshaft_snapshot`, and `-Picture live_review_profile_snapshot`
with the normal guard/profile and `MGLIVE_PICTURE_STAGE`. Timing follows
`frame_perf`'s UI/tessellation boundary, with 24 warmup and 120 measured frames
per mode. The initial result is `review-profile-effects.json`; one shared
`review-profile-fixture.json` retains its exact report, and the harness can
regenerate it from the already required morning input. Later runs check equality
with that fixture. A rejected narrow-toolbar diagnostic from item 3 was removed
after the cause/fix was recorded; **447,190 logical bytes** reclaimed, with no
claim about disk allocation or free-space changes. Required pictures/logs and
the replay input remain; the ownership ledger records exact paths/hashes.

Item 5 adds localized spell names and short location circles from the loaded
`spells.2da` (`TargetShape=sphere`, `TargetSizeX` in metres). Fireball row 58
reports 6.67 m; the installed NW_S0_Fireball impact script independently uses
`RADIUS_SIZE_HUGE` (6.67 m). Casting Range is not an area radius. Missing or
non-circular metadata keeps a named point flash; no radius is guessed. Both
spell and projectile events resolve names once on receipt, retaining cast
coalescing, filters and bounded lifetimes. Invalid/nonreported target IDs no
longer hide an otherwise valid location cue. VFX events without a spell use
the visualeffects label when supplied. Live Log can call public
`mg_ui::live_effects::spell_name(&GameData, i64)` (same module also exposes
`spell_radius`). No coordinator-owned log/menu code was edited.

Reproduce with `-Step review-spells -UiTest live_events_view` and `-Picture
live_review_spells_snapshot`, using the normal guard flags. The app check
covers localized Fireball/radius, unknown rows, location/object targets, expiry,
Off consumption and no static 3D redraw. Clippy/fmt and the inspected before/after
pictures pass; the after image shows Fireball and its world-space 6.67 m circle.
`review-spells-data.json` retains the loaded table metadata. This is replay
proof, not an additional native or owner acceptance.

Item 6 caches each view's raw report behind the store identity/boot/sequence
stamp, and shares that snapshot with input instead of cloning it again.
Reconciliation, blueprint/player looks, VFX attachment lists and faction palettes
are reused until report/view/resource inputs change. The UI uses the existing
bounded `status` API for the stamp; no coordinator/Astra2-owned store code changed.
Movement and local property previews update poses independently. Arrival still
uses the raw engine position. Event queries run on new reports; cue/bubble expiry
continues every frame. Filter/selection, workspace revision, faction overrides,
Full-mode camera position and replacement stores invalidate the appropriate data.

Live model posing now runs only when the existing animation/redraw scheduler
actually needs another GPU image. Animation flags are retained with prepared
extras, and hidden module overlays no longer clone the whole area model.
This preserves the same animation cadence and all intrinsic emitters.

The controlled workload is **218 crowd objects plus one pre-existing off-camera
Shop door**, on the same 71-object module at 1600×1000. Earlier metadata counted
only the crowd; the JSON records now count all 219 report objects. No fixture or
measured timings were changed by that correction. A baseline fixture comparison
initially failed by one f64 ULP because serde_json's decoder rounds a serialized
f32 coordinate; comparing both sides through the identical wire decoder fixes
the harness without tolerances or altered scene data.

The instrumented before/after records are `review-profile-{before,after}.json`
and inspected PNGs under `live-next/visual/`, with one shared fixture. Reproduce
using `-Phase 8 -Next -Step review-profile -Jobs 1 -NoUiDebug -UiUnoptimized
-Picture live_review_profile_snapshot` and `MGLIVE_PICTURE_STAGE=after`.
The before code is `ad8418f` plus the timing counters. CPU preparation and live
pose timings are public `LiveView.frame_cpu_ms`; report/reconciliation counters
make accidental per-frame work visible to the app regression test. Timings use
24 warmup and 120 measured frames per mode, matching the existing frame_perf
UI/tessellation boundary. Final egui paint/font uploads are excluded.

Final-code measurements (median / p95 milliseconds):

| Mode and stage | Before | After |
| --- | --- | --- |
| Markers CPU UI + tessellation | 15.922 / 145.905 | 1.913 / 107.000 |
| Markers Live preparation | 1.340 / 2.624 | 0.020 / 0.036 |
| Markers Live posing | 9.370 / 15.532 | 0.000 / 9.979 |
| Full CPU UI + tessellation | 19.823 / 100.742 | 1.618 / 55.282 |
| Full Live preparation | 1.746 / 2.859 | 0.017 / 0.034 |
| Full Live posing | 12.770 / 16.056 | 0.000 / 8.319 |

Over 144 unchanged frames, report copies fall from 144 to zero; reconciliations
fall from 144 to zero (one when switching to Full). Posing remains required on
redraw frames. GPU fence p95 remains hundreds of milliseconds (745/637 after);
these sequential, unoptimized headless measurements do **not** establish native
FPS or eliminate all renderer/GPU stalls. Full stays opt-in because tail frames
still exceed the 6.944 ms target. Owner smoothness acceptance remains due after
Claude's merge/build authorization; no `-Build` was run and `moonglow.exe` retains its prior size and timestamp.
The authorized workspace tests automatically relinked `mg.exe` for CLI
integration tests (00:29:13 local); no native game or demo process was controlled.

Final guarded gate (`-Phase 8 -Next -Step review-final -Jobs 1 -NoUiDebug
-UiUnoptimized`, without Build): **1,101 passed / 0 failed / 105 ignored**;
mg-live **74 passed / 3 ignored**; workspace/all-target Clippy and fmt clean.
Fourteen focused real-install app tests pass across review controls, effects,
events, speech, player looks, panel, targeting and movement. Movement checks
report/reconciliation reuse, interpolation/picking, refresh and a replacement
store with identical boot/sequence/timestamp. Final profile picture passes and
was inspected after the store-identity guard. Automated replay is separate from
the owner's two-monitor acceptance.

At that batch close, all six review items were ready for coordinator audit;
group Stop Fighting awaited the approved target field. It is now merged, as
recorded in the following patrol/tile section. No further decision is required for the current UI
batch. Knowledge lives here and in manual chapter 04/index/log. The exact owned
file ledger is `target/test-output/live-next/view-review-owned-files.json`;
minimal before/after proof, one fixture and per-item checks remain for review.
The closed narrow-toolbar diagnostic removal reclaimed 447,190 logical bytes;
no additional shared build/data cleanup or disk/free-space saving is claimed.
Session `01a12fe8-1458-7551-99fc-b277832def84` is registered in that ledger for
separate, approved history cleanup after closure; its active JSONL is untouched.

## Patrol and tile view tools after the combined acceptance

The owner accepted the preceding view batch; coordinator merge `007d1d5`
includes targeted Stop Group Fighting. The demo remains protected: these changes
use guarded tests only, no Build or Start/Stop-LiveDemo.

### Patrol targeting (2026-10-13)

`live_patrol.rs` adds Patrol? / Stop Patrol to single-NPC and group menus.
Click up to sixteen numbered ground points; Loop and Run are explicit toggles.
Enter over the view, double-click or Send Patrol submits one `patrol` command
per eligible NPC through the existing queue/scene recorder. Right-click, Escape
or Cancel Patrol discards the draft. Players, DMs, possessed and dead creatures
are excluded. Stop Patrol uses `stop_patrol {target}`, one per selected NPC.

The wire reports reached/blocked points, not complete routes or start/stop
notifications. Accepted commands retained by the store provide full routes;
otherwise the overlay explicitly says Observed patrol points. Successful new
orders, a blocked/finished route, disappearance/death, a server restart or a
125-second observation timeout remove the path. Only selected paths are drawn.
This is observed progress, not a claim of an authoritative patrol snapshot.

Reproduction: load ignored `tools/live/local.ps1`; run `tools/live/check.ps1`
with `-Phase 8 -Next -Jobs 1 -NoUiDebug -UiUnoptimized -Step patrol -UiTest
live_review` (five passing app checks), then `-Step patrol -Lint` (workspace
Clippy/fmt). Set `MGLIVE_PICTURE_STAGE=after` and use `-Step patrol-after -Picture
live_review_patrol_snapshot` for the real-install area picture. Before/after PNGs
are `target/test-output/live-next/visual/review-patrol-{before,after}.png`;
the baseline was captured on `007d1d5` before production changes. The after
picture is inspected: three selected NPCs, a numbered triangular loop, readable
controls. The fake-store app check covers bounds, player exclusion, Loop/Run,
Enter/double-click, cancellation, result/event progress and explicit stop;
workspace bytes remain unmodified. Tests and controlled pictures do not replace
owner/native acceptance. The initial test used the ground menu for a single
NPC (which selects the ground); it now opens the NPC marker menu. The double
click check separates the preceding button gesture before exercising two clicks.

Working-file ownership is recorded in
`target/test-output/live-next/view-tools-owned-files.json`; the existing demo
module, recorded report and tiny area fixture are reused. Minimal proof is kept
for coordinator/owner review; no additional module copy or cleanup is needed.

### Confirmed single-tile painting (2026-10-13)

`live_tiles.rs` reuses **Select Tiles** and redirects existing Tile Properties
while Live is on. Click a cell, choose its active-SET ID, orientation (0?3 CCW
quarter turns) and native height (0?255), then Apply Live Tile. The first command
per view/server session requires **Confirm Live Painting**, with the exact area,
cell and replacement. Cancel sends nothing. **Paint single tiles** applies those
values by click or drag, once per cell per stroke; right-click/Escape stops it.
The panel says **Live: painting the running server**. All commands use the
existing queue and scene recorder; successful submission does not change the
picture optimistically. Refusals remain visible in the panel and command log.

The ordinary terrain brush cannot map 1:1 to SetTile: `tile_command_keeping`
also changes ARE lights/loops and creates/deletes GIT doors while solving
neighbour transitions. Live therefore starts with explicit single-cell
replacement, as authorized. Selecting a terrain brush explains this limit;
module brush/property writes are bypassed while Live is active. Terrain
transitions, groups, neighbouring heights and door adjustment remain deferred.
This raw replacement can intentionally create seams or isolated geometry; the
picture demonstrates a turned, raised building tile replacing a garden, not a
claim that this is a valid terrain arrangement.

Tile events change only `AreaView.model`, then invalidate the scene, walkmesh
and particle caches. Original view tiles are kept once per changed cell and
restored when leaving Live; the workspace ARE/GIT, revision and Undo stay
unchanged. A workspace refresh rebases the presentation overrides. The earliest
observed native `before` value per cell supports **Restore First Observed Tile**
through another SetTile; it is not inferred from the module. Server boot/store
changes clear observations, pending requests and confirmation.

The restart app test exposed an additional constraint: `EventLog` retains old
boot history, and events/results carry no boot tag. Tile and patrol views now
start from the current cursor/results on first attachment or a boot change;
they never reconstruct state from ambiguous historical entries. Tile events
received while this view is closed, or in that initial boundary batch, cannot
be recovered reliably. The panel explicitly says earlier server terrain is
unknown. A future full terrain report or boot-stamped events would allow
late-join reconstruction; it is not claimed here. This also hardens the patrol
item against old arrivals reviving a stopped route after restart.

Checks: six real-install review app tests pass, including tile confirmation and
cancel, exact command values, click/drag queueing, event-driven model/height,
restore requests, server refusal, original terrain on Live-off, restart
invalidation/fresh confirmation, terrain-brush guidance, unchanged ARE and
workspace revision. The patrol test also checks old-boot history cannot revive a
path. Run `-Step tiles -UiTest live_review` with the common Phase 8/Next/Jobs 1/
NoUiDebug/UiUnoptimized arguments above. The area harness uses GPU `hold()`.

Pictures: `MGLIVE_PICTURE_STAGE=after` with `-Step tiles-after -Picture
live_review_tiles_snapshot` renders `review-tiles-confirm.png` and
`review-tiles-after.png`; both are inspected alongside the pre-change
`review-tiles-before.png`, under `target/test-output/live-next/visual/`.
These replay a fake store over the accepted isolated demo module and real
install; they do not control the owner's running demo or prove native delivery.
The first harness click hit the floating window, not the ground; the diagnostic
picture established this and the test now uses an uncovered cell. That closed
diagnostic is removed after recording it; the final proof stays for review.

Final guarded gate: `-Phase 8 -Next -Step view-tools-final -Jobs 1 -NoUiDebug
-UiUnoptimized` passes **1,120 / 0 failed / 109 ignored** across the workspace;
mg-live **86 / 4 ignored**; workspace/all-target Clippy and fmt clean. No Build,
launcher or process-control command was used; both app executables retain their
prior size/timestamp. Six focused app checks and both after picture harnesses
pass. Documentation local-file links were checked; OKF CLI is unavailable here,
so the existing index/log were updated directly, without claiming OKF lint.

These two tools are ready for coordinator audit and owner acceptance after an
authorized new build. No decision blocks the implemented scope. Full terrain
snapshots/boot-stamped event history and terrain transitions are future work.
The owned ledger retains five minimal PNGs and check logs; the closed click
diagnostic plus superseded failed lint log were removed (**430,688 logical
bytes**). No allocation/free-space saving is attributed. Reused demo/area inputs,
shared builds and pending-review proof remain. Session
`01a12fe8-1458-7551-99fc-b277832def84` and its exact history location/observed size
are in the ledger; the active history was untouched and requires separate
approved, application-supported cleanup after closure.

## Second owner view acceptance (2026-10-13)

The owner accepted patrols/single-tile painting, then requested nine usability
items. Work now shares `live-mode` with Astra2; her NWScript/mg-live and tool
window files remain hers. Never reset/checkout another worker's files. The demo
is protected: guarded tests only (`-Jobs 1 -NoUiDebug -UiUnoptimized`), no Build
or app rebuild. `check.ps1 -CheckFmt` skips the mutating workspace format pass;
format only owned Rust files (`rustfmt +1.98.0 --edition 2024 --config
skip_children=true <files>`) before the non-mutating checks.

### 1. Closable Live floating windows

Patrol and Live Tile Properties now have title-bar close buttons. Closing cancels
the draft/first-change confirmation and leaves no command; closing tile tools
also clears the active terrain brush so the window stays closed. Properties,
Say and script dialogs already had close controls. The appearance confirmation
was also made closable by Astra2 in `a7f4440` (`live_vfx.rs`).
Existing accepted `review-{tiles,patrol}-after.png` images are the baseline
(`68dccd3`); the new tile after picture is
`review-tiles-accept2-close-after.png`. The review app cases exercise both close
buttons and reopening without dispatch (six review tests passed). The after picture
was inspected: the title-bar close button is visible. Working-file ledger:
`target/test-output/live-next/accept2-owned-files.json`.

### 2. Palette-driven live tile painting

Live's Tiles palette now shows the active SET's single-tile thumbnails, filtered
by model name and virtualized by visible rows. Choosing a tile arms the brush;
click/drag sends one native replacement per changed cell through the same queue.
Q/E (the configured editor Turn commands) rotates in quarter turns. Palette
painting preserves each destination cell's height. A translucent preview and
Live banner distinguish pending painting from server-observed terrain. The
first-change confirmation and event-only model updates remain in place.
Numeric IDs/orientation/height and explicit-height painting are under Advanced
in the closable Tile Properties window. Terrain transitions are still deferred.
The accepted numeric-UI picture (`review-tiles-accept2-close-after.png`) is the
before evidence; `review-tiles-accept2-palette-after.png` is the inspected new
palette/preview view (captured with the following Live-layout work in progress).
Six guarded review app tests passed; the tile case verifies palette selection,
Q rotation, repeated-cell deduplication, drag, cell height preservation and no
workspace mutation. The guarded picture check passed.

### 3. Open numeric Visual Transform from the object menu

The direct Visual Transform entry opens the numeric editor for the clicked placeable/door. Reported static placeables disable transforms and explain the limitation; Make Dynamic queues set_static(false). Native clients must re-enter the area after Static changes. Properties preserve queued-on-release turn/move gestures.

Guarded live_review passes (11 tests); reviewed review-props-accept2-before/after.png under target/test-output/live-next/visual. The harness uses mg_testkit::gpu::hold and isolated/fake reports, not owner acceptance. Reproduce with tools/live/check.ps1 -Phase 8 -Next -Step accept2-batch -Jobs 1 -NoUiDebug -UiUnoptimized -UiTest live_review; pictures use -Picture live_review_props_snapshot and MGLIVE_PICTURE_STAGE=accept2-after. No app build or demo access.

The combined check also fixed a missing Teleport match and Astra2 repaired her Inventory UTF-8. Skill Check scope is resolved by her owner clarification: checks target one PC or players in a radius centred on an NPC; no NPC-target roll is claimed. The per-item records below cover the remaining batch.

### 4. Dock Live tools to the right when Live turns on

Live Log, Panel, Selection, Skill Check, VFX, Library, Scenes and Inventory open as tabs beside Palettes. Live Log is active first; reopening a tool moves/focuses it there. Live off/on reapplies the layout. Inventory registration belongs to Astra2 in this shared tree.

The guarded 11-test review suite verifies the common right-hand pane, area kept separate, and Selection focus preserving the group. Inspected visual/review-layout-accept2-before/after.png. Reproduce with -Picture live_accept2_layout_snapshot and MGLIVE_PICTURE_STAGE=accept2-after. Initial Live activation queues tabs, so harness interactions wait for that frame before selecting a tool.

### 5. Select live groups with plain drag and configurable control keys

Plain empty-ground drag selects; Shift adds and Ctrl toggles. The rotation handle wins over empty-ground selection, preserving single set_facing on release. Selection lists names, HP and kind; clicking a name focuses without changing the group. Its table rows were corrected after picture review.

Ten new Cmd IDs in Options > Keyboard > Live Area: live-store-group-1..5 (Ctrl+1..5) and live-recall-group-1..5 (1..5; double tap centres). These take precedence over camera digits and Ctrl+digit tab switching while pointing at Live. Groups reset on boot change. No commands.rs edits.

The guarded 11-test suite passes plain drag, keys, camera focus, group queueing and unchanged module state. Inspected visual/review-group-accept2-before/after.png; reproduce with -Picture live_review_group_snapshot and MGLIVE_PICTURE_STAGE=accept2-after. LiveSelection is the tab for opening the selection tool.

### 6. Filter Live marker categories and factions per view

Legend checkboxes control players, DM/possessed, NPCs and individual factions, placeables, doors, items, dead objects, combat rings, attack lines and labels. Only overlays change; reported models remain. A white selected-object ring remains a selection aid. Choices survive Live off/on for the view.

The legend app case passes in the 11-test guarded review suite. Inspected visual/review-legend-accept2-before/after.png: placeable markers hidden while their models remain, faction controls expanded. Reproduce with -Picture live_review_legend_snapshot and MGLIVE_PICTURE_STAGE=accept2-after.

### 7. Enter action values and open tools for the clicked target

Heal amount/full, Freeze seconds, Give Gold/XP and server hour/minute accept explicit values; preset shortcuts remain. Editable menus stay open while fields are clicked/typed and send only on action buttons. VFX, Appearance, Skill Check and Inventory open for the clicked selection. Skill Check uses single=None: one PC, or players in radius centred on an NPC; the UI and manual make this distinction explicit. Static placeables explain disabled VFX.

Guarded live_review verifies Heal 42, time 07:23, Gold 237, Freeze 17, no enqueue while editing, and NPC radius mode. Inspected visual/review-actions-accept2-before/after.png. Reproduce with -Picture live_accept2_actions_snapshot and MGLIVE_PICTURE_STAGE=accept2-after.

A first retry ran out of compiler memory at monomorphization under contention despite Jobs1/debug0/opt0. The same guarded profile compiled successfully after the concurrent metadata check finished; no cache wipe, process termination or app rebuild was used.

### 8. Offer player orders and explicit message, journal and Boot dialogs

With both player switches, the Player menu offers Walk/Attack, Freeze/Kill/Heal/Raise and aimed Teleport; targeted tools include VFX/Appearance/Skill/Inventory. Message accepts chat/floating/NUI, Journal takes quest/state, Fade chooses black/from-black. Boot names its retained target and requires Confirm Boot. Closing or cancelling sends nothing; dialogs expire on boot change. DM/possessed objects remain protected. Existing operations and the common queue perform all actions.

Guarded live_review (12 passed) proves explicit values, retained player target and no command on cancelled Boot. Inspected visual/review-actions-accept2-player-after.png shows the confirmation; before/after menu picture is shared with item7. Native player display and actual connected-client response remain owner acceptance, not a claim from fake reports. The test harness also waits for the new Live layout before returning to Palettes.

### 9. Draw live placeables and doors by reported appearance

LiveObject.app overrides blueprint/GIT appearance and is part of the look cache key. A missing blueprint can still render from a reported row. Placeables use placeables.2da; doors use genericdoors.2da and GenericType_New, with legacy fallback. Older reports retain blueprint fallback. Reported static state is applied to extras.

Guarded live_review (11 passed) verifies a cabinet override, cache refresh and missing-blueprint fallback. Inspected visual/review-appearance-accept2-before.png and -after.png show the cabinet replacing the blueprint model. Reproduce via -Picture live_accept2_appearance_snapshot, MGLIVE_PICTURE_STAGE=accept2-after. Native report/readback is Astra2's separate ownership.

### 10. Expose Run Here and both group Run destinations

Run Here sits beside Walk Here for NPCs and permitted players. Group menus add Run in Formation and Run to One Point. These share destination targeting and submit run:true; Walk targeting still supports Shift-click to run, as its aim hints state. No navigation is sent until the destination is clicked; Escape/right click cancels.

The guarded live_review_run_commands_and_shift_shortcut_queue_running_moves case passes within the 12-test suite: NPC and PC explicit Run, NPC Shift-Walk, group formation with distinct positions and group one-point with identical positions, one queued command per NPC. The action after-picture shows Run Here beside Walk Here. Manual 04 includes the commands and shortcut. The first fixture used a non-hex ID; replacing it with a valid fake game ID resolved that fixture refusal.

### Closure and coordinator handoff (2026-10-13)

The second acceptance UI batch and explicit Run addition are implemented. Local
commits: item1 `36cb9f0`, item2 `59ab89b`, item3 UI hunks in `446dfe4`, item4
`72354d8`, item5 `a586fe2`, item6 `da1288c`, item7 `a3314ee`, item8 `61d07d3`,
item9 `c6fd91e`, Run `2ce6ac5`. The item3 staging overlapped Astra2's appearance
commit; no history was rewritten. Separate temporary indexes were used for the
remaining commits. Astra2's Inventory hooks are in `463777e`.

Final guarded checks, Rust 1.98.0, Jobs1/NoUiDebug/UiUnoptimized:

- `-UiTest live_review`: 12 passed, 0 failed (real installed assets, fake Live reports).
- `-Package mg-ui -Filter live_scene`: the docked Scene regression passes.
- `-CheckFmt` workspace guard: Clippy clean, mg-live 93 passed / 4 ignored;
  workspace 1,137 passed / 0 failed / 114 ignored; formatting clean.
- The first workspace run found the Scene harness's old floating-window unwrap.
  It now asserts the main dock surface and runs the same record/save/validate/play/stop
  scenario in the actual pane; the full guard then passed.

Reproduce after dot-sourcing `tools/live/local.ps1`, using `$env:MGLIVE_PWSH`
to run `tools/live/check.ps1 -Phase 8 -Next -Jobs 1 -NoUiDebug -UiUnoptimized`.
Add `-Step accept2-values -UiTest live_review`, or
`-Step accept2-workspace -CheckFmt`. Logs are under `target/test-output/live-next/`:
`phase8-accept2-values-live_review.log` and `phase8-accept2-workspace-{clippy,live,workspace,fmt-check}.log`.
Per-item pictures above were inspected; Selection rows were corrected and recaptured.
These proofs do not replace the owner's connected-client/two-monitor acceptance.

No -Build flag, push, demo stop/start or native game-server action was performed.
**Final verification stop:** cargo test --workspace nevertheless rebuilt the normal
CLI executable for its integration tests. `target/debug/mg.exe` changed from
17,174,016 bytes at 10:27:07 to 12,039,168 bytes at 13:46:59 (2026-10-13).
The earlier assumption that a test-only guard preserves both normal executables
was wrong. Moonglow remains 56,911,872 bytes at 10:27:19, and the running demo
was not stopped. No attempt was made to restore or rebuild the CLI again.
The coordinator must decide whether to keep this tested CLI or restore a known
artifact; future guarded workspace runs during acceptance must account for
Cargo's integration-test binary build. The coordinator controls any refresh
of the old running demo. After that refresh, verify right-side tools, plain-drag
selection and control groups, tile palette painting/rotation, direct numeric
transform/static explanation, marker toggles, value menus, PC dialogs, cabinet
appearance and each Walk/Run targeting mode on both monitors. No UI design
question remains; native PC action delivery and appearance/static refresh still
need that acceptance.

Ownership/cleanup ledger: `target/test-output/live-next/accept2-owned-files.json`.
Removed own staging helper/index, redundant tile capture and obsolete review logs:
3,487,098 logical bytes across this task. No physical/free-space saving is claimed.
Kept current sources, minimum before/after proof and final checks; shared fixtures,
Cargo outputs and the running demo are preserved. Active history is untouched:
session `01a12fe8-1458-7551-99fc-b277832def84`,
the agent's local Codex session file.
History cleanup belongs to a separate authorized task; no in-session deletion
mechanism was used or approved.

## Owner additions 24 and 25 (2026-10-13)

### 24: Unified teleport UI

Built on Astra2 core commit `bde50fb`. `live_teleport.rs` provides the same
Teleport targeting for NPCs, placeables, doors, ground items and allowed players,
and mixed selections. Green ghosts preserve XYZ offsets from the first selected
object; editor turn bindings change each facing by the same amount. Invalid
out-of-bounds groups turn red and send nothing. Escape/right click cancels.
Teleport to Area lists reported server areas, defaults to their reported centre,
and offers a clickable coordinate map, XYZ and facing offset, Apply and Cancel/X.
Older reports without dimensions show a coordinate-entry explanation.

Each object uses the existing scene-aware queue with `Op::Teleport`; Command.area
is the destination. No editable GIT/ARE or normal application binary is touched.
A bounded pending-result tracker replaces selected IDs after reconstruction and
new reports, transfers selection to an already-open destination view, and tells
the user which area to open otherwise. It clears on server restart. This matters
for cross-area scenery/items and same-area doors with changed facing; native
JumpToLocation may adjust creature coordinates. Placeable drag remains intact.

Validation: 14 guarded real-data/fake-store `live_review` app tests pass (including
all kinds, player gate, facing, both cancellations, mixed offsets, bounds,
cross-area destination, replacement IDs and unchanged module). Scoped mg-ui
all-target Clippy passes with warnings denied. Owned Rust formatting is checked.
Before/after area-harness pictures inspected: group menu before, landing ghosts
and facing after, and the centre/map/coordinate dialog. These are UI proofs;
connected-player arrival and client scenery/door behaviour remain owner checks,
separate from Astra2's private native proof in the wire contract.

Reproduce from this tree (PowerShell 7 path comes from ignored local.ps1):

```powershell
. tools/live/local.ps1
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 8 -Next -Step teleport -UiTest live_review -Jobs 1 -NoUiDebug -UiUnoptimized
$env:MGLIVE_PICTURE_STAGE='teleport-after' # or teleport-area-after
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 8 -Next -Step teleport-after -Picture live_review_group_snapshot -Jobs 1 -NoUiDebug -UiUnoptimized
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 8 -Next -Step teleport -Package mg-ui -Lint -CheckFmt -Jobs 1 -NoUiDebug -UiUnoptimized
```

Proof lives under `target/test-output/live-next/`: `phase8-teleport-live_review.log`,
`phase8-teleport-clippy.log`, `phase8-teleport-{before,after,area-after}-picture.log`
and `visual/review-group-teleport-{before,after,area-after}.png`.
Ownership/closure ledger: `teleport-owned-files.json` in that directory. No
workspace test/build is used: CLI integration tests implicitly rebuild mg.exe.

### 25: Stop Attack

Resolved by core commit `11158f6`, then the UI batch below. **Stop Attack** now
appears on a single NPC, in Player for an allowed PC, and on a selection. Each
eligible living creature gets its own `stop_combat {target}` through the existing
queue; no radius/faction fields are sent. DMs, possession, dead creatures and
noncreatures are excluded. Disallowed PCs see a disabled entry and the existing
Options/server-switch hint. The group includes PCs only with the local player
policy enabled; the native server independently checks its switch. NPC menus
retain the separate **Stop Fighting Here (10 m)** radius command. PC Stop Attack
clears orders/releases owned holds without changing faction or reputation.

15 guarded `live_review` app tests pass, including exact single/group payloads,
player gating, radius separation, excluded targets and unchanged module state.
Player and group before/after menu pictures use the area replay harness with
`mg_testkit::gpu::hold()`. They prove the UI; connected-PC action delivery still
belongs to owner acceptance, as explained in Astra2's native/core wire record.
The manual and index name Stop Attack consistently.

Reproduce the focused test with the item24 command above, replacing Step with
`stop-attack`. Pictures use stage `stop-attack-before` / `stop-attack-after` and
Picture `live_accept2_actions_snapshot` (player) or `live_review_group_snapshot`.
Files: `target/test-output/live-next/phase8-stop-attack-live_review.log`,
`phase8-stop-attack-{player,group}-{before,after}-picture.log`, and
`visual/review-{actions,group}-stop-attack-{before,after}.png`.
Own-file ledger: `target/test-output/live-next/stop-attack-owned-files.json`.

Final item25 validation after UI commit `132ee7f`: the coordinator-requested
full guarded workspace run completed successfully: **1,144 passed / 0 failed /
114 ignored**, separate mg-live gate **97 passed / 0 failed / 4 ignored**,
workspace all-target Clippy with warnings denied, and workspace fmt check.
It used:

```powershell
. tools/live/local.ps1
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase 8 -Next -Step stop-attack-workspace -CheckFmt -Jobs 1 -NoUiDebug -UiUnoptimized
```

Final logs: `target/test-output/live-next/phase8-stop-attack-workspace-` followed
by `clippy.log`, `live.log`, `workspace.log` and `fmt-check.log`. No `-Build` or
process-control command ran. Both normal executables' SHA-256, byte length and
mtime match the ledger's pre-task snapshot (moonglow.exe 56,911,872 bytes;
mg.exe 12,039,168 bytes). This run did not change either binary, despite the
known possibility of Cargo building the normal CLI for integration tests.
The old acceptance demo was left running. Item25 is complete; stopped as asked.
The coordinator owns refreshing that demo and the connected-PC acceptance.

Retained this task's source, four inspected menu PNGs, ledger and final check
logs. No disposable helpers/copies were created: 0 files removed, 0 logical bytes
reclaimed; no physical/free-space attribution. Active session/history remain
untouched (same exact session/path recorded above); cleanup of history is a
separate authorized task. No unresolved UI decision or blocker remains.

Historical prerequisite (resolved): stopped after item24 commit `f512e7d`. The requested direct-player Stop Attack
cannot use the existing exact-target operation yet: `store.rs`'s player-target
allow-list excludes `Op::StopCombat`, and `tools/live/nwn/mglive_exec.nss`'s
targeted stop_combat branch unconditionally refuses `MgLiveDirectPlayer(actor)`.
Both facts were read on core commit `bde50fb`; enabling the players switch does
not bypass either check. NPC/group targeted stop already existed. Item25 needed
Astra2's core/native player-gated target support before the single NPC/player
and group menu labels could be completed and tested together. The radius form
must stay **Stop Fighting Here (10 m)**. Core/NWScript ownership remains Astra2;
the exact prerequisite was sent through `to-astra2.md`. No permission to edit
those files is inferred from this UI assignment.

Closure: current source, four minimal before/after UI pictures and final check
logs are retained for review; no redundant task files were created, so cleanup
removed 0 files / reclaimed 0 logical bytes (no disk-space claim). The ledger
confirms both normal application binaries retained their pre-task size and
mtime; no build, process control or push was performed. The old demo remains
for the owner. Active session `01a12fe8-1458-7551-99fc-b277832def84` and its history
path recorded above stay untouched; history cleanup is a separate authorized
task, not an in-session action. The core prerequisite and item25 UI are now complete; the coordinator controls
the demo refresh and connected-client acceptance.

## Third owner view acceptance (2026-10-13)

1. Legend: removed the duplicate Colour key section and its unused palette-list
   helper. NPC factions remains the sole faction colour/filter list; marker
   toggles and per-view collapse state remain tested. Guarded live_review:
   15 passed, 0 failed. Inspected before/after pictures:
   `target/test-output/live-next/visual/review-legend-accept3-{before,after}.png`.
   Check logs: `phase8-accept3-legend-live_review.log` and matching before/after
   picture logs under the same live-next directory. Reproduce with check.ps1
   Phase8/Next/UiTest live_review or Picture live_review_legend_snapshot, stage
   accept3-before/accept3-after; always Jobs1/NoUiDebug/UiUnoptimized, no Build.

2. Aim audit: `live_aim.rs` owns the map click/cancel gesture, crosshair, world
   circles and location drafts. Skill radius, ground VFX, library SpawnJson,
   nearby Stop Fighting and Jump All Players no longer send from a tool button
   or remembered position. Radius changes update the mouse-following circle;
   native library GFF JSON supplies the creature model ghost. Palette selection
   and drops arm a preview; the map click sends (Shift repeats). Movement,
   attack, patrol and teleport use the same gesture primitive. Patrol Enter /
   Finish on next click only arms its final click; cross-area teleport sends
   from its destination coordinate map, with bounds and offset checks.
   Manual04 enumerates all aimed actions and the direct-target exceptions.
   Guarded real-data checks: live_review 18/18, live_spawning 1/1,
   live_targeting 1/1, live_viewer 1/1; seven hexxar window regressions and
   mg-ui all-target Clippy pass. Inspected skill radius before/after and
   library model ghost at `visual/accept3-skill-aim-{before,after}.png` and
   `visual/accept3-library-ghost-after.png` beneath live-next. Reproduce with
   Phase8/Next/Step accept3-aim and those UiTest filters (Jobs1/debug0/opt0).
   The new tests use the existing GPU hold and fake stores, never the running
   server. Old immediate-send expectations were replaced by no-send-on-arm
   assertions and actual real-area clicks. Initial missing Jump label was an
   encoding typo; corrected. Library fixture now includes real body parts from
   CreatureLook::to_utc so the picture proves a body, not a marker-only ghost.
   Skill Roll core 0b1afa0 adds bonus/show_dc/timeout; the shared radius draft
   copies these live editor fields through aiming to the final queued command.
   The real-area radius click/cancel regression remains green.

3. Group parity: mixed selections expose numeric Heal/Freeze, Kill, rest, AI,
   invulnerability/immortality, Faction, Limbo, Say, VFX, Appearance and inventory
   logging, in addition to aimed movement/attack/teleport/patrol and exact Stop
   Attack. Each eligible object gets a normal queued command; permitted players
   now join group Walk/Run/Attack and supported creature orders. NPC-only native
   operations keep an explicit exclusion hint. Mixed appearance selections filter
   to permitted creatures; VFX filters protected/static targets. Animation now queues the proven
   creature operation for each eligible actor (item 7 below). Single-object
   Shaders opens Astra2's registered window for every object kind (PCs gated).
   Guarded live_review: 19 passed, 0 failed. Both menu pictures inspected.
   Before/after menu proof: `visual/review-group-accept3-{before,after}.png`;
   check recipe Phase8/Next/Step accept3-group/UiTest live_review and Picture
   live_review_group_snapshot with matching stage, Jobs1/debug0/opt0, no Build.

4. Painting exit: an interactive map banner exposes Stop painting. It, Escape,
   right-click and the Tile Properties close control share a complete stop path:
   tile mode, palette blueprint/terrain choices, pending confirmation, cell and
   stroke are cleared. Arming a window's location action also leaves painting.
   Existing view test proves Stop/Escape clear state, later map clicks queue no
   tile command and the module document/revision stays unchanged. Guarded
   live_review 19/19; inspected `visual/review-tiles-accept3-{before,after}.png`.
   Reproduce Phase8/Next/Step accept3-tiles/UiTest live_review, or Picture
   live_review_tiles_snapshot with stage accept3-before/after; Jobs1/debug0/opt0.

5. Scenes: prominent Record commands and a short how-to; readable command/target,
   delay and wait summaries with collapsed input editors. Save As keeps a named
   module copy, Export writes JSON without changing the draft, and Import opens
   validated JSON as an unsaved draft. Cancel/invalid files preserve the draft;
   none of these actions sends commands. Runtime enablement now overrides the
   saved option, matching the launcher. Guarded live_scene 1/1 (including file
   workflows), hexxar regressions 7/7 and mg-ui all-target Clippy pass. Inspected
   `visual/scene-accept3-{before,after}.png`. Reproduce Phase8/Next/Step
   accept3-scenes/UiTest live_scene or Picture live_scene_snapshot with stage
   accept3-before/after; Jobs1/debug0/opt0, no Build. No connected-game claim.

6. Effects: Markers retains the default; it now shows an explicit hidden-model
   hint with Show Full effects. The temporary Full-default change in 50f4857 was
   withdrawn after the active-effect crowd check below could not establish headroom. The hint reads the cached frame, with no added
   per-frame report query or creature scan. Exact player UTC look + stock
   VFX_DUR_MIND_AFFECTING_NEGATIVE (row7) already rendered through the existing
   player path: one attached model and 151 particles before the UI change.
   The regression now proves switching through the hint produces actual player
   emitters (153 particles in the final run), not just a model-count field.
   Four real-data effects tests and mg-ui all-target Clippy pass. Inspected
   `visual/accept3-player-markers-{before,after}.png` and
   `visual/accept3-player-vfx-after.png`; the redundant Full-before picture is
   disposable after documenting this finding. Reproduce Phase8/Next/Step
   accept3-vfx-after/UiTest live_effects, Jobs1/debug0/opt0, no Build.
   Fresh 218-object replay (plus off-camera door), 120 frames/mode, Full median
   UI+tessellation 2.5681ms versus Markers 2.4436ms; report reads0, reconcile1
   per mode. `review-profile-accept3-after.json` retains p95/max and GPU fences;
   shared-machine spikes remain large, so these medians are not a smoothness
   guarantee or connected-client acceptance. This base fixture has NO attached
   VFX; its mode comparison does not establish the cost of lasting effects.
   The added accept3-effects-after stage applies vfx:[7] to the first 16 creatures
   of the same input, leaving the base fixture unchanged. It reached the active
   effect picture after preparing 16 models, but the picture's GPU Wait timed out;
   the timing phase did not complete. No Full-default headroom is claimed from
   that failed run. Keep Markers with the one-click Full hint. Reproduce via
   Picture live_review_profile_snapshot with MGLIVE_PICTURE_STAGE=accept3-effects-after;
   failure log phase8-accept3-vfx-active-profile-picture.log. Full retains the
   64-model/20,000-particle budgets. The individual player effect passed; a
   heavy active-effect crowd and connected client remain acceptance work.

7. Animations: one shared chooser for living NPCs, allowed players and groups.
   After Astra2's native proof/commit 6ec4720 arrived during closure, the initially
   disabled chooser now queues typed PlayCreatureAnimation. Names come from
   loaded nwscript.nss, restricted to creature constant IDs 0..92/100..116. Custom
   names 1..16, looping, speed 0.1..10 and seconds 0.1..300 use Command validation;
   invalid drafts cannot send. Group templates retarget once per eligible actor.
   No situated-object animation substitution. Real-data live_review 22/22 passes,
   including numeric bow 105 for an NPC, custom name/loop/speed/duration for a PC,
   per-target group submissions, no-send edits and player-off exclusion. Inspected
   updated `visual/accept3-animation-{before,after}.png`. The named-constant parser
   regression and all-target UI Clippy are part of the final wiring checks.
   An initial nested combo closed its parent context menu; replaced by an inline
   collapsible constant list and verified actual selection/editing. Reproduce
   Phase8/Next/Step accept3-animation-wire/UiTest live_review, Jobs1/debug0/opt0.
   Connected-client/model-specific animation rendering remains owner acceptance.

8. Selection portraits: native reported base plus s, then m, through the existing
   resource/texture cache. GetPortraitResRef includes its prefix (proven by
   Astra2 e68e005); never prepend a second po_. Preserve custom bases. Portraits
   use the portrait canvas crop and click-to-focus while retaining the group;
   unavailable images have a hinted placeholder. Real-data live_review 22/22
   and mg-ui all-target Clippy pass; inspected
   `visual/accept3-portraits-{before,after}.png`. App proof resolves two actual
   installed images and verifies HP/name, portrait focus, unchanged selection
   and no command/module edit. Reproduce Phase8/Next/Step accept3-portraits/
   UiTest live_review, Jobs1/debug0/opt0. No game/client rebuild or capture.

9. Removal controls: positive-value Take Gold/Take XP menus and Remove Level
   for NPCs, permitted PCs and groups, through Astra2's committed 1705823 ops.
   Editing fields queues nothing; each eligible target gets one command. Group
   Remove Level skips reported level1; single level1 is disabled. Player-off
   menus retain explicit disabled entries and hints. Real-data live_review
   22/22 and all-target UI Clippy pass (combined with portraits). App assertions
   cover exact amounts/targets, group filtering, level1 and player exclusion.
   Inspected baseline `visual/accept3-animation-before.png` (old Player menu)
   and `visual/accept3-take-after.png`. Reproduce Phase8/Next/Step accept3-take/
   UiTest live_review, Jobs1/debug0/opt0. Engine removal/level-down semantics are
   Astra2's proof in live-ops-wire; no native acceptance is claimed by the UI test.

Ownership ledger: `target/test-output/live-next/accept3-owned-files.json`.
Shared app/screens/lib hooks belong to other workers; item commits isolate their
own hunks through a disposable private index. The running demo stays untouched.

### Third acceptance closure

UI item commits on live-mode: f8bebe2 legend; a569ace aiming; 2845623 group
parity; 8f373fd painting exit; 8571263 Scenes; 50f4857 effect visibility/hint;
5cd3f47 animation chooser (subsequently wired after core 6ec4720); d05f72b
portraits; 2392251 removals. Compatibility
1d11cb6 preserves new skill Roll options in radius aiming. Follow-up 4503ace
keeps Markers default after the active-effect crowd timeout. No new Cmd/Action
or keyboard bindings were introduced in this batch; commands.rs/keys.rs remain
coordinator-owned.

Broad guarded check.ps1 before late animation wiring, Phase8/Next/Step
accept3-final/Package mg-ui, Jobs1,
NoUiDebug, UiUnoptimized (no Build): 426 passed, 0 failed, 97 ignored across
11 test binaries/doc suites. The package guard disables native game inputs;
separate real-game-resource/fake-store checks are live_review 22/22 and
live_effects 4/4, plus the targeted Scenes/windows/spawn checks recorded above.
All-target mg-ui Clippy and a non-mutating rustfmt check of the 23 touched Rust
paths pass. Logs phase8-accept3-final-{focused,clippy,rustfmt}.log and
phase8-accept3-vfx-final-live_effects.log remain under live-next. This is UI
validation, not a new workspace gate or connected-client acceptance.

Animation execution is wired after committed core 6ec4720. The final wiring
check repeats the 22 real-area view tests and verifies the parser and UI Clippy
under Step accept3-animation-wire. It supersedes the earlier disabled-state
picture/check. Native animation storage/action proof belongs to Astra2;
connected-client and model-specific playback remains the owner's check.
The 16-effect crowd GPU timeout prevents a Full-default claim; the individual
player effect and the explicit Show Full effects affordance are proven.
Coordinator controls application/script refresh and the next two-monitor
acceptance. The running demo was never stopped or rebuilt.

Normal binaries match the start ledger in size, mtime and SHA256: moonglow.exe
57659392 bytes, mg.exe 17174016 bytes. No push or process control. Required
source, the one library ghost fixture, before/after pictures and final phase
logs remain. Cleanup removed 19 identified own staging/spec/helper files,
generated file-dialog test copies and superseded/partial outputs: 2772273
logical bytes. No shared caches/builds, other workers' files or directories
were removed; no physical/free-space recovery claim. The ledger records exact
paths/status and hashes; every retained required path was checked to exist.

Session 01a12fe8-1458-7551-99fc-b277832def84 history remains at
the agent's local Codex session file
(324640617 logical bytes when measured during closure; still active). History
cleanup is a separate consented task after this session; no deletion mechanism
was invoked or approved. This document/manual04 and the existing index/log
contain the durable implementation knowledge; the mailbox carries the short
coordinator handoff.

## Live filter and camera regressions (2026-10-13)

The coordinator's live-demo probe retained all 72 object previews across
view/service restarts; the reported player-only view matched the Players
filter. This is a filter discoverability fix, not cache invalidation.
The map now counts reported objects rejected by `live_view::visible()` and
shows **N of M hidden by filters / Show all** below the report-age line.
Show all resets panel chips/search and enables Creature/Placeable/Door/Item
kinds. Separate legend overlay preferences remain as chosen. No commands,
module edits or forced cache/report resets are involved.

Reproduce with the real-data area harness and a fake PC+NPC report:
`tools/live/check.ps1 -Phase 8 -Next -Step filter-after -UiTest live_review
-Jobs 1 -NoUiDebug -UiUnoptimized` (PowerShell 7, no `-Build`). The new
`live_review_filter_notice_restores_chips_search_and_kinds` clicks Players,
checks the notice, resets it and clicks both restored markers; it also tests
search/toolbar recovery, unchanged report reads and no game/module edits.
Before/notice/restored pictures are in
`target/test-output/live-next/visual/filter-{before,notice,restored}.png`.
`MGLIVE_FILTER_PICTURE=before` captures the original symptom on the old code.
Filter validation: baseline and fixed real-data `live_review` gates each passed
23/23; guarded all-target mg-ui Clippy and touched-file rustfmt passed.
All three pictures were inspected: only the PC before, the explicit
1-of-2 notice, then both restored markers and table rows.
Own-file ledger: `target/test-output/live-next/filter-pan-owned-files.json`;
this task uses the existing fixture, without additional module/game copies.

Camera correction: the editor's Ctrl+primary-drag pan path now also runs in
Live. Group input cancels/declines any Ctrl/Command box; plain drag replaces
selection and Shift-drag adds. Ctrl+click still reaches the existing marker
toggle, and Ctrl dragging a selected placeable never queues `set_position`.
The manual camera key table and Live selection text describe that split.

The new `live_review_ctrl_drag_pans_and_ctrl_click_toggles_without_a_box`
uses a PC, NPC and placeable in the area harness. It compares orbit targets
before/after a ground drag and selected-placeable drag, checks the selection
is preserved, toggles markers both ways without camera movement, then verifies
plain replacement and Shift addition without game commands/module edits.
`MGLIVE_PAN_PICTURE=before` captured the old camera target and visible Ctrl box;
the fixed run captures the panned map without a box at the same drag endpoint.
Pictures: `target/test-output/live-next/visual/ctrl-pan-{before,after}.png`.
Reproduce via the same guarded command above with `-Step pan-after`; these are
synthetic UI proofs, distinct from owner acceptance on the running demo.
Camera validation: baseline and fixed real-data Live review gates passed
24/24; guarded all-target mg-ui Clippy and touched-file rustfmt passed. The
before/after camera pictures were inspected. No normal app binary was rebuilt:
moonglow.exe and mg.exe sizes, mtimes and SHA256 hashes match the ledger's
start values. No demo processes were stopped/started and nothing was pushed.

The ledger tracks required paths and verified that each exists. No disposable
copies/scripts were created in this follow-up; retained source/manual/index/log,
five before/after pictures and six check logs remain for review (0 bytes removed;
no free-space claim). Shared fixtures/builds and other workers' changes remain
untouched. Session 01a12fe8-1458-7551-99fc-b277832def84 continues to use the history path
recorded above; history is untouched and eligible only for separately authorized
cleanup after the session ends. No new decision is needed for these fixes.

## Fourth acceptance integration (2026-10-13)

The third view list above is implemented on `live-mode`; its old animation
placeholder description is corrected to the wired operation. The window record
now identifies the integrated branch and current inventory/Roll/aiming behavior,
while retaining dated proof history. Original third-list numbering 10-14 was
not present in the supplied view brief or these records; the coordinator was
asked for that mapping rather than inventing it. Items 27 (Shaders) and 28
(module-text decoding) are on the branch and are included below.

### What to click in acceptance 4

Use the coordinator's refreshed private demo with its matching game scripts;
this integration task neither starts nor stops it. Player mutation checks need
both player-operation switches. Prefer disposable test objects for Kill/Take/
drop/destruction checks. Check the game client as well as Moonglow's command
result; synthetic tests do not establish connected-client appearance or NUI.

1. **Legend:** expand/collapse it; confirm one NPC faction colour/filter list.
   Toggle Players/NPCs in Live Panel and toolbar kinds: the hidden-count notice
   appears on the map. Show all restores objects. Toggle Live off/on and repeat.
2. **Aim audit:** try Walk/Run, Attack, Teleport, Patrol, palette and Library
   Spawn, radius Skill Check, radius Stop Fighting, ground VFX and Jump All
   Players. Each arms a crosshair/ghost; map click sends, right-click/Escape
   cancels. Change a radius while aiming and verify the circle follows it.
3. **Groups:** Ctrl+click two markers; plain box replaces, Shift box adds,
   Ctrl+drag pans without changing selection, including from a placeable.
   Check group movement/attack/Stop Attack, Heal, Freeze, Faction, AI, rest,
   invulnerability/immortality, Limbo, Say, VFX, appearance, animation and
   inventory-to-log; each eligible actor gets its own command. Check player-off
   gating and NPC-only hints before testing allowed-player operations.
4. **Painting:** pick a Tiles palette entry and confirm Live painting; click/
   drag a few cells. Stop painting/Escape clears the palette/stroke, later
   ground clicks paint nothing, and the module remains unmodified.
5. **Scenes:** Record two view commands, stop recording, inspect step summaries
   and timing, Save As/Export, Import, then Play/Stop on disposable targets.
   Cancel a file dialog; import an invalid JSON file without losing the draft.
6. **Effects:** apply a lasting mind-affecting VFX to a player. In Markers use
   Show Full effects; verify the player's body-attached effect in Moonglow and
   the client. Check Lightshaft Green's intrinsic beam; increase effect count
   gradually and assess frame time. Markers remains default after the recorded
   16-effect GPU timeout, so crowd Full smoothness is still an acceptance check.
7. **Animations:** on NPC, permitted PC and a group, choose a named constant,
   then a valid custom animation, loop/speed/seconds. Verify actual client
   playback, ending/restoration and cancellation by a new movement order.
8. **Portraits:** open Selection with two creatures; check portraits, names,
   HP and kind. Click a portrait to focus without losing the group; also check
   the Live Chat speaker portrait beside a run of that speaker's messages.
9. **Removal controls:** on test NPC/PC/group use Take Gold, Take XP and Remove
   Level, confirming actual balances and level/BAB/HP changes; level-one removal
   and player-off controls must refuse appropriately.

Additional implemented integration checks (numbering 10-14 awaiting the
coordinator's original brief):

- **Skill Roll:** select a PC, Skill Check, set bonus and optional hidden DC;
  Send displays a player Roll prompt without rolling. Click Roll once, then
  test no-click timeout. Check rank/ability/feat/item/effect contributions and
  matching outcome in the Skill window/Live Log. NPC selection uses radius.
- **Inventory:** select a creature, Inventory, Refresh; check native icons and
  equipped slots. Give a palette item, drag equip/unequip, confirm a ground
  drop and a named Take, then test gold. Verify the game after refresh. Backpack
  arrangement is intentionally local: native cells/footprints/stack splitting
  remain deferred, not claimed as implemented.
- **NPC Library:** capture a selected creature, choose its saved entry and Spawn;
  inspect the creature ghost, cancel once, then click the map to spawn it.
- **Live Chat and agents:** open Live Chat, select a speaker, send as NPC/to a
  selected player/to all players, check portraits and Polish text. For an agent,
  discover the session, call live_events since n, locate a target, act and wait
  for its result; compare the event/command with the GUI and live game.

27. **Shaders:** selected LC sail/map placeable > Shaders; verify actual MTR
    SHSailLogo/U/V/Width/Height defaults, edit a scalar/vector, Reset parameter
    and Reset material. Verify changes in the client and player-global uniforms
    on an allowed PC; defaults are asset defaults, not override readbacks.
28. **Module Polish text:** trigger existing module NPC speech/name and a server
    log message containing Polish characters; compare game, bubble, Live Chat,
    Live Log and live_events. Include module-origin CP1250 and UTF-8 text, not
    only newly typed editor text; native tags may be sanitized by the engine.

### Gate and reproduction

PowerShell 7 from the repository, pinned Cargo 1.98.0 through the existing guard:
`tools/live/check.ps1 -Phase 8 -Next -Step integration4 -CheckFmt -Jobs 1
-NoUiDebug -UiUnoptimized`. CheckFmt skips the formatter's mutation pass and
retains workspace Clippy (`--all-targets -- -D warnings`), mg-live/workspace
tests and `fmt --check`. Initial head `2f07995`: 1,173 workspace tests passed,
0 failed, 117 ignored; Clippy and fmt passed. NWN_ROOT stays guarded, so native
and installed-resource cases may skip; this is not owner acceptance.

`-GuiBuild` is a new narrow alternative to the existing multi-app `-Build`:
`tools/live/check.ps1 -Phase 8 -Next -Step integration4 -GuiBuild -Jobs 1
-NoUiDebug -UiUnoptimized` builds only package/bin moonglow. It never controls
demo processes. The requested exact `okf lint docs --links` is unresolved: no
okf command was found on Windows/WSL PATH or the usual local tool locations;
the coordinator was asked for its implementation/path. A supplemental Python
YAML/Markdown path scan found no issues in 61 documents / 665 local links, but
is explicitly not a substitute or claimed OKF pass. The first GUI-only build passed (58,785,280 bytes); it was never launched.
The workspace test build also refreshed mg.exe, separately from -GuiBuild.

The final gate then caught a new regression introduced by `e2f4ad5`:
`manual::tests::links_between_chapters_resolve` rejects chapter 18's new
`../live-campaign.md` link because that file is not a bundled manual chapter.
The fix labels `docs/live-campaign.md` as source-repository documentation,
which it is; the in-app chapter invariant remains strict. No campaign guide,
example test or manual navigator behavior is changed. Fix commit `b1e075d`
passes all four existing manual checks. The full rerun at `b1e075d` (including
Astra2's committed `e2f4ad5` example tests) passes **1,175 workspace tests,
0 failed, 117 ignored**; the separate mg-live phase passes 107 with 4 ignored.
Workspace all-target Clippy and fmt check pass. The input hashes recorded before
the final gate match Astra2's committed guide/test/hook; those files were not
edited by this task. The GUI rebuilt successfully after the fix in 15.90 s.
Use `-Step integration4-fixed` in both reproduction commands above for the
retained final logs. Guard-option commit: `32c254a`.

Current `target/debug/moonglow.exe` SHA256:
`1ab1d04f54ccb3d18a3418a559a2561a39a1a1468f95fe563b297722367fd8d4`.
No Start/Stop-LiveDemo, `-Build`, executable launch, merge or push was used.
The full requested integration gate is **not yet fully verified**: exact OKF
lint awaits the intended tool and original item numbering 10-14 awaits its
brief. These two questions were sent to the coordinator and remain unanswered;
no substitute lint or guessed numbering is labelled as a pass.

Cleanup after the Rust/build stage: removed seven identified superseded owned
logs (136,891 logical bytes); retained the root failure, focused manual check,
final gate/build, missing-OKF log and supplemental documentation check. No
physical/free-space delta is claimed. Shared targets, fixtures, the guide and
other workers' files remain. Required paths were checked to exist. Session
01a12fe8-1458-7551-99fc-b277832def84 and its active history are unchanged;
the ledger records that history's exact path/size for separate authorized
cleanup after the session, never during it.

Ownership ledger: `target/test-output/live-next/integration4-owned-files.json`.
Astra2 owns live-campaign.md and campaign_examples_tests.rs and their hooks;
this task does not edit them. Shared artifacts remain one current working set.

## Later (not in these phases)

Several servers at once; durable scene object aliases; the NWNX_Redis
reconnect patch proposed upstream; editor operations
([phase H of the Redis plan](live-mode-redis.md)).

## Order and acceptance

Phases 1 → 1b → 2 → 3 → 4 → 4b → 5 → 6 in order; one local commit per phase when its checks pass;
never `develop`, never push. Phase 1 acceptance is recorded above; the next
owner acceptance stops were after Phases **2 and 4b**. The later
owner-approved master roadmap supersedes those stops: **E2+E3, E4, E5+E6,
and the end** (see the implementation record). Each acceptance uses
the owner's two-monitor run (Moonglow left, client right) with the
scenario from the recording — the player provokes the crowd — and the DM
(or an LLM through `moonglow-mcp`) sees the fight in the view and the
event panel, stops it, raises the player, walks an NPC with the targeting
cursor and `hold`, and sees the player's model change armour.

## Fourth acceptance UI follow-up (2026-10-13)

A11: Live layout retains Live Chat and the other tools, but opens Live Log only
on demand. Fake-store area test verifies the initial absence and explicit
right-pane opening. Before/after layout images use the existing area fixture;
no normal binaries or demo lifecycle actions. Reproduce via guarded Phase8,
Next, Step accept4-layout, UiTest live_accept4, Jobs1/NoUiDebug/UiUnoptimized.
Own evidence is tracked in `target/test-output/live-next/accept4-ui-owned-files.json`.

A1/A2/A3/A16 share `live_context.rs`: one single/group section structure, direct
Walk/Attack/Say/Inventory shortcuts, concise eligibility, identical Attack labels
and an explicit disabled reason. Move reuses existing single/group aim paths;
Combat retains exact Stop Attack separately from aimed radius Stop Fighting.
Danger confirms Kill, removal without body, and Limbo. Inventory has one entry,
only for exactly one allowed creature; changing its primary target preserves
the selection. Scripts retarget each selected object through the existing
queue/scene recorder. No game-core/player policy bypass is introduced.
Player-only gold/XP grants, Give Level and Alignment retain their existing
PC eligibility. Message/Journal/Boot dialogs require one eligible player;
unsupported multi-recipient use is explained on hover.

Menu proof uses the same area fixture and fake store: before/after single/group
pictures, Combat and Kill confirmation pictures; targeted assertions cover
no send before aim/confirmation, exact group targets, unavailable attackers,
and one eligible inventory focus without losing the group. Reproduce guarded
`-Phase 8 -Next -Step accept4-menu-targeted -UiTest live_accept4 -Jobs 1
-NoUiDebug -UiUnoptimized`, plus `-Step accept4-menu-review -UiTest live_review`.

Menu checks: 24/24 real-resource view tests, 3/3 focused acceptance tests and
all-target mg-ui Clippy pass. The animation test caught selection drafts sharing
a temporary key; the menu now scopes drafts by area and selected IDs. Expanding
the nested animation list needs settled layout frames before a synthetic click.

A12: Scenes now explains a recorded list of DM commands in one sentence and
presents Record, Review steps, Replay in that order. The record button toggles
Recording... / Stop. Inline action/target/delay edits update the same validated
step draft; advanced JSON/schema/wait controls remain expandable. A compact
Scene menu contains all file actions; saved names use a nested Load menu
instead of a combo whose popup closed the parent. Empty state includes an
example, disabled workflow/file controls explain their prerequisites.
Reproduction: guarded Phase8/Next/Step accept4-scenes/UiTest live_scene and
Step accept4-scene-picture/Picture live_scene_snapshot with
MGLIVE_PICTURE_STAGE=accept4-after. Source test covers fake-store record,
file roundtrips/cancellation/invalid input, inline edits, Play/Stop.

Scenes verification: the fake-store workflow test passes, including inline target
and delay edits queuing nothing; the normal, file-menu and empty-state pictures
were inspected against scene-accept4-before.png. This proves the UI/file workflow,
not a new connected-client acceptance.

A14: faction lookup and faction marker filters already excluded non-creatures.
The confusing part was their fallback colour: the same orange as Commoner.
Placeables, doors and ground items now have distinct muted kind colours in
all colour modes, also shown by their legend labels. The faction list explicitly
says Creatures only. The before picture reproduces the commoner/cabinet ambiguity;
the after fixture additionally covers the report's `doors` and `items` arrays.
App proof verifies disabling Commoner leaves all three world kinds visible;
palette unit cases cover each kind/mode despite faction=2 and own=true.
Reproduce guarded Step accept4-colours with UiTest live_accept4 (4/4 passed,
pictures inspected) and Package mg-ui/Filter live_colors::tests.

A14 unit checks pass 2/2; current-tree all-target mg-ui Clippy passes.

Per-window polish follows the named A items. Committed checks and pictures:
Selection aa27630, Live Panel 1d84dea, Live Chat 10ff80b, Live Log 3d09063,
NPC Library 8110468. The shared right-dock audit and placeable Appearance
entry are 42ab03e. Workflow and reproduction notes accumulate in
[Live windows](live-windows.md#fourth-acceptance-ui-audit-preparation).
Captures use the existing GPU UI harnesses and fake reports, with guarded
Jobs1/NoUiDebug/UiUnoptimized checks. No demo lifecycle or normal app build
is part of this pass. Skill 6a0aa55, Shader 9d6d4f3, Properties 5d6dc0b,
Tile Properties af6dc62 and Options e207179 are also verified and committed.
Inventory 683e34e / 84662bb and item picker 572429c are verified after B6/B9
release. VFX was reserved until Astra2's B10 release 863ef0d, then polished with
the same window audit and a fresh released baseline. The final view regression
passes 24/24, including static transforms, tile confirmation/restore, menu and
group commands, aim/cancel and player gates. The animation test now settles its
popup with frames instead of writing an unconditional diagnostic PNG.

Final repair a0f6da5 fixes a pre-existing Panel test label collision (the map
legend also has Players) and the B6 inventory cell's eight-argument lint failure.
The UI lint and eight authored inventory checks pass after the repair. The final
real-resource acceptance audit passes 7/7, Panel 2/2, clients 1/1, Chat 1/1;
Scenes and shader checks/pictures remain passing. Own Rust formatting and local
Markdown file targets were checked. This pass does not claim a new full workspace
gate, exact OKF lint, or connected-client acceptance.

All named A items are complete. Released-window polish includes VFX after B10;
the incoming Live session subpane of Factions still belongs to Astra2 until release.
Final evidence, commands, cleanup and ownership limits are in
[the window handoff](live-windows.md#fourth-acceptance-polish-verification-and-handoff).
No demo control or application build occurred; both application hashes/timestamps
match the start of this batch. Five obsolete own outputs were removed (1,502,762
logical bytes), with all required before/after evidence retained for acceptance.


## Shared Live portrait fallback (2026-10-16)

Owner report: every Live Chat speaker had a chicken portrait. Read-only
`live_area_state` for the running demo's `demo_area` captured 32 NPCs,
all with `app:6`, `portrait:"po_"`, and native look `Gender:0`. The capture is
`target/test-output/live-portrait/report.json` (boot
`6616ab9dbc93e58ec772030e23a86093-13741407`, report seq18962); only Gender is
retained from the large looks. No demo build, start, stop or game mutation was
performed. The app proof replays three captured identities/appearance fields
in a small generated area, changing only their positions; its chat lines are
authored test input, not a claim about the owner's conversation.

Diagnosis against the installed Steam game, read through `mg cat --no-user-dir`:
`appearance.2da` row6 is Human, MODELTYPE=P, PORTRAIT=empty. The existing lookup
already used the correct row. Its unconditional `po_unknown_` fallback resolved
to `po_unknown_m`, which is a chicken in this installation. The shared resolver
now preserves reported/custom bases and size variants, then uses that exact
appearance row's PORTRAIT. Dynamic humanoids with no portrait map through
`racialtypes.Appearance` to `portraits.Race`, matching the reported Gender and
excluding plot/inanimate portraits. `appearance.RACIALTYPE` is not that race
index (Human contains11 there, while portraits uses6). Old reports without
Gender use the stable male default. An unknown/unresolvable appearance displays
`?`; it never gets an arbitrary animal image. This is one shared path for
Selection, Focus (including group members) and Chat. Persisted Focus members
accept older files without Gender.

On this install the first eligible human male row is portraits60,
`villager1_`, hence `po_villager1_s`. The full-size installed image was separately
inspected: it is a human man with a pipe. All these unassigned Human NPCs share
that generic default; distinct individual faces require reported portraits.
Tables resolve from `key:nwn_base`; exact CLI-output hashes are retained in
`target/test-output/live-portrait/tables.json`:

- appearance: `815c0b3bce0895e9f17d4b92cb02a6d34366267b5a4b9081dece0f4eee7d7a1a`
- racialtypes: `450dc70dbd971195638e308a97a420e1aa301454021fabee2b88b2f230f26e76`
- portraits: `b70e643efc226571e9502173781bcecb76de21ecdada0c7641479ee5dc467602`

Proof: three focused unit tests pass (reported row selection, race/gender mapping,
custom-size priority and absent fallback), and the real-resource app replay
passes. Before and after Chat/Focus/Selection PNGs are in
`target/test-output/live-portrait/`; the before images visibly reproduce the
chicken and the after images show human faces. App assertions also cover all
three names/portraits, unchanged module state and an empty game-command queue.
`mg-ui --all-targets` Clippy and workspace rustfmt checks pass. These are isolated
app-harness checks, not acceptance in the owner's running GUI.

Reproduce on this workstation (PowerShell7 path comes from local.ps1):

```powershell
. ./tools/live/local.ps1
$env:MGLIVE_PORTRAIT_REPORT = (Resolve-Path target/test-output/live-portrait/report.json).Path
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase F -UiTest live_portrait -Step portrait-after -Jobs 1 -NoUiDebug -UiUnoptimized
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase F -Package mg-ui -LibraryOnly -Filter live_portrait -Step portrait-unit -Jobs 1 -NoUiDebug -UiUnoptimized
```

Without `MGLIVE_PORTRAIT_REPORT`, the committed test uses authored equivalent
Human/po_/male reports. GPU access is serialized through `mg_testkit::gpu::hold`;
resources are read from the installed game, never written there. The scratch
module/data are in `target/test-output/ui-area-live-portrait`, rebuilt in place.
No copied game images or reports are committed. Workset ownership and cleanup
receipts are in `target/test-output/live-portrait/ledger.json`.


## NPC card and Memory menu (2026-10-16)

The Live object menu now offers NPC card and Memory directly after Focus for
one non-player, non-DM, non-possessed creature. NPC card resolves the stable
assistant card identity before opening `Tab::LiveNpcCard`; it does not confuse
the engine object id with a saved tag/resref card id. Memory selects/adds that
NPC's Focus entry, resets any prior member/area filter and reply draft, and
opens Memory. Reopening it does not duplicate the watch entry. With Focus
disabled, Memory is visibly disabled with the same Options hint. GM card stays
in the Live menu. Code is in `live_context.rs` and the shared card opener in
`live_assistant.rs`; Astra2's `live_view.rs`/`live_layout.rs` were not edited.

`live_npc_menu_card_memory_and_exclusions` passes using installed resources and
the real app UI: first-level buttons, stable card tab and form, selected Memory,
repeat-open dedupe, disabled Focus, PC/DM/possessed exclusion, empty game-command
queue and unchanged module. Before/after menu, opened card and selected Memory
pictures were rendered and inspected under `target/test-output/live-npc-menu`.
This is isolated harness proof, not a build or lifecycle action on the owner's
demo. The shared portrait change is `2662c27`; it supplies the human default in
the Memory picture. All-target mg-ui Clippy and workspace fmt checks pass.

Reproduce with PowerShell7 (after loading `tools/live/local.ps1`):

```powershell
& $env:MGLIVE_PWSH -NoProfile -File tools/live/check.ps1 -Phase F -UiTest live_npc_menu -Step npc-menu-after -Jobs 1 -NoUiDebug -UiUnoptimized
```

The fixture is rebuilt in place under `target/test-output/ui-area-live-npc-menu`.
Workset/session ownership and final cleanup receipts share
`target/test-output/live-portrait/ledger.json`. The baseline came from `0191299`;
before-only test switches label/capture the pre-fix UI, they do not undo fixes.


Cleanup at this handoff: automatic approval review rejected the native PowerShell
removal of 27 inventoried disposable files, stating `blocked by policy`.
Their total logical size is427,459 bytes: extracted portrait inspection images,
the two generated app fixtures/data and a superseded failed-compile log. No files
were removed and **0 bytes reclaimed**; no alternate deletion was attempted.
The ledger preserves exact paths and ownership. Retained deliverables are code,
report/table receipts, before/after app pictures and focused check logs. Shared
Cargo outputs, installed game data, demo files and other workers' files were
preserved. Physical allocation/free-space change was not measured or attributed.
The seven Focus unit checks and one event-person refresh check also pass.

OKF is unavailable on this workstation, so the existing index/log were updated
and the newly added local Markdown links checked directly; this does not claim
an `okf lint` run. Active Codex session
`01a1441b-229d-7572-8275-c6a7c7824670` was not edited or deleted. Its history is
reported for a separate retention decision after task closure; no supported,
approved history-cleanup mechanism is configured in this task.
