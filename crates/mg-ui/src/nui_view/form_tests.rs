//! The form's fields set their values as the other editors' do: one Undo
//! step an edit. No game install or GPU is needed.
use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};

fn key() -> ResKey {
    ResKey::parse("form_test", ResType::JUI).unwrap()
}

fn harness() -> Harness<'static, Moonglow> {
    let mut app = Moonglow::new(None, Box::new(crate::NoDialogs::default()));
    app.ws = Some(mg_edit::Workspace::new(mg_module::Module::new()));
    super::super::create(&mut app, "form_test");
    app.actions.clear();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_ui_state(
        |ui, app: &mut Moonglow| {
            // (As the app's frame begins: a press elsewhere leaves a field.)
            crate::widgets::text_begin(ui.ctx());
            super::super::ui(app, ui, Some(key()));
            app.run_actions();
        },
        app,
    );
    h.run();
    h
}

fn document(h: &Harness<'_, Moonglow>) -> Value {
    mg_nui::parse(h.state().ws.as_ref().unwrap().module.get(&key()).unwrap()).unwrap()
}

fn revision(h: &Harness<'_, Moonglow>) -> u64 {
    h.state().ws.as_ref().unwrap().revision()
}

fn undo(h: &mut Harness<'_, Moonglow>) {
    h.state_mut().ws.as_mut().unwrap().undo().unwrap();
    h.run();
}

/// Twenty letters typed are one change, made when the field is left, and
/// one Undo takes them back. The canvas shows them as they are typed.
#[test]
fn text_typed_in_a_field_is_one_undo_step() {
    let mut h = harness();
    h.get_by_label("Canvas Label · Label").click();
    h.run();
    let (before, at) = (document(&h), revision(&h));
    h.get_by_label("Text").click();
    h.run();
    h.get_by_label("Text").type_text(" of twenty letters");
    h.run();
    assert_eq!((document(&h), revision(&h)), (before.clone(), at), "nothing is set at a key");
    h.get_by_label("Canvas Label · Label of twenty letters");
    h.key_press(egui::Key::Tab);
    h.run();
    assert_eq!(revision(&h), at + 1, "one change");
    assert_eq!(document(&h)["root"]["children"][0]["value"], "Label of twenty letters");
    undo(&mut h);
    assert_eq!(document(&h), before);
    h.get_by_label("Canvas Label · Label");
}

/// What is typed in one control's field stays that control's when another
/// control is selected.
#[test]
fn text_typed_for_one_control_is_not_set_on_the_next() {
    let mut h = harness();
    h.get_by_label("Canvas Button · Close").click();
    h.run();
    let before = document(&h);
    h.get_by_label("Element ID").click();
    h.run();
    h.get_by_label("Element ID").type_text("_typed");
    h.run();
    h.get_by_label("Canvas Label · Label").click();
    h.run();
    let after = document(&h);
    assert_eq!(after["root"]["children"][0], before["root"]["children"][0], "the label's own");
    assert!(!after.to_string().contains("mg_close_typed_typed"));
}

/// A click in Layers on another control sets what was typed for the control
/// selected before it: the press leaves the field (`widgets::text_begin`),
/// which is still drawn then.
#[test]
fn text_typed_is_set_when_layers_selects_another_control() {
    let mut h = harness();
    h.get_by_label("Canvas Button · Close").click();
    h.run();
    h.get_by_label("Element ID").click();
    h.run();
    h.get_by_label("Element ID").type_text("_typed");
    h.run();
    let row = h.get_all_by_label("Label · Label").next().expect("its row in Layers").rect();
    h.hover_at(row.center());
    h.run();
    h.drag_at(row.center());
    h.drop_at(row.center());
    h.run();
    let after = document(&h);
    assert_eq!(after["root"]["children"][1]["id"], "mg_close_typed");
    assert_eq!(h.state().nui_views[&Some(key())].selected, "/root/children/0");
    assert!(after["root"]["children"][0]["id"].is_null(), "the label has no ID of its own");
}

/// A drag across a number is one change, made when it is let go; the number
/// it has reached shows meanwhile.
#[test]
fn a_number_dragged_is_one_undo_step() {
    let mut h = harness();
    h.get_by_label("Canvas Button · Close").click();
    h.run();
    let (before, at) = (document(&h), revision(&h));
    let height = before["root"]["children"][1]["height"].as_f64().expect("a button's height");
    let field = h
        .get_all_by_label("Height")
        .find(|n| n.accesskit_node().role() == egui::accesskit::Role::SpinButton)
        .expect("the height's number field")
        .rect()
        .center();
    h.hover_at(field);
    h.run();
    h.drag_at(field);
    h.run();
    for step in 1..=4 {
        h.hover_at(field + egui::vec2(10.0 * step as f32, 0.0));
        h.run();
        assert_eq!((document(&h), revision(&h)), (before.clone(), at), "nothing is set in a drag");
    }
    h.drop_at(field + egui::vec2(40.0, 0.0));
    h.run();
    assert_eq!(revision(&h), at + 1, "one change");
    let dragged = document(&h)["root"]["children"][1]["height"].as_f64().unwrap();
    assert!(dragged > height, "{dragged} from {height}");
    undo(&mut h);
    assert_eq!(document(&h), before);
}

/// Undo takes back the control that is selected: the next frame draws
/// without it, and nothing is selected.
#[test]
fn nui_selection_left_by_undo_is_cleared() {
    let key = Some(key());
    let mut h = harness();
    h.run();
    h.get_by_label("Toggle button").click();
    h.run();
    let selected = h.state().nui_views[&key].selected.clone();
    assert!(document(&h).pointer(&selected).is_some_and(|n| n["type"] != "window"));
    h.get_by_label("Draw layers").click();
    h.run();
    h.get_by_label("+ Draw primitive").scroll_to_me();
    h.run();
    h.get_by_label("+ Draw primitive").click();
    h.run();
    h.get_by_label("Rectangle").click();
    h.run();
    h.get_by_label("Draw layer 1 · Rectangle").click();
    h.run();
    assert!(h.state().nui_views[&key].draw_item.is_some());
    // The draw layer, then the control.
    for _ in 0..2 {
        h.state_mut().ws.as_mut().unwrap().undo().unwrap();
    }
    h.run();
    assert!(document(&h).pointer(&selected).is_none());
    let state = &h.state().nui_views[&key];
    assert_eq!(state.selected, "");
    assert!(state.selected_many.is_empty() && state.draw_item.is_none());
    // The pages that don't show the selection draw too.
    for page in ["Bindings", "Events & variants", "Design"] {
        h.get_by_label(page).click();
        h.run();
    }
}
