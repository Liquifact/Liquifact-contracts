#!/usr/bin/env python3
"""Unit tests for scripts/check_wasm_size.py."""

import contextlib
import io
import tempfile
import unittest
from pathlib import Path

import check_wasm_size


class TestCheckWasmSize(unittest.TestCase):
    def test_ok_within_budget(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            wasm = Path(tmpdir) / "test.wasm"
            wasm.write_bytes(b"\x00" * 1024)

            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                code = check_wasm_size.main([str(wasm), "--max", "2048"])

            self.assertEqual(code, check_wasm_size.EXIT_OK)
            self.assertIn("OK:", stdout.getvalue())
            self.assertIn("1024 bytes", stdout.getvalue())

    def test_ok_exact_budget(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            wasm = Path(tmpdir) / "exact.wasm"
            wasm.write_bytes(b"\x00" * 100)

            stdout = io.StringIO()
            with contextlib.redirect_stdout(stdout):
                code = check_wasm_size.main([str(wasm), "--max", "100"])

            self.assertEqual(code, check_wasm_size.EXIT_OK)
            self.assertIn("0 bytes remaining", stdout.getvalue())

    def test_over_budget(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            wasm = Path(tmpdir) / "big.wasm"
            wasm.write_bytes(b"\x00" * 2048)

            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                code = check_wasm_size.main([str(wasm), "--max", "1024"])

            self.assertEqual(code, check_wasm_size.EXIT_OVER_BUDGET)
            self.assertIn("OVER BUDGET:", stderr.getvalue())
            self.assertIn("by 1024 bytes", stderr.getvalue())

    def test_missing_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            missing = Path(tmpdir) / "does_not_exist.wasm"

            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr):
                code = check_wasm_size.main([str(missing)])

            self.assertEqual(code, check_wasm_size.EXIT_MISSING_FILE)
            self.assertIn("ERROR: WASM file not found", stderr.getvalue())

    def test_default_budget_is_128_kib(self) -> None:
        self.assertEqual(check_wasm_size.DEFAULT_MAX_BYTES, 131072)

    def test_negative_max_rejected(self) -> None:
        with self.assertRaises(SystemExit) as ctx:
            with contextlib.redirect_stderr(io.StringIO()):
                check_wasm_size.main(["--max", "-1"])
        self.assertEqual(ctx.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
