"""Aggregation checks independent of the installed game and captured traffic."""
import json
from pathlib import Path
import unittest
from unittest.mock import patch

from analyze_gui_stress import prefix, slope


class Summaries(unittest.TestCase):
    def test_prefix_weights_samples_and_keeps_quantiles_separate(self):
        rows = [dict(interval_s=60, elapsed_s=(i + 1) * 60,
                     cpu_one_core_percent=40 + 40 * i,
                     metrics=dict(counts=dict(frames=n, pictures=n * 2),
                                  timings=dict(ui=dict(n=n, mean_ms=mean,
                                                       p95_ms=mean + 2, max_ms=mean + 3))))
                for i, (n, mean) in enumerate([(2, 10), (3, 20)])]
        with patch.object(Path, "read_text", return_value="\n".join(map(json.dumps, rows))):
            result = prefix(Path("unused.json"), 2)
            self.assertEqual(result["cpu_one_core_percent"], 60)
            self.assertEqual(result["stages"]["ui"]["mean_ms"], 16)
            self.assertEqual(result["stages"]["ui"]["minute_p95_range_ms"], [12, 22])
            self.assertEqual(result["stages"]["ui"]["max_ms"], 23)
            self.assertEqual(result["ui_hz"], 5 / 120)
            with self.assertRaises(ValueError):
                prefix(Path("unused.json"), 3)

    def test_memory_drift_uses_mib_per_minute_and_excludes_warmup(self):
        rows = [dict(elapsed_s=i * 60, memory=dict(private_bytes=(i * 2 + 10) * 2**20))
                for i in range(8)]
        rows[0]["memory"]["private_bytes"] = 999 * 2**20
        self.assertEqual(slope(rows, "private_bytes"), 2)


if __name__ == "__main__":
    unittest.main()
