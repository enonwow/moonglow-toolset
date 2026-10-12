//! The 2DA editor: a table's cells changed in place, rows added at the end
//! and emptied (never taken out: a row's number is what everything else
//! knows it by), cells checked by what their column holds as they are
//! typed, and rows taken from another copy of the table (a lower hak's,
//! which this one hides).
//!
//! A table is edited where it lives: in the module, in a hak open in the
//! hak editor (whose Save writes it), or in a file anywhere. The game's
//! own tables are never changed: Edit on one makes a copy somewhere.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use egui::Ui;
use mg_2da::TwoDa;
use mg_core::{ResRef, ResType};
use mg_edit::{Command, Edit};
use mg_module::doctor::{CellFault, ColumnKind, TalkTables};
use mg_module::table_edit::{Candidate, Editing, Offered};
use mg_resman::ResKey;

use crate::palette_view::{Keys, give_keys, keys_are};
use crate::{Action, Moonglow, Tab};

/// Where a table being edited lives, and is saved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Home {
    /// The module's own resource: saved as a step of the module's.
    Module,
    /// A hak open in the hak editor (its id): saved into it, and written
    /// with the hak's own Save.
    Hak(u32),
    File(PathBuf),
}

/// Rows of another copy being looked through, to take.
#[derive(Debug, Clone, Default)]
pub struct Taking {
    /// Which of the other copies (by its layer's name).
    pub from: Option<String>,
    pub chosen: BTreeSet<usize>,
    /// The copies in the load order, read once as the window opens.
    copies: Option<Arc<Vec<(String, TwoDa)>>>,
    /// The rows offered, worked out for a copy and the table as it was
    /// (its revision); and whether whose change each is could be told.
    found: Option<(String, u64, Arc<Vec<Candidate>>, bool)>,
}

/// A table open in a tab.
#[derive(Debug)]
pub struct TableDoc {
    pub id: u32,
    pub key: ResKey,
    pub home: Home,
    pub edit: Editing,
    /// The cell the keys are at: its row and column.
    pub selected: Option<(usize, usize)>,
    /// What is being typed into the selected cell.
    pub typing: Option<String>,
    pub filter: String,
    go_to: String,
    /// The selected cell is to be brought into view.
    reveal: bool,
    /// How far the rows are scrolled: the column names and the rows'
    /// numbers follow.
    scroll: egui::Vec2,
    /// Talk-table columns show their text, not their numbers.
    pub strref_text: bool,
    /// Bumped at every change: what is worked out from the table is kept
    /// until then.
    revision: u64,
    faults: Option<(u64, Arc<Vec<CellFault>>)>,
    /// The name of a column being added, as typed.
    pub new_column: Option<String>,
    pub taking: Option<Taking>,
    /// Where a row is named, as last asked: the row, and each place.
    pub references: Option<(usize, Vec<Reference>)>,
    /// What went wrong last (a cell refused, a save that failed).
    pub problem: Option<String>,
}

/// A cell that names a row: the table, the row and the column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub table: String,
    pub row: usize,
    pub column: String,
}

impl TableDoc {
    fn new(id: u32, key: ResKey, home: Home, edit: Editing) -> TableDoc {
        TableDoc {
            id,
            key,
            home,
            edit,
            selected: None,
            typing: None,
            filter: String::new(),
            go_to: String::new(),
            reveal: false,
            scroll: egui::Vec2::ZERO,
            strref_text: true,
            revision: 0,
            faults: None,
            new_column: None,
            taking: None,
            references: None,
            problem: None,
        }
    }

    pub fn name(&self) -> String {
        self.key.resref.to_string()
    }

    pub fn title(&self) -> String {
        format!("{}{}", self.key, if self.edit.is_dirty() { " *" } else { "" })
    }

    /// Goes to a cell: selected, and brought into view.
    pub fn go_to(&mut self, row: usize, column: usize) {
        self.selected = Some((row, column));
        self.reveal = true;
    }

    fn changed(&mut self) {
        self.revision += 1;
        self.references = None;
    }

    /// Sets the selected cell from what was typed.
    fn commit(&mut self) {
        let (Some((row, column)), Some(text)) = (self.selected, self.typing.take()) else { return };
        let value = Some(text.trim().to_string()).filter(|v| !v.is_empty());
        match self.edit.set(row, column, value) {
            Ok(true) => self.changed(),
            Ok(false) => {}
            Err(e) => self.problem = Some(e.to_string()),
        }
    }
}

/// Where a home is, in words.
fn home_text(app: &Moonglow, home: &Home) -> String {
    match home {
        Home::Module => "in the module".to_string(),
        Home::Hak(id) => match app.haks.iter().find(|d| d.id == *id) {
            Some(d) => format!("in the hak {}", d.title().trim_end_matches(" *")),
            None => "in a hak that is closed: Save As File… keeps it".to_string(),
        },
        Home::File(path) => path.display().to_string(),
    }
}

fn codepage(app: &Moonglow) -> mg_core::Codepage {
    app.game.as_deref().map_or_else(mg_core::Codepage::default, |g| g.codepage())
}

fn open_doc(app: &mut Moonglow, key: ResKey, home: Home, edit: Editing) -> u32 {
    if let Some(d) = app.tables.iter().find(|d| d.key == key && d.home == home) {
        let id = d.id;
        app.actions.push(Action::OpenTab(Tab::Table(id)));
        return id;
    }
    app.next_table += 1;
    let id = app.next_table;
    app.tables.push(TableDoc::new(id, key, home, edit));
    app.actions.push(Action::OpenTab(Tab::Table(id)));
    id
}

fn parse(app: &Moonglow, key: ResKey, data: &[u8]) -> Result<TwoDa, String> {
    TwoDa::parse(data, codepage(app)).map_err(|e| format!("{key}: {e}"))
}

/// Opens the module's own table in the editor.
pub fn open_in_module(app: &mut Moonglow, key: ResKey) -> Option<u32> {
    let data = app.ws.as_ref()?.module.get(&key)?.to_vec();
    match parse(app, key, &data) {
        Ok(t) => Some(open_doc(app, key, Home::Module, Editing::new(t))),
        Err(e) => {
            app.log.error(e);
            None
        }
    }
}

/// Opens a table of a hak that is open in the hak editor.
pub fn open_in_hak(app: &mut Moonglow, hak: u32, key: ResKey) -> Option<u32> {
    let data = app.haks.iter().find(|d| d.id == hak)?.hak.data(key);
    match data.and_then(|d| parse(app, key, &d)) {
        Ok(t) => Some(open_doc(app, key, Home::Hak(hak), Editing::new(t))),
        Err(e) => {
            app.log.error(e);
            None
        }
    }
}

/// Opens a 2DA file anywhere.
pub fn open_path(app: &mut Moonglow, path: &std::path::Path) -> Option<u32> {
    let name = path.file_stem().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let read = || -> Result<(ResKey, TwoDa), String> {
        let resref = ResRef::from_str(&name).map_err(|e| format!("{}: {e}", path.display()))?;
        let key = ResKey::new(resref, ResType::TWODA);
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok((key, parse(app, key, &data)?))
    };
    match read() {
        Ok((key, t)) => Some(open_doc(app, key, Home::File(path.to_path_buf()), Editing::new(t))),
        Err(e) => {
            app.log.error(e);
            None
        }
    }
}

/// Tools › Open 2DA File…
pub fn open_file(app: &mut Moonglow) {
    if let Some(path) = app.dialogs.open_file(crate::dialogs::FileKind::Any, None) {
        open_path(app, &path);
    }
}

/// A copy of the table the game reads (the highest in the load order),
/// to be edited in `home`: not saved there until Save.
pub fn edit_copy(app: &mut Moonglow, key: ResKey, home: Home) -> Option<u32> {
    let data = app.game.as_ref()?.resman.get(&key).ok()?.into_owned();
    match parse(app, key, &data) {
        Ok(t) => {
            let mut edit = Editing::new(t);
            edit.mark_unsaved();
            Some(open_doc(app, key, home, edit))
        }
        Err(e) => {
            app.log.error(e);
            None
        }
    }
}

/// Edit a Copy In › a file: asks where.
pub fn edit_copy_in_file(app: &mut Moonglow, key: ResKey) {
    let suggested = PathBuf::from(key.to_string());
    if let Some(path) = app.dialogs.save_file(crate::dialogs::FileKind::Any, Some(&suggested)) {
        let path = if path.extension().is_some() { path } else { path.with_extension("2da") };
        edit_copy(app, key, Home::File(path));
    }
}

/// Saves a table where it lives. Whether it was saved.
pub fn save(app: &mut Moonglow, id: u32) -> bool {
    let cp = codepage(app);
    let Some(at) = app.tables.iter().position(|d| d.id == id) else { return false };
    app.tables[at].commit();
    let (key, home) = (app.tables[at].key, app.tables[at].home.clone());
    let bytes = match app.tables[at].edit.table().to_bytes(cp) {
        Ok(b) => b,
        Err(e) => {
            app.tables[at].problem = Some(format!("Not saved: {e}"));
            return false;
        }
    };
    let saved = match &home {
        Home::Module => {
            let edit = Edit::SetResource { key, data: Some(bytes) };
            app.apply(Command::new(format!("Edit {key}"), vec![edit])).map_err(|e| e.to_string())
        }
        Home::Hak(hak) => match app.haks.iter_mut().find(|d| d.id == *hak) {
            Some(doc) => {
                let source = mg_module::hak_edit::Source::Bytes(bytes.into());
                let done = doc.hak.add_sources_as(&format!("Edit {key}"), vec![(key, source)]);
                doc.changed();
                done.map(|_| ())
            }
            None => Err("its hak is closed: Save As File… keeps the table".to_string()),
        },
        Home::File(path) => {
            std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        }
    };
    match saved {
        Ok(()) => {
            let where_ = home_text(app, &home);
            let doc = &mut app.tables[at];
            doc.edit.mark_saved();
            doc.problem = None;
            let more = match home {
                Home::Hak(_) => " (the hak's own Save writes its file)",
                _ => "",
            };
            app.log.info(format!("Saved {key} {where_}{more}"));
            true
        }
        Err(e) => {
            app.tables[at].problem = Some(format!("Not saved: {e}"));
            false
        }
    }
}

/// Save As File…: the table written to a file, which is its home from
/// then on.
fn save_as_file(app: &mut Moonglow, id: u32) {
    let Some(doc) = app.tables.iter().find(|d| d.id == id) else { return };
    let suggested = PathBuf::from(doc.key.to_string());
    let Some(path) = app.dialogs.save_file(crate::dialogs::FileKind::Any, Some(&suggested)) else {
        return;
    };
    let path = if path.extension().is_some() { path } else { path.with_extension("2da") };
    if let Some(doc) = app.tables.iter_mut().find(|d| d.id == id) {
        doc.home = Home::File(path);
    }
    save(app, id);
}

/// What the table's columns hold, where the checks know.
fn kinds(app: &Moonglow, name: &str) -> Vec<(&'static str, ColumnKind)> {
    let read = |n: &str| app.game.as_deref().and_then(|g| g.table(n).ok());
    mg_module::doctor::column_kinds_as_named(name, &read)
}

/// How many strings the talk tables have, for the checks.
fn talk_tables(app: &Moonglow) -> TalkTables {
    let base = app.game.as_deref().map_or(0, |g| g.tlk().entries.len());
    let custom = app.game.as_deref().and_then(|g| g.custom_tlk().map(|t| t.entries.len()));
    TalkTables { base, custom }
}

/// The cells that are wrong, kept until the table changes.
fn faults(app: &mut Moonglow, at: usize) -> Arc<Vec<CellFault>> {
    let doc = &app.tables[at];
    if let Some((revision, found)) = &doc.faults
        && *revision == doc.revision
    {
        return found.clone();
    }
    let name = doc.name();
    let kinds = kinds(app, &name);
    let game = app.game.as_deref();
    let rows_of = |n: &str| game.and_then(|g| g.table(n).ok()).map(|t| t.len());
    let found = mg_module::doctor::table_faults(
        &name,
        doc.edit.table(),
        &kinds,
        &rows_of,
        talk_tables(app),
    );
    let found = Arc::new(found);
    let doc = &mut app.tables[at];
    doc.faults = Some((doc.revision, found.clone()));
    found
}

/// The other copies of the table in the load order: each layer's name and
/// its copy, highest first.
fn copies(app: &Moonglow, key: ResKey) -> Vec<(String, TwoDa)> {
    let Some(game) = app.game.as_deref() else { return Vec::new() };
    mg_module::table_layers::layered(&game.resman, &key, codepage(app))
        .map(|l| l.copies)
        .unwrap_or_default()
}

/// Where row `row` of the table `name` is named: the cells of the columns
/// known to name its rows, in the tables the game has (and in `own`, the
/// table as it is being edited, in place of the one of its name).
pub fn references(app: &Moonglow, name: &str, own: &TwoDa, row: usize) -> Vec<Reference> {
    use mg_module::doctor::{column_kinds, row_named};
    let Some(game) = app.game.as_deref() else { return Vec::new() };
    let mut out = Vec::new();
    let mut names: Vec<String> = game
        .resman
        .list(ResType::TWODA)
        .into_iter()
        .map(|r| r.to_string().to_lowercase())
        .collect();
    if !names.iter().any(|n| n == name) {
        names.push(name.to_string());
    }
    names.sort();
    for table in names {
        let kinds = column_kinds(&table);
        let wanted: Vec<&str> = kinds
            .iter()
            .filter(|(_, k)| matches!(k, ColumnKind::Row(t) if t.eq_ignore_ascii_case(name)))
            .map(|(c, _)| *c)
            .collect();
        if wanted.is_empty() {
            continue;
        }
        let read;
        let t: &TwoDa = if table == name {
            own
        } else {
            match game.table(&table) {
                Ok(t) => {
                    read = t;
                    &read
                }
                Err(_) => continue,
            }
        };
        for column in wanted {
            for r in 0..t.len() {
                let named = t.get_int(r, column).and_then(|v| row_named(&table, column, v));
                if named == Some(row) {
                    out.push(Reference { table: table.clone(), row: r, column: column.into() });
                }
            }
        }
    }
    out
}

const CELL: f32 = 116.0;
const NUMBER: f32 = 54.0;

/// The tab of a table.
pub(crate) fn ui(app: &mut Moonglow, ui: &mut Ui, id: u32) {
    let Some(at) = app.tables.iter().position(|d| d.id == id) else {
        ui.label("This table is closed.");
        return;
    };
    let faults = faults(app, at);
    let name = app.tables[at].name();
    let kinds = kinds(app, &name);
    let home = home_text(app, &app.tables[at].home);
    // (Read once: every copy of a large table is not parsed each frame.)
    if app.tables[at].taking.as_ref().is_some_and(|t| t.copies.is_none()) {
        let read = Arc::new(copies(app, app.tables[at].key));
        if let Some(t) = app.tables[at].taking.as_mut() {
            t.copies = Some(read);
        }
    }
    let others: Arc<Vec<(String, TwoDa)>> =
        app.tables[at].taking.as_ref().and_then(|t| t.copies.clone()).unwrap_or_default();
    let has_copies = app.game.as_deref().is_some_and(|g| g.resman.contains(&app.tables[at].key));
    let mut act = None;
    header(app, ui, at, &home, has_copies, faults.len(), &mut act);
    let pane = ui.available_rect_before_wrap();
    keys(app, ui, at, pane);
    let below = 96.0;
    let grid_height = (ui.available_height() - below).max(120.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), grid_height), |ui| {
        grid(app, ui, at, &kinds, &faults);
    });
    ui.separator();
    inspector(app, ui, at, &kinds, &faults);
    taking_window(app, ui.ctx(), at, &others);
    column_window(app, ui.ctx(), at);
    match act {
        Some(Act::Save) => _ = save(app, id),
        Some(Act::SaveAs) => save_as_file(app, id),
        None => {}
    }
}

enum Act {
    Save,
    SaveAs,
}

fn header(
    app: &mut Moonglow,
    ui: &mut Ui,
    at: usize,
    home: &str,
    has_copies: bool,
    wrong: usize,
    act: &mut Option<Act>,
) {
    let doc = &mut app.tables[at];
    ui.horizontal(|ui| {
        ui.strong(doc.key.to_string());
        if doc.edit.is_dirty() {
            ui.weak("(unsaved)");
        }
        let t = doc.edit.table();
        let about = format!("{} rows, {} columns; {home}", t.len(), t.columns().len());
        ui.add(egui::Label::new(egui::RichText::new(&about).weak()).truncate())
            .on_hover_text(about);
    });
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(doc.edit.is_dirty(), egui::Button::new("Save")).clicked() {
            *act = Some(Act::Save);
        }
        if ui
            .button("Save As File…")
            .on_hover_text("Write the table to a file, which it is edited in from then on")
            .clicked()
        {
            *act = Some(Act::SaveAs);
        }
        let undo = doc.edit.undo_label().map(str::to_string);
        let tip = undo.clone().unwrap_or_else(|| "Nothing to undo".into());
        if ui.add_enabled(undo.is_some(), egui::Button::new("Undo")).on_hover_text(tip).clicked() {
            doc.typing = None;
            doc.edit.undo();
            doc.changed();
        }
        let redo = doc.edit.redo_label().map(str::to_string);
        let tip = redo.clone().unwrap_or_else(|| "Nothing to redo".into());
        if ui.add_enabled(redo.is_some(), egui::Button::new("Redo")).on_hover_text(tip).clicked() {
            doc.typing = None;
            doc.edit.redo();
            doc.changed();
        }
        ui.separator();
        if ui
            .button("Add Row")
            .on_hover_text("An empty row at the end (a row's number is what names it: none is put between others)")
            .clicked()
        {
            doc.commit();
            let row = doc.edit.add_rows(1);
            doc.selected = Some((row, doc.selected.map_or(0, |s| s.1)));
            doc.reveal = true;
            doc.changed();
        }
        let row = doc.selected.map(|s| s.0);
        if ui
            .add_enabled(row.is_some(), egui::Button::new("Empty Row"))
            .on_hover_text(
                "Every cell of the selected row emptied (****). The row stays, so that the \
                 rows after it keep their numbers",
            )
            .clicked()
            && let Some(row) = row
        {
            doc.typing = None;
            doc.edit.clear_row(row);
            doc.changed();
        }
        if ui.button("Add Column…").clicked() {
            doc.new_column = Some(String::new());
        }
        ui.separator();
        if ui
            .add_enabled(has_copies, egui::Button::new("Take Rows…"))
            .on_hover_text(
                "Rows another copy of this table has (a hak below this one, the game's) and \
                 this one lacks or has differently",
            )
            .clicked()
        {
            doc.commit();
            doc.taking = Some(Taking::default());
        }
        ui.checkbox(&mut doc.strref_text, "Strings as text").on_hover_text(
            "Columns that hold a talk-table string's number show the string; the number is \
             what is edited",
        );
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Find");
        let field = ui.id().with(("table-filter", doc.id));
        crate::widgets::pane_filter_keys(ui, field, &mut doc.filter);
        ui.add(
            egui::TextEdit::singleline(&mut doc.filter)
                .id(field)
                .desired_width(170.0)
                .hint_text("text in any cell"),
        );
        ui.label("Go to");
        let go =
            ui.add(egui::TextEdit::singleline(&mut doc.go_to).desired_width(70.0).hint_text("row"));
        if go.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Ok(row) = doc.go_to.trim().parse::<usize>()
            && row < doc.edit.table().len()
        {
            doc.filter.clear();
            doc.selected = Some((row, doc.selected.map_or(0, |s| s.1)));
            doc.reveal = true;
            give_keys(ui.ctx(), Keys::Table);
        }
        if wrong > 0 {
            let text = egui::RichText::new(match wrong {
                1 => "1 cell is wrong".to_string(),
                n => format!("{n} cells are wrong"),
            })
            .color(ui.visuals().warn_fg_color);
            if ui
                .button(text)
                .on_hover_text(
                    "Goes to the next of them. They are what Verify Module would say of this table",
                )
                .clicked()
            {
                *act = None;
                doc.go_to = "!".into();
            }
        }
        if let Some(problem) = &doc.problem {
            ui.colored_label(ui.visuals().error_fg_color, problem);
        }
    });
}

/// The rows shown: all, or those with the filter's text in a cell.
fn shown_rows(doc: &TableDoc) -> Vec<usize> {
    let t = doc.edit.table();
    let needle = doc.filter.trim().to_lowercase();
    if needle.is_empty() {
        return (0..t.len()).collect();
    }
    (0..t.len())
        .filter(|&r| {
            (0..t.columns().len())
                .any(|c| t.cell(r, c).is_some_and(|v| v.to_lowercase().contains(&needle)))
        })
        .collect()
}

/// The keys in the grid, from a click on a cell: the arrows, Tab, Page Up
/// and Down, Home and End move; Enter or F2 types into the cell, Delete
/// empties it; Ctrl+C copies it, Ctrl+V pastes cells from there on;
/// Ctrl+Z and Ctrl+Y are the table's while the pointer is over it.
fn keys(app: &mut Moonglow, ui: &mut Ui, at: usize, pane: egui::Rect) {
    let doc = &mut app.tables[at];
    // "Go to the next wrong cell", from the header's button.
    if doc.go_to == "!" {
        doc.go_to.clear();
        if let Some((_, found)) = &doc.faults
            && !found.is_empty()
        {
            let now = doc.selected.unwrap_or((usize::MAX, usize::MAX));
            let next = (found.iter().find(|f| (f.row, f.column) > now))
                .or(found.first())
                .map(|f| (f.row, f.column));
            doc.selected = next;
            doc.filter.clear();
            doc.reveal = true;
            give_keys(ui.ctx(), Keys::Table);
        }
    }
    let over = ui.ctx().pointer_hover_pos().is_some_and(|p| pane.contains(p));
    if over && doc.typing.is_none() && !ui.ctx().egui_wants_keyboard_input() {
        let (undo, redo) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y),
            )
        });
        if undo && doc.edit.undo() || redo && doc.edit.redo() {
            doc.changed();
        }
    }
    if !keys_are(ui.ctx(), Keys::Table) || doc.typing.is_some() {
        return;
    }
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let Some((row, column)) = doc.selected else { return };
    let rows = shown_rows(doc);
    let Some(place) = rows.iter().position(|r| *r == row) else { return };
    let columns = doc.edit.table().columns().len();
    use egui::Key;
    let none = egui::Modifiers::NONE;
    let pressed = |ui: &mut Ui, m: egui::Modifiers, k: Key| ui.input_mut(|i| i.consume_key(m, k));
    let last = rows.len() as isize - 1;
    let mut to = None;
    for (key, by) in [
        (Key::ArrowDown, 1),
        (Key::ArrowUp, -1),
        (Key::PageDown, 15),
        (Key::PageUp, -15),
        (Key::End, isize::MAX / 2),
        (Key::Home, isize::MIN / 2),
    ] {
        if pressed(ui, none, key) {
            let next = (place as isize).saturating_add(by).clamp(0, last) as usize;
            to = Some((rows[next], column));
        }
    }
    if pressed(ui, none, Key::ArrowRight) || pressed(ui, none, Key::Tab) {
        to = Some((row, (column + 1).min(columns.saturating_sub(1))));
    }
    if pressed(ui, none, Key::ArrowLeft) || pressed(ui, egui::Modifiers::SHIFT, Key::Tab) {
        to = Some((row, column.saturating_sub(1)));
    }
    if let Some(to) = to {
        doc.selected = Some(to);
        doc.reveal = true;
    }
    if pressed(ui, none, Key::Enter) || pressed(ui, none, Key::F2) {
        let now = doc.edit.table().cell(row, column).unwrap_or_default().to_string();
        doc.typing = Some(now);
    }
    if pressed(ui, none, Key::Delete) && doc.edit.set(row, column, None) == Ok(true) {
        doc.changed();
    }
    if pressed(ui, none, Key::Escape) {
        give_keys(ui.ctx(), Keys::Area);
    }
    let (copy, _, paste) = crate::widgets::copy_cut_paste(ui);
    if copy {
        let text = doc.edit.table().cell(row, column).unwrap_or("****").to_string();
        ui.ctx().copy_text(text);
    }
    if paste {
        let text = ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(t) => Some(t.clone()),
                _ => None,
            })
        });
        if let Some(text) = text {
            match doc.edit.paste(row, column, &text) {
                Ok(0) => {}
                Ok(_) => doc.changed(),
                Err(e) => doc.problem = Some(e.to_string()),
            }
        }
    }
}

fn grid(
    app: &mut Moonglow,
    ui: &mut Ui,
    at: usize,
    kinds: &[(&'static str, ColumnKind)],
    faults: &[CellFault],
) {
    let game = app.game.clone();
    let doc = &mut app.tables[at];
    let rows = shown_rows(doc);
    let columns: Vec<String> = doc.edit.table().columns().to_vec();
    let kind_of = |c: &str| kinds.iter().find(|(k, _)| k.eq_ignore_ascii_case(c)).map(|(_, k)| *k);
    let strref: Vec<bool> =
        columns.iter().map(|c| doc.strref_text && kind_of(c) == Some(ColumnKind::StrRef)).collect();
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 6.0;
    let pitch = row_height + ui.spacing().item_spacing.y;
    let step = CELL + ui.spacing().item_spacing.x;
    let reveal = std::mem::take(&mut doc.reveal);
    let selected = doc.selected;
    let has_keys = keys_are(ui.ctx(), Keys::Table);
    let (mut clicked, mut begin, mut done, mut cancel) = (None, false, false, false);
    let wrong = ui.visuals().error_fg_color;
    // The column names stay in sight over the rows, and the rows' numbers
    // beside them: each goes with the rows' own scroll, a frame behind.
    let hidden = egui::scroll_area::ScrollBarVisibility::AlwaysHidden;
    let still = egui::scroll_area::ScrollSource::NONE;
    let (scroll_x, scroll_y) = (doc.scroll.x, doc.scroll.y);
    let id = doc.id;
    ui.horizontal(|ui| {
        ui.add_sized([NUMBER, row_height], egui::Label::new(""));
        egui::ScrollArea::horizontal()
            .id_salt(("table-head", id))
            .auto_shrink([false, true])
            .scroll_bar_visibility(hidden)
            .scroll_source(still)
            .horizontal_scroll_offset(scroll_x)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for c in &columns {
                        let text = egui::RichText::new(c).strong();
                        let (rect, r) = ui.allocate_exact_size(
                            egui::vec2(CELL, row_height),
                            egui::Sense::hover(),
                        );
                        let inner = rect.shrink2(egui::vec2(4.0, 0.0));
                        // (In the cell's place, taking none of its own: the
                        // cells after it stay in their columns.)
                        let layout = egui::Layout::left_to_right(egui::Align::Center);
                        ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(layout))
                            .add(egui::Label::new(text).truncate().selectable(false));
                        r.on_hover_text(match kind_of(c) {
                            Some(ColumnKind::StrRef) => {
                                format!("{c}: a talk-table string's number")
                            }
                            Some(ColumnKind::Model) => format!("{c}: a model's name"),
                            Some(ColumnKind::Row(t)) => format!("{c}: a row of {t}.2da"),
                            None => c.clone(),
                        });
                    }
                    // (Room for the rows' own scroll bar at the right.)
                    ui.add_space(ui.spacing().scroll.bar_width + 12.0);
                });
            });
    });
    let mut list =
        egui::ScrollArea::both().id_salt(("table-rows", doc.id)).auto_shrink([false, false]);
    if reveal && let Some((row, column)) = selected {
        if let Some(place) = rows.iter().position(|r| *r == row) {
            let above = (ui.available_height() / 2.0 - pitch).max(0.0);
            list = list.vertical_scroll_offset((place as f32 * pitch - above).max(0.0));
        }
        // Sideways only where the cell is out of sight.
        let (left, width) = (column as f32 * step, ui.available_width() - NUMBER);
        if left < scroll_x || left + step > scroll_x + width {
            list = list.horizontal_scroll_offset((left - width / 2.0).max(0.0));
        }
    }
    let room = ui.available_height();
    let bar = ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_inner_margin;
    let shown = ui
        .horizontal_top(|ui| {
            let gutter = egui::vec2(NUMBER, (room - bar).max(row_height));
            let down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(gutter, down, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("table-numbers", id))
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(hidden)
                    .scroll_source(still)
                    .vertical_scroll_offset(scroll_y)
                    .show_rows(ui, row_height, rows.len(), |ui, range| {
                        for &r in &rows[range] {
                            let number = egui::RichText::new(r.to_string()).weak().monospace();
                            let label = egui::Label::new(number).selectable(false);
                            ui.add_sized([NUMBER - 8.0, row_height], label);
                        }
                    });
            });
            ui.vertical(|ui| {
                list.show_rows(ui, row_height, rows.len(), |ui, range| {
                    for &r in &rows[range] {
                        ui.horizontal(|ui| {
                            for (c, is_strref) in strref.iter().copied().enumerate() {
                                let here = selected == Some((r, c));
                                if here && let Some(text) = doc.typing.as_mut() {
                                    let edit = egui::TextEdit::singleline(text)
                                        .id(egui::Id::new(("table-cell", id)))
                                        .font(egui::TextStyle::Monospace)
                                        .margin(egui::Margin::symmetric(4, 0))
                                        .vertical_align(egui::Align::Center);
                                    // (In the cell's own place, as wide.)
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(CELL, row_height),
                                        egui::Sense::hover(),
                                    );
                                    // (A place of its own: what the field takes beyond
                                    // the cell does not move the cells after it.)
                                    let mut within =
                                        ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                    let field = within.add_sized(rect.size(), edit);
                                    if !field.has_focus() && !field.lost_focus() {
                                        field.request_focus();
                                    }
                                    if field.lost_focus() {
                                        match ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                            true => cancel = true,
                                            false => done = true,
                                        }
                                    }
                                    continue;
                                }
                                let raw = doc.edit.table().cell(r, c);
                                let fault = faults.iter().find(|f| f.row == r && f.column == c);
                                let text = raw
                                    .filter(|_| is_strref)
                                    .and_then(|v| v.parse::<u32>().ok())
                                    .and_then(|n| game.as_deref()?.string(mg_core::StrRef(n)));
                                let label =
                                    text.clone().unwrap_or_else(|| raw.unwrap_or("****").into());
                                let mut rich = egui::RichText::new(label).monospace();
                                if raw.is_none() {
                                    rich = rich.weak();
                                }
                                if fault.is_some() {
                                    rich = rich.color(wrong);
                                }
                                let (rect, cell) = ui.allocate_exact_size(
                                    egui::vec2(CELL, row_height),
                                    egui::Sense::click(),
                                );
                                if here {
                                    let fill = match has_keys {
                                        true => ui.visuals().selection.bg_fill,
                                        false => ui.visuals().widgets.inactive.weak_bg_fill,
                                    };
                                    ui.painter().rect_filled(rect, 2.0, fill);
                                } else if cell.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        2.0,
                                        ui.visuals().widgets.hovered.weak_bg_fill,
                                    );
                                }
                                if fault.is_some() {
                                    let stroke = egui::Stroke::new(1.0, wrong);
                                    ui.painter().rect_stroke(
                                        rect.shrink(1.0),
                                        2.0,
                                        stroke,
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                let inner = rect.shrink2(egui::vec2(4.0, 0.0));
                                // (In the cell's place, taking none of its own: the
                                // cells after it stay in their columns.)
                                let layout = egui::Layout::left_to_right(egui::Align::Center);
                                ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(layout))
                                    .add(egui::Label::new(rich).truncate().selectable(false));
                                let mut tip = Vec::new();
                                if text.is_some() {
                                    tip.push(format!("StrRef {}", raw.unwrap_or_default()));
                                }
                                if let Some(f) = fault {
                                    tip.push(f.message.clone());
                                }
                                let cell = match tip.is_empty() {
                                    true => cell,
                                    false => cell.on_hover_text(tip.join("\n")),
                                };
                                if cell.clicked() || cell.double_clicked() {
                                    clicked = Some((r, c));
                                    begin = cell.double_clicked();
                                }
                            }
                        });
                    }
                })
            })
            .inner
        })
        .inner;
    doc.scroll = shown.state.offset;
    crate::palette_view::keys_outline(ui, shown.inner_rect, Keys::Table);
    if done {
        doc.commit();
        // (Enter: on to the row below, as a spreadsheet does.)
        if let Some((row, column)) = doc.selected
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && let Some(place) = rows.iter().position(|r| *r == row)
            && let Some(next) = rows.get(place + 1)
        {
            doc.selected = Some((*next, column));
            doc.reveal = true;
        }
        give_keys(ui.ctx(), Keys::Table);
    }
    if cancel {
        doc.typing = None;
        give_keys(ui.ctx(), Keys::Table);
    }
    if let Some(cell) = clicked {
        doc.commit();
        doc.selected = Some(cell);
        give_keys(ui.ctx(), Keys::Table);
        if begin {
            let now = doc.edit.table().cell(cell.0, cell.1).unwrap_or_default().to_string();
            doc.typing = Some(now);
        }
    }
}

/// A row's name for a list of rows to choose from: its number and its
/// first cell that says something.
fn row_label(t: &TwoDa, row: usize) -> String {
    let named = ["Label", "LABEL", "Name", "NAME"].iter().find_map(|c| t.get(row, c));
    let text = named.or_else(|| (0..t.columns().len()).find_map(|c| t.cell(row, c)));
    match text {
        Some(text) => format!("{row}  {text}"),
        None => format!("{row}  ****"),
    }
}

/// Under the grid: the selected cell, by what its column holds.
fn inspector(
    app: &mut Moonglow,
    ui: &mut Ui,
    at: usize,
    kinds: &[(&'static str, ColumnKind)],
    faults: &[CellFault],
) {
    let game = app.game.clone();
    let name = app.tables[at].name();
    let Some((row, column)) = app.tables[at].selected else {
        ui.weak(
            "Click a cell to select it; double click, Enter or F2 to type in it; Delete \
             empties it.",
        );
        return;
    };
    let table: &TwoDa = app.tables[at].edit.table();
    let Some(column_name) = table.columns().get(column).cloned() else { return };
    let value = table.cell(row, column).map(str::to_string);
    let kind = kinds.iter().find(|(k, _)| k.eq_ignore_ascii_case(&column_name)).map(|(_, k)| *k);
    let mut set: Option<Option<String>> = None;
    let mut find_references = false;
    let mut talk = false;
    ui.horizontal_wrapped(|ui| {
        ui.strong(format!("Row {row}, {column_name}"));
        ui.monospace(value.as_deref().unwrap_or("****"));
        match kind {
            Some(ColumnKind::StrRef) => {
                let text = value
                    .as_deref()
                    .and_then(|v| v.parse::<u32>().ok())
                    .and_then(|n| game.as_deref()?.string(mg_core::StrRef(n)));
                match text {
                    Some(text) => ui.label(format!("\u{201c}{text}\u{201d}")),
                    None => ui.weak("a talk-table string's number"),
                };
                if ui.button("Talk Table").on_hover_text("Open the talk table editor").clicked() {
                    talk = true;
                }
            }
            Some(ColumnKind::Model) => {
                ui.weak("a model's name");
            }
            Some(ColumnKind::Row(target)) => {
                // (The table itself, as it is now, for a row of its own.)
                let read = match target.eq_ignore_ascii_case(&name) {
                    true => None,
                    false => game.as_deref().and_then(|g| g.table(target).ok()),
                };
                let other: Option<&TwoDa> =
                    if target.eq_ignore_ascii_case(&name) { Some(table) } else { read.as_deref() };
                let now = value.as_deref().and_then(mg_2da::parse_int);
                let named = now.and_then(|v| mg_module::doctor::row_named(&name, &column_name, v));
                let shown = match (&other, named) {
                    (Some(t), Some(n)) if n < t.len() => row_label(t, n),
                    (_, Some(n)) => format!("{n}  (no such row)"),
                    _ => "none".to_string(),
                };
                ui.weak(format!("a row of {target}.2da:"));
                if let Some(t) = other {
                    egui::ComboBox::from_id_salt(("table-row-choice", app.tables[at].id))
                        .selected_text(shown)
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(named.is_none(), "none (****)").clicked() {
                                set = Some(None);
                            }
                            let height = ui.text_style_height(&egui::TextStyle::Body) + 2.0;
                            egui::ScrollArea::vertical().max_height(260.0).show_rows(
                                ui,
                                height,
                                t.len(),
                                |ui, range| {
                                    for r in range {
                                        let label = row_label(t, r);
                                        if ui.selectable_label(named == Some(r), label).clicked() {
                                            set = Some(Some(r.to_string()));
                                        }
                                    }
                                },
                            );
                        });
                }
            }
            None => {}
        }
        if let Some(f) = faults.iter().find(|f| f.row == row && f.column == column) {
            ui.colored_label(ui.visuals().error_fg_color, &f.message);
        }
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(format!("Find References to Row {row}"))
            .on_hover_text(
                "The cells of other tables (and this one) that name this row, in the columns \
                 known to name a row of this table",
            )
            .clicked()
        {
            find_references = true;
        }
        if let Some((of, found)) = &app.tables[at].references
            && *of == row
        {
            if found.is_empty() {
                ui.weak("No table's known columns name it.");
            }
            for r in found.iter().take(12) {
                ui.label(format!("{}.2da row {}, {}", r.table, r.row, r.column));
            }
            if found.len() > 12 {
                ui.weak(format!("and {} more", found.len() - 12));
            }
        }
    });
    let found = find_references.then(|| references(app, &name, table, row));
    if let Some(value) = set {
        let doc = &mut app.tables[at];
        doc.typing = None;
        match doc.edit.set(row, column, value) {
            Ok(true) => doc.changed(),
            Ok(false) => {}
            Err(e) => doc.problem = Some(e.to_string()),
        }
    }
    if let Some(found) = found {
        app.tables[at].references = Some((row, found));
    }
    if talk {
        app.actions.push(Action::OpenTab(Tab::TalkTable));
    }
}

/// Add Column…: its name.
fn column_window(app: &mut Moonglow, ctx: &egui::Context, at: usize) {
    let doc = &mut app.tables[at];
    let Some(mut name) = doc.new_column.clone() else { return };
    let mut open = true;
    let mut cancelled = false;
    let mut add = false;
    crate::widgets::dialog(ctx, "Add Column", &mut open).collapsible(false).resizable(false).show(
        ctx,
        |ui| {
            ui.label("The column's name (no spaces)");
            let field = ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.0));
            crate::widgets::autofocus(ui, &field);
            let fits = !name.trim().is_empty() && !name.contains([' ', '\t', '"']);
            let taken = doc.edit.table().column(name.trim()).is_some();
            if taken {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "The table has a column of this name.",
                );
            }
            ui.horizontal(|ui| {
                let may = fits && !taken;
                if ui.add_enabled(may, egui::Button::new("Add")).clicked()
                    || (may && crate::widgets::enter(ui))
                {
                    add = true;
                }
                if crate::widgets::cancel(ui) {
                    cancelled = true;
                }
            });
        },
    );
    if add {
        match doc.edit.add_column(name.trim()) {
            Ok(()) => doc.changed(),
            Err(e) => doc.problem = Some(e.to_string()),
        }
        doc.new_column = None;
    } else {
        doc.new_column = (open && !cancelled).then_some(name);
    }
}

/// Take Rows…: the rows another copy has and this table lacks or has
/// differently, to choose from.
fn taking_window(app: &mut Moonglow, ctx: &egui::Context, at: usize, others: &[(String, TwoDa)]) {
    let doc = &mut app.tables[at];
    let Some(mut taking) = doc.taking.clone() else { return };
    let mut open = true;
    let mut cancelled = false;
    let mut take = false;
    let columns: Vec<String> = doc.edit.table().columns().to_vec();
    // The lowest copy is what the others are told from.
    let base = others.last().filter(|_| others.len() > 1).map(|(_, t)| t);
    if taking.from.as_ref().is_none_or(|f| !others.iter().any(|(l, _)| l == f)) {
        taking.from = others.first().map(|(l, _)| l.clone());
        taking.chosen.clear();
    }
    let from = taking.from.clone();
    let other = from.as_ref().and_then(|f| others.iter().find(|(l, _)| l == f));
    // (Whose change a difference is can be told only against a lower
    // copy. Worked out once for a copy and the table as it is.)
    let kept =
        taking.found.as_ref().filter(|(l, r, _, _)| Some(l) == from.as_ref() && *r == doc.revision);
    let (found, told): (Arc<Vec<Candidate>>, bool) = match (kept, other) {
        (Some((_, _, found, told)), _) => (found.clone(), *told),
        (None, Some((label, t))) => {
            let base = base.filter(|_| others.last().is_some_and(|(l, _)| l != label));
            let found = Arc::new(mg_module::table_edit::candidates(doc.edit.table(), t, base));
            taking.found = Some((label.clone(), doc.revision, found.clone(), base.is_some()));
            (found, base.is_some())
        }
        (None, None) => (Arc::default(), false),
    };
    crate::widgets::dialog(ctx, "Take Rows from Another Copy", &mut open)
        .default_size([760.0, 520.0])
        .show(ctx, |ui| {
            if others.is_empty() {
                ui.label("No other copy of this table is in the load order.");
                return;
            }
            ui.horizontal(|ui| {
                ui.label("From");
                egui::ComboBox::from_id_salt(("take-from", doc.id))
                    .selected_text(from.clone().unwrap_or_default())
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for (label, t) in others {
                            let text = format!("{label} ({} rows)", t.len());
                            if ui.selectable_label(from.as_ref() == Some(label), text).clicked() {
                                taking.from = Some(label.clone());
                                taking.chosen.clear();
                            }
                        }
                    });
                let count = |o: Offered| found.iter().filter(|c| c.offered == o).count();
                ui.weak(match told {
                    true => format!(
                        "{} new, {} changed there, {} changed in both",
                        count(Offered::New),
                        count(Offered::Changed),
                        count(Offered::Conflict)
                    ),
                    false => format!(
                        "{} new, {} that differ (the lowest copy: nothing to tell by whose \
                         change it is)",
                        count(Offered::New),
                        count(Offered::Changed)
                    ),
                });
            });
            ui.horizontal(|ui| {
                // A box is ticked while every row of its kind is.
                let kinds = [
                    (Offered::New, "New", "The rows this table lacks"),
                    (
                        Offered::Changed,
                        "Differ",
                        "The rows the other copy has differently, which this table did not \
                         change itself (where that can be told)",
                    ),
                ];
                for (kind, label, hint) in kinds {
                    let rows: Vec<_> =
                        found.iter().filter(|c| c.offered == kind).map(|c| c.row).collect();
                    let mut all =
                        !rows.is_empty() && rows.iter().all(|r| taking.chosen.contains(r));
                    let tick =
                        ui.add_enabled(!rows.is_empty(), egui::Checkbox::new(&mut all, label));
                    if tick.on_hover_text(hint).changed() {
                        match all {
                            true => taking.chosen.extend(rows),
                            false => rows.iter().for_each(|r| {
                                taking.chosen.remove(r);
                            }),
                        }
                    }
                }
                if ui.button("Clear Selections").clicked() {
                    taking.chosen.clear();
                }
            });
            ui.separator();
            if found.is_empty() {
                ui.weak("This table has every row of that copy as it has them.");
            }
            let height = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
            let room = (ui.available_height() - 40.0).max(120.0);
            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, false]).show_rows(
                ui,
                height,
                found.len(),
                |ui, range| {
                    for c in &found[range] {
                        ui.horizontal(|ui| {
                            let mut on = taking.chosen.contains(&c.row);
                            let what = match c.offered {
                                Offered::New => "new",
                                Offered::Changed if told => "changed there",
                                Offered::Changed => "differs",
                                Offered::Conflict => "changed in both",
                            };
                            if ui.checkbox(&mut on, format!("Row {}", c.row)).changed() {
                                match on {
                                    true => _ = taking.chosen.insert(c.row),
                                    false => _ = taking.chosen.remove(&c.row),
                                }
                            }
                            let text = egui::RichText::new(what);
                            ui.label(match c.offered {
                                Offered::Conflict => text.color(ui.visuals().warn_fg_color),
                                _ => text.weak(),
                            });
                            // The cells that differ: here, then there.
                            let cells: Vec<String> = c
                                .cells
                                .iter()
                                .map(|(col, mine, theirs)| {
                                    let (mine, theirs) = (
                                        mine.as_deref().unwrap_or("****"),
                                        theirs.as_deref().unwrap_or("****"),
                                    );
                                    match c.offered {
                                        Offered::New => format!("{}: {theirs}", columns[*col]),
                                        _ => format!("{}: {mine} \u{2192} {theirs}", columns[*col]),
                                    }
                                })
                                .collect();
                            let line = cells.join("   ");
                            ui.add(
                                egui::Label::new(egui::RichText::new(&line).monospace()).truncate(),
                            )
                            .on_hover_text(cells.join("\n"));
                        });
                    }
                },
            );
            ui.separator();
            ui.horizontal(|ui| {
                let n = taking.chosen.len();
                if ui
                    .add_enabled(n > 0, egui::Button::new(format!("Take {n} Rows")))
                    .on_hover_text("Each row whole, as that copy has it; one step to undo")
                    .clicked()
                {
                    take = true;
                }
                if crate::widgets::cancel(ui) {
                    cancelled = true;
                }
            });
        });
    if take && let Some((_, t)) = other {
        let rows: Vec<usize> = taking.chosen.iter().copied().collect();
        if doc.edit.take_rows(t, &rows) > 0 {
            doc.changed();
        }
        doc.taking = None;
    } else {
        doc.taking = (open && !cancelled).then_some(taking);
    }
}

/// Asks what to do with a table's unsaved changes when its tab is closed.
pub(crate) fn closing_window(app: &mut Moonglow, ctx: &egui::Context) {
    let Some(id) = app.table_closing else { return };
    let Some(doc) = app.tables.iter().find(|d| d.id == id) else {
        app.table_closing = None;
        return;
    };
    let title = doc.key.to_string();
    let mut answer = None;
    egui::Window::new("Unsaved Table")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("{title} has unsaved changes."));
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(true);
                }
                if ui.button("Discard").clicked() {
                    answer = Some(false);
                }
                if crate::widgets::cancel(ui) {
                    app.table_closing = None;
                }
            });
        });
    let Some(keep) = answer else { return };
    app.table_closing = None;
    if keep && !save(app, id) {
        return;
    }
    app.tables.retain(|d| d.id != id);
    if let Some(t) = app.dock.find_tab(&Tab::Table(id)) {
        app.dock.remove_tab(t);
    }
}
