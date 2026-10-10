//! 2DAs and talk tables as values a plugin reads, changes and writes out:
//! `mg.twoda` and `mg.tlk`. A generator makes its tables with these
//! instead of putting a 2DA's text together by hand.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use mg_2da::TwoDa;
use mg_module::doctor::ColumnKind;
use mg_tlk::{Tlk, TlkEntry};
use mlua::{Lua, Table, Value as LuaValue};

use crate::runtime::{codepage, fail};

type This = LuaValue;

/// What these objects have, for the list of the API's names.
pub(crate) const NAMES: [&str; 9] = [
    "twoda:set",
    "twoda:add_row",
    "twoda:add_column",
    "twoda:write",
    "tlk.lines",
    "tlk:get",
    "tlk:set",
    "tlk:add",
    "tlk:write",
];

/// A cell's text from what a plugin gives: text, or a number written as
/// the game's tables write one. Nil is an empty cell.
fn cell(value: &LuaValue) -> mlua::Result<Option<String>> {
    Ok(match value {
        LuaValue::Nil => None,
        LuaValue::String(s) => Some(s.to_str()?.to_string()).filter(|s| s != "****"),
        LuaValue::Integer(n) => Some(n.to_string()),
        LuaValue::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => Some((*n as i64).to_string()),
        LuaValue::Number(n) => Some(n.to_string()),
        LuaValue::Boolean(b) => Some(if *b { "1" } else { "0" }.to_string()),
        _ => return fail("a cell is text, a number or nil"),
    })
}

/// A 2DA as plugins have it: what `ctx.game:table` and `mg.twoda` give.
/// It is the plugin's own: changing it changes nothing else (a table of
/// the game's is copied when first changed).
pub(crate) fn twoda(lua: &Lua, table: Arc<TwoDa>) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    out.set("rows", table.len())?;
    out.set("columns", table.columns().to_vec())?;
    let shared = Rc::new(RefCell::new(table));
    let t = shared.clone();
    out.set(
        "get",
        lua.create_function(move |_, (_, row, column): (This, usize, String)| {
            Ok(t.borrow().get(row, &column).map(str::to_string))
        })?,
    )?;
    let t = shared.clone();
    out.set(
        "set",
        lua.create_function(move |_, (_, row, column, value): (This, usize, String, LuaValue)| {
            let mut t = t.borrow_mut();
            let t = Arc::make_mut(&mut t);
            let Some(at) = t.column(&column) else {
                return fail(format!("the table has no column {column}"));
            };
            if row >= t.len() {
                return fail(format!("the table has no row {row} (add_row adds one)"));
            }
            let value = cell(&value)?;
            if value.as_ref().is_some_and(|v| v.contains('"')) {
                return fail("a cell can't hold a double quote");
            }
            t.rows[row][at] = value;
            Ok(())
        })?,
    )?;
    let t = shared.clone();
    out.set(
        "add_row",
        lua.create_function(move |_, (this, cells): (Table, Option<Table>)| {
            let mut t = t.borrow_mut();
            let t = Arc::make_mut(&mut t);
            let mut row = vec![None; t.columns().len()];
            for pair in cells.iter().flat_map(|c| c.pairs::<String, LuaValue>()) {
                let (column, value) = pair?;
                let Some(at) = t.column(&column) else {
                    return fail(format!("the table has no column {column}"));
                };
                row[at] = cell(&value)?;
                if row[at].as_ref().is_some_and(|v| v.contains('"')) {
                    return fail("a cell can't hold a double quote");
                }
            }
            t.rows.push(row);
            this.set("rows", t.len())?;
            Ok(t.len() - 1)
        })?,
    )?;
    let t = shared.clone();
    out.set(
        "add_column",
        lua.create_function(move |_, (this, name): (Table, String)| {
            let mut t = t.borrow_mut();
            let t = Arc::make_mut(&mut t);
            if t.column(&name).is_some() {
                return fail(format!("the table has a column {name}"));
            }
            let mut columns = t.columns().to_vec();
            columns.push(name);
            t.set_columns(columns).or_else(fail)?;
            for row in &mut t.rows {
                row.push(None);
            }
            this.set("columns", t.columns().to_vec())
        })?,
    )?;
    let t = shared;
    out.set(
        "write",
        lua.create_function(move |lua, _: This| {
            lua.create_string(t.borrow().to_bytes(codepage()).or_else(fail)?)
        })?,
    )?;
    Ok(out)
}

/// `mg.twoda`: tables made and read.
pub(crate) fn twoda_api(lua: &Lua) -> mlua::Result<Table> {
    let api = lua.create_table()?;
    api.set(
        "parse",
        lua.create_function(|lua, bytes: mlua::LuaString| {
            let table = TwoDa::parse(&bytes.as_bytes(), codepage()).or_else(fail)?;
            twoda(lua, Arc::new(table))
        })?,
    )?;
    api.set(
        "new",
        lua.create_function(|lua, columns: Vec<String>| {
            twoda(lua, Arc::new(TwoDa::new(columns).or_else(fail)?))
        })?,
    )?;
    api.set(
        "kinds",
        lua.create_function(|lua, name: String| {
            let out = lua.create_table()?;
            for (column, kind) in mg_module::doctor::column_kinds(&name) {
                let kind = match kind {
                    ColumnKind::StrRef => "strref",
                    ColumnKind::Model => "model",
                };
                out.set(column, kind)?;
            }
            Ok(out)
        })?,
    )?;
    Ok(api)
}

fn entry_table(lua: &Lua, e: &TlkEntry) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("text", lua.create_string(codepage().decode(&e.text).as_bytes())?)?;
    if let Some(sound) = e.sound_resref() {
        t.set("sound", sound.to_string())?;
    }
    if e.sound_length != 0.0 {
        t.set("sound_length", e.sound_length)?;
    }
    Ok(t)
}

/// A line from what a plugin gives: its text, or a table with `text`,
/// `sound` and `sound_length`. Nil is no line.
fn entry(value: &LuaValue) -> mlua::Result<TlkEntry> {
    let encoded = |text: &str| match codepage().encode(text) {
        Some(bytes) => Ok(bytes.into_owned()),
        None => fail("the text has letters the game's codepage lacks"),
    };
    Ok(match value {
        LuaValue::Nil => TlkEntry::default(),
        LuaValue::String(s) => TlkEntry::text(encoded(&s.to_str()?)?),
        LuaValue::Table(t) => {
            let text: Option<String> = t.get("text")?;
            let mut e = TlkEntry::text(encoded(text.as_deref().unwrap_or_default())?);
            if let Some(sound) = t.get::<Option<String>>("sound")?.filter(|s| !s.is_empty()) {
                if sound.len() > 16 || !sound.is_ascii() {
                    return fail(format!("{sound}: a sound's name is at most 16 letters"));
                }
                e.sound = sound.into_bytes();
                e.flags |= mg_tlk::FLAG_SOUND;
            }
            if let Some(length) = t.get::<Option<f32>>("sound_length")?.filter(|l| *l != 0.0) {
                e.sound_length = length;
                e.flags |= mg_tlk::FLAG_SOUND_LENGTH;
            }
            e
        }
        _ => return fail("a line is its text, a table with a text, or nil"),
    })
}

/// A talk table as plugins have it. Lines count from 0, as in the file (a
/// custom table's line 0 is the game's string 16777216).
fn tlk(lua: &Lua, table: Tlk) -> mlua::Result<Table> {
    let out = lua.create_table()?;
    out.set("lines", table.entries.len())?;
    let shared = Rc::new(RefCell::new(table));
    let t = shared.clone();
    out.set(
        "get",
        lua.create_function(move |lua, (_, line): (This, usize)| {
            match t.borrow().entries.get(line).filter(|e| **e != TlkEntry::default()) {
                Some(e) => Ok(Some(entry_table(lua, e)?)),
                None => Ok(None),
            }
        })?,
    )?;
    let t = shared.clone();
    out.set(
        "set",
        lua.create_function(move |_, (this, line, value): (Table, usize, LuaValue)| {
            if line >= mg_core::StrRef::CUSTOM_FLAG as usize {
                return fail(format!("{line} is past what a table holds"));
            }
            let e = entry(&value)?;
            let mut t = t.borrow_mut();
            if t.entries.len() <= line {
                t.entries.resize(line + 1, TlkEntry::default());
            }
            t.entries[line] = e;
            this.set("lines", t.entries.len())
        })?,
    )?;
    let t = shared.clone();
    out.set(
        "add",
        lua.create_function(move |_, (this, value): (Table, LuaValue)| {
            let e = entry(&value)?;
            let mut t = t.borrow_mut();
            t.entries.push(e);
            this.set("lines", t.entries.len())?;
            Ok(t.entries.len() - 1)
        })?,
    )?;
    let t = shared;
    out.set(
        "write",
        lua.create_function(move |lua, _: This| {
            lua.create_string(t.borrow().to_bytes().or_else(fail)?)
        })?,
    )?;
    Ok(out)
}

/// `mg.tlk`: talk tables made and read.
pub(crate) fn tlk_api(lua: &Lua) -> mlua::Result<Table> {
    let api = lua.create_table()?;
    api.set(
        "parse",
        lua.create_function(|lua, bytes: mlua::LuaString| {
            tlk(lua, Tlk::read(&bytes.as_bytes()).or_else(fail)?)
        })?,
    )?;
    api.set("new", lua.create_function(|lua, ()| tlk(lua, Tlk::new(mg_core::Language::ENGLISH)))?)?;
    Ok(api)
}
