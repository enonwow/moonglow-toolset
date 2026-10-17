"""Private proof helpers for the durable event wire."""
import os
from pathlib import Path


def redis_args(scratch, port=6392):
    args = ["redis-server", "--bind", "127.0.0.1", "--port", str(port), "--save", ""]
    if os.environ.get("MGLIVE_STREAM_PROOF") == "1":
        root = Path(scratch) / "redis-stream"
        if root.is_symlink(): raise RuntimeError("Private Redis directory is a symlink")
        root.mkdir(exist_ok=True)
        args += ["--appendonly", "yes", "--appendfsync", "always", "--dir", str(root)]
    else: args += ["--appendonly", "no", "--dir", str(scratch)]
    return args


def read_events(connection, prefix, cursor):
    connection.send("XREAD", "COUNT", "256", "STREAMS", prefix + ".events", cursor)
    try:
        reply = connection.read()
    except RuntimeError as error:
        # Redis accepts connections before AOF recovery finishes. The error
        # consumes this reply; preserve the cursor and retry on the next poll.
        if str(error).startswith("LOADING "):
            return cursor, []
        raise
    rows = []
    for _, entries in reply or []:
        for ident, fields in entries:
            rows.append((ident, dict(zip(fields[::2], fields[1::2]))))
            cursor = ident
    return cursor, rows
