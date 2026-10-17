"""Private engine proof of the Rust-built copy; never builds/runs the demo.
Run in WSL after the guarded prepare_private_live_build test.
"""
import fcntl
import importlib.util
import json
from pathlib import Path
import secrets
import shutil
import subprocess
import time

spec = importlib.util.spec_from_file_location("events", Path(__file__).with_name("test-events.py"))
h = importlib.util.module_from_spec(spec)
spec.loader.exec_module(h)


def main():
    out = h.REPO / "target/test-output/live-build"
    source = out / "prepared.mod"
    scratch = h.SCRATCH
    assert scratch.resolve() == Path.home() / "mglive-events"
    assert (scratch / ".moonglow-live-events").read_text().strip() == "moonglow-live-events-v1"
    assert not scratch.is_symlink()
    lock = (scratch / "test.lock").open("w")
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    for protocol, port in [("-lun", 5126), ("-ltn", 6392)]:
        assert not subprocess.check_output(["ss", "-H", protocol, f"sport = :{port}"], text=True).strip()
    destination = scratch / "modules/live_build.mod"
    assert not destination.is_symlink()
    shutil.copyfile(source, destination)
    processes = []
    proof = None
    result = {"module_sha256": h.sha(source), "source_sha256": h.sha(out / "source.mod"), "server_sha256": h.sha(h.SERVER), "nwnx_zip_sha256": h.sha(h.PLUGINS / "NWScript.zip"), "passed": False}
    log = scratch / "logs.0/nwserverLog1.txt"
    try:
        redis = subprocess.Popen(["redis-server", "--bind", "127.0.0.1", "--port", "6392", "--save", "", "--appendonly", "no", "--dir", str(scratch)], stdout=(out / "redis.log").open("w"), stderr=subprocess.STDOUT)
        processes.append((redis, h.identity(redis.pid)))
        for _ in range(50):
            try:
                proof = h.Proof()
                break
            except ConnectionRefusedError:
                time.sleep(.1)
        assert proof
        env = {"HOME": str(Path.home()), "PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "LD_PRELOAD": str(h.PLUGINS / "NWNX_Core.so"), "NWNX_CORE_LOAD_PATH": str(h.PLUGINS), "NWNX_CORE_SKIP_ALL": "y", "NWNX_REDIS_HOST": "127.0.0.1", "NWNX_REDIS_PORT": "6392", "NWNX_REDIS_PUBSUB_SCRIPT": "mglive_ps", "NWNX_REDIS_PUBSUB_CHANNELS": h.PREFIX + ".cmd", "MGLIVE_PREFIX": h.PREFIX, "MGLIVE_BOOT": secrets.token_hex(16), "MGLIVE_AREAS": "events", "MGLIVE_ALLOW_CHUNK": "1", "MGLIVE_TEXT_CODEPAGE": "cp1250"}
        for plugin in ["REDIS", "UTIL", "CREATURE", "CHAT", "DAMAGE", "EVENTS", "OBJECT", "DIALOG", "AREA"]:
            env[f"NWNX_{plugin}_SKIP"] = "n"
        server = subprocess.Popen([str(h.SERVER), "-userdirectory", str(scratch), "-module", "live_build", "-port", "5126", "-publicserver", "0", "-reloadwhenempty", "0", "-adminpassword", secrets.token_hex(24)], cwd=h.SERVER.parent, env=env, stdin=subprocess.PIPE, stdout=(out / "server.log").open("w"), stderr=subprocess.STDOUT)
        processes.append((server, h.identity(server.pid)))
        proof.until(lambda: proof.state is not None, timeout=90)
        result["original_local"] = proof.native('JsonInt(GetLocalInt(GetModule(),"LIVE_BUILD_ORIGINAL"))')
        assert result["original_local"] == 123
        result["spawn"] = proof.command(op="spawn", kind="creature", r="nw_bartender", x=3, y=3, z=0, f=0)
        proof.until(lambda: proof.object_row(result["spawn"]["obj"], "cre") is not None)
        text = log.read_text(errors="replace")
        assert "LIVE_BUILD: original before Live=0" in text
        assert "TOO MANY INSTRUCTIONS" not in text and "Script runtime error" not in text
        result.update(passed=True, boot=proof.state["boot"], seq=proof.state["seq"], spawned_row=proof.object_row(result["spawn"]["obj"], "cre"), original_before_live=True)
        (out / "engine-state.json").write_text(json.dumps(proof.state, ensure_ascii=False))
    finally:
        if proof:
            proof.pub.close()
            proof.sub.close()
        result["cleanup"] = []
        for process, identity in reversed(processes):
            if process.poll() is None:
                assert h.identity(process.pid) == identity
                if process.stdin:
                    process.stdin.write(b"exit\n")
                    process.stdin.flush()
                else:
                    process.terminate()
                try:
                    process.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    assert h.identity(process.pid) == identity
                    process.terminate()
                    process.wait(timeout=8)
            result["cleanup"].append({**identity, "exited": process.poll() is not None})
        (out / "engine-proof.json").write_text(json.dumps(result, indent=2))
    print(json.dumps({k: result[k] for k in ["passed", "module_sha256", "original_local", "original_before_live", "seq", "cleanup"]}))


if __name__ == "__main__":
    main()
