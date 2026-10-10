"""The song speaks Ableton's words (#486, ADR-0031): `clip`, `scene`, `snapshot`.

No tracked file outside the history uses the old words for the song's parts:
`frag` (now `clip`) or `section` (now `scene`). The old `scene <name>:`
snapshot form can't be told apart by a grep, so the parser's tests guard it.

Allowed: the history (`CHANGELOG.md`, `changes/`, ADRs before 0031), the
parser's reading of the old words and its tests, the old-words table, and
"section" where it means something else (a synth faceplate's panel, the
mixer's master section, a string section, the HTML element).
"""

import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OLD = re.compile(r"\b(frags?|fragments?|sections?)\b", re.IGNORECASE)
TEXT = (".rs", ".ts", ".vue", ".js", ".mjs", ".md", ".song", ".json", ".py", ".html", ".css", ".toml", ".yml")

# Whole files: history, or "section" as a faceplate's or the master's panel.
SKIP_FILES = {
    "CHANGELOG.md",
    "CLAUDE.md",
    "tools/release.py",
    "tools/test_release.py",
    "tools/test_words.py",
    "web/src/audio/models.ts",
    "web/src/audio/models.test.ts",
    "web/src/components/SynthFaceplate.vue",
    "web/src/components/console/MasterSection.vue",
}
SKIP_DIRS = ("changes/", "web/public/samples/", "tools/samples/", "node_modules/")
# ADRs keep the words of their time; 0031 and the index name both.
ADR = re.compile(r"^\.openspec/adr/")

# A line with any of these uses an old word for something else, or on purpose.
ALLOWED = (
    "<section",
    "</section",
    "MasterSection",
    "master section",
    "modulation sections",
    "filter section",
    "string section",
    "changelog fragment",
    "def.sections",
    # The parser reads the old words (song::current_keyword) and tests that.
    '"frag"',
    '"section"',
    '"frag "',
    '"section "',
    "frag beat",
    "section a 2",
    "section b 2",
    "`frag`",
    "`section`",
    # The editor grammars read them as the parser does (#482).
    "'frag', 'section'",
)


def tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True)
    return out.stdout.split()


def strays() -> list[str]:
    found = []
    for path in tracked():
        if not path.endswith(TEXT) or path in SKIP_FILES or path.startswith(SKIP_DIRS) or ADR.match(path):
            continue
        try:
            text = (ROOT / path).read_text(encoding="utf-8")
        except (UnicodeDecodeError, FileNotFoundError):
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if OLD.search(line) and not any(a in line for a in ALLOWED):
                found.append(f"{path}:{n}: {line.strip()[:120]}")
    return found


class TestWords(unittest.TestCase):
    def test_no_old_words_for_the_songs_parts(self) -> None:
        found = strays()
        self.assertEqual(found, [], "old words (frag, section) are back:\n" + "\n".join(found))


if __name__ == "__main__":
    unittest.main()
