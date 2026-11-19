import unittest

from report import paired_interval, quantile, summarize


class ReportingTests(unittest.TestCase):
    def test_latency_percentiles_use_individual_samples_and_keep_the_tail(self):
        samples = list(range(1, 100)) + [10000]
        self.assertEqual(quantile(samples, 0.5), 50)
        self.assertEqual(quantile(samples, 0.95), 95)
        self.assertEqual(quantile(samples, 0.99), 99)
        self.assertEqual(quantile(samples, 1.0), 10000)

    def test_constant_paired_ratio_has_exact_interval(self):
        for value in (0.5, 1.0, 2.0):
            result = paired_interval([value] * 7)
            for key in ("point", "lower_95", "upper_95"):
                self.assertAlmostEqual(result[key], value)

    def test_paired_ratio_uses_geometric_mean_across_rounds(self):
        result = paired_interval([0.5, 1.0, 2.0])
        self.assertAlmostEqual(result["point"], 1.0)
        self.assertLessEqual(result["lower_95"], 1.0)
        self.assertGreaterEqual(result["upper_95"], 1.0)

    def test_incomplete_backend_measurement_cannot_be_reported(self):
        with self.assertRaisesRegex(ValueError, "both implementations"):
            summarize([{"measurement": {"rows": 1, "workload": "fresh", "backend": "zgpui"}}], [])


if __name__ == "__main__":
    unittest.main()
