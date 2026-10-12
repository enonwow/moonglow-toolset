---
type: Proposal
title: 'A 2DA editor and merger: a proposal'
description: Editing a 2DA's cells and rows inside Moonglow and taking rows from another hak's copy - what exists to build on, the decisions that are the owner's (where a table is edited, what a merger is, rows in the middle, cells by what they hold), four phases with their sizes and tests, what is not known yet, and what was built of it, with pictures.
tags: [proposal, 2da, custom-content, haks, editor]
generated: { by: claude-code/claude-opus-5-5, at: 2026-10-12T08:08:37Z }
stale_after: 2027-01-12T00:00:00Z
sources:
  - id: request
    resource: human:august (the owner, 2026-10-12)
    title: The ask - propose a plan for a 2DA editor
  - id: pain
    resource: research/community_pain_points.md
    title: 'Builders'' pain points: no 2DA or TLK editing, rows merged by hand; builders split on whether it belongs in a module toolset'
  - id: deferred
    resource: deferred.md
    title: 'The deferred list: "No 2DA editor or merger" (L), tables for plugins, diffs and merges from a terminal'
  - id: code
    resource: crates/mg-2da, crates/mg-module/src/table_layers.rs, crates/mg-module/src/doctor.rs, crates/mg-plugin/src/tables.rs, crates/mg-ui/src/browser.rs, crates/mg-ui/src/hak_view.rs (read 2026-10-12, at 1.21.3 and after)
    title: The code this would be built on
---

# A 2DA editor and merger: a proposal

2DAs are read-only in Moonglow's window: the resource browser shows a
table with the hak each row comes from, and nothing changes a cell. The
research that set the first scope left the editor to Eos Toolset
("revisit when asked");[^pain] the deferred list has carried it as its
largest open piece since.[^deferred] This is the asking.[^request]

The owner took the recommendations as they stand (2026-10-12), and the
editor was built by them: see [What was built](#what-was-built). The
rest of this page is the proposal as it was made, for the reasons.

## What the editor is for

Hand editing. A project that makes its tables from data already has
`mg.twoda` for plugins and `mg 2da-diff` with a three-way merge from a
terminal;[^deferred] those are not what is missing. What is missing is
the builder's own day: add a row to `placeables.2da` for a new model,
change a spell's column, take the forty rows a hak adds to
`appearance.2da` into the table of the hak above it, and see at once
that the row a feat names is there.

## What there is to build on

| Piece | Where | What it gives the editor |
| --- | --- | --- |
| The table as a value | `mg_2da::TwoDa`: `parse`, `new`, `set_columns`, `to_bytes` | reading, and writing laid out as the game's own tables are |
| Changing a table | the plugin host's `set`, `add_row`, `add_column` (`mg-plugin/src/tables.rs`) | the three operations, to be moved down into `mg-2da` so both use them |
| Every copy in the load order | `mg_module::table_layers::Layered`: each cell's origin | the "From" column and the changed-cell colors the browser draws today; the merger's source |
| Comparing and merging | `mg_2da::diff`, `mg_2da::merge` with `Prefer` | what a paste of rows conflicts with; the three-way merge already used by `mg 2da-diff` |
| What a column holds | `doctor::column_kinds`: a talk-table string, a model, a row of another table (61 columns of 11 tables, and the lists a class, a race or a package names) | a cell's own editor, and a fault shown as it is typed |
| The checks | `2da-row`, `2da-read`, `2da-limit`, `2da-strref`, `2da-ref`, `2da-shadow` | the same findings, on the table in the editor, before it is saved |
| Where a table is saved | the hak editor (a hak open with its own undo and Save), `Edit::SetResource` (the module's own resources, one undoable step) | two save paths that exist |
| The sibling | the talk table editor: its own undo, Find, Go to, CSV and JSON in and out, and no inserting in the middle | the pattern for a table edited outside the module's undo |

The plugin host's three operations work on a `TwoDa` today, so the
editor's model is mostly there; the work is the window, the save paths
and the merger.

## The decisions

Each is the owner's. A recommendation is given with its reason.

### 1. Where a table is edited

| Place | For | Against |
| --- | --- | --- |
| A hak open in the hak editor | where custom tables live; the hak editor has undo and Save, and already refuses to half-write a file | a table is edited inside another editor's document |
| The module's own resources | one undoable step like any resource | few modules keep 2DAs in the module |
| The `override` and `development` folders | where a builder tries a change | loose files, with no undo beyond the editor's own |
| A file anywhere | a repository's `2da/` folder, a nasher project's sources | the same |

**Recommended:** a hak open in the hak editor, the module, and a file
anywhere (which covers `override`, `development` and a repository), in
that order of building. The game's own tables are never changed: Edit on
one makes a copy in a place the builder chooses, as the talk table
editor's Move to Talk Table does for strings.

### 2. What a merger is

`Layered` already knows which layer each cell comes from. A general
three-way merge exists from a terminal. In the window, the builder's
question is narrower: "this hak hides rows the one below it added".

**Recommended:** the merger is *taking rows from another copy*. With a
table that has several copies open, the rows a lower copy has and this
one lacks (or has differently) are listed; chosen ones are taken in as
one step, each conflict (a row both changed) shown side by side with
the lower copy's cells to choose from. No base is asked for: the lowest
copy, usually the game's, is the base. The terminal's three-way merge
stays for repositories.

### 3. Rows in the middle

The talk table editor refuses to insert or remove a line in the middle,
because every later line's number changes and something would have to
follow each one. For 2DAs that has changed since: `2da-ref`'s tables say
which columns name a row of which table.

| Way | What it does | Cost |
| --- | --- | --- |
| Refuse | rows are added at the end, a row is blanked (`****`) and never removed | none; what Aurora-era tools do, and what the game's own tables do (padding rows) |
| Renumber what is known | inserting or removing a row also changes every cell of a known column that names a later row, in every table of the same hak | misses what the tables do not know: scripts, blueprints' fields, columns of custom tables; a false sense of safety |

**Recommended:** refuse, and blank. Saved characters, areas and scripts
name rows by number, and no table of columns reaches them. Find
References on a row (below) shows who names it, which is the safe half
of renumbering.

### 4. Cells by what they hold

**Recommended:** where `column_kinds` knows a column, the cell is
edited as what it is: a talk-table string shows its text beside the
number and offers the talk table (and New Line, in a custom table); a
model offers the models there are; a row of another table offers that
table's rows by label. Everything else is text, with `****` for empty.
A fault (`2da-ref`, `2da-strref`, more cells than columns) marks the
cell as it is typed.

### 5. The module's undo

A hak's table is the hak editor's to undo; a file's is the editor's
own, as the talk table's is. Only a table in the module goes through
the module's Undo. **Recommended as stated**: it is how the two
siblings already work.

## Phases

Sizes as elsewhere: S an hour or two, M a day or so, L several days.

| Phase | What | Size | Held by |
| --- | --- | --- | --- |
| 1 | `set`, `add_row`, `add_column`, and a row blanked, moved from the plugin host into `mg-2da` (the host calls them); a 2DA tab that edits cells and adds rows on a table in the module, with its checks shown; Find, Go to row, copy and paste of cells as text | M | unit tests in `mg-2da`; UI tests; an engine test that the server reads the saved table (`Get2DAString`) |
| 2 | The same tab on a table inside a hak open in the hak editor, and on a file anywhere; Edit on a game table copies it to a chosen place first | M | UI tests; the hak round trip in `hak_edit.rs` |
| 3 | Cells by what they hold (decision 4), and Find References on a row: the cells of known columns, and the module's blueprints and areas, that name it | M | UI tests; the row references against `2da-ref`'s corpus test |
| 4 | Take Rows from Another Copy (decision 2), with conflicts side by side | M–L | unit tests on `Layered` and `merge`; a UI test on two haks' copies; the bundled campaigns' haks as a corpus case |
| — | Renumbering (decision 3, the other way) | L | not proposed |

Phases 1 and 2 are an editor a builder can use; 3 is what makes it
better than a spreadsheet; 4 is the merger. Each phase is a release on
its own.

## What is not known

- **How builders keep their tables.** In a hak, loose in a repository,
  or both: decision 1's order rests on it. Asking two or three of the
  builders who report would settle it.
- **Whether the game reads a table changed in `development` without a
  restart.** The manual says 2DAs are read at the server's start; a
  quick engine run would say whether an editor's "try it" is a restart
  every time.
- **Large tables.** `appearance.2da` in a persistent world's haks has
  tens of thousands of rows: the browser's view draws only the rows in
  sight, and an editor has to keep to that. Not measured with editing.
- **What Eos does that builders would miss.** Its editor was not looked
  at for this proposal.

## What was built

All four phases, by the recommendations, after 1.21.3 (the manual's
[2DA editor](manual/03-modules.md#the-2da-editor) says how it is used):

| Phase | Built | Held by |
| --- | --- | --- |
| 1 | `TwoDa::set`, `add_row`, `add_column`, `clear_row` (the plugin host calls them); `mg_module::table_edit::Editing` (every change a step); the editor's tab on a module's table: cells, rows added and emptied, columns added, Find, Go to, copy and paste | `mg-2da` and `table_edit` unit tests; `a_module_s_table_is_edited_cell_by_cell`; `engine_reads_a_table_the_editor_wrote` (the server reads the written table cell for cell) |
| 2 | A hak's table (Edit Table in the hak editor, saved as a step of the hak's), a file anywhere (Tools › 2DA File…), and Edit… in the resource browser: the module's table, or a copy of the one the game reads in the module, a hak or a file | `a_hak_s_table_and_a_file_s_are_saved_where_they_live`, `a_game_table_is_edited_as_a_copy` |
| 3 | Cells checked as typed (`doctor::table_faults`: a row of another table, a talk-table string), a talk-table string shown as its text, a row of another table chosen by name, Find References to Row | `a_table_s_cells_are_checked_and_rows_taken_from_another_copy`; `the_game_s_own_tables_have_one_cell_fault` (every game table is sound by the same check, but for one row of the game's own) |
| 4 | Take Rows from Another Copy: new rows, rows that differ, and against a lower copy the rows only the other changed apart from those both did | `rows_are_offered_and_taken_from_another_copy`; the same UI test |

![The editor on a copy of feat.2da: a cell that names a feat that is not there](images/2da-table-editor.png)

![Typing in a cell](images/2da-table-editor-typing.png)

![Take Rows from Another Copy](images/2da-table-editor-take-rows.png)

![Find References to a row](images/2da-table-editor-references.png)

![Edit… on a table in the resource browser](images/2da-table-editor-edit-menu.png)

Not as proposed, or not done (each is in the [deferred list](deferred.md)):

- A model's cell has no list of models to choose from, and is not
  checked by the row: the game's own tables name models it never
  shipped, and an appearance's is a letter for a body of parts.
- A talk-table string's cell offers the talk table, not a new line in it.
- Find References looks in the tables' known columns, not in blueprints,
  areas or scripts.
- A row both copies changed is taken whole or not at all: its cells are
  shown side by side, not chosen one by one.
- None of the unknowns below was settled first: the order of the save
  paths is as recommended, not as builders said.

## What it would not be

Not a spreadsheet (no formulas, no sorting that changes row numbers);
not an editor of the game's own tables in place; not a replacement for
the generators (`mg.twoda`, a repository's scripts) that write whole
tables.

[^request]: The owner's ask, 2026-10-12.
[^pain]: `research/community_pain_points.md`: the pain point's row, and the tier-3 entry "Custom content data".
[^deferred]: `deferred.md`, "Custom content data (2DAs and talk tables)".
