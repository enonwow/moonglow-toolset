"""Prove the owned launcher defaults on private Redis6394; never start a game."""
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SCRATCH = Path.home() / "mglive-events"
PORT = "6394"


def main():
    assert SCRATCH.resolve() == SCRATCH
    assert (SCRATCH / ".moonglow-live-events").read_text().strip() == "moonglow-live-events-v1"
    with socket.socket() as probe: probe.bind(("127.0.0.1", int(PORT)))
    data = SCRATCH / f"redis-{PORT}-data"
    assert not data.is_symlink()
    marker = data / ".astra1-stream-proof"
    if data.exists(): assert marker.read_text() == "astra1-event-stream-v1"
    else:
        for suffix in ["pid", "start", "exe", "log"]:
            assert not (SCRATCH / f"redis-{PORT}.{suffix}").exists()
        data.mkdir()
        marker.write_text("astra1-event-stream-v1")
    env = dict(os.environ, MGLIVE_INSTANCE="events")
    def helper(name):
        subprocess.run(["bash", str(ROOT / f"tools/live/wsl/{name}-redis.sh"), PORT], env=env, check=True)
    def redis(*args):
        return subprocess.check_output(["redis-cli", "-h", "127.0.0.1", "-p", PORT, "--raw", *args], text=True).strip()
    key = "mg-launcher-test-" + secrets.token_hex(8)
    result = {}
    try:
        helper("start")
        assert redis("CONFIG", "GET", "appendonly").splitlines() == ["appendonly", "yes"]
        assert redis("CONFIG", "GET", "appendfsync").splitlines() == ["appendfsync", "always"]
        pid = (SCRATCH / f"redis-{PORT}.pid").read_text()
        helper("start")
        assert (SCRATCH / f"redis-{PORT}.pid").read_text() == pid
        entry = redis("XADD", key, "*", "event", "durable")
        assert redis("XGROUP", "CREATE", key, "test", "0") == "OK"
        redis("XREADGROUP", "GROUP", "test", "before", "STREAMS", key, ">")
        helper("stop")
        helper("start")
        assert redis("XLEN", key) == "1"
        pending = redis("XPENDING", key, "test").splitlines()
        assert pending[:3] == ["1", entry, entry]
        assert entry in redis("XAUTOCLAIM", key, "test", "after", "0", "0-0")
        assert redis("XACK", key, "test", entry) == "1"
        assert redis("DEL", key) == "1"
        result = {"passed": True, "port": int(PORT), "aof": True, "appendfsync": "always",
                  "survived_restart": 1, "pending_reclaimed": 1, "reused_existing_pid": True}
    finally:
        helper("stop")
    result["owned_process_stopped"] = True
    output = ROOT / "target/test-output/live-stream/launcher.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__": main()
