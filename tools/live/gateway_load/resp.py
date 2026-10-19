"""Minimal blocking RESP2 client (stdlib only) for the synthetic game and the monitors."""
import socket
import time


class RedisError(Exception):
    pass


class Redis:
    def __init__(self, host="redis", port=6379, timeout=5.0):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        self.sock.settimeout(timeout)
        self.buf = b""

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    @staticmethod
    def encode(args):
        out = [b"*%d\r\n" % len(args)]
        for a in args:
            if not isinstance(a, bytes):
                a = str(a).encode()
            out.append(b"$%d\r\n%s\r\n" % (len(a), a))
        return b"".join(out)

    def send(self, *args):
        self.sock.sendall(self.encode(args))

    def _line(self):
        while b"\r\n" not in self.buf:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ConnectionError("Redis closed the connection")
            self.buf += chunk
        line, self.buf = self.buf.split(b"\r\n", 1)
        return line

    def _exact(self, n):
        while len(self.buf) < n + 2:
            chunk = self.sock.recv(max(65536, n + 2 - len(self.buf)))
            if not chunk:
                raise ConnectionError("Redis closed the connection")
            self.buf += chunk
        data, self.buf = self.buf[:n], self.buf[n + 2:]
        return data

    def read(self):
        line = self._line()
        tag, rest = line[:1], line[1:]
        if tag == b"+":
            return rest.decode()
        if tag == b"-":
            raise RedisError(rest.decode())
        if tag == b":":
            return int(rest)
        if tag == b"$":
            n = int(rest)
            return None if n < 0 else self._exact(n)
        if tag == b"*":
            n = int(rest)
            return None if n < 0 else [self.read() for _ in range(n)]
        raise RedisError("bad reply %r" % line)

    def call(self, *args):
        self.send(*args)
        return self.read()


def connect_retry(host, port, stop=None, delay=0.2):
    """Block until Redis accepts a connection (Redis restarts are part of the scenario)."""
    while stop is None or not stop.is_set():
        try:
            r = Redis(host, port)
            r.call("PING")
            return r
        except (OSError, RedisError, ConnectionError):
            time.sleep(delay)
    return None
