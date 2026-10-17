"""Read-only SUBSCRIBE sample: counts wake-producing traffic, never stores chat text."""
import argparse
import collections
import json
import socket
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--port', type=int, default=6379)
parser.add_argument('--prefix', default='mglive')
parser.add_argument('--seconds', type=float, default=30)
parser.add_argument('--out', type=Path, required=True)
args = parser.parse_args()
counts = collections.Counter()
sizes = collections.Counter()
events = collections.Counter()
with socket.create_connection(('127.0.0.1', args.port), timeout=3) as sock:
    sock.settimeout(None)
    reader = sock.makefile('rb')

    def response():
        line = reader.readline()
        if not line:
            raise EOFError('Redis closed the subscription')
        if line[:1] == b'*':
            return [response() for _ in range(int(line[1:]))]
        if line[:1] == b'$':
            data = reader.read(int(line[1:]))
            assert reader.read(2) == b'\r\n'
            return data
        if line[:1] == b':':
            return int(line[1:])
        raise RuntimeError('Unexpected Redis response type')

    parts = [b'SUBSCRIBE', f'{args.prefix}.state'.encode(), f'{args.prefix}.log'.encode()]
    sock.sendall(b'*3\r\n' + b''.join(b'$%d\r\n' % len(p) + p + b'\r\n' for p in parts))
    response()
    response()
    start = time.monotonic()
    # Buffered RESP may already contain data: use a socket timeout per next complete frame.
    sock.settimeout(args.seconds)
    while time.monotonic() - start < args.seconds:
        try:
            message = response()
        except TimeoutError:
            break
        if message[0] != b'message':
            continue
        channel = message[1].decode()
        counts[channel] += 1
        sizes[channel] += len(message[2])
        if channel.endswith('.state'):
            state = json.loads(message[2])
            for event in state.get('events', []):
                events[event.get('kind', 'unknown')] += 1
    elapsed = time.monotonic() - start
args.out.write_text(json.dumps({'seconds': elapsed, 'messages': counts, 'bytes': sizes, 'event_kinds': events,
                               'note': 'Read-only subscription; excludes reconnect/outbound-command wake calls and saves no payloads.'}, indent=2) + '\n')
