"""Validate measurement aggregation without claiming a GPU run."""
import copy
import unittest

import bench_materials as bench


def record():
    return {
        "schema_version": 1, "renderer": "fixture renderer", "quality": "balanced",
        "iterations": 120, "warmup": 12, "size": [960, 640], "clock": "explicit finish",
        "capabilities": {"gpu": True},
        "results": [
            {"material": "frosted", "case": case,
             **{name: 10.0 for name in bench.TIMINGS},
             "texture_bytes": 1024, "peak_texture_bytes": 2048,
             "stats_delta": {name: 0 for name in bench.COUNTERS}}
            for case in ("cached", "invalidated")
        ],
    }


class MaterialsReportTests(unittest.TestCase):
    def test_valid_measurements_are_preserved(self):
        source = record()
        self.assertIs(bench.validate_record(source), source)

    def test_missing_or_duplicate_workloads_are_rejected(self):
        for rows in ([], record()["results"][:1], record()["results"] * 2):
            candidate = record()
            candidate["results"] = rows
            with self.subTest(rows=len(rows)), self.assertRaises(ValueError):
                bench.validate_record(candidate)

    def test_invalid_timing_values_are_rejected(self):
        for value in (float("nan"), float("inf"), -1.0, True, "10"):
            candidate = record()
            candidate["results"][0]["submit_median_us"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                bench.validate_record(candidate)

    def test_memory_and_counters_require_real_nonnegative_integers(self):
        for field, value in (("texture_bytes", 4096), ("peak_texture_bytes", -1)):
            candidate = record()
            candidate["results"][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                bench.validate_record(candidate)
        for value in (-1, 1.5, True):
            candidate = record()
            candidate["results"][0]["stats_delta"]["captures"] = value
            with self.subTest(counter=value), self.assertRaises(ValueError):
                bench.validate_record(candidate)

    def test_different_renderer_resolution_quality_or_clock_cannot_be_pooled(self):
        for field, value in (("renderer", "other GPU"), ("quality", "high"),
                             ("size", [1280, 720]), ("clock", "different clock"),
                             ("capabilities", {"gpu": False}), ("iterations", 500)):
            second = record()
            second[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                bench.summarize([record(), second])

    def test_summary_keeps_raw_counter_deltas_and_marks_fallbacks(self):
        first = record()
        second = copy.deepcopy(first)
        second["results"][0]["completed_median_us"] = 30.0
        second["results"][0]["stats_delta"]["fallbacks"] = 120
        summary = bench.summarize([first, second])["results"][0]
        self.assertEqual(summary["completed_median_us"], 20.0)
        self.assertEqual(summary["fallback_repetitions"], 1)
        self.assertEqual(summary["repetitions"], 2)
        self.assertEqual(summary["stats_delta_by_repetition"][1]["fallbacks"], 120)


if __name__ == "__main__":
    unittest.main()
