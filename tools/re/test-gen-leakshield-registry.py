#!/usr/bin/env python3
"""Test of gen-leakshield-registry.py: it keeps the verification states that a Verification write set.

Run: python3 -I tools/re/test-gen-leakshield-registry.py
The test copies the registry, the fixture and the protocol document into a temporary directory,
runs the generator there and compares the registry with the original.
"""

import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
FILES = (
    "crates/nicecooling-protocol/registry/leakshield.toml",
    "tests/fixtures/leakshield/fw1022/report3.bin",
    "docs/protocol/leakshield.md",
)


def generate(repo):
    return subprocess.run(
        [sys.executable, "-I", str(ROOT / "tools/re/gen-leakshield-registry.py"), str(repo)],
        capture_output=True, text=True, check=True,
    )


class Generator(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self.tmp.name)
        for name in FILES:
            (self.repo / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / name, self.repo / name)
        self.registry = self.repo / FILES[0]

    def tearDown(self):
        self.tmp.cleanup()

    def test_a_run_changes_nothing(self):
        before = self.registry.read_text()
        generate(self.repo)
        self.assertEqual(self.registry.read_text(), before)

    def test_failed_stays(self):
        # `display_brightness_idle` is a low-risk field that the generator sets to plausible-read.
        text = self.registry.read_text()
        head, tail = text.split('id = "display_brightness_idle"')
        self.assertIn('verification_state = "plausible-read"', tail.split("[[field]]")[0])
        tail = tail.replace('verification_state = "plausible-read"', 'verification_state = "failed"', 1)
        self.registry.write_text(head + 'id = "display_brightness_idle"' + tail)
        generate(self.repo)
        block = self.registry.read_text().split('id = "display_brightness_idle"')[1].split("[[field]]")[0]
        self.assertIn('verification_state = "failed"', block)

    def test_the_count_of_verified_write_stays(self):
        count = self.registry.read_text().count('"verified-write"')
        self.assertGreater(count, 0)
        generate(self.repo)
        self.assertEqual(self.registry.read_text().count('"verified-write"'), count)


if __name__ == "__main__":
    unittest.main()
