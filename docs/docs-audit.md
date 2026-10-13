---
type: Report
title: 'Audit: the documentation at 1.21.3'
description: An audit of all 46 documents in docs/ against the code at 1.21.3 - per document, whether it is correct, what was wrong and fixed, what is wrong and left, and whether it is still worth keeping (keep, update, trim, deprecate) - with the decisions it leaves to the owner.
tags: [audit, documentation, review]
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-12T05:37:34Z }
stale_after: 2026-12-01T00:00:00Z
sources:
  - id: tree
    resource: The repository at commit bc9a124 (v1.21.3), worktree docs-audit, 2026-10-12
    title: The code the documents were checked against
  - id: mg
    resource: mg 1.21.3 as installed (mg --help and every command's --help, mg checks, mg cat)
    title: The command-line tool's own account of itself
  - id: wiki
    resource: ~/.local/opt/neverwinter/wiki (nwn.wiki mirror, last synced 2026-10-05)
    title: The local nwn.wiki mirror
  - id: reviewers
    resource: Seven review passes (claude-opus-5-5), one per group of documents, each reporting the code line, command output or wiki page behind every verdict
    title: The reviewers' reports of 2026-10-12
---

# Audit: the documentation at 1.21.3

All 46 documents of `docs/` were read against the code at v1.21.3 (commit
bc9a124) on 2026-10-12, for three things: is it correct, is it true of
the present, and is it still worth keeping.

## Outcome

- **Nothing should be removed.** Thirty documents are sound; nine
  need an update that takes judgment; seven are candidates to trim or
  mark historical (a decision for the owner, below).
- **The manual is accurate.** 17 chapters and its front page, 746
  claims checked against the UI code and `mg`'s help, 34 defects, 20
  fixed here. No
  chapter describes a feature that isn't there.
- **Where the documents are wrong, they are stale, not invented.** The
  code moved on through 58 releases, and status lines, "not yet"s and
  open-question lists were left behind. The worst cases are `PLAN.md`'s
  status tables, about 20 entries of `deferred.md`, and the two surveys
  in `research/`.
- **The plugin API version was stated five ways** (0.1 in chapter 15 and
  the repository's README, 0.2 in the plugins' README and the type
  file's header, 0.3 in the code and chapters 16 and 17). All now say
  0.3.
- **Three things found in passing were about the code**, not the
  documents; two were defects and are fixed (below).

## How it was done, and what it does not cover

Seven reviewers took disjoint groups of documents. Each verdict had to
name what it was checked against (a source line, a command's output, a
test, a commit, a page of the wiki mirror); what could not be checked is
listed as unverifiable rather than passed. Claims were sampled, not
exhausted: the specific ones that drift (labels, keys, flags, paths,
counts, versions, "not yet" and status statements).

Not covered, and so not vouched for:

- **Nothing was run but `mg`'s read-only commands.** No test, no build,
  no server, client or Aurora. Every measurement the documents report
  (timings, corpus counts, what the client draws, what Aurora writes)
  is unverified by this audit; what was checked is that the test or
  capture each one names exists and asserts what the document says.
  Two tests were run afterwards on the edited documents, and pass:
  `cargo test -p mg-plugin --test examples` (7 tests; it holds chapters
  16 and 17 and the examples to the code) and `cargo test -p mg-ui
  --lib manual` (4 tests; the built-in manual).
- **Statements about Aurora's and the game's behavior**, and about
  other projects.
- **Parts of three large notes were not read**: `notes_gff_changelog.md`
  §D (sampled only), §B18–B19, §E9–E11; `notes_models.md` §C, §D1,
  §D4–D6, most of §E, §F–§H; `notes_shaders.md` §A.3's table, §B.4–B.5,
  §G–§I, §K; `notes_tilesets.md` §D.1–D.3, §E, §G.
- `deferred.md`: 86 of about 290 entries were checked in the code.

No document was given a `verified` entry: a sample is not a
verification.

## Verdicts

"Checked/defects/fixed" count sampled claims. A defect left unfixed
needed more than a proven change of a few words.

### Top level

| Document | Verdict | Checked / defects / fixed | What is left |
| --- | --- | --- | --- |
| [PLAN.md](PLAN.md) | Keep §1–§5, §7, §8; **trim** the histories | 40 / 16 / 2 | See "The plan" below |
| [deferred.md](deferred.md) | Keep; **needs a tidy** | 86 entries / 37 / 0 (report only) | See "The deferred list" below |
| [findings.md](findings.md) | Keep | 75 / 8 / 0 | Twelve findings lack the "Moonglow:" line the preamble promises; "mirror synced 2026-09-29" reads as current (it is 2026-10-05); counts such as "every base-game item (2,763)" are a full run's, an everyday run sweeps a sample |
| [test-review.md](test-review.md) | **Deprecate or keep as dated** | 22 / 1 / 0 | Its mechanics are in PLAN §7 and `CLAUDE.md`, its open points in the deferred list; only its timing tables are nowhere else |
| [plugin-proposal.md](plugin-proposal.md) | **Trim** to the decision record | 27 / 9 / 0 | Describes API 0.1 as current; its roadmap for 0.2 is not what shipped as 0.2 and 0.3; "cannot reach other files" and "a time budget per job" are not what was built |
| [lights-proposal.md](lights-proposal.md) | Keep, **needs update** | 16 / 4 / 0 | `status: draft` though way A was built and measured on 2026-10-07; way B's "no model writer", "no 2DA merger" are overtaken |
| [wiki-proposal.md](wiki-proposal.md) | Keep until submitted | 30 / 2 / 0 | All fifteen wiki pages are unchanged upstream; no trace the corrections were sent. Entry 3 attributes a sentence to the HAK page that only Resource Limits has; entry 13 names the wrong test for "1,520 of 1,582" |

### Manual

| Document | Verdict | Checked / defects / fixed | What is left |
| --- | --- | --- | --- |
| [README.md](manual/README.md) | Keep | 6 / 0 / 0 | |
| [01 Getting started](manual/01-getting-started.md) | Keep | 22 / 3 / 0 | Omits the Scoop install; "Vulkan, Metal or Direct3D 12" though OpenGL is tried too; GOG's folder is not looked for on macOS |
| [02 The main window](manual/02-main-window.md) | Keep | 48 / 6 / 5 | (F10: fixed in the code, below) |
| [03 Modules](manual/03-modules.md) | Keep | 62 / 1 / 1 | |
| [04 Areas](manual/04-areas.md) | Keep | 95 / 3 / 1 | The button is "Give Every Area This Page's *page* Settings" |
| [05 Blueprints](manual/05-blueprints.md) | Keep | 70 / 4 / 3 | "neighbouring", "neighbours" |
| [06 Conversations](manual/06-conversations.md) | Keep | 34 / 0 / 0 | |
| [07 Scripts](manual/07-scripts.md) | Keep | 36 / 1 / 0 | "Compile in its editor says so." is a fragment with no subject |
| [08 Journal and factions](manual/08-journal-and-factions.md) | Keep | 16 / 0 / 0 | Omits the Faction Editor's Full Detail |
| [09 Build, verify and test](manual/09-build-and-test.md) | Keep | 38 / 0 / 0 | The Compile sub-checks show only with Advanced Controls |
| [10 Options](manual/10-options.md) | Keep | 62 / 3 / 2 | (Language reload: fixed in the code, below) |
| [11 Command-line tools](manual/11-command-line.md) | Keep | 75 / 1 / 1 | Omits `mg serve --port`/`--name`, `mg 2da-diff --resource`, `watch --json`'s `ended`; its index description leaves out four of its sections |
| [12 Differences from Aurora](manual/12-differences.md) | Keep | 17 / 2 / 1 | Says spell checking is left out, while Build Module shows a disabled Spell Check box saying "Not yet" |
| [13 Troubleshooting](manual/13-troubleshooting.md) | Keep; small trim | 27 / 1 / 0 | The "window shows only the module tree" section is about a bug fixed before 1.10 |
| [14 Coming from Aurora](manual/14-coming-from-aurora.md) | Keep | 41 / 2 / 1 | "There's no OK or Cancel" is true of property editors only; repeats chapter 12 in "Habits to change" |
| [15 Plugins](manual/15-plugins.md) | Keep, **needs update** | 22 / 3 / 1 | "A plugin written for one version of the API is not run by a Moonglow that has another" is wrong (0.1 and 0.2 plugins run under 0.3); the `--event` example names `tag-conventions`, which declares no event |
| [16 Writing plugins](manual/16-writing-plugins.md) | Keep | 30 / 2 / 2 | "it writes one kind of file, a hak" two paragraphs before folder writes |
| [17 Plugin API reference](manual/17-plugin-api.md) | Keep | 45 / 2 / 2 | A hak name's 32-character limit is not stated |

### Plugins

| Document | Verdict | Checked / defects / fixed | What is left |
| --- | --- | --- | --- |
| [README.md](plugins/README.md) | **Trim** to a pointer page | 9 / 2 / 1 | Repeats chapter 16's commands and the index; "what is planned beyond API 0.2, is in the proposal" |
| [CHANGES.md](plugins/CHANGES.md) | Keep | 12 / 1 / 1 | Lacks what 1.21.3 added without a new API version: kept event grants, `kinds` knowing the named list tables |
| `examples/`, `types/moonglow.luau` | Keep | 11 files / 1 / 1 | No example uses anything of 0.3 (tables, folder writes, events) |

What holds these to the code: `crates/mg-plugin/tests/examples.rs`
requires every API name in chapter 17 and the type file, `hello`'s lines
in chapter 16, and the examples to load and run. It does not hold
arguments, returns, limits, chapter 15, or the entries of CHANGES; and
its name match is by prefix (`mg.int` is satisfied by `mg.int64`).

### Parity

| Document | Verdict | Checked / defects / fixed | What is left |
| --- | --- | --- | --- |
| [checklists.md](parity/checklists.md) | Keep, **needs update** | 62 / 11 / 6 | Rows marked ◐ whose stated reason is gone (creature inventory, Update Instances, Creature Classes); "Phase 7/8/9" wording; eight tests not in `app.rs` named without their file; no rows for the Plot Wizard, the item generators or the main menus |
| [aurora-ui-inventory.md](parity/aurora-ui-inventory.md) (generated) | Keep, committed | provenance only | Not edited by hand. Cannot be regenerated: `gen.py` reads `forms.json`, `tlk/dialog.json`, `tools/exe/.data`, `locbases.json`, none in the repository and none with a recipe; its "regeneration" trailer names scratchpad paths; `generated.at` is the bootstrap commit's time, not a run's |
| | | | Read by `mg-ui/src/keys.rs`, the checklists, the plan |

### Research

| Document | Verdict | Checked / defects / fixed | What is left |
| --- | --- | --- | --- |
| [ee_tech_brief.md](research/ee_tech_brief.md) | **Trim** §4–§6 | 45 / 12 / 6 | §0.1 and §6.6 still recommend compiling the stock shaders (decided against, PLAN §5.5); "open items" answered in the findings; names `draft_mine.md` and scratchpad scripts that are gone; 441 areas here, 439 elsewhere |
| [notes_gff_changelog.md](research/notes_gff_changelog.md) | Keep, needs update | 55 / 4 / 0 | §B17's palette folder IDs contradict `notes_tilesets.md` §F.3 and the code; "verify by saving a DLG" was verified; §G's open questions partly answered |
| [notes_models.md](research/notes_models.md) | Keep, needs update | 30 / 7 / 6 | §I.2's open list: six of eight answered in §B; §0.6 still says the texture is the mesh's bitmap |
| [notes_shaders.md](research/notes_shaders.md) | Keep, needs update | 22 / 4 / 2 | §M.5, §M.11 and §A.3 on Aurora's renderer are moot; scratchpad paths |
| [notes_tilesets.md](research/notes_tilesets.md) | Keep, needs update | 22 / 4 / 1 | §B.4 and §I items 1–6 and 9 still open on the page, settled by §B.4a/b and the findings; 441 against 439 areas |
| [notes_creature_cr.md](research/notes_creature_cr.md) | Keep | 14 / 3 / 0 | "4182 creatures of four captures" is five captures, 4204; struct ids ≥ 0x4000 count nothing |
| [notes_creature_spells.md](research/notes_creature_spells.md) | Keep | 12 / 0 / 0 | |
| [notes_dialog_options.md](research/notes_dialog_options.md) | Keep | 9 / 0 / 0 | |
| [notes_doctor.md](research/notes_doctor.md) | Keep, **needs update** | 16 / 3 / 0 | Lists 16 checks, `mg checks` has 20 (`custom-tlk`, `mdl-read`, `2da-read`, `mdl-text` missing); the calibration list is behind the test's `KNOWN` |
| [notes_ee_fields.md](research/notes_ee_fields.md) | Keep | 10 / 0 / 0 | |
| [notes_frames.md](research/notes_frames.md) | Keep as dated | 15 / 2 / 0 | Its `verified` entry (stamped 12 s after writing) stands for the measurements of 7–8 October only: 22 later commits touch the code it describes; 1,191 against 1,184 meshes |
| [notes_levelup.md](research/notes_levelup.md) | Keep | 10 / 0 / 0 | |
| [notes_nasher.md](research/notes_nasher.md) | Keep | 12 / 0 / 0 | |
| [notes_scale.md](research/notes_scale.md) | Keep | 22 / 0 / 0 | The Measured column is of 2 October |
| [prior_art.md](research/prior_art.md) | **Mark historical** | 9 / 5 / 0 | Every recommendation was decided the other way (stock shaders, license first, compiler as a subprocess, an outside LSP); unmarked, it still argues for them |
| [community_pain_points.md](research/community_pain_points.md) | **Mark historical** | 30 / 19 / 0 | Its "Moonglow" column is of 0.2.0: Find References, nasher projects, Reload Resources, locking, drop to ground, script navigation are all marked missing; §3's plans have shipped |
| [gff-observed-schema.md](research/gff-observed-schema.md) (generated) | Keep (weak case) | provenance only | Not edited by hand; regenerable from any install; names no game build; `generated.at` is the commit's time. Read by no test or build step |

## The plan

`PLAN.md` is cited by `CLAUDE.md` for §4 (layering), §5.5 (renderer) and
§7 (tests), and those hold: no crate of the 28 depends on one listed
after it, the resource priorities and the sweep mechanics match the
code. What does not hold is that it is a plan "with its current status":

- The Status table, §6 Phases and "After parity" are three overlapping
  histories that disagree (Phase 5 "In progress" with "Exit met"; ✅ on
  five phase headings where Status has more done).
- Phase 12 says "Still to do: build the packages on CI"; the release
  workflow has built them for 58 tags. Phase 8 lists as still to do the
  CR calculation, the wizards and icons, which Phase 11 of the same
  table reports done. §6 Phase 4's "Remaining: building the C++ on
  Windows and macOS in CI" contradicts §9.
- §4 marks `mg-script`, `mg-edit`, `mg-plugin`, `mg-ui` as unfinished
  and `mg-mdl`, `mg-tiles`, `mg-render`, `mg-area` as partial; lists
  eight `mg` commands "so far" of 43, and a `mg render` that does not
  exist.
- §7 claims golden images with an SSIM threshold (no SSIM anywhere; the
  method is region means against client screenshots) and performance
  budgets "checked in CI" (the perf tests are run by hand).
- §9's risks have all been met or retired.

Suggested: one short record of the phases in place of Status and §6,
§9 dropped, the title changed to say architecture, the section numbers
4, 5.5 and 7 kept.

## The deferred list

Of 86 entries checked, every named symbol, test and flag exists. What
is wrong is the list's upkeep:

- **Stale (10):** "Particles in the area: of at most 96 objects" (256);
  "The module tree follows every selection" (only where the area is
  opened out); "Panes folded away: not done, a collapse button" (there
  is one); "An item stood up in its viewer: items on the ground lie as
  modelled" (turned by `RotateOnGround`); "Minimap pictures for
  tilesets without them: isn't offered" (Render Minimap Pictures);
  "Inserting or removing lines in the middle" (its reason is gone);
  "`app.rs`: 15,162 lines" (15,307); "No zip (a new dependency)" (zip
  is one already); "No 2DA editor or merger" and the lights entry that
  waits on a merger (`mg 2da-diff --base` merges); the heading "Release
  and packaging (v0.1.0)".
- **Done, with no gap stated (11):** the four sized "(—)"; in "Cut-outs
  and shadows" the two leading bullets and the "Done:" paragraph;
  "Animations bind by part number: done"; "The middle with no area
  open"; "A waterfall's water ran back up the stream"; "The palette's
  header"; "Items lying in an area"; "The module tree and the object
  selected in the area" (a decision: belongs in Settled).
- **The same gap twice (11 groups):** the tree following the selection;
  the pane's fold button; issue 11; files changed outside (three
  entries); the feminine table as JSON; minimap pictures; issue 3;
  particles; lighting schemes not applied; markers against Aurora; and
  client measurements that the log says were moved to the findings.
- **Misfiled:** three entries under Settled are still open ("Ctrl+V
  refused on Windows", "Sharing key sets", "Area transitions can't be
  selected visually: not understood; asked").

The cause is the two filings that cut across each other, by area and by
the report an entry came from, and entries that are release notes with
a "Not done:" tail. Suggested: file by area only, with the report as a
word in the entry; cut each entry to its gap. Nothing here was changed:
the entries are the owner's to rule on.

## Found in passing: the code

Three things the manual and the code disagreed about. Each was then
tested, on the same day, and the two that were defects were fixed:

1. **F10 had two meanings, and one was dead.** The menu bar took the
   key before the area view could read Select Tiles' default key. A
   test (`f10_over_an_area_selects_tiles`, `crates/mg-ui/tests/app.rs`)
   failed on it. Fixed: with the pointer over an area's view, and while
   F10 is Select Tiles' key, it is the view's, as in Aurora; elsewhere,
   and to close an open menu, it is the menus'. Alt opens them anywhere.
   Chapter 02 says so. (With a text field holding the keyboard and
   the pointer over a view, F10 does neither: the view takes no keys
   while one is typed in.)
2. **A change of language alone was applied as a look.** OK took the
   path that reloads only for another game folder, user folder or
   Workshop switch, so text fields kept the other language's text and
   the game's data was not read again where the install has the
   language. A test (`another_language_alone_is_applied_as_one`,
   `crates/mg-ui/src/options.rs`) failed on it. Fixed: a changed
   language goes the way the folders do, as chapter 10 says under
   Language; its opening paragraph now names the language and the
   Workshop switch beside the folders.
3. **"Build › Pack Target"** was in a hover text and two log messages
   while the menu says "Pack *file*": wording only, changed.

Smaller: `packaging/README.md` leaves `packaging/scoop/` out of its
folder table and calls the Flatpak ID provisional "until the project has
a public home", which it has; `CLAUDE.md` reads as if the 22 examples
were documentation tests.

## What was changed

Corrections of fact, each proved against the line given in the
reviewers' reports; nothing was reworded beyond them. Four go a little
past a changed word and were read again against the code before being
kept: PLAN §5.1 (`mg-schema/src/lib.rs:20-22`), a part's own texture in
`ee_tech_brief.md` §5.4 and `notes_models.md` §D2
(`mg-preview/src/creature.rs:386-399`; the full rule is in the
findings), and `notes_shaders.md` §M.1, struck as §M.2–4 already were.

- **Manual:** 02 (File, Edit, Wizards and Tools menus; Ctrl+H), 03
  (`dialog-export`, `dialog-import` for a `mg dialog` that does not
  exist), 04 (what is on Area Properties' Basic, Visual and Advanced),
  05 (stores; the Creature Wizard's ResRef and Tag; the Visuals page),
  10 (Pack *file*), 11 (`mg compile`'s JSON counts), 12 (dragging
  between inventories is done), 14 (the theme is an option, not the
  system's), 15 (API 0.3), 16 (grants are kept between sessions; `mg
  plugin run --event`), 17 (text is in the module's codepage).
- **Plugins:** README and the type file's header say 0.3; CHANGES notes
  `mg plugin run --event`.
- **The repository's `README.md`:** the plugin API is at 0.3.
- **PLAN:** 17 chapters; wrappers generated from the observed schema,
  not by a derive macro.
- **Parity checklists:** six rows (the Keyboard page, the Test window's
  conditions, Update Instances' Aurora test, drag and drop, renaming an
  area, Minimize Toolset).
- **Research:** `ee_tech_brief.md` (six: the attenuation uniforms as
  measured, tile main lights forced to 10/5, the talk table's order,
  the Eraser, where a compiled model keeps its material, a part's own
  texture), `notes_models.md` (six), `notes_shaders.md` (two),
  `notes_tilesets.md` (one).

- **Code** (the two defects above): `crates/mg-ui/src/commands.rs`,
  `area_view.rs`, `lib.rs`, `options.rs`, with a test each
  (`tests/app.rs`, `src/options.rs`). `cargo test -p mg-ui --lib
  --tests` passes (93, 264 and 1), and `cargo clippy -p mg-ui
  --all-targets` is clean; the tests run by hand and the rest of the
  workspace were not run.

## Decided and done (2026-10-12)

The owner ruled on the decisions below the same day, and they were
carried out. The verdict tables above are the audit as it found the
documents; this is what they are now.

| Decision | Ruling | Done |
| --- | --- | --- |
| PLAN.md | Collapse and retitle | "Architecture and Record of the Phases", a Reference: one table of the phases in §6's place, the 14 stale lines of §4, §5.5 and §7 put right against the code, §9 a paragraph. Sections keep their numbers. 48 KB to 32 KB |
| deferred.md | Tidy and refile by area | Filed by area alone (34 sections; the 14 by-report sections dissolved, each entry's source kept as its last words), 271 entries to 253: 10 done ones removed, 9 doubles merged, the stale ones corrected, the misfiled moved (252); then three of this audit's entries closed by the rulings here and four added (the inventory's hand-made parts, the plan's "held by" lists, three Aurora faults never checked, what is left of the palette's header). Each entry's text kept. What went and why is listed in the log |
| plugin-proposal.md, test-review.md | Trim to a decision record | The proposal keeps its summary, principles, safety, the runtimes compared, decisions and what was not proposed, with what shipped set against what was proposed (29 KB to 16 KB); the review keeps its measurements, defects and decisions, in the past tense |
| prior_art.md, community_pain_points.md | Mark historical | `status: deprecated`, with a note under the title saying when each was written and what was decided since; bodies as they were |
| lights-proposal.md | Drop the draft, cut way A to what was built | Done; ways B and C and two unmeasured questions remain the proposal |
| ee_tech_brief.md | Cut §4–§6 to pointers | Three short sections under the same numbers, pointing into the four notes and the findings; §0 brought in line. Nothing was found only in the brief (for §5, by a search of the models note, not a reading). 86 KB to 53 KB |
| wiki-proposal.md | The owner submits the corrections later | Unchanged |
| The generated references | Commit the recipe | `tools/aurora/uiinv/inputs.py` makes the inventory's four inputs from the installed game (kept outside the repository), `gen.py` writes the document: run twice from nothing, the body came out as committed, line for line. Twelve table offsets and the mappings stay hand-made for build 26c6e573, and the generator warns on another. The schema's generator names the game build; run again, its body was unchanged |
| notes_frames.md's `verified` | Drop it | Dropped; the note says its measurements are of 7–8 October and what changed since |

Still open after this, in the deferred list's Documentation section:
the open-question lists in four research notes, `notes_doctor.md`'s
check table, the parity checklists' rows, four small things in the
manual, and the wiki corrections.

## The decisions as they were put

1. **PLAN.md:** collapse its three histories and retitle it, or leave
   it as the record it is.
2. **deferred.md:** tidy the 37 entries named above; refile by area.
3. **plugin-proposal.md, test-review.md:** trim to a decision record or
   mark `status: deprecated` with pointers (the manual and CHANGES; PLAN
   §7). Both are linked from other documents, so neither should go.
4. **prior_art.md, community_pain_points.md:** mark historical, with a
   line at the top saying what was decided since.
5. **lights-proposal.md:** drop `draft`; cut way A's design to what was
   built, or move way B to the deferred list and deprecate the rest.
6. **ee_tech_brief.md:** cut §4–§6 to pointers at the four notes, or
   bring them up to the notes.
7. **wiki-proposal.md:** submit the corrections, which is what would
   let it go.
8. **The two generated references:** say in the inventory's trailer
   that it is not reproducible, or commit the recipe for its inputs;
   name the game build in the schema.
9. **notes_frames.md's `verified` entry:** rerun `frame_perf` and
   restamp, or drop it.
