//! Search module and standard item palettes; icons resolve through the same stack.
use crate::Moonglow;
use mg_core::{ResRef, ResType};
use mg_module::palette::{BlueprintKind, PaletteNode};
use mg_resman::ResKey;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
struct Item {
    resref: ResRef,
    name: String,
    category: String,
    source: &'static str,
}

#[derive(Debug, Clone)]
struct Branch {
    name: String,
    children: Vec<Branch>,
    items: Vec<usize>,
}

#[derive(Debug, Default)]
pub(super) struct Picker {
    pub open: bool,
    pub search: String,
    items: Vec<Item>,
    roots: Vec<Branch>,
    icons: HashMap<ResRef, Vec<crate::images::Picture>>,
}

impl Picker {
    pub(super) fn load(&mut self, app: &mut Moonglow) {
        self.items.clear();
        self.icons.clear();
        self.roots.clear();
        let custom = crate::palette_view::palette(app, BlueprintKind::Item, true);
        let standard = crate::palette_view::palette(app, BlueprintKind::Item, false);
        let Some(game) = &app.game else { return };
        let overrides: HashSet<_> = custom
            .as_ref()
            .into_iter()
            .flat_map(|p| p.blueprints())
            .map(|(_, b)| b.resref)
            .collect();
        fn branch(
            node: &PaletteNode,
            path: &str,
            source: &'static str,
            game: &mg_rules::GameData,
            overrides: &HashSet<ResRef>,
            items: &mut Vec<Item>,
        ) -> Branch {
            let name = node.name.text(game);
            let path = if path.is_empty() { name.clone() } else { format!("{path} / {name}") };
            let children = node
                .children
                .iter()
                .map(|n| branch(n, &path, source, game, overrides, items))
                .collect();
            let mut here = Vec::new();
            for bp in &node.blueprints {
                if source == "Standard" && overrides.contains(&bp.resref) {
                    continue;
                }
                here.push(items.len());
                items.push(Item {
                    resref: bp.resref,
                    name: bp.name.text(game),
                    category: path.clone(),
                    source,
                });
            }
            Branch { name, children, items: here }
        }
        // Use precisely the same cached Palette nodes as the Palettes window.
        // No category inference, alphabetic re-sorting or path-string splitting.
        for (name, palette) in [("Standard", standard), ("Module", custom)] {
            if let Some(palette) = palette {
                self.roots.push(Branch {
                    name: name.into(),
                    children: palette
                        .nodes
                        .iter()
                        .map(|n| branch(n, "", name, game, &overrides, &mut self.items))
                        .collect(),
                    items: Vec::new(),
                });
            }
        }
    }

    pub(super) fn ui(
        &mut self,
        app: &mut Moonglow,
        ctx: &egui::Context,
        target: &str,
        resref: &mut String,
        ready: bool,
    ) -> Option<String> {
        if !self.open {
            return None;
        }
        let mut open = true;
        let mut give = None;
        egui::Window::new(format!("Give item to {target}"))
            .id(egui::Id::new("live-item-picker"))
            .open(&mut open)
            .default_width(570.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.weak("Double-click an item to give it now, or select it and press Give.");
                let search_label = ui.label("Search items");
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search item name, resref or category")
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(search_label.id);
                let words: Vec<_> = self.search.split_whitespace().map(str::to_lowercase).collect();
                let filtered: Vec<_> = self
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        let text = format!(
                            "{} {} {} {}",
                            item.name, item.resref, item.category, item.source
                        )
                        .to_lowercase();
                        words.iter().all(|word| text.contains(word))
                    })
                    .map(|(i, _)| i)
                    .collect();
                ui.weak(format!("{} items · module and standard palettes", filtered.len()));
                if filtered.is_empty() {
                    ui.label(if self.items.is_empty() { "No item palettes are loaded. Open a module with game resources." } else { "No items match this search. Clear or shorten the search text." });
                }
                let matching: HashSet<_> = filtered.into_iter().collect();
                let searching = !words.is_empty();
                egui::ScrollArea::vertical()
                    .id_salt("live-item-picker-list")
                    .max_height(360.)
                    .show(ui, |ui| {
                        for (index, root) in self.roots.iter().enumerate() {
                            draw_branch(ui, app, root, &[index], &self.items, &mut self.icons, &matching, searching, resref, ready, &mut give);
                        }
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let name = self
                        .items
                        .iter()
                        .find(|i| i.resref.as_str() == Some(resref.as_str()))
                        .map(|i| i.name.as_str())
                        .unwrap_or(resref);
                    ui.label(if name.is_empty() { "Select an item" } else { name });
                    if ui
                        .add_enabled(ready && !resref.is_empty(), egui::Button::new("Give"))
                        .on_disabled_hover_text(if !ready { "The Inventory window explains the pending request or required player permission." } else { "Select an item first." })
                        .on_hover_text("Give this item to the creature named in the window title.")
                        .clicked()
                    {
                        give = Some(resref.clone());
                    }
                });
                ui.collapsing("Advanced: item resref", |ui| {
                    ui.add(
                        egui::TextEdit::singleline(resref)
                            .char_limit(16)
                            .hint_text("Known server resref"),
                    );
                    ui.weak("For resources not present in the loaded module or standard palette.");
                });
            });
        self.open = open && give.is_none();
        give
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_branch(
    ui: &mut egui::Ui,
    app: &mut Moonglow,
    branch: &Branch,
    path: &[usize],
    items: &[Item],
    icons: &mut HashMap<ResRef, Vec<crate::images::Picture>>,
    matching: &HashSet<usize>,
    searching: bool,
    resref: &mut String,
    ready: bool,
    give: &mut Option<String>,
) {
    fn shown(b: &Branch, matching: &HashSet<usize>) -> bool {
        b.items.iter().any(|i| matching.contains(i))
            || b.children.iter().any(|c| shown(c, matching))
    }
    if searching && !shown(branch, matching) {
        return;
    }
    egui::CollapsingHeader::new(&branch.name)
        .id_salt(("live-item-category", searching, path))
        .open(searching.then_some(true))
        .show(ui, |ui| {
            for (index, child) in branch.children.iter().enumerate() {
                let mut path = path.to_vec();
                path.push(index);
                draw_branch(
                    ui, app, child, &path, items, icons, matching, searching, resref, ready, give,
                );
            }
            for index in branch.items.iter().filter(|i| matching.contains(i)) {
                let item = &items[*index];
                // Closed branches do no work. Offscreen rows reserve layout only;
                // resolving thousands of blueprint icons would defeat the tree.
                let rect = egui::Rect::from_min_size(
                    ui.next_widget_position(),
                    egui::vec2(ui.available_width(), 52.),
                );
                if !ui.is_rect_visible(rect) {
                    ui.allocate_space(rect.size());
                    continue;
                }
                let icons = icons.entry(item.resref).or_insert_with(|| {
                    crate::area_tools::blueprint(app, ResKey::new(item.resref, ResType::UTI))
                        .map(|g| app.item_icon(ui.ctx(), &g.root))
                        .unwrap_or_default()
                });
                ui.horizontal(|ui| {
                    crate::images::icon_box(ui, icons, egui::vec2(44., 44.), &item.name);
                    ui.vertical(|ui| {
                        let response = ui.selectable_label(
                            item.resref.as_str() == Some(resref.as_str()),
                            format!("{} ({})", item.name, item.resref),
                        );
                        if response.clicked() || response.double_clicked() {
                            *resref = item.resref.to_string();
                        }
                        if response.double_clicked() && ready {
                            *give = Some(item.resref.to_string());
                        }
                        ui.add(egui::Label::new(&item.category).truncate())
                            .on_hover_text(&item.category);
                    });
                });
            }
        });
}
