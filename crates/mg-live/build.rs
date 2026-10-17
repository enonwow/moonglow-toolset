//! Embed the complete Live source set, including newly added event helpers.
fn main() {
    let directory = std::path::Path::new("../../tools/live/nwn").canonicalize().unwrap();
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut files: Vec<_> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name().unwrap().to_string_lossy().starts_with("mglive_")
                && p.extension().is_some_and(|e| e == "nss")
        })
        .collect();
    files.sort();
    let mut text = String::from("pub const SOURCES: &[(&str, &str)] = &[\n");
    for path in files {
        text.push_str(&format!(
            "({:?}, include_str!({:?})),\n",
            path.file_name().unwrap().to_str().unwrap(),
            path.to_str().unwrap()
        ));
    }
    text.push_str("];\n");
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("live_sources.rs"),
        text,
    )
    .unwrap();
}
