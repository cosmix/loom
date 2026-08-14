"""Regression tests for the conservative GFM table normalizer."""

import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("fix-md-tables.py")
SPEC = importlib.util.spec_from_file_location("fix_md_tables", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.dont_write_bytecode = True
SPEC.loader.exec_module(MODULE)


class FixMarkdownTablesTests(unittest.TestCase):
    def test_preserves_escaped_pipe_in_a_cell(self):
        output = MODULE.fix_tables_in_content(
            "| A | B |\n| --- | --- |\n| x\\|y | z |\n"
        )
        self.assertIn("| x\\|y | z   |", output)

    def test_preserves_an_escaped_terminal_pipe(self):
        output = MODULE.fix_tables_in_content("| A |\n| --- |\n| x\\|\n")
        self.assertIn("| x\\| |", output)

    def test_preserves_pipe_lines_in_fenced_code(self):
        content = "```text\n| A | B |\n| --- | --- |\n| x | y |\n```\n"
        self.assertEqual(MODULE.fix_tables_in_content(content), content)

    def test_does_not_close_a_fence_with_trailing_text(self):
        content = "```text\n```not-a-close\n| A | B |\n| --- | --- |\n| x | y |\n```\n"
        self.assertEqual(MODULE.fix_tables_in_content(content), content)

    def test_preserves_pipe_bounded_prose_without_delimiter(self):
        content = "| one | two |\n| three | four |\n"
        self.assertEqual(MODULE.fix_tables_in_content(content), content)

    def test_requires_matching_header_and_delimiter_widths(self):
        content = "| A | B |\n| --- |\n| x |\n"
        self.assertEqual(MODULE.fix_tables_in_content(content), content)

    def test_preserves_blockquoted_table(self):
        content = "> | A | B |\n> | --- | --- |\n> | x | y |\n"
        self.assertEqual(MODULE.fix_tables_in_content(content), content)

    def test_preserves_indented_code_that_looks_like_a_table(self):
        content = "    | A | B |\n    | --- | --- |\n    | x | y |\n"
        self.assertEqual(MODULE.fix_tables_in_content(content), content)

    def test_preserves_valid_zero_to_three_space_table_indent(self):
        for indent_width in range(4):
            with self.subTest(indent_width=indent_width):
                indent = " " * indent_width
                content = f"{indent}| A | B |\n{indent}| --- | --- |\n"
                output = MODULE.fix_tables_in_content(content)
                self.assertTrue(
                    all(line.startswith(indent) for line in output.splitlines())
                )

    def test_preserves_table_like_content_in_raw_html(self):
        cases = (
            "<pre>\n| A | B |\n| --- | --- |\n| x | y |\n</pre>\n",
            "<x-widget>\n| A | B |\n| --- | --- |\n</x-widget>\n\n",
        )
        for content in cases:
            with self.subTest(content=content):
                self.assertEqual(MODULE.fix_tables_in_content(content), content)

    def test_blockquote_with_pipes_terminates_a_table(self):
        content = "| A | B |\n| --- | --- |\n> quoted | text\n"
        output = MODULE.fix_tables_in_content(content)
        self.assertIn("\n\n> quoted | text\n", output)

    def test_formats_a_pipe_less_gfm_body_row(self):
        output = MODULE.fix_tables_in_content("| A | B |\n| --- | --- |\nvalue\n")
        self.assertIn("| value |     |", output)

    def test_does_not_treat_dash_only_body_cell_as_a_delimiter(self):
        output = MODULE.fix_tables_in_content("| A |\n| --- |\n| - |\n")
        self.assertIn("| -   |", output)

    def test_pads_ragged_body_row_with_an_empty_cell(self):
        output = MODULE.fix_tables_in_content("| A | B |\n| --- | --- |\n| x |\n")
        self.assertIn("| x   |     |", output)
        self.assertNotIn("| x   | --- |", output)

    def test_drops_excess_body_cells_to_the_header_width(self):
        output = MODULE.fix_tables_in_content(
            "| A | B |\n| --- | --- |\n| x | y | z |\n"
        )
        self.assertIn("| x   | y   |", output)
        self.assertNotIn("z", output)


if __name__ == "__main__":
    unittest.main()
