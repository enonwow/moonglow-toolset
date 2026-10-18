"""Runner acceptance checks; no game data or engine required."""
import unittest
from test_dense_report import completed_scan, check_frame


def frame(seq, index, complete, ids, generation=1):
    return {"seq": seq, "areas": [{"area": "events", "paged": True,
        "proof_instructions": 100, "proof_limit": 10000,
        "cre": [{"id": "a", "pc": True}, {"id": "b", "dm": True}, {"id": "c"}],
        "static_page": {"generation": generation, "index": index, "complete": complete},
        "plc": [{"id": str(i), "tag": "DENSE_FILLER"} for i in ids]}, {"area": "events2"}]}


class DenseProof(unittest.TestCase):
    def test_convergence_counts_unique_ids_across_pages(self):
        frames = [frame(1, 0, False, range(600)), frame(2, 1, True, range(500, 1152))]
        self.assertEqual(completed_scan(frames, {"a", "b", "c"}, 1152)["full_reports"], 2)

    def test_missing_page_or_generation_cannot_claim_completeness(self):
        self.assertIsNone(completed_scan([frame(1, 0, False, range(100)), frame(3, 2, True, range(100, 300))], {"a"}, 300))
        self.assertIsNone(completed_scan([frame(1, 0, False, range(100)), frame(2, 1, True, range(100, 300), 2)], {"a"}, 300))

    def test_new_scan_recovers_after_gap(self):
        frames = [frame(1, 1, True, [0]), frame(2, 0, False, [0], 2), frame(3, 1, True, [1], 2)]
        self.assertEqual(completed_scan(frames, {"a"}, 2)["objects"], 2)

    def test_every_frame_requires_all_creatures_and_priority_order(self):
        f = frame(1, 0, True, [])
        with self.assertRaises(AssertionError):
            check_frame(f, {"missing"})
        f["areas"][0]["cre"].reverse()
        with self.assertRaises(AssertionError):
            check_frame(f, {"a"})

    def test_partial_creature_report_fails_even_if_all_expected_actors_present(self):
        f = frame(1, 0, True, [])
        f["areas"][0]["cre_partial"] = True
        with self.assertRaises(AssertionError):
            check_frame(f, {"a"})


if __name__ == "__main__":
    unittest.main()
