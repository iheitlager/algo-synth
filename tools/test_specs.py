"""Every **Implementation:** and **Tests:** reference in the specs resolves (#229).

A backticked token that starts like `dir/...` is a path (a file, or a
directory ending in `/`), optionally followed by `::Type::name`. The path must
exist; for a `.rs` file the last segment must be defined there (`fn`,
`struct`, `enum`, `const`, ..., or an enum variant) or in the module's `name/`
directory; for other files it must appear as a word. A line starting
`**Implementation:** (planned)` or `**Tests:** (planned)` is skipped.
"""

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPECS = ROOT / ".openspec" / "specs"
LINE = re.compile(r"^\*\*(Implementation|Tests):\*\*(.*)$")
TOKEN = re.compile(r"`([^`]+)`")
PATH = re.compile(r"^[\w.-]+/\S*$")
DEF = r"\b(fn|struct|enum|const|static|type|trait|mod)\s+{0}\b|^\s*{0}\s*([,({{=]|$)"


def references(text: str) -> list[str]:
    refs = []
    for line in text.splitlines():
        m = LINE.match(line)
        if not m or m.group(2).strip().startswith("(planned)"):
            continue
        refs += [t for t in TOKEN.findall(m.group(2)) if PATH.match(t)]
    return refs


def problem(ref: str) -> str | None:
    path, _, symbol = ref.partition("::")
    file = ROOT / path
    if path.endswith("/"):
        return None if file.is_dir() else "no such directory"
    if not file.is_file():
        return "no such file"
    if not symbol:
        return None
    name = symbol.split("::")[-1]
    if path.endswith(".rs"):
        files = [file, *sorted(file.with_suffix("").glob("*.rs"))]
        pattern = re.compile(DEF.format(re.escape(name)), re.MULTILINE)
    else:
        files = [file]
        pattern = re.compile(rf"\b{re.escape(name)}\b")
    if any(pattern.search(f.read_text()) for f in files):
        return None
    return f"`{name}` not defined"


class SpecLinks(unittest.TestCase):
    def test_every_reference_resolves(self):
        broken = []
        for spec in sorted(SPECS.glob("*/spec.md")):
            for ref in references(spec.read_text()):
                if why := problem(ref):
                    broken.append(f"{spec.parent.name}: {ref} ({why})")
        self.assertEqual(broken, [], "\n" + "\n".join(broken))

    def test_planned_lines_are_skipped(self):
        text = "**Tests:** (planned) `crates/nope.rs::x`\n**Tests:** `a/b.rs`\n"
        self.assertEqual(references(text), ["a/b.rs"])

    def test_a_missing_symbol_is_reported(self):
        self.assertEqual(problem("tools/release.py::no_such_name_here_x"), "`no_such_name_here_x` not defined")
        self.assertIsNone(problem("crates/dsp/src/voice.rs::midi_to_hz"))


if __name__ == "__main__":
    unittest.main()
