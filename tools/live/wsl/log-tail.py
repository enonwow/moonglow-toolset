"""Owned NWN log follower. Invoked by log-tail.sh after marker/path checks."""
import json
import os
from pathlib import Path
import re
import secrets
import signal
import socket
import stat
import sys
import time
from urllib.parse import quote_from_bytes

ERROR = re.compile(r"(?:error (?:running|executing|while executing) (?:a )?script|"
                   r"script (?:runtime )?error|too many instructions|stack (?:underflow|overflow)|"
                   r"division by zero|unrecognized opcode)", re.I)
STREAM_SCRIPT = (Path(__file__).resolve().parents[1] / "event-stream.lua").read_bytes()


def script_error(line):
    # Do not flag quoted chunk source that happens to contain an error string.
    body = re.sub(r"^\[[^]]+\]\s*", "", line)
    return not body.startswith("MGLIVE: chunk ") and bool(ERROR.search(body))


def main():
    scratch, prefix, port, boot = sys.argv[1:]
    root = Path(scratch)
    owners = {Path.home() / "mglive-home": (".moonglow-live-demo", "moonglow-live-demo"),
              Path.home() / "mglive-events": (".moonglow-live-events", "moonglow-live-events-v1")}
    if root not in owners:
        raise RuntimeError("Unknown scratch")
    marker_name, owner = owners[root]
    marker = root / marker_name
    if root.is_symlink() or marker.is_symlink() or marker.read_text().strip() != owner:
        raise RuntimeError("Unowned scratch")
    path = root / "logs.0/nwserverLog1.txt"
    stream = secrets.token_hex(16)
    seq = 0
    conn = None
    handle = None
    first = True
    partial = b""
    running = True
    announced = False

    def stop(*_):
        nonlocal running
        running = False

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)

    def publish(payload):
        nonlocal conn
        key = (prefix + ".events").encode()
        args = [b"EVAL", STREAM_SCRIPT, b"3", key, key + b".meta",
                key + b".producer:log:" + stream.encode(), stream.encode(), b"log",
                os.environ.get("MGLIVE_TEXT_CODEPAGE", "cp1250").encode(), str(seq).encode(), payload]
        request = b"*%d\r\n" % len(args) + b"".join(b"$%d\r\n" % len(a) + a + b"\r\n" for a in args)
        while running:
            try:
                if conn is None:
                    conn = socket.create_connection(("127.0.0.1", int(port)), 1)
                    conn.settimeout(1)
                conn.sendall(request)
                reply = b""
                while not reply.endswith(b"\r\n"):
                    part = conn.recv(1)
                    if not part:
                        raise ConnectionError("Redis closed")
                    reply += part
                    if len(reply) > 1024:
                        raise ConnectionError("Invalid Redis response")
                if not reply.startswith(b":"):
                    raise ConnectionError("Redis publish failed")
                if int(reply[1:-2]) >= seq:
                    return
            except (OSError, ValueError, ConnectionError):
                if conn:
                    conn.close()
                conn = None
            time.sleep(.2)

    try:
        while running:
            if root.resolve() != root or path.parent.resolve() != path.parent or path.is_symlink():
                raise RuntimeError("Log path escapes owned scratch or is a symlink")
            if handle is None:
                try:
                    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                except FileNotFoundError:
                    # If the log is created after attachment, read it from the start.
                    first = False
                    time.sleep(.1)
                    continue
                if not stat.S_ISREG(os.fstat(fd).st_mode):
                    os.close(fd)
                    raise RuntimeError("Log is not a regular file")
                handle = os.fdopen(fd, "rb", buffering=0)
                if first:
                    handle.seek(0, os.SEEK_END)
                first = False
                if not announced:
                    print("log-tail ready", flush=True)
                    announced = True
            data = handle.readline()
            if data:
                partial += data
                if not partial.endswith(b"\n"):
                    time.sleep(.1)
                    continue
                raw = partial.rstrip(b"\r\n")
                line = raw.decode("utf-8", errors="replace")
                partial = b""
                seq += 1
                now = time.time_ns()
                event = {"v": 1, "kind": "server_log", "boot": boot or None, "stream": stream,
                         "seq": seq, "time": f"{now // 10**9}.{now % 10**9 // 1000:06d}",
                         "line": line, "script_error": script_error(line)}
                if not raw.isascii():
                    event["line_bytes"] = quote_from_bytes(raw, safe="")
                    event["text_codepage"] = os.environ.get("MGLIVE_TEXT_CODEPAGE", "cp1250")
                publish(json.dumps(event, ensure_ascii=False).encode())
                continue
            try:
                current = path.stat()
            except FileNotFoundError:
                current = None
            opened = os.fstat(handle.fileno())
            if current and (current.st_dev, current.st_ino) != (opened.st_dev, opened.st_ino):
                handle.close()
                handle = None
                partial = b""
            elif opened.st_size < handle.tell():
                handle.seek(0)
                partial = b""
            time.sleep(.1)
    finally:
        if handle:
            handle.close()
        if conn:
            conn.close()


if __name__ == "__main__":
    main()
