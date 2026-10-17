//! Shared creature animation chooser for single targets and selections.
use crate::live::Op;

#[derive(Clone)]
struct Draft {
    animation: i32,
    custom: bool,
    name: String,
    looping: bool,
    speed: f32,
    seconds: f32,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            animation: 0,
            custom: false,
            name: String::new(),
            looping: false,
            speed: 1.,
            seconds: 30.,
        }
    }
}

fn constants(source: &str) -> Vec<(String, i32)> {
    let mut result: Vec<_> = source
        .lines()
        .filter_map(|line| {
            let (declaration, value) = line.split_once('=')?;
            let name = declaration.split_whitespace().last()?;
            if !name.starts_with("ANIMATION_") {
                return None;
            }
            let value = value.split(';').next()?.trim().parse::<i32>().ok()?;
            ((0..=92).contains(&value) || (100..=116).contains(&value))
                .then(|| (name.to_string(), value))
        })
        .collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result.dedup();
    result
}

pub(crate) fn menu(
    ui: &mut egui::Ui,
    game: Option<&mg_rules::GameData>,
    target: &str,
) -> Option<Op> {
    let mut chosen = None;
    crate::live_menu::values(ui, "Animation", |ui| {
        let id = ui.make_persistent_id(("creature-animation", target));
        let mut draft: Draft = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        let cache =
            egui::Id::new(("live-animation-constants", game.map(|g| g as *const _ as usize)));
        let names: std::sync::Arc<Vec<(String, i32)>> = ui.data_mut(|data| {
            data.get_temp(cache).unwrap_or_else(|| {
                let rows = game
                    .and_then(|g| g.resman.get_named("nwscript", mg_core::ResType::NSS).ok())
                    .map(|bytes| constants(&String::from_utf8_lossy(&bytes)))
                    .unwrap_or_default();
                let rows = std::sync::Arc::new(rows);
                data.insert_temp(cache, rows.clone());
                rows
            })
        });
        ui.set_min_width(300.);
        ui.checkbox(&mut draft.custom, "Custom animation");
        ui.add_enabled_ui(!draft.custom, |ui| {
            ui.label(
                names
                    .iter()
                    .find(|(_, value)| *value == draft.animation)
                    .map_or("Choose an ANIMATION_* constant", |(name, _)| name.as_str()),
            );
            ui.collapsing("ANIMATION_* constants", |ui| {
                egui::ScrollArea::vertical().max_height(160.).show(ui, |ui| {
                    for (name, value) in names.iter() {
                        ui.selectable_value(&mut draft.animation, *value, name);
                    }
                });
            });
        });
        ui.add_enabled_ui(draft.custom, |ui| {
            let label = ui.label("Custom animation name");
            ui.text_edit_singleline(&mut draft.name).labelled_by(label.id);
        });
        ui.checkbox(&mut draft.looping, "Loop animation");
        ui.horizontal(|ui| {
            let label = ui.label("Animation speed");
            ui.add(egui::DragValue::new(&mut draft.speed).range(0.1..=10.).speed(0.1))
                .labelled_by(label.id);
        });
        ui.horizontal(|ui| {
            let label = ui.label("Animation seconds");
            ui.add(egui::DragValue::new(&mut draft.seconds).range(0.1..=300.).speed(0.5))
                .labelled_by(label.id);
        });
        let op = Op::PlayCreatureAnimation {
            target: target.into(),
            animation: if draft.custom {
                mg_live::store::CreatureAnimation::Name(draft.name.trim().into())
            } else {
                mg_live::store::CreatureAnimation::Id(draft.animation)
            },
            looping: draft.looping,
            speed: draft.speed,
            seconds: draft.seconds,
        };
        let error = crate::live::Command { id: String::new(), area: "live".into(), op: op.clone() }
            .validate()
            .err();
        if ui
            .add_enabled(error.is_none(), egui::Button::new("Play animation"))
            .on_disabled_hover_text(error.as_deref().unwrap_or_default())
            .clicked()
        {
            chosen = Some(op);
        }
        if let Some(error) = error {
            ui.weak(error);
        }
        ui.weak("Looping constants use seconds. One-shots play once, or repeat with Loop.");
        ui.data_mut(|d| d.insert_temp(id, draft));
    });
    chosen
}

#[cfg(test)]
mod tests {
    #[test]
    fn animation_names_come_from_the_loaded_script_constants() {
        assert_eq!(
            super::constants(
                "int ANIMATION_LOOPING_PAUSE = 0; // idle\nconst int ANIMATION_FIREFORGET_BOW=105;\nint ACTION_ATTACK=3;\nint ANIMATION_DOOR_OPEN1=10000;"
            ),
            vec![("ANIMATION_FIREFORGET_BOW".into(), 105), ("ANIMATION_LOOPING_PAUSE".into(), 0)]
        );
    }
}
