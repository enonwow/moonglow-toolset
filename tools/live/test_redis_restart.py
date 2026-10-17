"""Restart only the Redis child of the 5126/6392 owned private proof harness."""
import json
import subprocess
import time


def run(proof, redis, server, processes, identity, scratch):
    owner = identity(server.pid)
    boot = proof.state["boot"]
    actor = proof.command(op="spawn", kind="creature", r="nw_bartender", x=4, y=4, z=0, f=0)["obj"]
    cycles = []
    connection = type(proof.pub)
    for cycle in range(3):
        text = f"outage speech {cycle}"
        proof.chunk(f'SetLocalString(StringToObject("{actor}"),"MGET_OUTAGE_TEXT","{text}");')
        proof.command(op="run_script", target=actor, script="mget_outage")
        before = proof.state["seq"]
        proof.pub.close()
        proof.sub.close()
        recorded = next(owned for process, owned in processes if process is redis)
        assert redis.poll() is None and identity(redis.pid) == recorded
        redis.terminate()
        redis.wait(timeout=8)
        time.sleep(10)
        assert server.poll() is None and identity(server.pid) == owner, "Game died during outage"
        started = time.monotonic()
        redis = subprocess.Popen(["redis-server", "--bind", "127.0.0.1", "--port", "6392",
                                  "--save", "", "--appendonly", "no", "--dir", str(scratch)],
                                 stdout=(scratch / "redis.log").open("a"), stderr=subprocess.STDOUT)
        processes.append((redis, identity(redis.pid)))
        (scratch / "owned-processes.json").write_text(json.dumps([i for _, i in processes]))
        while True:
            try:
                proof.pub, proof.sub = connection(), connection()
                break
            except ConnectionRefusedError:
                assert time.monotonic() - started < 5
                time.sleep(.05)
        proof.sub.send("SUBSCRIBE", "mglive-events.state", "mglive-events.log")
        proof.sub.read()
        proof.sub.read()
        proof.until(lambda: proof.state["seq"] > before, timeout=45)
        published_ms = (time.monotonic() - started) * 1000
        assert proof.state["boot"] == boot
        # The private server is the only pattern subscriber. Wait before sending
        # exactly one command, rather than hiding a resubscription failure by retry.
        while True:
            proof.pub.send("PUBSUB", "NUMPAT")
            if proof.pub.read() > 0:
                break
            assert time.monotonic() - started < 45, "Command subscription never recovered"
            proof.poll(.1)
        subscribed_ms = (time.monotonic() - started) * 1000
        name = f"reconnected {cycle}"
        result = proof.command(op="set_name", target=actor, name=name)
        proof.until(lambda: proof.creature(actor) is not None and proof.creature(actor)["name"] == name)
        proof.until(lambda: any(e.get("text") == text for e in proof.matching("chat")))
        assert identity(server.pid) == owner and proof.state["boot"] == boot
        cycles.append({"outage_seconds": 10, "publish_ms": published_ms,
                       "subscription_ms": subscribed_ms, "before_seq": before,
                       "after_seq": proof.state["seq"], "name_readback": name,
                       "command": result, "buffered_chat_recovered": True})
        print(f"Redis restart {cycle + 1}: state {published_ms:.0f} ms, pubsub {subscribed_ms:.0f} ms; same game PID/boot", flush=True)
    return {"server_identity": owner, "boot": boot, "cycles": cycles}
