"""Runner regression tests; no game installation required."""
import unittest
from test_translation_budget import measure


class Proof:
    def __init__(self, row=None):
        self.row = row
        self.compiled = False

    def chunk(self, code):
        assert "NWNX_" not in code, "NWNX calls are disabled in ExecuteScriptChunk"
        assert "MGET_XL_LABEL" in code

    def command(self, **args):
        assert args == {"op": "run_script", "script": "mget_xlbudget"}
        self.compiled = True

    def native(self, expression):
        assert self.compiled, "measurement must execute a compiled fixture"
        return self.row if self.row is not None else {
            "label": "baseline", "channel": 1, "captured": 1,
            "instructions": 200, "limit": 10000,
        }


class BudgetRunnerTests(unittest.TestCase):
    def test_measurement_uses_compiled_fixture(self):
        self.assertEqual(measure(Proof(), 1, "baseline")["captured"], 1)

    def test_missing_or_stale_measurement_is_rejected(self):
        for row in [None, {}, {"captured": 0, "instructions": 0, "limit": 0},
                    {"label": "old", "channel": 1, "captured": 1,
                     "instructions": 200, "limit": 10000}]:
            with self.subTest(row=row), self.assertRaises(AssertionError):
                measure(Proof({} if row is None else row), 1, "baseline")

    def test_instruction_budget_is_enforced(self):
        for used, limit in [(0, 0), (0, 10000), (5000, 10000)]:
            with self.subTest(used=used, limit=limit), self.assertRaises(AssertionError):
                measure(Proof({"label": "baseline", "channel": 1,
                               "captured": 1, "instructions": used,
                               "limit": limit}), 1, "baseline")


if __name__ == "__main__":
    unittest.main()
