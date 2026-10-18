use anyhow::{Result, ensure};
use mg_module::Module;
use mg_resman::GameInstall;
use std::path::Path;

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
fn linux(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.as_bytes().get(1) == Some(&b':') {
        return format!("/mnt/{}{}", s[..1].to_ascii_lowercase(), &s[2..]);
    }
    for prefix in ["//wsl.localhost/", "//wsl$/"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return rest.find('/').map(|i| rest[i..].into()).unwrap_or(s);
        }
    }
    s
}

pub(super) fn settings(nwnx: &Path, module: &Module, install: &GameInstall) -> Result<String> {
    let mut text = format!(
        r#"# Moonglow Live server settings. Read by the sibling .start.sh; edit before use.
# Linux paths (WSL /mnt/... is accepted). NWNX must match this server version.
NWNX_ROOT={}
NWN_SERVER={}
# Private server UDP port. Choose a free port; the script never stops a server.
MGLIVE_PORT=5126
# Only the plugins Live uses; other module plugins can be enabled explicitly.
NWNX_CORE_SKIP_ALL=y
"#,
        quote(&linux(&std::path::absolute(nwnx)?)),
        quote(&linux(&install.root.join("bin/linux-x86/nwserver-linux")))
    );
    for plugin in
        ["REDIS", "UTIL", "CREATURE", "CHAT", "DAMAGE", "EVENTS", "OBJECT", "DIALOG", "AREA"]
    {
        text.push_str(&format!(
            "# Enable NWNX {plugin}, required by the embedded Live scripts.\nNWNX_{plugin}_SKIP=n\n"
        ));
    }
    text.push_str(&format!(
        r#"# Redis must already be running. Use a local, private Redis instance.
NWNX_REDIS_HOST=127.0.0.1
# Redis TCP port (configure the same address in Moonglow Options > Live).
NWNX_REDIS_PORT=6392
# Pub/sub callback: commands execute promptly without waiting for a tick.
NWNX_REDIS_PUBSUB_SCRIPT=mglive_ps
# One unique prefix per server, also selected in Moonglow.
MGLIVE_PREFIX=mglive
# Optional extra area resrefs; views watch areas automatically. PCs always report.
MGLIVE_AREAS=''
# Allow dynamically compiled NWScript chunks. OFF unless explicitly enabled.
MGLIVE_ALLOW_CHUNK=0
# Allow DM force operations on players; Moonglow's players switch is also required.
MGLIVE_ALLOW_PLAYER_OPS=0
# Legacy game string charset; cp1250 (Polish), cp1251 or cp1252.
MGLIVE_TEXT_CODEPAGE=cp1250
# Derived when starting: NWNX_REDIS_PUBSUB_CHANNELS=$MGLIVE_PREFIX.cmd,$MGLIVE_PREFIX.xl.
# Derived when starting: MGLIVE_BOOT is a fresh random server identity.
# Derived when starting: NWNX_CORE_LOAD_PATH and LD_PRELOAD use NWNX_ROOT/Binaries.
# Required HAKs: {}
# Required custom TLK: {}
# Put these existing dependencies into the scratch hak/ and tlk/ folders.
"#,
        module.haks()?.join(", "),
        module.custom_tlk()?.unwrap_or_default()
    ));
    Ok(text)
}

pub(super) fn start(output: &Path, settings: &Path) -> Result<String> {
    let name = output.file_name().unwrap().to_string_lossy();
    ensure!(!name.contains(['\n', '\r']), "Invalid output filename");
    let config = settings.file_name().unwrap().to_string_lossy();
    Ok(format!(
        r#"#!/usr/bin/env bash
# Run: bash <this-file>. Requires bash, python3, Redis and Linux NWNX/server.
# No public-server mode, no lifecycle management of any other process.
set -euo pipefail
here=$(cd -- "$(dirname -- "${{BASH_SOURCE[0]}}")" && pwd)
set -a
source "$here/"{}
set +a
plugins="$NWNX_ROOT/Binaries"
[[ -x "$NWN_SERVER" && -f "$plugins/NWNX_Core.so" ]] || {{ echo 'Edit NWNX_ROOT/NWN_SERVER in the .live.env file' >&2; exit 2; }}
[[ "$MGLIVE_PORT" =~ ^[0-9]+$ && "$MGLIVE_PORT" -gt 0 && "$MGLIVE_PORT" -le 65535 ]] || exit 2
[[ "$MGLIVE_PREFIX" =~ ^[A-Za-z0-9_:.-]{{1,64}}$ ]] || exit 2
if [[ -n "$(ss -H -lun "sport = :$MGLIVE_PORT")" ]]; then echo 'UDP port already in use; nothing stopped' >&2; exit 2; fi
# One dedicated runtime copy. Never use the real NWN user directory.
scratch="$here/"{}
[[ ! -L "$scratch" ]] || exit 2
if [[ -e "$scratch" ]]; then
    [[ -f "$scratch/.moonglow-live-build" && ! -L "$scratch/.moonglow-live-build" && "$(cat "$scratch/.moonglow-live-build")" == moonglow-live-build-v1 ]] || {{ echo 'Unowned scratch folder' >&2; exit 2; }}
else
    mkdir -- "$scratch"
    printf '%s\n' moonglow-live-build-v1 > "$scratch/.moonglow-live-build"
fi
for path in modules hak tlk logs.0 settings.tml modules/live.mod; do [[ ! -L "$scratch/$path" ]] || exit 2; done
mkdir -p -- "$scratch/modules" "$scratch/hak" "$scratch/tlk" "$scratch/logs.0"
cp -- "$here/"{} "$scratch/modules/live.mod"
# The charset must agree with the wire decoder; leave other owned settings alone.
if [[ ! -e "$scratch/settings.tml" ]]; then
    [[ "$MGLIVE_TEXT_CODEPAGE" =~ ^cp125[012]$ ]] || exit 2
    printf '[game.language]\ncodepage = "%s"\n' "$MGLIVE_TEXT_CODEPAGE" > "$scratch/settings.tml"
fi
export MGLIVE_BOOT=$(python3 -c 'import secrets; print(secrets.token_hex(16))')
admin=$(python3 -c 'import secrets; print(secrets.token_hex(24))')
cd -- "$(dirname -- "$NWN_SERVER")"
export NWNX_CORE_LOAD_PATH="$plugins" LD_PRELOAD="$plugins/NWNX_Core.so"
export NWNX_REDIS_PUBSUB_CHANNELS="$MGLIVE_PREFIX.cmd,$MGLIVE_PREFIX.xl"
exec "$NWN_SERVER" -userdirectory "$scratch" -module live -port "$MGLIVE_PORT" -publicserver 0 -reloadwhenempty 0 -servervault 1 -adminpassword "$admin"
"#,
        quote(&config),
        quote(&format!("{}-server", output.file_stem().unwrap().to_string_lossy())),
        quote(&name)
    ))
}
