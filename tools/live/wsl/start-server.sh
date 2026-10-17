#!/usr/bin/env bash
# Usage: start-server.sh MODULE_PATH [AREAS]
source "$(dirname -- "$0")/owned.sh"
if (( $# < 1 || $# > 2 )); then echo 'Expected MODULE_PATH [AREAS]' >&2; exit 2; fi
module=$(realpath -- "$1")
areas=${2:-}
server=${NWN_SERVER:-${NWN_ROOT:-}/bin/linux-x86/nwserver-linux}
plugins=${NWNX_ROOT:?Set NWNX_ROOT to the directory containing Binaries}/Binaries
server=$(realpath -- "$server")
port=${MGLIVE_REDIS_PORT:-6379}
prefix=${MGLIVE_PREFIX:-mglive}
# This helper only starts private demos. In particular, never silently turn
# a request for a public/password-free server into a public launch.
if [[ "${MGLIVE_PUBLIC:-0}" != 0 ]]; then echo 'Public servers are not supported by this private demo launcher' >&2; exit 2; fi
passwords=${MGLIVE_PASSWORDS:-0}
[[ "$passwords" == 0 || "$passwords" == 1 ]] || { echo 'MGLIVE_PASSWORDS must be 0 or 1' >&2; exit 2; }
[[ -f "$module" && -x "$server" && -f "$plugins/NWNX_Redis.so" &&
   "$port" =~ ^[0-9]+$ && "$port" -ge 1 && "$port" -le 65535 &&
   "$prefix" =~ ^[A-Za-z0-9_:.-]{1,64}$ ]] || { echo 'Invalid server/Redis configuration' >&2; exit 2; }
[[ -z "$areas" || "$areas" =~ ^[A-Za-z0-9_-]{1,16}(,[A-Za-z0-9_-]{1,16})*$ ]] || exit 2
if [[ -n "$(ss -H -lun 'sport = :5121')" ]]; then echo 'UDP 5121 is already in use; no process was stopped' >&2; exit 1; fi
for path in "$scratch/modules" "$scratch/hak" "$scratch/tlk" "$scratch/logs.0" "$scratch/modules/DEMO-Live-01.mod"; do safe_path "$path"; done
mkdir -p "$scratch/modules" "$scratch/hak" "$scratch/tlk" "$scratch/logs.0"
# One Linux runtime copy, reused between starts; never touch the real NWN user directory.
if ! cmp -s -- "$module" "$scratch/modules/DEMO-Live-01.mod"; then cp -- "$module" "$scratch/modules/DEMO-Live-01.mod"; fi
dependencies="$(dirname -- "$module")/user"
for kind in hak tlk; do
    if [[ -d "$dependencies/$kind" ]]; then
        while IFS= read -r -d '' source; do
            destination="$scratch/$kind/$(basename -- "$source")"
            safe_path "$destination"
            if ! cmp -s -- "$source" "$destination"; then cp -- "$source" "$destination"; fi
        done < <(find "$dependencies/$kind" -maxdepth 1 -type f -name "*.$kind" -print0)
    fi
done
boot=$(python3 -c 'import secrets; print(secrets.token_hex(16))')
# JsonGetString with the default/empty charset loses some UTF-8 literal bytes
# before a chat hook can observe them. Use the same explicit legacy charset as
# the wire decoder. This changes only owned scratch settings on the next start.
safe_path "$scratch/settings.tml"
python3 "$(dirname -- "$0")/configure-text.py" "$scratch/settings.tml" "${MGLIVE_TEXT_CODEPAGE:-cp1250}"
join_args=()
if [[ "$passwords" == 1 ]]; then
    player_password=$(python3 -c 'import secrets; print(secrets.token_hex(16))')
    dm_password=$(python3 -c 'import secrets; print(secrets.token_hex(16))')
    join_args=(-playerpassword "$player_password" -dmpassword "$dm_password")
    if [[ "${MGLIVE_PRINT_PASSWORD:-0}" == 1 ]]; then printf 'Player password: %s\n' "$player_password"; fi
fi
admin_password=$(python3 -c 'import secrets; print(secrets.token_hex(16))')
record_process server "$$"
# exec preserves PID/start tick; record the executable it will become.
printf '%s\n' "$server" > "$scratch/server.exe"
cd "$(dirname -- "$server")"
exec env -i HOME="$HOME" PATH=/usr/bin:/bin LANG=C.UTF-8 \
    LD_PRELOAD="$plugins/NWNX_Core.so" NWNX_CORE_LOAD_PATH="$plugins" NWNX_CORE_SKIP_ALL=y \
    NWNX_REDIS_SKIP=n NWNX_UTIL_SKIP=n NWNX_CREATURE_SKIP=n NWNX_CHAT_SKIP=n NWNX_REDIS_HOST=127.0.0.1 NWNX_REDIS_PORT="$port" \
    NWNX_DAMAGE_SKIP=n NWNX_EVENTS_SKIP=n NWNX_AREA_SKIP=n NWNX_OBJECT_SKIP=n NWNX_DIALOG_SKIP=n \
    NWNX_REDIS_PUBSUB_SCRIPT=mglive_ps NWNX_REDIS_PUBSUB_CHANNELS="$prefix.cmd" \
    MGLIVE_ALLOW_PLAYER_OPS="${MGLIVE_ALLOW_PLAYER_OPS:-0}" \
    MGLIVE_TEXT_CODEPAGE="${MGLIVE_TEXT_CODEPAGE:-cp1250}" \
    MGLIVE_PREFIX="$prefix" MGLIVE_AREAS="$areas" MGLIVE_BOOT="$boot" MGLIVE_ALLOW_CHUNK="${MGLIVE_ALLOW_CHUNK:-0}" \
    "$server" -userdirectory "$scratch" -module DEMO-Live-01 -port 5121 -publicserver 0 -reloadwhenempty 0 \
    "${join_args[@]}" -adminpassword "$admin_password" -servervault 1
