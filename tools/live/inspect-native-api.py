"""Read installed NWScript declarations without launching apps/extracting assets.

Usage: python inspect-native-api.py GAME_ROOT 'Weather|Fog|Music|AmbientSound'
KEY/BIF layout is the same bounded format used by crates/mg-key.
"""
import hashlib
from pathlib import Path
import re
import struct
import sys


def source(root):
    for key in [root / "data/nwn_retail.key", root / "data/nwn_base.key"]:
        data = key.read_bytes()
        assert data[:8] == b"KEY V1  "
        _, count, files, entries = struct.unpack_from("<4I", data, 8)
        for n in range(count):
            offset = entries+22*n
            name = data[offset:offset+16].split(b"\0")[0]
            kind, resource = struct.unpack_from("<HI", data, offset+16)
            if name != b"nwscript" or kind != 2009:
                continue
            _, name_offset, length, _ = struct.unpack_from("<IIHH", data, files+12*(resource >> 20))
            relative = data[name_offset:name_offset+length].split(b"\0")[0].decode().replace("\\", "/")
            bif = root / relative
            if not bif.is_file():
                bif = root / "data" / Path(relative).name
            with bif.open("rb") as stream:
                header = stream.read(20)
                assert header[:8] == b"BIFFV1  "
                table = struct.unpack_from("<I", header, 16)[0]
                stream.seek(table+16*(resource & 0xfffff))
                _, offset, size, kind = struct.unpack("<4I", stream.read(16))
                assert kind == 2009 and size < 4*1024*1024
                stream.seek(offset)
                content = stream.read(size)
            return content, key, bif
    raise RuntimeError("No installed nwscript.nss")


if __name__ == "__main__":
    content, key, bif = source(Path(sys.argv[1]))
    print(f"// source={key} bif={bif} nwscript_sha256={hashlib.sha256(content).hexdigest()}")
    pattern = re.compile(sys.argv[2])
    lines = content.decode("cp1252").splitlines()
    for n, line in enumerate(lines):
        if pattern.search(line) and (re.match(r"\w+ \w+\(", line) or line.startswith("int ")):
            print(f"// nwscript.nss:{n+1}")
            print("\n".join(lines[max(0, n-10):n+1]))
