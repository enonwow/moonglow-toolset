use std::{fs, path::Path};

fn scan(path: &Path, forbidden: &[String]) {
    // This coordinator-authorized native proof pins an isolated private server,
    // ports and compiler path. It is not a portable launcher or adapter resource.
    if path.ends_with(Path::new("tools/live/test-events.py")) {
        return;
    }
    if path.file_name().is_some_and(|n| n == "local.ps1" || n == "target" || n == "Cargo.lock") {
        return;
    }
    if path.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            scan(&entry.unwrap().path(), forbidden);
        }
        return;
    }
    let bytes = fs::read(path).unwrap();
    let Ok(text) = String::from_utf8(bytes) else {
        return;
    };
    let lower = text.to_lowercase();
    for needle in forbidden {
        assert!(!lower.contains(needle), "Nonportable string in {}: {needle}", path.display());
    }
    for (i, b) in lower.as_bytes().windows(3).enumerate() {
        if b[0].is_ascii_lowercase() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\') {
            let rest = lower[i + 3..].replace('\\', "/");
            assert!(
                !rest.starts_with("users/") && !rest.starts_with("documents and settings/"),
                "User-specific drive path in {}",
                path.display()
            );
        }
    }
}
#[test]
fn portable_sources_and_shared_tls_dependency() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    // A user's own names (account, private projects) come from the environment,
    // comma-separated, never from the repository.
    let mut forbidden = vec![format!("/{}/", "home")];
    let private = std::env::var("MOONGLOW_PRIVATE_WORDS").unwrap_or_default();
    forbidden.extend(private.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()));
    for path in ["apps/moonglow-mcp", "crates/mg-live", "docs/agent-protocol.md", "tools/live"] {
        scan(&root.join(path), &forbidden);
    }
    let manifest = fs::read_to_string(root.join("apps/moonglow-mcp/Cargo.toml")).unwrap();
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .split("[dev-dependencies]")
        .next()
        .unwrap();
    assert!(
        dependencies.contains("mg-live-link ="),
        "Gateway TLS must reuse the shared trust implementation"
    );
    for line in dependencies.lines().filter(|line| line.contains("path =")) {
        assert!(
            line.starts_with("mg-live-link ="),
            "Unexpected runtime project dependency: {line}"
        );
    }
}
