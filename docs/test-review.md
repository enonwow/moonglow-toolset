---
type: Report
title: 'Review: test necessity, test speed and codebase cleanliness'
description: A dated record (2026-10-11, version 1.21.1) of where the time between a finished change and a release went, the decisions taken about it, the measurements before and after, and the state of the code by size and shape. How the tests run today is in the plan's §7 and CLAUDE.md.
tags: [tests, build, ci, release, review, cleanliness]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-12T05:30:26Z }
stale_after: 2027-01-01T00:00:00Z
sources:
  - id: logs
    resource: target/ws-test.txt and target/corpus-test.txt of the main checkout, 2026-10-11 (cargo test --workspace; cargo test -p mg-corpus-tests --release), version 1.21.1
    title: The last recorded test runs before the change
  - id: runs
    resource: Test runs in the worktree on 2026-10-11, release profile; the first round while the main checkout ran its own workspace tests (16 cores shared), the second on an idle machine
    title: Runs of the regrouped and reworked tests
  - id: decision
    resource: human:august, 2026-10-11, in conversation - clear target/, one CI run per release, sample the whole-install sweeps (and say so in the plan), share the repeated sweeps, correct the three defects, then merge
    title: The decisions taken on the review's open points
  - id: actions
    resource: https://github.com/jadzziaa/moonglow-toolset/actions (runs 38084618594 to 38191903814)
    title: GitHub Actions runs of releases 1.21.0 to 1.21.2
  - id: reading
    resource: crates/*/tests, apps/mg/tests, crates/*/src, Cargo manifests at commit eca41c9
    title: The test files and manifests, read (three read-only audits)
---

# Review: test necessity, test speed and codebase cleanliness

Asked for on 2026-10-11: the way from a finished change to a release
took longer than the project's complexity explained. In short: **hardly
any test was unnecessary; the time went into how the tests were built and
run, and into running the same things more than once.** The tests stood.
The review's open points were decided the same day[^decision] and done.

This is a record of that day: every figure is of 2026-10-11 (version
1.21.1, commit eca41c9) and none was measured again. How the tests are
grouped, sampled and run now is in [the plan](PLAN.md) §7 ("Sweeps and
samples") and in `CLAUDE.md` (Commands); what the review left open is in
the [deferred list](deferred.md) ("Tests and the build").

## What a release cost

| Step | Time | Note |
| --- | --- | --- |
| `cargo test --workspace` | 655 s of tests + build | included every corpus test, unoptimized[^logs] |
| `cargo test -p mg-corpus-tests --release` | 471 s of tests + build | the same corpus tests again[^logs] |
| CI on `tmp/v…`, three systems | 7 to 9 min | [^actions] |
| CI on `develop`, the same commit | 8 to 10 min | [^actions] |
| Release workflow | 13 to 15 min | macOS universal build is the longest[^actions] |
| CI on `develop` for the Scoop manifest commit | 9 to 10 min | a JSON file nothing built reads[^actions] |

Four findings explained most of it:

1. **170 test binaries, run one at a time.** `mg-corpus-tests` had 80
   test files for 129 tests, `apps/mg` 23 for 34, `mg-ui` 7 (5 of them
   holding only ignored tests), plus 22 examples that `cargo test` links
   too. Each was a link of the whole stack, and Cargo runs binaries one
   after another, so 16 cores ran mostly one test.[^logs][^reading]
2. **The corpus suite ran twice per release**, once unoptimized inside
   `cargo test --workspace` (the game is installed here, so nothing
   skipped) and once in release. The plan then said these "all run in a
   few seconds"; it was corrected with this review.
3. **Full debug information locally.** CI already built with line
   tables only; local builds did not: `app` (the UI tests) was 637 MB a
   copy, and `target/` was **917 GB** (524 GB `debug/deps`, 326 GB
   `debug/incremental`, 40 GB examples; some 370 stale copies of every
   crate) on a disk that was 90% full.
4. **Three CI runs and a release build per release**, two of the CI
   runs on a commit already tested or on a file nothing reads.

## Were the tests necessary?

Yes, nearly all. The three audits looked for tests asserting the same
thing on the same data at the same tier and found none inside
`mg-ui/tests/app.rs` (275 tests) and none in `apps/mg/tests`; what
overlaps across tiers (a unit test, a command-line test and a UI test
on the same fixture) tests different wiring each time.[^reading] What
they did find was the same expensive loop paid for more than once:

| Finding | Where |
| --- | --- |
| A test that asserted nothing (it printed a tally) | `mdl_lint.rs` `game_ascii_census` |
| Every model of the game read nine times, decompiled three times | `mdl_compile.rs` (3 sweeps), `mdl_decompile.rs` (2), `mdl_lint.rs` (2), `models.rs` (2) |
| Every script compiled three times, two of them identically | `script_analysis.rs` and `script_frontend.rs`; `scripts.rs` differs |
| Every area of every module walked four times | `areas.rs`, `object_walkmeshes.rs`, `minimaps.rs`, `tiles.rs` |
| The same "original module + probe" server run started in four tests | `engine_modules.rs` (twice), `engine_rename.rs`, `engine_locked.rs`: 7 servers per `.mod` |
| nasher unpacked each module two or three times | `nasher.rs` |
| `audio.rs` loaded all 12,000 sounds twice | both tests |

## What was decided, and what it gained

In two rounds: the structure first, then, once decided,[^decision] the
sweeps, the sample and the defects.

| Decision | Why | Measured effect |
| --- | --- | --- |
| Group the test files into few binaries: `mg-corpus-tests` 80 → 6, `apps/mg` 23 → 1; the hand-run targets (`screens`, the four `*_perf`, `client_render`, `client_minimap`) are `test = false` | each binary was a link of the whole stack, run one after another | 103 fewer links per profile; the 35 command-line tests in about 15 s[^runs] |
| At most eight servers at once per binary | what made merging the engine tests safe | |
| A test fails when a test file is left out of its group | a file left out would silently not run | |
| Line tables instead of full debug information for local builds | the size of `target/` | `app` 637 MB → 185 MB; backtraces keep file and line |
| CI: no run for `bucket/**`, no examples, and a commit that passed on one branch is not tested again on another | two of three runs per release tested nothing new | one CI run per release where there were three |
| The two test commands no longer overlap | the corpus suite ran twice | it runs once, in release |
| Sample the long sweeps on an ordinary run; everything with `MOONGLOW_FULL_CORPUS=1` before a release | an everyday run should take about a minute; a fault only the rest of the install shows is caught before a release | an everyday corpus run in 44 s |
| Share what was swept more than once: one decompile of each model for three tests, one compile of each script for two, the original module's server run once, nasher's unpack once, the sounds read one at a time | the table above | see below |
| Ignore `game_ascii_census` (run by hand) | it asserts nothing, and what it should assert is undecided | |
| Empty the main checkout's `target/` | 917 GB | 917 GB → 2.4 MB; the disk from 90% to 66% full |

Measured in release:[^logs][^runs]

| Group | Before (sum of its binaries) | Grouped | Shared, in full | Everyday sample |
| --- | --- | --- | --- | --- |
| areas (5 files) | 100.5 s | not run | 78.8 s | 6.2 s |
| aurora (18 files) | 1.4 s | 0.5 s | 0.3 s | 0.3 s |
| engine (26 files) | 88.0 s | 51.4 s | 22.2 s | 9.2 s |
| formats (19 files) | 145.6 s | 55.9 s | 51.0 s | 15.8 s |
| models (6 files) | 35.8 s | 31.3 s | 27.9 s | 6.0 s |
| scripts (4 files) | 99.9 s | 78.9 s | 54.1 s | 6.2 s |
| **all** | **471 s** | | **234 s** | **44 s** |

("Grouped" was measured while another test suite ran on the machine,
the two columns after it on an idle one.) Every group passed in full
and sampled; the UI tests (261) and the command-line tests (35) passed.

With about 70 s of unit, UI and command-line tests, the tests of an
ordinary change took about two minutes where they had taken nineteen
(1,125 s: the corpus suite twice), and those before a release about five.

CI ran the new workflow on `tmp/test-review` (run 38196706314) and
passed on Linux, Windows and macOS. That run started from an empty cache
(the profile change rebuilds every dependency) and took 14 to 24
minutes; a warm run was not measured.

Three defects were found on the way and corrected:

- **Tests and the developer's data folder.** Every command-line test
  starts `mg` through one function (`apps/mg/tests/groups/cli.rs`) that
  points Moonglow's data and cache folders into `target/test-output`. As
  measured, nothing was being written to the real folder at that commit;
  the audit had read the code paths, which are fenced now.
- **Renders outside the GPU guard.** The five `blueprint_harness` tests
  that render a picture hold the GPU before they do.
- **Shared scratch directories.** `screens.rs` adds its pictures to
  `target/test-output/screens` without emptying it, so a full run keeps
  them all; the second test on `ui-bp-bandit_named` has its own copy.

One thing was looked at and kept: `small_formats.rs` has the suite's one
wall-clock assertion (tilesets with absurd door counts parse within 20 s;
it guards against a loop of a billion turns).

## Cleanliness

The code was clean by the usual measures: no TODO or FIXME in
first-party code, no commented-out blocks, no `todo!` or `dbg!`, no
unused dependency, no `allow(dead_code)` in any `src/` outside the
generated schema.[^reading] What stood out was size and shape:

| Finding | Detail |
| --- | --- |
| `mg-ui` is 40% of the code in one crate (50,943 lines), at the top of the graph | nearly any change recompiles it and relinks its tests; `area_view.rs` (5,046 lines) and `blueprint/` are self-contained clusters |
| `crates/mg-ui/tests/app.rs`: 15,162 lines, 275 tests in chronological order | splits into about a dozen subject modules under one binary (no link cost); 47 copies of `GameInstall::new(&root, None, "en")`, 115 hand-built harnesses, 7 copies of the GPU set-up |
| About a dozen UI tests use `area_harness` only as "a module with game data" | they take the GPU lock and run one at a time for nothing |
| `mg` (the command line) links wgpu and never uses it | through `mg-preview` → `mg-render`; only `mg-preview/src/compose.rs` needs the GPU: a feature or a split would take 96 crates out of `mg`'s build |
| `apps/mg/src/main.rs`: one `fn run` of about 1,490 lines | eight subcommand modules exist already; the rest of the match could follow |
| The `mg` runner is written ten times in `apps/mg/tests`, the module resource stack twelve times in the corpus tests, `fn lines` six times | fixtures, not logic; a `common` module in each group would hold them |
| Every release rebuilds everything | the version is `version.workspace` in all 28 crates, read in five places; a version in the two apps alone would leave the libraries' artifacts valid across a version bump |
| `[profile.test] opt-level = 1` while dev is 0 | `cargo build` and `cargo test` share no workspace artifacts |
| Generated schema: 609 `#[allow(unused_imports)]` | the generator emits three imports into every nested module: about 1,800 lines |

None of these was changed by the review. Those that are still open are in
the [deferred list](deferred.md).

[^logs]: The recorded runs of 2026-10-11 in the main checkout's `target/`.
[^runs]: Runs in the worktree, 2026-10-11.
[^actions]: GitHub Actions, releases 1.21.0 to 1.21.2.
[^reading]: The test files and manifests as of commit eca41c9.
[^decision]: The user's decisions on the review's open points, 2026-10-11.
