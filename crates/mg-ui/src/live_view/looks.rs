//! Report-to-preview reconciliation and look/effect cache refresh.
use super::*;

pub(super) fn rebuild(
    app: &mut Moonglow,
    view: &mut AreaView,
    report: &AreaLive,
    moving: bool,
    key: LiveDrawn,
    now: f64,
) -> Option<LiveFrame> {
    #[cfg(feature = "measure")]
    let _rebuild = crate::measure::span("live.rebuild");
    let live = (*report).clone();
    let model = view.model.as_ref()?;
    let templates: Vec<_> = model
        .objects
        .iter()
        .map(|o| o.template.map(|r| r.to_string()).unwrap_or_default())
        .collect();
    let git: Vec<_> = model
        .objects
        .iter()
        .zip(&templates)
        .map(|(o, template)| GitObject {
            kind: o.kind,
            template,
            position: o.position,
            is_static: o.is_static,
        })
        .collect();
    #[cfg(feature = "measure")]
    let reconcile_measure = crate::measure::span("live.reconcile");
    let mut reconciled = crate::live::reconcile(&git, &live);
    #[cfg(feature = "measure")]
    drop(reconcile_measure);
    #[cfg(feature = "measure")]
    let _looks_measure = crate::measure::span("live.looks");
    view.live.reconciliations += 1;
    // Reported props replace their matched GIT drawing, retaining its exact look.
    let matched: HashMap<_, _> = reconciled
        .matched
        .iter()
        .enumerate()
        .filter_map(|(i, pair)| pair.map(|pair| (pair, model.objects[i].clone())))
        .collect();
    for (i, pair) in reconciled.matched.iter().enumerate() {
        if let Some(pair) = pair {
            reconciled.hidden[i] = true;
            reconciled.extra.push(*pair);
        }
    }

    let hidden =
        reconciled.hidden.iter().enumerate().filter_map(|(i, hide)| hide.then_some(i)).collect();
    let revision = app.ws.as_ref().map(|ws| ws.revision());
    if revision != view.live.revision {
        view.live.looks.clear();
        view.live.player_looks.clear();
        view.live.effect_rows.clear();
        view.live.revision = revision;
    }
    if view.live.selected.as_ref().is_some_and(|id| live.find(id).is_none()) {
        view.live.selected = None;
    }
    view.live.player_looks.retain(|id, _| {
        live.find(id).is_some_and(|(kind, o)| kind == ObjectKind::Creature && o.look.is_some())
    });
    let mut markers: HashSet<String> = live
        .all()
        .filter(|(kind, o)| {
            !matches!(
                kind,
                ObjectKind::Creature | ObjectKind::Placeable | ObjectKind::Door | ObjectKind::Item
            ) && visible(view, &live, *kind, o)
        })
        .map(|(_, o)| o.id.clone())
        .collect();
    let mut extras = Vec::new();
    for (kind, index) in reconciled.extra {
        let list = match kind {
            ObjectKind::Creature => &live.creatures,
            ObjectKind::Placeable => &live.placeables,
            ObjectKind::Door => &live.doors,
            ObjectKind::Item => &live.items,
            _ => continue,
        };
        let o = &list[index];
        if !visible(view, &live, kind, o) {
            continue;
        }
        markers.insert(o.id.clone());
        if o.dm {
            continue;
        }
        // NPC equipment is authoritative too; an empty list must not restore
        // equipment from the module blueprint. Hash changes rebuild the preview.
        if kind == ObjectKind::Creature && (o.pc || o.look.is_some()) {
            if let Some(raw) = &o.look {
                let changed = view.live.player_looks.get(&o.id).is_none_or(|(hash, old, _)| {
                    *hash != o.look_hash || (hash.is_none() && old != raw)
                });
                if changed {
                    let parsed = read_player_look(app.game.as_deref()?, raw);
                    if let Err(error) = &parsed
                        && view.live.bad_looks.insert(o.id.clone())
                    {
                        app.log.warn(format!("Live: creature {} look: {error}", o.id));
                    }
                    view.live
                        .player_looks
                        .insert(o.id.clone(), (o.look_hash, raw.clone(), parsed.ok()));
                }
                if let Some((_, _, Some(look))) = view.live.player_looks.get(&o.id) {
                    let mut look = look.object.clone();
                    look.position = o.position();
                    look.rotation = o.facing.to_radians() - FRAC_PI_2;
                    extras.push(LiveExtra {
                        id: o.id.clone(),
                        object: look,
                        dead: o.dead,
                        combat: o.combat,
                        size: view.live.player_looks[&o.id].2.as_ref().unwrap().size,
                        effect_models: 0,
                        source: index,
                        animates: false,
                    });
                }
            } else {
                view.live.player_looks.remove(&o.id);
            }
            continue;
        }
        let appearance =
            matches!(kind, ObjectKind::Placeable | ObjectKind::Door).then_some(o.app).flatten();
        let cache_key = (kind, o.resref.to_ascii_lowercase(), appearance);
        if (appearance.is_some() || !matched.contains_key(&(kind, index)))
            && !view.live.looks.contains_key(&cache_key)
        {
            let look = (|| {
                let at = mg_module::instances::Placement { position: [0.; 3], rotation: 0. };
                let item = ResKey::parse(&o.resref, kind.restype())
                    .and_then(|key| area_view::instance_of(app, key, at, &[]).ok().flatten());
                let mut item = item.or_else(|| appearance.map(|_| mg_gff::Struct::new(0)))?;
                if let Some(row) = appearance {
                    use mg_gff::Value;
                    if kind == ObjectKind::Placeable {
                        item.set("Appearance", Value::Dword(u32::from(row)));
                    } else {
                        item.set("Appearance", Value::Dword(0));
                        item.set("GenericType_New", Value::Dword(u32::from(row)));
                        item.set("GenericType", Value::Byte(row.min(255) as u8));
                    }
                }
                let game = app.game.as_deref()?;
                Some(LiveLook {
                    object: AreaObject::read(game, kind, usize::MAX, &item),
                    size: crate::live_effects::size(game, &item),
                })
            })();
            view.live.looks.insert(cache_key.clone(), look);
        }
        let cached = matched
            .get(&(kind, index))
            .filter(|_| appearance.is_none())
            .map(|object| LiveLook { object: object.clone(), size: mg_preview::vfx::Size::Medium })
            .or_else(|| view.live.looks.get(&cache_key).cloned().flatten());
        if let Some(cached) = cached {
            let mut look = cached.object;
            if let Some(is_static) = o.is_static {
                look.is_static = is_static;
            }
            look.position = o.position();
            look.rotation = o.facing.to_radians() - FRAC_PI_2;
            if matches!(kind, ObjectKind::Placeable | ObjectKind::Door) {
                let vt = o.vt.clone().unwrap_or_default();
                look.visual = Some(mg_area::VisualTransform {
                    scale: glam::Vec3::splat(vt.scale),
                    rotate: glam::vec3(vt.rotate.x, vt.rotate.y, vt.rotate.z),
                    translate: glam::vec3(vt.translate.x, vt.translate.y, vt.translate.z),
                });
            }
            extras.push(LiveExtra {
                id: o.id.clone(),
                object: look,
                dead: kind == ObjectKind::Creature && o.dead,
                combat: o.combat,
                size: cached.size,
                effect_models: 0,
                source: index,
                animates: false,
            });
        }
    }
    let palette = Palette::for_app(app);
    let mut effects: HashMap<String, Vec<Effect>> = HashMap::new();
    if view.live.effects != EffectsMode::Off {
        for o in &live.creatures {
            if !visible(view, &live, ObjectKind::Creature, o) || o.vfx.is_empty() {
                continue;
            }
            let rows = o
                .vfx
                .iter()
                .take(16)
                .map(|&row| {
                    view.live
                        .effect_rows
                        .entry(row)
                        .or_insert_with(|| {
                            Effect::read(app.game.as_deref().expect("loaded area"), row)
                        })
                        .clone()
                })
                .collect();
            effects.insert(o.id.clone(), rows);
        }
    }
    view.effect_models_shown = 0;
    if view.live.effects == EffectsMode::Full {
        let eye = view.camera().map_or(glam::Vec3::ZERO, |camera| camera.eye);
        extras.sort_by(|a, b| {
            a.object
                .position
                .distance_squared(eye)
                .total_cmp(&b.object.position.distance_squared(eye))
        });
        for extra in &mut extras {
            let Some(preview) = extra.object.preview.take() else { continue };
            let models: Vec<_> = effects
                .get(&extra.id)
                .into_iter()
                .flatten()
                .flat_map(|effect| effect.models(extra.size))
                .take(crate::live_effects::MAX_MODELS - view.effect_models_shown)
                .collect();
            extra.effect_models = models.len();
            view.effect_models_shown += models.len();
            extra.object.preview = Some(mg_preview::vfx::with_models(preview, &models));
        }
    }
    let mut frame = LiveFrame { live, hidden, extras, markers, palette, effects, moving, key };
    apply_poses(&view.live, &mut frame, now);
    Some(frame)
}

pub(super) fn read_player_look(
    game: &mg_rules::GameData,
    look: &serde_json::Value,
) -> Result<LiveLook, String> {
    let visual = crate::live_look::visual_json(look);
    let gff = mg_gff::from_json(&visual, game.codepage()).map_err(|e| e.to_string())?;
    let object = AreaObject::read(game, ObjectKind::Creature, usize::MAX, &gff.root);
    if object.preview.is_none() {
        return Err(object.problem.clone().unwrap_or_else(|| "No creature preview".into()));
    }
    Ok(LiveLook { object, size: crate::live_effects::size(game, &gff.root) })
}
