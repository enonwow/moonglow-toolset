//! Diagnostics against real models: the decompiler's output is clean, and
//! the game's own ASCII models raise only what is really there.

use std::collections::BTreeMap;

use mg_core::ResType;
use mg_mdl::lint::Severity;
use mg_resman::{GameInstall, ResKey, ResMan};
use rayon::prelude::*;

fn game() -> Option<ResMan> {
    let root = mg_testkit::nwn_root()?;
    ResMan::for_game(&GameInstall::new(root, None, "en")).ok()
}

/// What the decompiler writes draws no warnings or errors (linted in
/// `mdl_decompile.rs`'s sweep, where each model is decompiled).
#[test]
fn decompiled_models_are_clean() {
    let root = mg_testkit::corpus!();
    let noisy = &crate::mdl_decompile::swept(&root).noisy;
    for n in noisy.iter().take(20) {
        eprintln!("{n}");
    }
    assert!(noisy.is_empty(), "{} decompiled models draw diagnostics", noisy.len());
}

/// A census of what the game's own ASCII models draw (printed; it asserts
/// nothing).
#[test]
#[ignore = "a census, printed: run by hand"]
fn game_ascii_census() {
    let _ = mg_testkit::corpus!();
    let rm = game().unwrap();
    let names = rm.list(ResType::MDL);
    let found: Vec<(String, Severity, String)> = names
        .par_iter()
        .flat_map_iter(|name| {
            let data = rm.get(&ResKey::new(*name, ResType::MDL)).ok();
            let out: Vec<(String, Severity, String)> = match data {
                Some(d) if !mg_mdl::is_binary(&d) => {
                    let text = String::from_utf8_lossy(&d);
                    mg_mdl::lint::check(&text)
                        .into_iter()
                        .map(|x| (name.to_string(), x.severity, x.message))
                        .collect()
                }
                _ => Vec::new(),
            };
            out
        })
        .collect();
    let mut by_kind: BTreeMap<(Severity, String), (usize, String)> = BTreeMap::new();
    for (name, sev, msg) in &found {
        // Group messages by their wording without names and numbers.
        let shape: String = msg
            .split_whitespace()
            .map(|w| if w.chars().any(|c| c.is_ascii_digit() || c == '_') { "#" } else { w })
            .collect::<Vec<_>>()
            .join(" ");
        let e = by_kind.entry((*sev, shape)).or_insert((0, name.clone()));
        e.0 += 1;
    }
    for ((sev, shape), (count, example)) in &by_kind {
        eprintln!("{sev:?} {count:6} {shape}   (e.g. {example})");
    }
}
