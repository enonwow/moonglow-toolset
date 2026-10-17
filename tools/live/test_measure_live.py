"""Offline contract of private measurement generation; no server or game data."""
import unittest
from pathlib import Path

from measure_live import instrument, stats


class MeasurementContract(unittest.TestCase):
    def test_instrumentation_is_explicit_and_does_not_mutate_sources(self):
        root = Path(__file__).resolve().parents[2] / "tools/live/nwn"
        inc = (root / "mglive_inc.nss").read_text()
        tick = (root / "mglive_tick.nss").read_text()
        generated, measured = instrument(inc, tick)
        self.assertIn('int MP_look = 0;', generated)
        self.assertIn('MP_look += GetMicrosecondCounter() - mpLook;', generated)
        self.assertIn('MP_npc_builds++;', generated)
        self.assertIn('else MP_npc_hits++;', generated)
        self.assertIn('else look = MgLiveNpcLook(o);', generated)
        self.assertIn('bFull && GetLocalInt(o, "MP_LOOK")', generated)
        self.assertIn('MP_text += GetMicrosecondCounter() - start;', generated)
        self.assertEqual(measured.count('NWNX_Redis_PUBLISH('), 1)
        self.assertEqual(measured.count('"MP_TICK"'), 1)
        self.assertNotIn('DelayCommand(', measured)
        self.assertEqual(inc, (root / "mglive_inc.nss").read_text())
        self.assertEqual(tick, (root / "mglive_tick.nss").read_text())
        simple, simple_tick = instrument(inc, tick, row_timers=False)
        self.assertNotIn("MP_cre_build", simple)
        self.assertNotIn("cre_build=", simple_tick)

    def test_missing_anchor_rejects_a_stale_probe(self):
        with self.assertRaises(AssertionError):
            instrument("// unsupported source", "void main() {}")

    def test_empty_sample_is_not_zero_cost(self):
        self.assertEqual(stats([]), {"n": 0})
        self.assertEqual(stats([1, 2, 3])["median"], 2)


if __name__ == "__main__":
    unittest.main()
