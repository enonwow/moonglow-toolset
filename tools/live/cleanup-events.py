"""Remove only closed, task-owned private event-proof transients (run in WSL)."""
import argparse
import fcntl
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("phase", help="Completed test-events.py phase")
    parser.add_argument("--ledger", default="dm-files", choices=["dm-files", "qol-files", "followup-files", "teleport-files", "pc-stop-files", "shader-files", "accept3-files", "accept4-files"], help="Task ownership ledger")
    args = parser.parse_args()
    if not args.phase.replace("_", "").isalnum():
        raise ValueError("Invalid phase name")
    scratch = Path.home() / "mglive-events"
    out = Path(__file__).resolve().parents[2] / "target/test-output/live-events"
    assert scratch.resolve() == scratch and not scratch.is_symlink()
    marker = scratch / ".moonglow-live-events"
    assert not marker.is_symlink() and marker.read_text().strip() == "moonglow-live-events-v1"
    lock = (scratch / "test.lock").open("a")
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    for flag, port in [("-lun", 5126), ("-ltn", 6392)]:
        assert not subprocess.check_output(
            ["ss", "-H", flag, "sport = :" + str(port)], text=True
        ).strip(), "Private port still occupied"
    manifest = json.loads((out / (args.phase + ".json")).read_text())
    assert manifest["passed"] and all(p["exited"] for p in manifest["cleanup"])
    owned = json.loads((scratch / "owned-processes.json").read_text())
    assert {(p["pid"], p["start"], p["exe"]) for p in owned} <= {
        (p["pid"], p["start"], p["exe"]) for p in manifest["cleanup"]
    }, "Not the last run"
    for process in owned:
        stat = Path("/proc") / str(process["pid"]) / "stat"
        if stat.exists():
            assert stat.read_text().rsplit(")", 1)[1].split()[19] != process["start"], "Owned process still alive"
    names = [
        "cryptographic_secret", "nwn.ini", "nwnplayer.ini", "settings.tml",
        "redis.log", "server.out", ".nwnpid", "logs.0/nwengineLog.txt",
        "logs.0/nwserverLog1.txt", "nwsync/nwsyncmeta.sqlite3", "cache/http.sqlite3",
    ]
    paths = [scratch / name for name in names if (scratch / name).exists()]
    empty_log = out / (args.phase + "-log.json")
    if empty_log.exists() and json.loads(empty_log.read_text()) == []:
        paths.append(empty_log)
    removed = []
    for path in paths:
        root = scratch if path.is_relative_to(scratch) else out
        assert path.resolve() == path and path.is_relative_to(root) and path.is_file()
        assert not path.is_symlink()
        assert all(not p.is_symlink() for p in path.parents if p.is_relative_to(root))
        removed.append({"path": str(path), "logical_bytes": path.stat().st_size})
    ledgers = {"shader-files": out.parent / "live-shader/files.json",
               "accept4-files": out.parent / "live-accept4-game/files.json",
               "accept3-files": out.parent / "live-accept3-game/files.json",
               "followup-files": out.parent / "live-dm-followup/files.json",
               "teleport-files": out.parent / "live-teleport/files.json",
               "pc-stop-files": out.parent / "live-pc-stop/files.json"}
    ledger_path = ledgers.get(args.ledger, out / (args.ledger + ".json"))
    ledger = json.loads(ledger_path.read_text()) if ledger_path.exists() else {"task": "live-events-proof"}
    entry = {"phase": args.phase, "files": removed, "status": "pending"}
    ledger.setdefault("cleanup_runs", []).append(entry)
    ledger_path.write_text(json.dumps(ledger, indent=2), encoding="utf-8")
    for path in paths:
        path.unlink()
    entry.update(status="removed", logical_bytes=sum(p["logical_bytes"] for p in removed))
    ledger_path.write_text(json.dumps(ledger, indent=2), encoding="utf-8")
    print(json.dumps({"removed_count": len(removed), "logical_bytes": entry["logical_bytes"]}))


if __name__ == "__main__":
    main()
