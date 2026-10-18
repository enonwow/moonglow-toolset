"""Runner regression tests; no game installation required."""
import unittest
from unittest.mock import patch
from test_translation_budget import measure, run


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


class PopulationProof:
    """Model a reporter that cannot finish a snapshot of the loaded area."""
    def __init__(self, short_population=False, wrong_report=False,
                 stale_report=False, short_creatures=False):
        self.state = {"seq": 1, "areas": [{"area": "events"}]}
        self.reporting_switched = False
        self.reporting_confirmed = False
        self.placeables = 0
        self.creatures = 0
        self.short_population = short_population
        self.wrong_report = wrong_report
        self.stale_report = stale_report
        self.short_creatures = short_creatures

    def chunk(self, code):
        if 'SetLocalString(GetModule(),"MGLIVE_AREAS","events2")' in code:
            self.reporting_switched = True
        if '"XL_BUDGET_FILLER"' in code:
            self.placeables += 128
        if '"XL_BUDGET_NPC"' in code:
            self.creatures += 32
        if self.placeables > 1024 and not self.reporting_confirmed:
            raise AssertionError("report stalled: stress area still in snapshots")

    def command(self, **args):
        assert args == {"op": "run_script", "script": "mget_xl"}

    def until(self, predicate):
        assert self.placeables == self.creatures == 0, "confirm isolation before loading"
        assert not predicate(), "a pre-switch report cannot confirm isolation"
        if self.reporting_switched:
            self.state = {"seq": 1 if self.stale_report else 2,
                          "areas": [{"area": "events" if self.wrong_report else "events2"}]}
        assert predicate(), "fresh report must exclude the stress area"
        self.reporting_confirmed = True

    def native(self, expression):
        if "MGET_XL_SETUP_FAILED" in expression:
            return False
        if "MGET_XL_PLACEABLES" in expression:
            return self.placeables - int(self.short_population)
        if "MGET_XL_CREATURES" in expression:
            return self.creatures - int(self.short_creatures)
        raise AssertionError(expression)


def sample(_proof, channel, label):
    return {"channel": channel, "label": label,
            "captured": int(label != "budget_crowded"),
            "instructions": 200, "limit": 10000}


class BudgetRunnerTests(unittest.TestCase):
    @patch("test_translation_budget.measure", side_effect=sample)
    def test_population_does_not_block_measurement_reports(self, _measure):
        proof = PopulationProof()
        result = run(proof)
        self.assertTrue(proof.reporting_confirmed)
        self.assertEqual(result["placeables"], 4096)
        self.assertEqual(result["extra_creatures"], 288)
        self.assertEqual(proof.placeables, 4096)
        self.assertEqual(proof.creatures, 288)

    @patch("test_translation_budget.measure", side_effect=sample)
    def test_incomplete_population_is_rejected(self, _measure):
        with self.assertRaisesRegex(AssertionError, "placeable population"):
            run(PopulationProof(short_population=True))

    @patch("test_translation_budget.measure", side_effect=sample)
    def test_wrong_reporting_area_is_rejected_before_population(self, _measure):
        for args in [{"wrong_report": True}, {"stale_report": True}]:
            with self.subTest(args=args):
                proof = PopulationProof(**args)
                with self.assertRaisesRegex(AssertionError, "fresh report"):
                    run(proof)
                self.assertEqual(proof.placeables, 0)

    @patch("test_translation_budget.measure", side_effect=sample)
    def test_incomplete_creature_population_is_rejected(self, _measure):
        with self.assertRaisesRegex(AssertionError, "creature population"):
            run(PopulationProof(short_creatures=True))

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
