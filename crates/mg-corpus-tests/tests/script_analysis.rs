//! The script analysis' locals and parameters against the compiler: for
//! every compiled function of every script the game and the shipped modules
//! contain, the names the compiler's debug output (NDB) gives its variables
//! are the parameters and locals the analysis finds in its body. Checked in
//! `script_frontend.rs`'s sweep, which compiles each script as this needs
//! it (unoptimized, with debug output).

use std::collections::BTreeMap;

use mg_script::analysis::Index;

/// Each compiled function's variable names (parameters and locals, sorted),
/// from the NDB: variables belong to the function whose code holds their
/// first instruction.
fn ndb_locals(ndb: &[u8]) -> BTreeMap<String, Vec<String>> {
    let text = String::from_utf8_lossy(ndb);
    let hex = |s: &str| u32::from_str_radix(s, 16).unwrap_or(u32::MAX);
    let mut funcs: Vec<(u32, u32, String)> = Vec::new();
    let mut vars: Vec<(u32, String)> = Vec::new();
    for line in text.lines() {
        let w: Vec<&str> = line.split(' ').collect();
        match w.as_slice() {
            ["f", start, end, _, _, name] if *start != "ffffffff" => {
                funcs.push((hex(start), hex(end), name.to_string()))
            }
            ["v", start, end, _, _, name] if *end != "ffffffff" && !name.starts_with('#') => {
                vars.push((hex(start), name.to_string()))
            }
            _ => {}
        }
    }
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (start, end, name) in &funcs {
        if name.starts_with('#') {
            continue;
        }
        let mut names: Vec<String> =
            vars.iter().filter(|(s, _)| s >= start && s < end).map(|(_, n)| n.clone()).collect();
        names.sort();
        out.insert(name.clone(), names);
    }
    out
}

/// Compares the locals of each function `name`.nss compiled to (its `ndb`)
/// with the analysis'; the differences go to `fails`. Returns how many
/// functions were compared.
pub(crate) fn check_locals(
    label: &str,
    name: &str,
    ndb: &[u8],
    index: &mut Index,
    sources: &mut dyn mg_script::analysis::Sources,
    fails: &mut Vec<String>,
) -> usize {
    let mut compared = 0;
    for (func, theirs) in ndb_locals(ndb) {
        let Some(ours) = index.function_locals(name, &func, sources) else {
            fails.push(format!("{label}: {name}.nss: {func} not found"));
            continue;
        };
        let mut ours: Vec<String> = ours.into_iter().map(|(n, _)| n).collect();
        ours.sort();
        compared += 1;
        if ours != theirs {
            fails.push(format!("{label}: {name}.nss: {func}: ours {ours:?}, NDB {theirs:?}"));
        }
    }
    compared
}
