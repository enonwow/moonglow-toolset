"""Pure measurement regression: NWN reuses four log names during a long soak."""
from pathlib import Path
import os
import tempfile
import unittest
from stress_metrics import NativeMetrics


class Rotation(unittest.TestCase):
    def test_reuse_partial_lines_and_old_run_exclusion(self):
        with tempfile.TemporaryDirectory(prefix="mglive-stress-metrics-") as folder:
            root=Path(folder);logs=root/"logs.0";logs.mkdir();path=logs/"nwserverLog1.txt"
            path.write_text("old run\n");os.utime(path,(1,1))
            metrics=NativeMetrics(root,root,2)
            self.assertFalse(metrics.ticks)
            path.write_text("header A\nMGLIVE: seq=1 full=1 tick_us=100 bytes=1\n")
            metrics.poll();metrics.poll();self.assertEqual(metrics.ticks,[100])
            with path.open("a") as out:out.write("MGLIVE: seq=2 full=0 tick_us=200")
            metrics.poll();self.assertEqual(metrics.ticks,[100])
            with path.open("a") as out:out.write(" bytes=1\n")
            metrics.poll();self.assertEqual(metrics.ticks,[100,200])
            path.write_text("header B\nMGLIVE: seq=3 full=0 tick_us=300 bytes=1\nTOO MANY INSTRUCTIONS\n")
            metrics.poll();result=metrics.close()
            self.assertEqual(metrics.ticks,[100,200,300])
            self.assertEqual(result["native_error_count"],1)
            self.assertEqual(result["tick_us"]["max"],300)

    def test_plugin_outage_errors_are_separate_and_secrets_redacted(self):
        with tempfile.TemporaryDirectory(prefix="mglive-stress-metrics-") as folder:
            root=Path(folder);(root/"logs.0").mkdir()
            path=root/"server.out"
            path.write_text("E [12:00] NWNX_Redis: connection refused\nE [12:01] NWNX_Redis: connection refused\nI [12:01] harmless\nE [12:02] secret=test\n")
            metrics=NativeMetrics(root,root,0);metrics.poll();result=metrics.close()
            self.assertEqual(result["native_error_count"],0)
            self.assertEqual(result["plugin_errors"],{"redis":2,"other":1})
            self.assertEqual(result["plugin_error_examples"]["redis"],["NWNX_Redis: connection refused"])
            self.assertEqual(result["plugin_error_examples"]["other"],["[credential-related diagnostic redacted]"])

if __name__=="__main__":unittest.main()
