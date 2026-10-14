//! Renaming a window: its JUI and what it has beside it in the module.
//!
//! A window's name is in more than its JUI's: its settings are the TXT of
//! that name, its scripts are `NAME_o` and `NAME_e`, the opener's functions
//! are named after it (`Open_NAME`, and one for each swap layout variant
//! and each swap layout's first contents), the event script includes the
//! opener, and the module's own scripts include it and call those
//! functions. [`rename`] moves all of it as one command.

use std::collections::BTreeMap;

use mg_core::{ResRef, ResType};
use mg_edit::{Command, Edit};
use mg_module::Module;
use mg_module::refs::includers;
use mg_resman::ResKey;
use mg_script::lex::{TokenKind, tokenize};

use crate::actions::{initial_function, merge_events, variant_function};
use crate::script::{bytes_text, fingerprint, sectioned, swap_groups, text_bytes};
use crate::{NuiError, Resources, Settings, key, opener_source, parse};

/// What [`rename`] made.
#[derive(Debug, Clone)]
pub struct Renamed {
    /// Everything to change in the module, as one undoable command.
    pub command: Command,
    /// The places in the module's own scripts that were pointed at the new
    /// name: `#include` lines of the opener, and calls of its functions.
    pub in_scripts: usize,
    /// The names scripts may still spell in strings, where nothing is
    /// changed (a string may mean something else): the window's old name
    /// (its ID in the game, unless the settings give it another) and those
    /// of its two scripts. For the caller to look for and say.
    pub spelled: Vec<String>,
    /// Whether the window has scripts to build under its new name: its
    /// compiled scripts, and those of the scripts that open it, are gone.
    pub build: bool,
}

/// Renames the window `from` to `to`, with what it has beside it, as one
/// command.
///
/// The JUI moves as it is. A TXT of its name moves with it if it is the
/// window's settings (one that does not read as settings is some other
/// text, and stays; the scripts are then not known to be the window's, and
/// stay too). With settings, the scripts follow:
///
/// - The opener (`from_o`) is written anew for the new name, and the
///   settings note it as Build's. One changed by hand moves as it is, with
///   its function names changed, and stays one changed by hand.
/// - The event script (`from_e`) is rebuilt for the new name around the
///   code in its sections. One without sections, or changed outside them,
///   moves as it is.
/// - The compiled scripts of both are removed, not moved: they carry the
///   old names. The settings forget the event script as last compiled, so
///   the window reads as not built; Build makes them.
/// - The module's scripts that include the opener are pointed at the new
///   one: their `#include` line (`mg_module::rename`), and the opener's
///   function names where they are used as names. Their compiled scripts
///   are removed too, as a renamed include's are: Build compiles them.
///
/// Strings are left alone, as a string may mean something else:
/// [`Renamed::spelled`] names what to look for. With `literals`, those of
/// the module's scripts that spell the window's old name (its ID in the
/// game: `NuiFindWindow`) are changed too, as the module's rename changes
/// a resource's.
///
/// Refused, with nothing changed ([`can_rename`]): a new name that is not
/// a window's, and one whose resources the module has.
pub fn rename(module: &Module, from: &str, to: &str, literals: bool) -> Result<Renamed, NuiError> {
    can_rename(module, from, to)?;
    let layout = key(from, ResType::JUI)?;
    let jui = module.get(&layout).ok_or(NuiError::Missing(layout))?;
    let new = Resources::of(to)?;
    let mut staged = module.clone();
    staged.remove(&layout);
    staged.set(new.layout, jui.to_vec());
    let old_settings = key(from, ResType::TXT)?;
    // (A name with no room for the suffixes was not made here, and has no
    // scripts; its settings still move.)
    let old = Resources::of(from).ok();
    let settings = module.get(&old_settings).and_then(|b| Settings::parse(b).ok());
    let mut renamed = Renamed {
        command: Command::new(String::new(), Vec::new()),
        in_scripts: 0,
        spelled: Vec::new(),
        build: false,
    };
    if let Some(mut settings) = settings {
        let was = settings.clone();
        if let Some(old) = &old {
            let window = parse(jui).ok();
            move_scripts(module, &mut staged, (old, &new), window.as_ref(), &mut settings)
                .map(|n| renamed.in_scripts = n)?;
            renamed.spelled = vec![from.to_owned(), old.opener.clone(), old.events.clone()];
            renamed.build = scripts(old).any(|k| module.contains(&k));
        }
        // (As they are, where nothing in them changed.)
        let bytes = match settings == was {
            true => module.get(&old_settings).map(<[u8]>::to_vec).unwrap_or_default(),
            false => settings.bytes(),
        };
        staged.remove(&old_settings);
        staged.set(new.settings, bytes);
    }
    if literals {
        renamed.in_scripts += rename_strings(&mut staged, from, to);
    }
    let mut keys: Vec<ResKey> = staged.keys().chain(module.keys()).copied().collect();
    keys.sort();
    keys.dedup();
    let edits = keys
        .into_iter()
        .filter(|k| staged.get(k) != module.get(k))
        .map(|k| Edit::SetResource { key: k, data: staged.get(&k).map(<[u8]>::to_vec) })
        .collect();
    renamed.command = Command::new(format!("Rename {layout} to {}", new.layout), edits);
    Ok(renamed)
}

/// A window's two scripts, source and compiled.
fn scripts(r: &Resources) -> impl Iterator<Item = ResKey> {
    [ResType::NSS, ResType::NCS, ResType::NDB].into_iter().flat_map(|t| [r.opener(t), r.events(t)])
}

/// Whether the window `from` can be renamed to `to`, and why not: it is
/// not in the module, `to` is not a window's name ([`check_name`]: no room
/// for the `_o` and `_e` of its scripts), or the module has a resource the
/// window or its scripts would take under that name.
pub fn can_rename(module: &Module, from: &str, to: &str) -> Result<(), NuiError> {
    let layout = key(from, ResType::JUI)?;
    if !module.contains(&layout) {
        return Err(NuiError::Missing(layout));
    }
    let new = Resources::of(to)?;
    match [new.layout, new.settings].into_iter().chain(scripts(&new)).find(|k| module.contains(k)) {
        Some(k) => Err(NuiError::Exists(k)),
        None => Ok(()),
    }
}

/// The scripts of the window `old` in `staged`, moved to `new`, and the
/// module's scripts that include its opener pointed at the new one; the
/// settings' notes of the two scripts as they are after. Returns how many
/// places in scripts were changed.
fn move_scripts(
    module: &Module,
    staged: &mut Module,
    (old, new): (&Resources, &Resources),
    window: Option<&serde_json::Value>,
    settings: &mut Settings,
) -> Result<usize, NuiError> {
    let (from, to) = (name_of(old), name_of(new));
    let nss = ResType::NSS;
    let compiled = |k: ResKey| [ResType::NCS, ResType::NDB].map(|t| ResKey::new(k.resref, t));
    // The scripts that use the opener's functions: those that include it.
    let users = includers(module, &[old.opener(nss)]);
    let names = function_names(from, to, settings);

    // The include lines, as a renamed include's are changed: the module's
    // rename moves the opener, points every `#include` of it at the new
    // name and removes the compiled scripts of what includes it. (For a
    // window not built yet, an empty opener is moved, and taken out again.)
    let opener = module.get(&old.opener(nss));
    if opener.is_none() {
        staged.set(old.opener(nss), Vec::new());
    }
    let to_opener =
        ResRef::from_str(&new.opener).map_err(|_| NuiError::Name { name: to.into() })?;
    let report = mg_module::rename::rename(staged, old.opener(nss), to_opener, false)
        .map_err(|e| NuiError::Rename(e.to_string()))?;
    let mut places = report.in_scripts;
    for k in compiled(new.opener(nss)) {
        staged.remove(&k);
    }
    match opener {
        None => _ = staged.remove(&new.opener(nss)),
        Some(was) => {
            let built = settings.opener_hash.as_deref() == Some(&fingerprint(was))
                || window.is_some_and(|w| {
                    opener_source(from, w, settings).is_ok_and(|s| s.as_bytes() == was)
                });
            let anew = window.and_then(|w| opener_source(to, w, settings).ok());
            match (built, anew) {
                // Build's own: written anew, and noted as Build's.
                (true, Some(source)) => {
                    settings.opener_hash = Some(fingerprint(source.as_bytes()));
                    staged.set(new.opener(nss), source.into_bytes());
                }
                // Build's own, of a window that cannot be read: there is
                // nothing to write it from. Build writes it when it can.
                (true, None) => {
                    settings.opener_hash = None;
                    staged.remove(&new.opener(nss));
                }
                // Changed by hand: as it is, but for its function names.
                (false, _) => {
                    let (text, n) = rename_functions(was, &names);
                    places += n;
                    staged.set(new.opener(nss), text);
                }
            }
        }
    }

    // The event script: rebuilt for the new name around its sections, or
    // (written by hand) moved as the include lines left it.
    let events = staged.get(&old.events(nss)).map(<[u8]>::to_vec);
    for k in [old.events(nss)].into_iter().chain(compiled(old.events(nss))) {
        staged.remove(&k);
    }
    if let Some(moved) = events {
        let rebuilt = module.get(&old.events(nss)).filter(|b| sectioned(from, b)).and_then(|b| {
            let text = bytes_text(b).replace("\r\n", "\n");
            merge_events(to, settings, Some(&text)).ok()
        });
        let text = rebuilt.as_deref().map_or(moved, text_bytes);
        staged.set(new.events(nss), text);
        // (Nothing is compiled of it under this name.)
        settings.event_hash = None;
    }

    // The opener's functions, where the scripts that include it name them.
    for user in users {
        let user = if user == old.events(nss) { new.events(nss) } else { user };
        let Some(src) = staged.get(&user) else { continue };
        let (text, n) = rename_functions(src, &names);
        if n > 0 {
            places += n;
            staged.set(user, text);
        }
    }
    Ok(places)
}

/// Points the strings of the module's scripts that spell `from` (in any
/// case, as the game looks names up) at `to`, and removes the compiled
/// scripts of those changed and of what includes them. Returns how many
/// strings were changed.
fn rename_strings(staged: &mut Module, from: &str, to: &str) -> usize {
    let keys: Vec<ResKey> = staged.keys_of(ResType::NSS).copied().collect();
    let (mut changed, mut strings) = (Vec::new(), 0);
    for k in keys {
        let Some(src) = staged.get(&k) else { continue };
        let mut out = Vec::with_capacity(src.len());
        let mut at = 0;
        for token in tokenize(src) {
            let inside = token.span.start + 1..token.span.end.saturating_sub(1);
            if token.kind == (TokenKind::String { terminated: true })
                && src.get(inside.clone()).is_some_and(|s| s.eq_ignore_ascii_case(from.as_bytes()))
            {
                out.extend_from_slice(&src[at..inside.start]);
                out.extend_from_slice(to.as_bytes());
                at = inside.end;
                strings += 1;
            }
        }
        if at > 0 {
            out.extend_from_slice(&src[at..]);
            staged.set(k, out);
            changed.push(k);
        }
    }
    let stale: Vec<ResKey> = includers(staged, &changed).into_iter().chain(changed).collect();
    for k in stale {
        for t in [ResType::NCS, ResType::NDB] {
            staged.remove(&ResKey::new(k.resref, t));
        }
    }
    strings
}

/// The name a window's resources are of.
fn name_of(r: &Resources) -> &str {
    r.opener.strip_suffix("_o").unwrap_or(&r.opener)
}

/// The functions of the opener of `from`, each with its name in the opener
/// of `to`: `Open_NAME`, and those of the settings' variants and swap
/// layouts. Whole names, so that a window whose name another's starts with
/// is not taken for it.
fn function_names(from: &str, to: &str, settings: &Settings) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    names.insert(format!("Open_{from}"), format!("Open_{to}"));
    for variant in settings.views.keys() {
        names.insert(variant_function(from, variant), variant_function(to, variant));
    }
    for group in swap_groups(settings) {
        names.insert(initial_function(from, &group), initial_function(to, &group));
    }
    names
}

/// A script with the names of `names` changed where they are names (not
/// in strings or comments), and how many were.
fn rename_functions(src: &[u8], names: &BTreeMap<String, String>) -> (Vec<u8>, usize) {
    let mut out = Vec::with_capacity(src.len());
    let (mut at, mut changed) = (0, 0);
    for token in tokenize(src) {
        if token.kind != TokenKind::Ident {
            continue;
        }
        let Some(span) = src.get(token.span.clone()) else { continue };
        let new = std::str::from_utf8(span).ok().and_then(|name| names.get(name));
        if let Some(new) = new {
            out.extend_from_slice(&src[at..token.span.start]);
            out.extend_from_slice(new.as_bytes());
            at = token.span.end;
            changed += 1;
        }
    }
    out.extend_from_slice(&src[at..]);
    (out, changed)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{Action, Route, window};

    fn k(name: &str, ty: ResType) -> ResKey {
        ResKey::parse(name, ty).unwrap()
    }

    fn text(module: &Module, name: &str, ty: ResType) -> String {
        String::from_utf8(module.get(&k(name, ty)).unwrap().to_vec()).unwrap()
    }

    fn applied(module: &Module, command: &Command) -> Module {
        let mut after = module.clone();
        for edit in &command.edits {
            match edit {
                Edit::SetResource { key, data: Some(data) } => after.set(*key, data.clone()),
                Edit::SetResource { key, data: None } => _ = after.remove(key),
                other => panic!("{other:?}"),
            }
        }
        after
    }

    /// The window `shop` as Build leaves it (but for what is compiled,
    /// which stands in), with a variant, a handler that shows it and code
    /// in its section, and a script of the module's that opens it.
    fn built() -> Module {
        let mut w = window();
        w["root"]["children"] = json!([
            {"type": "group", "id": "pages", "children": [{"type": "label", "value": "First"}]}
        ]);
        let mut s = Settings::default();
        s.views.insert("details".into(), json!({"type": "label", "value": "Second"}));
        s.view_targets.insert("details".into(), "pages".into());
        let show = Action::View { group: "pages".into(), view: "details".into() };
        s.actions.push(Route::new("click", "more", show));
        let opener = opener_source("shop", &w, &s).unwrap();
        let events = merge_events("shop", &s, None).unwrap().replace(
            "        NuiSetGroupLayout(",
            "        SendMessageToPC(oPlayer, \"Open_shop stays in a string\");\n        NuiSetGroupLayout(",
        );
        assert!(events.contains("Variant_shop_details()"), "{events}");
        s.opener_hash = Some(fingerprint(opener.as_bytes()));
        s.event_hash = Some(fingerprint(events.as_bytes()));
        let mut m = Module::new();
        m.set(k("shop", ResType::JUI), serde_json::to_vec_pretty(&w).unwrap());
        m.set(k("shop", ResType::TXT), s.bytes());
        m.set(k("shop_o", ResType::NSS), opener.into_bytes());
        m.set(k("shop_e", ResType::NSS), events.into_bytes());
        m.set(k("shop_e", ResType::NCS), b"NCS V1.0B e".to_vec());
        m.set(k("shop_e", ResType::NDB), b"NDB V1.0 e".to_vec());
        let caller = "#include \"shop_o\"\n// Open_shop opens it.\nvoid main()\n{\n    Open_shop(GetEnteringObject());\n    json j = Initial_shop_pages();\n    NuiFindWindow(OBJECT_SELF, \"shop\");\n}\n";
        m.set(k("enter", ResType::NSS), caller.as_bytes().to_vec());
        m.set(k("enter", ResType::NCS), b"NCS V1.0B enter".to_vec());
        // A window whose name this one's starts: not this one's.
        m.set(
            k("other", ResType::NSS),
            b"void main() { Open_shop_big(OBJECT_SELF); Open_shop(OBJECT_SELF); }".to_vec(),
        );
        m.set(k("other", ResType::NCS), b"NCS V1.0B other".to_vec());
        m
    }

    #[test]
    fn a_window_moves_with_its_settings_and_its_scripts() {
        let module = built();
        let made = rename(&module, "shop", "store", false).unwrap();
        assert_eq!(made.command.label, "Rename shop.jui to store.jui");
        let after = applied(&module, &made.command);
        // Nothing of the old name is left, compiled or not.
        let old: Vec<_> =
            after.keys().filter(|k| k.resref.to_string().starts_with("shop")).collect();
        assert!(old.is_empty(), "{old:?}");
        assert_eq!(after.get(&k("store", ResType::JUI)), module.get(&k("shop", ResType::JUI)));
        // The opener is the one Build would write for the new name, and the
        // settings say so; nothing is compiled, so the window is to be built.
        let w = parse(after.get(&k("store", ResType::JUI)).unwrap()).unwrap();
        let s = Settings::parse(after.get(&k("store", ResType::TXT)).unwrap()).unwrap();
        let opener = text(&after, "store_o", ResType::NSS);
        assert_eq!(opener, opener_source("store", &w, &s).unwrap());
        assert_eq!(s.opener_hash, Some(fingerprint(opener.as_bytes())));
        assert_eq!(s.event_hash, None);
        assert!(!after.contains(&k("store_e", ResType::NCS)));
        assert!(!after.contains(&k("store_e", ResType::NDB)));
        assert!(!crate::is_current(&after, "store", &w, &s));
        assert!(made.build);
    }

    #[test]
    fn the_event_script_keeps_its_code_in_a_frame_for_the_new_name() {
        let module = built();
        let made = rename(&module, "shop", "store", false).unwrap();
        let after = applied(&module, &made.command);
        let events = text(&after, "store_e", ResType::NSS);
        assert!(events.contains("#include \"store_o\"\n// mg:begin top\n"), "{events}");
        // The code written in the section is there, its call of the
        // opener's function by the new name; a string is as it was.
        assert!(events.contains("SendMessageToPC(oPlayer, \"Open_shop stays in a string\");"));
        assert!(
            events.contains(
                "NuiSetGroupLayout(oPlayer, nToken, \"pages\", Variant_store_details());"
            )
        );
        assert!(!events.contains("Variant_shop_"), "{events}");
        // Build takes it as the window's own: nothing was written outside
        // its sections.
        let s = Settings::parse(after.get(&k("store", ResType::TXT)).unwrap()).unwrap();
        let again = crate::rebuild_events("store", &s, Some(events.as_bytes())).unwrap();
        assert_eq!(again, events.as_bytes());
    }

    #[test]
    fn the_scripts_that_open_the_window_are_pointed_at_the_new_one() {
        let module = built();
        let made = rename(&module, "shop", "store", false).unwrap();
        let after = applied(&module, &made.command);
        assert_eq!(
            text(&after, "enter", ResType::NSS),
            "#include \"store_o\"\n// Open_shop opens it.\nvoid main()\n{\n    Open_store(GetEnteringObject());\n    json j = Initial_store_pages();\n    NuiFindWindow(OBJECT_SELF, \"shop\");\n}\n"
        );
        // Compiled, it would open the old window.
        assert!(!after.contains(&k("enter", ResType::NCS)));
        // A script that does not include the opener is not the window's
        // to change.
        assert_eq!(after.get(&k("other", ResType::NSS)), module.get(&k("other", ResType::NSS)));
        assert!(after.contains(&k("other", ResType::NCS)));
        // The include lines of the caller and the event script, the two
        // calls of the caller and the one of the event script.
        assert_eq!(made.in_scripts, 5);
        assert_eq!(made.spelled, ["shop", "shop_o", "shop_e"]);
    }

    #[test]
    fn strings_spelling_the_window_are_changed_when_asked() {
        let module = built();
        let made = rename(&module, "shop", "store", true).unwrap();
        let after = applied(&module, &made.command);
        let caller = text(&after, "enter", ResType::NSS);
        assert!(caller.contains("NuiFindWindow(OBJECT_SELF, \"store\");"), "{caller}");
        // (Another string, and a comment, are not the window's name.)
        assert!(text(&after, "store_e", ResType::NSS).contains("\"Open_shop stays in a string\""));
        assert!(caller.contains("// Open_shop opens it."));
        assert_eq!(made.in_scripts, 6);
    }

    #[test]
    fn a_name_without_room_for_the_scripts_is_refused() {
        let module = built();
        for name in ["fifteen_charnam", "sixteen_charname", "Store", ""] {
            let refused = rename(&module, "shop", name, false).unwrap_err();
            assert_eq!(refused, NuiError::Name { name: name.into() });
        }
    }

    #[test]
    fn a_name_whose_resources_are_taken_is_refused() {
        for (taken, ty) in [
            ("store", ResType::JUI),
            ("store", ResType::TXT),
            ("store_o", ResType::NSS),
            ("store_e", ResType::NSS),
            ("store_e", ResType::NCS),
            ("store_o", ResType::NDB),
        ] {
            let mut module = built();
            module.set(k(taken, ty), b"taken".to_vec());
            let refused = rename(&module, "shop", "store", false).unwrap_err();
            assert_eq!(refused, NuiError::Exists(k(taken, ty)));
        }
        let same = rename(&built(), "shop", "shop", false).unwrap_err();
        assert_eq!(same, NuiError::Exists(k("shop", ResType::JUI)));
        let none = rename(&built(), "nothing", "store", false).unwrap_err();
        assert_eq!(none, NuiError::Missing(k("nothing", ResType::JUI)));
    }

    #[test]
    fn a_text_that_is_not_the_windows_settings_stays_with_its_scripts() {
        let mut module = built();
        module.set(k("shop", ResType::TXT), b"Notes about the shop.".to_vec());
        let made = rename(&module, "shop", "store", false).unwrap();
        let moved: Vec<String> = made
            .command
            .edits
            .iter()
            .map(|e| match e {
                Edit::SetResource { key, data } => format!("{key} {}", data.is_some()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(moved, ["shop.jui false", "store.jui true"]);
        assert!(!made.build && made.spelled.is_empty() && made.in_scripts == 0);
    }

    #[test]
    fn a_window_not_built_yet_moves_its_settings_as_they_are() {
        let mut module = Module::new();
        let settings = b"{\n \"format\": \"moonglow.nui/1\",\n \"mine\": 1\n}".to_vec();
        module.set(k("shop", ResType::JUI), serde_json::to_vec(&window()).unwrap());
        module.set(k("shop", ResType::TXT), settings.clone());
        // A script written for it before its first Build.
        module.set(k("enter", ResType::NSS), b"#include \"shop_o\"\nvoid main() {}".to_vec());
        let made = rename(&module, "shop", "store", false).unwrap();
        let after = applied(&module, &made.command);
        assert_eq!(after.get(&k("store", ResType::TXT)), Some(settings.as_slice()));
        assert!(!after.contains(&k("store_o", ResType::NSS)));
        assert!(text(&after, "enter", ResType::NSS).starts_with("#include \"store_o\"\n"));
        assert_eq!(after.keys().count(), 3);
        assert!(!made.build);
    }

    #[test]
    fn scripts_written_by_hand_move_as_they_are_but_for_the_names() {
        let mut module = built();
        let opener = "// Mine.\nvoid Open_shop(object oPlayer) { /* Open_shop */ }\n";
        let events = "#include \"shop_o\"\nvoid main() { Open_shop(OBJECT_SELF); }\n";
        module.set(k("shop_o", ResType::NSS), opener.as_bytes().to_vec());
        module.set(k("shop_e", ResType::NSS), events.as_bytes().to_vec());
        let made = rename(&module, "shop", "store", false).unwrap();
        let after = applied(&module, &made.command);
        assert_eq!(
            text(&after, "store_o", ResType::NSS),
            "// Mine.\nvoid Open_store(object oPlayer) { /* Open_shop */ }\n"
        );
        assert_eq!(
            text(&after, "store_e", ResType::NSS),
            "#include \"store_o\"\nvoid main() { Open_store(OBJECT_SELF); }\n"
        );
        // Still one changed by hand, which Build does not write over.
        let before = Settings::parse(module.get(&k("shop", ResType::TXT)).unwrap()).unwrap();
        let s = Settings::parse(after.get(&k("store", ResType::TXT)).unwrap()).unwrap();
        assert_eq!(s.opener_hash, before.opener_hash);
        assert_ne!(
            s.opener_hash,
            Some(fingerprint(after.get(&k("store_o", ResType::NSS)).unwrap()))
        );
    }

    #[test]
    fn a_name_too_long_for_scripts_moves_to_one_that_has_room() {
        let mut module = Module::new();
        module.set(k("sixteen_charname", ResType::JUI), b"{}".to_vec());
        module.set(k("sixteen_charname", ResType::TXT), Settings::default().bytes());
        let made = rename(&module, "sixteen_charname", "short", false).unwrap();
        let after = applied(&module, &made.command);
        let keys: Vec<String> = after.keys().map(ToString::to_string).collect();
        assert!(keys.len() == 2 && keys.iter().all(|k| k.starts_with("short.")), "{keys:?}");
    }
}
