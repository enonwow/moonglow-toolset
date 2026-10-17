"""Summarize look traffic/cache invalidations in retained private report samples."""
import collections
import json
from pathlib import Path
import sys


def summarize(frames):
    previous = {}
    positions = {}
    movers = set()
    changes = collections.Counter()
    fields = collections.Counter()
    looks = look_bytes = 0
    for frame in frames:
        for area in frame["areas"]:
            for row in area["cre"]:
                key = (area["area"], row["id"])
                position = tuple(row.get(k) for k in ("x", "y", "z"))
                if key in positions and positions[key] != position:
                    movers.add(key)
                positions[key] = position
                if "look" not in row:
                    continue
                look = row["look"]
                looks += 1
                look_bytes += len(json.dumps(look, ensure_ascii=False, separators=(",", ":")).encode())
                if key in previous and previous[key] != look:
                    changes[row["id"]] += 1
                    old = previous[key]
                    fields.update(k for k in set(old) | set(look) if old.get(k) != look.get(k))
                previous[key] = look
    return {"reports": len(frames), "objects": len(positions), "moving_creatures": len(movers),
            "look_payloads": looks, "look_bytes": look_bytes,
            "hash_invalidations_after_initial": sum(changes.values()),
            "changes_by_id": dict(changes), "changed_root_fields": dict(fields)}


if __name__ == "__main__":
    for name in sys.argv[1:]:
        path = Path(name)
        print(json.dumps({"source": str(path), **summarize(json.loads(path.read_text()))}))
