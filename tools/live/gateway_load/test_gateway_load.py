"""Checks of the load tools' own arithmetic and framing: `python -m unittest` in this directory."""
import unittest

import report
import resp
import wsclient


class Report(unittest.TestCase):
    def test_percentiles_use_nearest_rank(self):
        values = list(range(1, 101))
        self.assertEqual(report.pct(values, 50), 50)
        self.assertEqual(report.pct(values, 95), 95)
        self.assertEqual(report.pct(values, 99), 99)
        self.assertIsNone(report.pct([], 50))
        self.assertEqual(report.summary([5])["p99"], 5)

    def test_windows_skip_outages(self):
        rows = [[t, t] for t in range(100)]
        kept = report.steady(rows, 10, 90, [(20, 30), (50, 55)])
        self.assertEqual([r[0] for r in kept][:3], [10, 11, 12])
        self.assertNotIn(25, [r[0] for r in kept])
        self.assertNotIn(54, [r[0] for r in kept])
        self.assertIn(55, [r[0] for r in kept])
        self.assertNotIn(95, [r[0] for r in kept])

    def test_counter_rates_survive_a_process_restart(self):
        series = [{"t": 0, "c": 0}, {"t": 10, "c": 20_000_000}, {"t": 20, "c": 5_000_000}, {"t": 30, "c": 25_000_000}]
        got = [round(r) for _, r in report.rates(series, "c", [], 0, 100)]
        self.assertEqual(got, [2_000_000, 500_000, 2_000_000])
        skipped = report.rates(series, "c", [(-1, 5)], 0, 100)
        self.assertEqual(len(skipped), 2)

    def test_flat_and_slope(self):
        self.assertTrue(report.flat([100 + i % 3 for i in range(50)]))
        self.assertFalse(report.flat([100 + i * 5 for i in range(50)]))
        self.assertAlmostEqual(report.slope([(0, 1), (1, 3), (2, 5)]), 2.0)


class Framing(unittest.TestCase):
    def unmask(self, data):
        n, off = data[1] & 0x7F, 2
        if n == 126:
            n, off = int.from_bytes(data[2:4], "big"), 4
        elif n == 127:
            n, off = int.from_bytes(data[2:10], "big"), 10
        key, body = data[off:off + 4], data[off + 4:]
        self.assertEqual(len(body), n)
        return bytes(b ^ key[i % 4] for i, b in enumerate(body))

    def test_client_frames_are_masked_at_every_length(self):
        for n in (0, 1, 125, 126, 65535, 65536, 70000):
            payload = bytes(range(256)) * (n // 256 + 1)
            payload = payload[:n]
            data = wsclient.frame(1, payload)
            self.assertEqual(data[0], 0x81)
            self.assertTrue(data[1] & 0x80)
            self.assertEqual(self.unmask(data), payload)

    def test_resp_encoding(self):
        self.assertEqual(resp.Redis.encode(("PUBLISH", "c", b"x y")), b"*3\r\n$7\r\nPUBLISH\r\n$1\r\nc\r\n$3\r\nx y\r\n")


if __name__ == "__main__":
    unittest.main()
