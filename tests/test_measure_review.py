import json
import unittest
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location('dreamer_measure_review', Path(__file__).resolve().parents[1] / 'scripts/measure_review.py')
measurement = importlib.util.module_from_spec(spec)
spec.loader.exec_module(measurement)
summarize_events = measurement.summarize_events
usage_counts = measurement.usage_counts


class UsageTests(unittest.TestCase):
    def events(self, *extra):
        return [json.dumps(e) for e in [
            {'type': 'item.completed', 'item': {'type': 'agent_message', 'text': '{}'}},
            {'type': 'turn.completed', 'usage': {'input_tokens': 100, 'cached_input_tokens': 60, 'output_tokens': 30, 'reasoning_output_tokens': 20}},
            *extra,
        ]]

    def test_cache_and_reasoning_are_not_counted_twice(self):
        r = summarize_events(self.events(), 0, 1.5)
        self.assertTrue(r['comparable'])
        self.assertEqual(r['usage']['uncached_input_tokens'], 40)
        self.assertEqual(r['usage']['input_plus_output_tokens'], 130)

    def test_missing_usage_is_unknown_not_zero(self):
        self.assertIsNone(usage_counts(None))
        self.assertIsNone(usage_counts({'input_tokens': 10, 'output_tokens': 3}))
        self.assertFalse(summarize_events(self.events()[:-1], 0, 1)['comparable'])

    def test_invalid_counters_and_cache_subset_are_rejected(self):
        for n in [-1, True, 1.5]:
            self.assertIsNone(usage_counts({'input_tokens': n, 'cached_input_tokens': 0, 'output_tokens': 3}))
        self.assertIsNone(usage_counts({'input_tokens': 1, 'cached_input_tokens': 2, 'output_tokens': 3}))

    def test_timeout_failure_and_duplicate_turns_are_not_success(self):
        self.assertFalse(summarize_events(self.events(), -15, 240, True)['comparable'])
        self.assertFalse(summarize_events(self.events(), 1, 10)['comparable'])
        self.assertFalse(summarize_events(self.events({'type': 'turn.failed'}), 0, 10)['comparable'])
        duplicate = {'type': 'turn.completed', 'usage': {'input_tokens': 5, 'cached_input_tokens': 0, 'output_tokens': 2}}
        self.assertFalse(summarize_events(self.events(duplicate), 0, 10)['comparable'])

    def test_no_answer_is_not_a_completed_review(self):
        self.assertFalse(summarize_events(self.events()[1:], 0, 1)['comparable'])

    def test_command_output_accounting_does_not_retain_arguments(self):
        r = summarize_events(self.events({'type': 'item.completed', 'item': {'type': 'command_execution', 'command': 'private command', 'aggregated_output': 'hé'}}), 0, 1)
        self.assertEqual(r['command_count'], 1)
        self.assertEqual(r['command_output_bytes'], 3)
        self.assertNotIn('private command', json.dumps(r))


if __name__ == '__main__':
    unittest.main()
