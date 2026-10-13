#!/usr/bin/env python3
"""Output-path safety regressions; no pricing process or measurement is started."""
from pathlib import Path
import tempfile
import unittest

from workload import validate_output_paths


class WorkloadPaths(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="copperline-workload-paths-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_dotdot_cannot_hide_equal_or_nested_outputs(self):
        (self.root / "sub").mkdir()
        data = self.root / "data"
        for evidence in [self.root / "sub" / ".." / "data",
                         self.root / "sub" / ".." / "data" / "evidence",
                         self.root / "sub" / ".."]:
            with self.subTest(evidence=evidence), self.assertRaises(RuntimeError):
                validate_output_paths(data, evidence)
        self.assertFalse(data.exists())

    def test_separate_outputs_resolve_without_creating_them(self):
        (self.root / "sub").mkdir()
        data, evidence = validate_output_paths(self.root / "data", self.root / "sub" / ".." / "evidence")
        self.assertEqual(data, self.root.resolve() / "data")
        self.assertEqual(evidence, self.root.resolve() / "evidence")
        self.assertFalse(data.exists())
        self.assertFalse(evidence.exists())

    def test_existing_output_is_preserved(self):
        data = self.root / "data"
        data.mkdir()
        original = data / "owned-fixture"
        original.write_bytes(b"preserve")
        with self.assertRaises(RuntimeError):
            validate_output_paths(data, self.root / "evidence")
        self.assertEqual(original.read_bytes(), b"preserve")
        self.assertFalse((self.root / "evidence").exists())

    def test_windows_namespace_spellings_are_refused(self):
        for spelling in (r"\\?\C:\scratch\data", r"\\?\UNC\host\share\data", r"\\.\C:\scratch\data"):
            with self.subTest(spelling=spelling), self.assertRaises(RuntimeError):
                validate_output_paths(Path(spelling), self.root / "evidence")
            with self.subTest(evidence=spelling), self.assertRaises(RuntimeError):
                validate_output_paths(self.root / "data", Path(spelling))
        self.assertEqual(list(self.root.iterdir()), [])

    def test_aliases_and_dangling_symlinks_are_rejected(self):
        alias = self.root / "alias"
        try:
            alias.symlink_to(self.root, target_is_directory=True)
        except (OSError, NotImplementedError) as error:
            self.skipTest(f"Directory symlinks unavailable: {error}")
        data = self.root / "data"
        with self.assertRaises(RuntimeError):
            validate_output_paths(data, alias / "data" / "evidence")
        dangling = self.root / "dangling"
        dangling.symlink_to(self.root / "missing", target_is_directory=True)
        with self.assertRaises(RuntimeError):
            validate_output_paths(dangling, self.root / "evidence")
        self.assertTrue(dangling.is_symlink())
        self.assertFalse(data.exists())


if __name__ == "__main__":
    unittest.main()
