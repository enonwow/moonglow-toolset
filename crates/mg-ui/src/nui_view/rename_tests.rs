//! Renaming a NUI window from the app (Rename… on its row or its tab): its
//! settings and its scripts go with it, and the open tabs follow. No game
//! install or GPU is needed.
use super::*;
use egui_kittest::Harness;
use mg_core::ResRef;

fn key(name: &str, ty: ResType) -> ResKey {
    ResKey::parse(name, ty).unwrap()
}

fn text(app: &Moonglow, name: &str, ty: ResType) -> String {
    crate::text::decode(app.ws.as_ref().unwrap().module.get(&key(name, ty)).unwrap())
}

/// Every resource of the module, with its bytes.
fn resources(app: &Moonglow) -> Vec<(ResKey, Vec<u8>)> {
    let module = &app.ws.as_ref().unwrap().module;
    let mut all: Vec<_> = module.keys().map(|k| (*k, module.get(k).unwrap().to_vec())).collect();
    all.sort();
    all
}

/// The app with the window `shop` as Build leaves it (what is compiled
/// stands in), code written in the Close button's handler, and a script of
/// the module's that opens the window.
fn app() -> Moonglow {
    let mut app = Moonglow::new(None, Box::new(crate::NoDialogs::default()));
    app.ws = Some(mg_edit::Workspace::new(mg_module::Module::new()));
    create(&mut app, "shop");
    app.actions.clear();
    let module = &mut app.ws.as_mut().unwrap().module;
    let window = mg_nui::parse(module.get(&key("shop", ResType::JUI)).unwrap()).unwrap();
    let settings = Settings::parse(module.get(&key("shop", ResType::TXT)).unwrap()).unwrap();
    let opener = mg_nui::opener_source("shop", &window, &settings).unwrap();
    let events = mg_nui::event_source("shop", &settings).unwrap();
    let events =
        mg_nui::insert_code(&events, "click mg_close", "        SpeakString(\"Bye\");\n").unwrap();
    module.set(key("shop_o", ResType::NSS), opener.into_bytes());
    module.set(key("shop_e", ResType::NSS), events.into_bytes());
    module.set(key("shop_e", ResType::NCS), b"NCS V1.0B".to_vec());
    module.set(
        key("enter", ResType::NSS),
        b"#include \"shop_o\"\nvoid main()\n{\n    Open_shop(GetEnteringObject());\n}\n".to_vec(),
    );
    module.set(key("enter", ResType::NCS), b"NCS V1.0B".to_vec());
    app
}

/// A window that draws the script whose tab is open, as the dock would.
fn script_harness(app: Moonglow) -> Harness<'static, Moonglow> {
    Harness::builder().with_size(egui::vec2(900.0, 600.0)).build_ui_state(
        |ui, app: &mut Moonglow| {
            let open = app.dock.iter_all_tabs().find_map(|(_, tab)| match tab {
                Tab::Script(k) => Some(*k),
                _ => None,
            });
            if let Some(k) = open {
                crate::script_view::ui(app, ui, k);
            }
            app.run_actions();
        },
        app,
    )
}

#[test]
fn nui_rename_takes_the_settings_and_the_scripts_and_one_undo_brings_them_back() {
    let mut h = script_harness(app());
    let (old_script, new_script) = (key("shop_e", ResType::NSS), key("store_e", ResType::NSS));
    h.state_mut().run(Action::OpenTab(Tab::Nui(key("shop", ResType::JUI))));
    h.state_mut().run(Action::OpenTab(Tab::Script(old_script)));
    h.run();
    assert!(h.state().scripts.contains_key(&old_script));
    let before = resources(h.state());
    let step = |app: &Moonglow| app.ws.as_ref().unwrap().can_undo().map(str::to_owned);
    let last = step(h.state());

    let to = ResRef::from_str("store").unwrap();
    h.state_mut().rename_resource(key("shop", ResType::JUI), to, false);
    h.run();
    let app = h.state();
    // Nothing is left under the old name, compiled or not.
    let left: Vec<String> = resources(app)
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| k.starts_with("shop"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    let module = &app.ws.as_ref().unwrap().module;
    for (name, ty) in [("store", ResType::JUI), ("store", ResType::TXT), ("store_o", ResType::NSS)]
    {
        assert!(module.contains(&key(name, ty)), "{name}");
    }
    assert!(!module.contains(&key("store_e", ResType::NCS)));
    // The handler's code is in the new event script, in a frame for the new name.
    let events = text(app, "store_e", ResType::NSS);
    assert!(events.contains("        // mg:begin click mg_close\n"), "{events}");
    assert!(events.contains("        SpeakString(\"Bye\");\n        // mg:end\n"), "{events}");
    assert!(events.contains("#include \"store_o\"\n"), "{events}");
    assert!(text(app, "store_o", ResType::NSS).contains("void Open_store(object oPlayer)"));
    // The script that opens the window includes and calls the new one, and
    // is to be compiled again.
    assert_eq!(
        text(app, "enter", ResType::NSS),
        "#include \"store_o\"\nvoid main()\n{\n    Open_store(GetEnteringObject());\n}\n"
    );
    assert!(!module.contains(&key("enter", ResType::NCS)));
    // The tabs follow, and the script's buffer shows the script as it is now.
    assert!(app.dock.find_tab(&Tab::Nui(key("store", ResType::JUI))).is_some());
    assert!(app.dock.find_tab(&Tab::Nui(key("shop", ResType::JUI))).is_none());
    assert!(app.dock.find_tab(&Tab::Script(new_script)).is_some());
    assert!(app.dock.find_tab(&Tab::Script(old_script)).is_none());
    assert!(!app.scripts.contains_key(&old_script));
    assert_eq!(app.scripts[&new_script].text, events);
    assert!(!app.scripts[&new_script].is_dirty());
    assert!(
        app.log.entries.iter().any(|(_, said)| said.starts_with("Renamed shop.jui to store.jui"))
    );

    // One Undo: every resource as it was, byte for byte.
    assert_eq!(step(h.state()).as_deref(), Some("Rename shop.jui to store.jui"));
    h.state_mut().ws.as_mut().unwrap().undo().unwrap();
    assert_eq!(resources(h.state()), before);
    assert_eq!(step(h.state()), last);
}

#[test]
fn nui_rename_is_refused_for_a_name_without_room_for_the_scripts() {
    let mut app = app();
    let before = resources(&app);
    let revision = app.ws.as_ref().unwrap().revision();
    // Fifteen characters: `_o` and `_e` would not fit.
    let to = ResRef::from_str("fifteen_charnam").unwrap();
    app.rename_resource(key("shop", ResType::JUI), to, false);
    assert_eq!(resources(&app), before);
    assert_eq!(app.ws.as_ref().unwrap().revision(), revision);
    let said = &app.log.entries.last().unwrap().1;
    assert!(said.starts_with("Rename: \"fifteen_charnam\": use 1–14 lowercase"), "{said}");
    // So is a name the module has a script of the window's under.
    app.ws.as_mut().unwrap().module.set(key("store_e", ResType::NSS), b"void main() {}".to_vec());
    let before = resources(&app);
    app.rename_resource(key("shop", ResType::JUI), ResRef::from_str("store").unwrap(), false);
    assert_eq!(resources(&app), before);
    let said = &app.log.entries.last().unwrap().1;
    assert_eq!(said, "Rename: Resource already exists: store_e.nss");
}

#[test]
fn nui_rename_leaves_a_text_that_is_not_the_windows_settings() {
    let mut app = app();
    let module = &mut app.ws.as_mut().unwrap().module;
    module.set(key("notes", ResType::JUI), b"{}".to_vec());
    module.set(key("notes", ResType::TXT), b"Not settings.".to_vec());
    module.set(key("notes_e", ResType::NSS), b"void main() {}".to_vec());
    let before = resources(&app).len();
    app.rename_resource(key("notes", ResType::JUI), ResRef::from_str("pages").unwrap(), false);
    let module = &app.ws.as_ref().unwrap().module;
    assert!(module.contains(&key("pages", ResType::JUI)));
    assert!(!module.contains(&key("notes", ResType::JUI)));
    assert_eq!(module.get(&key("notes", ResType::TXT)), Some(b"Not settings.".as_slice()));
    assert!(module.contains(&key("notes_e", ResType::NSS)));
    assert!(!module.contains(&key("pages", ResType::TXT)));
    assert_eq!(resources(&app).len(), before);
}
