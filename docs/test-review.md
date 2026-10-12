---
type: Report
title: 'Review: test necessity, test speed and codebase cleanliness'
description: Where the time between a finished change and a release goes (test binaries linked, sweeps repeated, CI runs per release, a 917 GB target directory), what was changed on the branch worktree-test-review with its measurements, and what is left for a decision.
tags: [tests, build, ci, release, review, cleanliness]
status: stable
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-12T02:10:00Z }
stale_after: 2027-01-01T00:00:00Z
sources:
  - id: logs
    resource: target/ws-test.txt and target/corpus-test.txt of the main checkout, 2026-10-11 (cargo test --workspace; cargo test -p mg-corpus-tests --release), version 1.21.1
    title: The last recorded test runs before the change
  - id: runs
    resource: Test runs in the worktree on 2026-10-11, release profile, while the main checkout ran its own workspace tests (16 cores shared)
    title: Runs of the regrouped tests
  - id: actions
    resource: https://github.com/jadzziaa/moonglow-toolset/actions (runs 38084618594 to 38191903814)
    title: GitHub Actions runs of releases 1.21.0 to 1.21.2
  - id: reading
    resource: crates/*/tests, apps/mg/tests, crates/*/src, Cargo manifests at commit eca41c9
    title: The test files and manifests, read (three read-only audits)
---

# Review: test necessity, test speed and codebase cleanliness

Asked for on 2026-10-11: the way from a finished change to a release
takes longer than the project's complexity explains. In short: **hardly
any test is unnecessary; the time goes into how the tests are built and
run, and into running the same things more than once.** The tests stand.
What changed is on the branch `worktree-test-review`; what did not is
listed at the end and in the [deferred list](deferred.md).

## What a release cost

| Step | Time | Note |
| --- | --- | --- |
| `cargo test --workspace` | 655 s of tests + build | includes every corpus test, unoptimized[^logs] |
| `cargo test -p mg-corpus-tests --release` | 471 s of tests + build | the same corpus tests again[^logs] |
| CI on `tmp/v…`, three systems | 7 to 9 min | [^actions] |
| CI on `develop`, the same commit | 8 to 10 min | [^actions] |
| Release workflow | 13 to 15 min | macOS universal build is the longest[^actions] |
| CI on `develop` for the Scoop manifest commit | 9 to 10 min | a JSON file nothing built reads[^actions] |

Four findings explain most of it:

1. **170 test binaries, run one at a time.** `mg-corpus-tests` had 80
   test files for 129 tests, `apps/mg` 23 for 34, `mg-ui` 7 (5 of them
   holding only ignored tests), plus 22 examples that `cargo test` links
   too. Each is a link of the whole stack, and Cargo runs binaries one
   after another, so 16 cores ran mostly one test.[^logs][^reading]
2. **The corpus suite ran twice per release**, once unoptimized inside
   `cargo test --workspace` (the game is installed here, so nothing
   skips) and once in release. `docs/PLAN.md` still says these "all run
   in a few seconds".
3. **Full debug information locally.** CI already built with line
   tables only; local builds did not: `app` (the UI tests) was 637 MB a
   copy, and `target/` is **917 GB** (524 GB `debug/deps`, 326 GB
   `debug/incremental`, 40 GB examples; some 370 stale copies of every
   crate) on a disk that is 90% full.
4. **Three CI runs and a release build per release**, two of the CI
   runs on a commit already tested or on a file nothing reads.

## Are the tests necessary?

Yes, nearly all. The three audits looked for tests asserting the same
thing on the same data at the same tier and found none inside
`mg-ui/tests/app.rs` (275 tests) and none in `apps/mg/tests`; what
overlaps across tiers (a unit test, a command-line test and a UI test
on the same fixture) tests different wiring each time.[^reading] What
they did find:

| Finding | Where | Verdict |
| --- | --- | --- |
| A test that asserts nothing (prints a tally) | `mdl_lint.rs` `game_ascii_census` | make it assert, or ignore it as the two "exploration" tests in `models.rs` are |
| Every model of the game is read nine times, decompiled three times | `mdl_compile.rs` (3 sweeps), `mdl_decompile.rs` (2), `mdl_lint.rs` (2), `models.rs` (2) | one sweep could feed several checks |
| Every script compiled three times, two of them identically | `script_analysis.rs` and `script_frontend.rs` (same settings, both read only the NDB); `scripts.rs` differs | share the compile: about 25 s |
| Every area of every module walked four times | `areas.rs`, `object_walkmeshes.rs`, `minimaps.rs`, `tiles.rs` | candidates to share the walk |
| The same "original module + probe" server run started in four tests | `engine_modules.rs` (twice), `engine_rename.rs`, `engine_locked.rs`: 7 servers per `.mod` | a shared baseline would halve them |
| nasher unpacks each module two or three times | `nasher.rs` | share the unpack |
| `audio.rs` loads all 12,000 sounds twice | both tests | share or sample |

None of these is wrong; they are the same expensive loop paid for more
than once. They are in the deferred list, not changed here: each is a
rewrite of test logic that deserves its own look.

## What was changed (branch `worktree-test-review`)

| Change | Effect |
| --- | --- |
| `mg-corpus-tests`: 80 test binaries become 6 (`areas`, `aurora`, `engine`, `formats`, `models`, `scripts`); the files stay where they are and are modules of `tests/groups/<name>.rs` | 74 fewer links per profile; tests of different files run side by side |
| `apps/mg`: 23 test binaries become 1 (`cli`) | 22 fewer links; 35 tests in 14.6 s[^runs] |
| Hand-run targets are `test = false`: `screens`, the four `*_perf` (mg-ui), `client_render`, `client_minimap` | 7 fewer links per run; `--test screens` and the documented commands work as before; clippy still checks them |
| `mg_testkit::engine::run_server` runs at most four servers at once per binary; `one_at_a_time()` for the three command-line tests that wait on `mg serve` by the clock | what made merging the engine tests safe |
| `mg_testkit::ungrouped_tests` and a test in each grouped crate | a new test file left out of its group fails a test instead of silently not running |
| `[profile.dev] debug = "line-tables-only"` in `Cargo.toml` (CI's two variables removed) | `app` 637 MB → 185 MB; backtraces keep file and line |
| CI: no run for `bucket/**`; tests without the examples (`--lib --bins --tests`, then `--doc`) | one CI run fewer per release; 22 fewer links on each system |
| `CLAUDE.md`, `README.md`: the two test commands no longer overlap | the corpus suite runs once per release, in release |

Measured, release profile, on a machine that was also running the main
checkout's test suite:[^runs]

| Group | Before (sum of its binaries)[^logs] | After |
| --- | --- | --- |
| formats (19 files) | 145.6 s | 55.9 s |
| engine (26 files) | 88.0 s | 51.4 s |
| scripts (4 files) | 99.9 s | 78.9 s |
| models (6 files) | 35.8 s | 31.3 s |
| aurora (18 files) | 1.4 s | 0.5 s |
| areas (5 files) | 100.5 s | not run (see below) |

Together with not running the corpus suite a second time, a release's
local test time goes from about 1,125 s to roughly 400 s (some 70 s of
unit, UI and command-line tests, some 310 s of corpus tests with the
areas at their old time), before counting the links saved.

**Not verified:** the `areas` group was compiled, passes clippy and
lists its 13 tests, but was not run: it opens a GPU device, and the main
checkout was running its own tests at the time (a full GPU test run once
brought this machine down). Its GPU tests take turns as before
(`mg_testkit::gpu::hold`), so about 90 s is expected. CI has not run
these changes on Windows or macOS.

Things to know when merging:

- The main checkout has uncommitted changes to `apps/mg/tests/compile.rs`
  and `crates/mg-corpus-tests/tests/doctor.rs`; the branch touches
  neither. Thirty corpus files change one line (`mod common;` to
  `use crate::common;`), and three command-line tests gain one.
- `--test <file>` becomes `--test <group> <file>` for corpus and
  command-line tests (`--test engine engine_tlk`); test names gain the
  file's name as a prefix.
- A new test file in those two crates is added to its group file.

## Cleanliness

The code is clean by the usual measures: no TODO or FIXME in
first-party code, no commented-out blocks, no `todo!` or `dbg!`, no
unused dependency, no `allow(dead_code)` in any `src/` outside the
generated schema.[^reading] What stands out is size and shape:

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

Three things that are defects, not style:

- **Tests write to the developer's real Moonglow data folder**: the
  compile memory (`$XDG_DATA_HOME/moonglow/compiled`) from
  `apps/mg/tests/json.rs`, `nasher.rs` and `watch.rs`, and the script
  cache from `lsp.rs`. Only `compile.rs` redirects it.
- **Five UI tests render without the GPU guard** (`app.rs` around lines
  7883, 8219, 8233, 8278, 8307: `h.render()` in `blueprint_harness`
  tests), which opens a device outside the one-at-a-time rule.
- **`screens.rs`: 22 tests share the scratch directory `screens`**, and
  each empties it first, so a full run keeps only the last test's
  pictures; two `app.rs` tests share `ui-bp-bandit_named` and can run at
  the same time.

## For a decision

Ordered by what they would save:

1. **Empty `target/`** in the main checkout (`cargo clean`, or
   `cargo sweep`): 917 GB. Not done from here: another session builds
   there. With line tables only it regrows at about a third of the rate.
2. **One CI run per release instead of two**: the release commit is
   tested on `tmp/v…` and again, identically, on `develop`. Either push
   to `develop` directly and wait for that run, or skip CI on `tmp/**`
   when nothing in the renderer changed.
3. **Sweeps as their own tier**: the whole-install sweeps (every area
   rendered, every sound decoded, every script compiled twice) are what
   is left of the corpus time. The plan (§7) wants them whenever a game
   is found; they could run before a release only, with a sample for the
   everyday run. A decision about the testing strategy, so not taken
   here.
4. The shared sweeps in the table under "Are the tests necessary?".
5. Splitting `app.rs` and taking the GPU lock only where a test draws
   an area: best done by whoever is working on `develop`, since that
   file changes every day.

[^logs]: The recorded runs of 2026-10-11 in the main checkout's `target/`.
[^runs]: Runs in the worktree, 2026-10-11.
[^actions]: GitHub Actions, releases 1.21.0 to 1.21.2.
[^reading]: The test files and manifests as of commit eca41c9.
