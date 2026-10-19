"""Small asyncio WebSocket client (RFC 6455, client side) and HTTP/1.1 keep-alive client."""
import asyncio
import base64
import json
import os
import struct


class Closed(Exception):
    pass


def frame(opcode, payload, mask=True):
    """Encode one final frame; clients must mask."""
    head = bytes([0x80 | opcode])
    n = len(payload)
    flag = 0x80 if mask else 0
    if n < 126:
        head += bytes([flag | n])
    elif n < 65536:
        head += bytes([flag | 126]) + struct.pack(">H", n)
    else:
        head += bytes([flag | 127]) + struct.pack(">Q", n)
    if not mask:
        return head + payload
    key = os.urandom(4)
    # XOR through a big integer: fast enough for small client frames.
    if n:
        padded = (key * (n // 4 + 1))[:n]
        payload = (int.from_bytes(payload, "big") ^ int.from_bytes(padded, "big")).to_bytes(n, "big")
    return head + key + payload


class WebSocket:
    def __init__(self, reader, writer, protocol):
        self.reader, self.writer, self.protocol = reader, writer, protocol

    @classmethod
    async def connect(cls, host, port, path, token, protocol="moonglow-gw.1", timeout=10):
        reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port, limit=1 << 22), timeout)
        key = base64.b64encode(os.urandom(16)).decode()
        writer.write(
            (f"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
             f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: {protocol}\r\n"
             f"Authorization: Bearer {token}\r\n\r\n").encode())
        head = await asyncio.wait_for(reader.readuntil(b"\r\n\r\n"), timeout)
        status = int(head.split(b" ", 2)[1])
        if status != 101:
            writer.close()
            raise Closed(f"upgrade refused: {status}")
        return cls(reader, writer, protocol)

    async def send(self, text):
        self.writer.write(frame(1, text.encode() if isinstance(text, str) else text))
        await self.writer.drain()

    async def recv(self):
        """Next text/binary message as bytes; answers pings; raises Closed on a close frame."""
        parts = []
        while True:
            try:
                h = await self.reader.readexactly(2)
                op, n = h[0] & 0x0F, h[1] & 0x7F
                if n == 126:
                    n = struct.unpack(">H", await self.reader.readexactly(2))[0]
                elif n == 127:
                    n = struct.unpack(">Q", await self.reader.readexactly(8))[0]
                data = await self.reader.readexactly(n) if n else b""
            except (asyncio.IncompleteReadError, ConnectionError) as e:
                raise Closed("connection lost") from e
            if op == 9:
                self.writer.write(frame(10, data))
                continue
            if op == 10:
                continue
            if op == 8:
                code = struct.unpack(">H", data[:2])[0] if len(data) >= 2 else 0
                raise Closed(f"close {code}")
            parts.append(data)
            if h[0] & 0x80:
                return parts[0] if len(parts) == 1 else b"".join(parts)

    def close(self):
        try:
            self.writer.close()
        except Exception:
            pass


class Http:
    """One keep-alive connection; reconnects after an error."""

    def __init__(self, host, port, token):
        self.host, self.port, self.token = host, port, token
        self.reader = self.writer = None

    async def post(self, path, body, timeout=40):
        payload = json.dumps(body).encode()
        for attempt in (0, 1):
            try:
                if self.writer is None:
                    self.reader, self.writer = await asyncio.wait_for(asyncio.open_connection(self.host, self.port, limit=1 << 22), 10)
                self.writer.write(
                    (f"POST {path} HTTP/1.1\r\nHost: {self.host}\r\nAuthorization: Bearer {self.token}\r\n"
                     f"Content-Type: application/json\r\nContent-Length: {len(payload)}\r\n\r\n").encode() + payload)
                await self.writer.drain()
                head = await asyncio.wait_for(self.reader.readuntil(b"\r\n\r\n"), timeout)
                status = int(head.split(b" ", 2)[1])
                length = 0
                for line in head.split(b"\r\n")[1:]:
                    if line.lower().startswith(b"content-length:"):
                        length = int(line.split(b":")[1])
                data = await asyncio.wait_for(self.reader.readexactly(length), timeout) if length else b""
                return status, json.loads(data) if data else None
            except (asyncio.IncompleteReadError, ConnectionError, asyncio.TimeoutError, OSError):
                self.close()
                if attempt:
                    raise
        raise Closed("unreachable")

    def close(self):
        if self.writer:
            try:
                self.writer.close()
            except Exception:
                pass
        self.reader = self.writer = None
