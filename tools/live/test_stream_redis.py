"""Hold one owned, private Redis for the opt-in Rust stream integration test."""
import json
from pathlib import Path
import socket
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2] / "target/test-output/live-stream/redis"


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    assert ROOT.resolve() == ROOT and not ROOT.is_symlink()
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 6393))
    marker = ROOT / ".owner"
    if marker.exists(): assert marker.read_text() == "astra1-event-stream-v1"
    else: marker.write_text("astra1-event-stream-v1")
    stop = ROOT / "stop"
    if stop.exists(): stop.unlink()
    with (ROOT / "redis.log").open("w") as log:
        child = subprocess.Popen(["redis-server", "--bind", "127.0.0.1", "--port", "6393",
            "--save", "", "--appendonly", "yes", "--appendfsync", "always", "--dir", str(ROOT)], stdout=log, stderr=subprocess.STDOUT)
        identity = {"pid": child.pid, "start": Path(f"/proc/{child.pid}/stat").read_text().rsplit(")", 1)[1].split()[19]}
        (ROOT / "process.json").write_text(json.dumps(identity))
        try:
            for _ in range(100):
                try:
                    with socket.create_connection(("127.0.0.1", 6393), .2): break
                except OSError: time.sleep(.05)
            print("private Redis 6393 ready", flush=True)
            until = time.monotonic() + 900
            while not stop.exists() and time.monotonic() < until:
                assert child.poll() is None
                time.sleep(.2)
        finally:
            if child.poll() is None:
                assert Path(f"/proc/{child.pid}/stat").read_text().rsplit(")", 1)[1].split()[19] == identity["start"]
                child.terminate()
            child.wait(timeout=10)
            identity["exited"] = True
            (ROOT / "process.json").write_text(json.dumps(identity))


if __name__ == "__main__": main()
