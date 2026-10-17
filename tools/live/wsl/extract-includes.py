"""Private NSS experiment fixtures only; production builds use mg live-build.
Copy required NWNX includes without altering the installed zip.
"""
import hashlib
import json
import pathlib
import sys
import zipfile

source = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
destination.mkdir(parents=True, exist_ok=True)
names = ["nwnx_redis.nss", "nwnx_redis_lib.nss", "nwnx_redis_ps.nss", "nwnx_util.nss",
         "nwnx_creature.nss", "nwnx_chat.nss", "nwnx_damage.nss", "nwnx_events.nss", "nwnx_object.nss", "nwnx_dialog.nss", "nwnx_area.nss"]
with zipfile.ZipFile(source) as archive:
    for name in names:
        contents = archive.read(name)
        path = destination / name
        if not path.exists() or path.read_bytes() != contents:
            path.write_bytes(contents)
print(json.dumps({"source": str(source), "sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "includes": names}))
