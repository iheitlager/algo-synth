#!/usr/bin/env python3
"""Compare an editor's highlighting of song files with the engine's (#482).

The engine's classes come from `song-lsp tokens`; the editor's from headless
Neovim, either with the regex syntax (`vim`) or the tree-sitter grammar
(`ts`). Each character the engine colours is compared, and the script fails
when the agreement of any class drops under its floor.

    syntax/compare.py vim|ts SONG_LSP FILE...
"""

import json
import os
import subprocess
import sys
import tempfile
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
CLASSES = ["kw", "name", "num", "note", "pad", "step", "rest", "call", "param", "punct", "comment"]

# The share of each class's characters an editor must colour the same as the
# engine. The editors read an indented line by its shape, not its track's
# kind, so a little drift is allowed; a broken rule drops far below this.
FLOOR = 0.95

# Vim's syntax groups (syntax/vim/syntax/song.vim) by engine class.
VIM = {
    "songKeyword": "kw", "songWord": "kw", "songName": "name", "songNumber": "num",
    "songNote": "note", "songPad": "pad", "songStep": "step", "songRest": "rest",
    "songNoteRest": "rest", "songLaneRest": "rest", "songCall": "call",
    "songParam": "param", "songPunct": "punct", "songComment": "comment",
}

# tree-sitter captures (tree-sitter-song/queries/highlights.scm) by class.
TS = {
    "keyword": "kw", "variable": "name", "number": "num", "string.special": "note",
    "type": "pad", "constant": "step", "comment.rest": "rest", "function.call": "call",
    "property": "param", "punctuation.delimiter": "punct", "comment": "comment",
}

# Run inside Neovim: the class name under each character of the buffer, a
# line of the output per line of the song (`-` where there is none).
DUMP = r"""
local map, ts = vim.json.decode(vim.env.SONG_MAP), vim.env.SONG_TS == '1'
local out = {}
-- Headless Neovim never redraws, so nothing has parsed the buffer yet.
if ts then vim.treesitter.get_parser(0, 'song'):parse(true) end
for l, text in ipairs(vim.api.nvim_buf_get_lines(0, 0, -1, false)) do
  local row, i = {}, 1
  while i <= #text do
    local n = vim.str_utf_end(text, i) + 1
    local cls = '-'
    if ts then
      for _, c in ipairs(vim.treesitter.get_captures_at_pos(0, l - 1, i - 1)) do
        if map[c.capture] then cls = map[c.capture] end
      end
    else
      local id = vim.fn.synID(l, i, 1)
      cls = map[vim.fn.synIDattr(id, 'name')] or '-'
    end
    table.insert(row, cls)
    i = i + n
  end
  table.insert(out, table.concat(row, ' '))
end
vim.fn.writefile(out, vim.env.SONG_OUT)
vim.cmd('qa!')
"""


def engine(song_lsp: str, path: str) -> dict[tuple[int, int], str]:
    """The engine's class of each (line, char) it colours."""
    out = subprocess.run([song_lsp, "tokens", path], check=True, capture_output=True, text=True)
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")
    classes = {}
    for row in out.stdout.splitlines():
        line, col, n, cls = row.split()
        line, col, n = int(line), int(col), int(n)
        # UTF-16 units to chars, over the line's text.
        units, char = 0, 0
        text = lines[line] if line < len(lines) else ""
        for k, ch in enumerate(text):
            if units >= col:
                char = k
                break
            units += 2 if ord(ch) > 0xFFFF else 1
        else:
            char = len(text)
        for k in range(char, char + n):
            classes[(line, k)] = cls
    return classes


def editor(mode: str, path: str) -> dict[tuple[int, int], str]:
    """The editor's class of each (line, char)."""
    with tempfile.TemporaryDirectory() as tmp:
        out, script = os.path.join(tmp, "out"), os.path.join(tmp, "dump.lua")
        with open(script, "w") as f:
            f.write(DUMP)
        env = dict(os.environ, SONG_OUT=out, SONG_MAP=json.dumps(VIM if mode == "vim" else TS),
                   SONG_TS="1" if mode == "ts" else "0")
        setup = (["--cmd", f"set rtp^={HERE}/vim", "-c", "syntax on", "-c", "set ft=song"]
                 if mode == "vim" else
                 ["-c", f"lua vim.treesitter.language.add('song', {{ path = '{HERE}/tree-sitter-song/song.so' }})",
                  "-c", f"lua vim.treesitter.query.set('song', 'highlights', "
                        f"table.concat(vim.fn.readfile('{HERE}/tree-sitter-song/queries/highlights.scm'), '\\n'))",
                  "-c", "lua vim.treesitter.start(0, 'song')"])
        subprocess.run(["nvim", "--headless", "--clean", path, *setup, "-c", f"luafile {script}"],
                       check=True, env=env, capture_output=True, timeout=60)
        with open(out, encoding="utf-8") as f:
            rows = f.read().split("\n")
    return {(l, k): c for l, row in enumerate(rows) for k, c in enumerate(row.split(" ")) if c != "-"}


def main() -> int:
    if len(sys.argv) < 4 or sys.argv[1] not in ("vim", "ts"):
        print(__doc__.strip().splitlines()[-1].strip(), file=sys.stderr)
        return 2
    mode, song_lsp, paths = sys.argv[1], sys.argv[2], sys.argv[3:]
    same, total, misses = Counter(), Counter(), Counter()
    for path in paths:
        want, got = engine(song_lsp, path), editor(mode, path)
        for at, cls in want.items():
            total[cls] += 1
            if got.get(at) == cls:
                same[cls] += 1
            else:
                misses[(cls, got.get(at, "-"))] += 1
    ok = True
    for cls in CLASSES:
        if total[cls]:
            share = same[cls] / total[cls]
            ok &= share >= FLOOR
            print(f"{cls:8} {share:7.2%}  of {total[cls]}{'' if share >= FLOOR else '  < floor'}")
    for (want, got), n in misses.most_common(8):
        print(f"  {n:5}  engine {want}, {mode} {got}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
