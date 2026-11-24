#!/usr/bin/env python3
"""Unit tests for scripts/strip_spec_docs.py helpers."""

import copy
import unittest

import strip_spec_docs


class TestLeb128(unittest.TestCase):
    def test_round_trip_boundaries(self) -> None:
        cases = (
            (0, b"\x00"),
            (127, b"\x7f"),
            (128, b"\x80\x01"),
            (16383, b"\xff\x7f"),
            (2**32, b"\x80\x80\x80\x80\x10"),
        )
        for value, expected in cases:
            with self.subTest(value=value):
                encoded = strip_spec_docs.leb_write(value)
                self.assertEqual(encoded, expected)
                self.assertEqual(
                    strip_spec_docs.leb_read(encoded, 0), (value, len(encoded))
                )

    def test_read_uses_offset_and_ignores_trailing_bytes(self) -> None:
        encoded = strip_spec_docs.leb_write(16383)
        data = b"\xff" + encoded + b"tail"
        self.assertEqual(strip_spec_docs.leb_read(data, 1), (16383, 1 + len(encoded)))


class TestBlankDocs(unittest.TestCase):
    def test_blanks_nested_docs_and_preserves_other_fields(self) -> None:
        spec = {
            "doc": "top-level doc",
            "name": "foo",
            "count": 3,
            "enabled": True,
            "missing": None,
            "nested": {
                "doc": "nested doc",
                "value": "keep",
                "entries": [
                    {"doc": "list doc", "kind": "function"},
                    {"name": "bar", "items": [{"doc": "deep doc", "index": 1}, 7]},
                ],
            },
            "top_list": [{"doc": "another doc", "flag": False}],
        }
        original = copy.deepcopy(spec)
        expected = {
            "doc": "",
            "name": "foo",
            "count": 3,
            "enabled": True,
            "missing": None,
            "nested": {
                "doc": "",
                "value": "keep",
                "entries": [
                    {"doc": "", "kind": "function"},
                    {"name": "bar", "items": [{"doc": "", "index": 1}, 7]},
                ],
            },
            "top_list": [{"doc": "", "flag": False}],
        }

        self.assertEqual(strip_spec_docs.blank_docs(spec), expected)
        self.assertEqual(spec, original)


if __name__ == "__main__":
    unittest.main()
