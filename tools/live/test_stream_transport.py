import unittest
from stream_transport import read_events


class StreamReader(unittest.TestCase):
    def test_loading_preserves_cursor_then_resumes(self):
        class Connection:
            replies = [RuntimeError("LOADING Redis is loading the dataset"),
                       [["p.events", [["4-0", ["type", "game", "n", "9"]]]]]]
            calls = []
            def send(self, *args): self.calls.append(args)
            def read(self):
                reply = self.replies.pop(0)
                if isinstance(reply, Exception): raise reply
                return reply
        connection = Connection()
        self.assertEqual(read_events(connection, "p", "3-0"), ("3-0", []))
        self.assertEqual(read_events(connection, "p", "3-0"),
                         ("4-0", [("4-0", {"type": "game", "n": "9"})]))
        self.assertTrue(all(call[-1] == "3-0" for call in connection.calls))

    def test_other_errors_are_not_hidden(self):
        class Connection:
            def send(self, *args): pass
            def read(self): raise RuntimeError("WRONGTYPE bad stream")
        with self.assertRaisesRegex(RuntimeError, "WRONGTYPE"):
            read_events(Connection(), "p", "0-0")


if __name__ == "__main__": unittest.main()
