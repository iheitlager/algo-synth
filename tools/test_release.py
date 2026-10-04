import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import release as rel

CHANGELOG = """# Changelog

Intro.

## [Unreleased]

## [0.30.0] - 2026-10-04

### Added

- Old.
"""


class Release(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "changes").mkdir()
        (self.root / "changes" / "README.md").write_text("How to add a fragment.")
        (self.root / "CHANGELOG.md").write_text(CHANGELOG)
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "0.30.0"\n')
        (self.root / "README.md").write_text("# x\n\n## Version: 0.30.0\n")

    def tearDown(self):
        self.tmp.cleanup()

    def add(self, name: str, text: str) -> None:
        (self.root / "changes" / name).write_text(text)

    def test_fragments_become_a_section_and_a_minor_bump(self):
        self.add("190.fixed.md", "- A fix (#190).")
        self.add("188.added.md", "**Two** (#188).")
        self.add("187.added.md", "- One (#187).\n- And more.")
        self.add("189.changed.md", "- Changed (#189).")
        self.assertEqual(rel.release(self.root, "2026-10-05"), "0.31.0")
        log = (self.root / "CHANGELOG.md").read_text()
        want = (
            "## [Unreleased]\n\n## [0.31.0] - 2026-10-05\n\n"
            "### Added\n\n- One (#187).\n- And more.\n- **Two** (#188).\n\n"
            "### Changed\n\n- Changed (#189).\n\n"
            "### Fixed\n\n- A fix (#190).\n\n"
            "## [0.30.0] - 2026-10-04"
        )
        self.assertIn(want, log)
        self.assertIn('version = "0.31.0"', (self.root / "Cargo.toml").read_text())
        self.assertIn("## Version: 0.31.0", (self.root / "README.md").read_text())
        left = sorted(p.name for p in (self.root / "changes").iterdir())
        self.assertEqual(left, ["README.md"], "the fragments are gone, the README stays")

    def test_only_fixes_make_a_patch(self):
        self.add("191.fixed.md", "- Fixed.")
        self.assertEqual(rel.release(self.root, "2026-10-05"), "0.30.1")

    def test_nothing_to_release_changes_nothing(self):
        self.assertIsNone(rel.release(self.root, "2026-10-05"))
        self.assertEqual((self.root / "CHANGELOG.md").read_text(), CHANGELOG)

    def test_a_badly_named_fragment_stops_before_writing(self):
        self.add("192.added.md", "- Fine.")
        self.add("oops.md", "- Not a fragment.")
        with self.assertRaises(SystemExit):
            rel.release(self.root, "2026-10-05")
        self.assertEqual((self.root / "CHANGELOG.md").read_text(), CHANGELOG)
        self.assertTrue((self.root / "changes" / "192.added.md").exists())


if __name__ == "__main__":
    unittest.main()
