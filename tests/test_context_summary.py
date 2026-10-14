import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "skills/kemi/scripts/context_summary.py"


def context():
    return {
        "kemi": 3,
        "review": "01K00000000000000000000000",
        "diff_display_version": "01K00000000000000000000000:1",
        "explanation_revision": 1,
        "explanation": {
            "revision": 1,
            "base_display_version": "01K00000000000000000000000:1",
            "sections": [{
                "id": "overview", "title": "概要", "blocks": [
                    {"id": "intent", "part": {"type": "text", "body": "大きな本文" * 1000}, "references": []},
                    {"id": "diagram", "part": {"type": "diagram", "source": "digraph G { A -> B; }", "title": "Flow", "role": "example"}, "references": [{"type": "page", "url": "/preview"}]},
                ],
            }],
        },
        "references": {
            "code": [
                {"unit": "file", "group_id": "all", "path": "src/a.rs", "old_present": True, "new_present": True, "old_lines": None, "new_lines": 10},
                {"unit": "commit", "group_id": "commit1", "path": "src/a.rs", "old_present": False, "new_present": True, "old_lines": None, "new_lines": 10},
                {"unit": "file", "group_id": "all", "path": "src/b.rs", "old_present": True, "new_present": True, "old_lines": 20, "new_lines": 20},
            ],
            "pages": [{"url": "/preview"}, {"url": "/other"}],
        },
    }


class ContextSummaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location("context_summary", SCRIPT)
        cls.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.module)

    def test_initial_overview_excludes_bodies_and_full_reference_lists(self):
        summary = self.module.project(context())
        self.assertTrue(summary["projection"])
        self.assertEqual(summary["available"], {"code": 3, "pages": 2, "sections": 1, "blocks": 2})
        self.assertEqual(summary["outline"], [{"id": "overview", "title": "概要", "blocks": ["intent", "diagram"]}])
        self.assertEqual(summary["references"], {"code": [], "pages": []})
        self.assertEqual(summary["selected_sections"], [])
        self.assertNotIn("大きな本文", json.dumps(summary, ensure_ascii=False))

    def test_path_selection_preserves_every_matching_unit_and_unknown_line_count(self):
        source = context()
        summary = self.module.project(source, paths=["src/a.rs"])
        self.assertEqual(summary["references"]["code"], source["references"]["code"][:2])
        self.assertEqual(summary["diff_display_version"], source["diff_display_version"])
        self.assertEqual(summary["explanation_revision"], 1)

    def test_selected_blocks_keep_typed_parts_and_references_without_mutating_input(self):
        source = context()
        before = copy.deepcopy(source)
        summary = self.module.project(source, blocks=["diagram"], pages=["/preview"])
        self.assertEqual(summary["selected_sections"][0]["blocks"], [source["explanation"]["sections"][0]["blocks"][1]])
        self.assertEqual(summary["references"]["pages"], [{"url": "/preview"}])
        self.assertEqual(source, before)

    def test_a_context_without_an_explanation_still_reports_available_references(self):
        source = context()
        source["explanation"] = None
        source["explanation_revision"] = 0
        summary = self.module.project(source)
        self.assertEqual(summary["outline"], [])
        self.assertEqual(summary["available"]["blocks"], 0)

    def test_missing_selections_fail_instead_of_silently_returning_an_empty_match(self):
        for options in [{"paths": ["missing.rs"]}, {"pages": ["/missing"]}, {"blocks": ["missing"]}]:
            with self.subTest(options=options), self.assertRaises(ValueError):
                self.module.project(context(), **options)

    def test_unknown_contract_versions_and_non_context_documents_are_rejected(self):
        for source in [{"kemi": 2}, {"kemi": 3, "verdict": "approved"}, [], None]:
            with self.subTest(source=source), self.assertRaises(ValueError):
                self.module.project(source)

    def test_command_reads_stdin_and_prints_one_compact_projection(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "-", "--path", "src/a.rs"], input=json.dumps(context()), capture_output=True, text=True, encoding="utf-8", timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(json.loads(result.stdout)["references"]["code"]), 2)
        self.assertEqual(result.stderr, "")

    def test_invalid_input_or_missing_selection_has_empty_stdout_and_exit_two(self):
        for value, args in [("{", []), (json.dumps(context()), ["--path", "missing.rs"])]:
            result = subprocess.run([sys.executable, str(SCRIPT), "-", *args], input=value, capture_output=True, text=True, encoding="utf-8", timeout=5)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(result.stdout, "")
            self.assertTrue(result.stderr)

    def test_reading_a_saved_context_keeps_its_original_bytes_and_selected_block(self):
        with tempfile.TemporaryDirectory(prefix="kemi-context-test-") as directory:
            saved = Path(directory) / "context.json"
            raw = json.dumps(context(), ensure_ascii=False).encode("utf-8")
            saved.write_bytes(raw)
            result = subprocess.run([sys.executable, str(SCRIPT), str(saved), "--block", "diagram"], capture_output=True, encoding="utf-8", timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(saved.read_bytes(), raw)
            block = json.loads(result.stdout)["selected_sections"][0]["blocks"][0]
            self.assertEqual(block, context()["explanation"]["sections"][0]["blocks"][1])

    def test_duplicate_keys_and_non_json_numbers_are_rejected_without_output(self):
        raw = json.dumps(context())
        invalid = [raw.replace('"kemi": 3', '"kemi": 3, "kemi": 3', 1), raw.replace('"old_lines": null', '"old_lines": NaN', 1)]
        for value in invalid:
            result = subprocess.run([sys.executable, str(SCRIPT), "-"], input=value, capture_output=True, encoding="utf-8", timeout=5)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
