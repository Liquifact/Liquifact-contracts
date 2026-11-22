#!/usr/bin/env python3
"""Check that the compiled Soroban escrow WASM stays within the size budget.

Soroban enforces a 128 KiB (131,072 bytes) contract size limit. Run this script
after building the release WASM to catch size regressions locally.

Usage:
    python3 scripts/check_wasm_size.py [--max BYTES] [WASM_PATH]

Exit codes:
    0  WASM file is within the size budget
    1  WASM file exceeds the size budget
    2  WASM file is missing (or not a regular file)
"""

import argparse
import sys
from pathlib import Path

DEFAULT_MAX_BYTES = 131072  # 128 KiB Soroban contract size limit
REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_WASM_PATH = (
    REPO_ROOT / "target" / "wasm32v1-none" / "release" / "liquifact_escrow.wasm"
)

EXIT_OK = 0
EXIT_OVER_BUDGET = 1
EXIT_MISSING_FILE = 2


def non_negative_int(value: str) -> int:
    try:
        parsed = int(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError(f"invalid integer value: {value!r}") from exc
    if parsed < 0:
        raise argparse.ArgumentTypeError(f"--max must be non-negative, got {parsed}")
    return parsed


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "Check that the compiled liquifact_escrow WASM fits within the "
            "Soroban size budget."
        )
    )
    parser.add_argument(
        "wasm_path",
        nargs="?",
        type=Path,
        default=DEFAULT_WASM_PATH,
        help=(
            "Path to the compiled .wasm file "
            "(default: target/wasm32v1-none/release/liquifact_escrow.wasm)"
        ),
    )
    parser.add_argument(
        "--max",
        dest="max_bytes",
        type=non_negative_int,
        default=DEFAULT_MAX_BYTES,
        help=f"Maximum allowed WASM size in bytes (default: {DEFAULT_MAX_BYTES})",
    )
    return parser


def check_wasm_size(wasm_path: Path, max_bytes: int) -> int:
    if not wasm_path.is_file():
        print(
            f"ERROR: WASM file not found: {wasm_path}\n"
            "Build it first with: cargo build --target wasm32v1-none --release -p liquifact_escrow",
            file=sys.stderr,
        )
        return EXIT_MISSING_FILE

    size = wasm_path.stat().st_size
    size_kib = size / 1024
    max_kib = max_bytes / 1024

    if size > max_bytes:
        over_by = size - max_bytes
        print(
            f"OVER BUDGET: {wasm_path} is {size} bytes ({size_kib:.2f} KiB), "
            f"exceeds budget of {max_bytes} bytes ({max_kib:.2f} KiB) by {over_by} bytes.",
            file=sys.stderr,
        )
        return EXIT_OVER_BUDGET

    remaining = max_bytes - size
    print(
        f"OK: {wasm_path} is {size} bytes ({size_kib:.2f} KiB), "
        f"within budget of {max_bytes} bytes ({max_kib:.2f} KiB) "
        f"({remaining} bytes remaining)."
    )
    return EXIT_OK


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return check_wasm_size(args.wasm_path, args.max_bytes)


if __name__ == "__main__":
    sys.exit(main())
