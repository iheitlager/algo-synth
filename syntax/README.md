# Song files in your editor

Highlighting and checking for `*.song` in Neovim (or Vim) and Zed (#482).
Three layers, pick one or stack them:

| Layer | Where | What it gives | Works in |
|---|---|---|---|
| Vim syntax | `vim/` | regex highlighting, no plugins | Vim, Neovim |
| tree-sitter grammar | `tree-sitter-song/` | highlighting, SuperCollider injected into Modular settings | Neovim, Zed |
| language server | `song-lsp/` | the engine's own highlighting and parse errors as you type | Neovim, Zed |

The engine's lexer (`crates/dsp/src/song/lex.rs`) is the reference for all of
them. It knows which track a clip plays, which a regex or a grammar does not,
so those two read an indented line by its shape: a pad and steps
(`bd x...X...`) or a pad and a call (`ch euclid(3,8)`) is a drum lane,
anything else is notes, and the body under `setting … = Modular` is code.
`make syntax-test` checks every character of `examples/songs/*.song` against the
engine. Both are at 100% today.

## Neovim

### Vim syntax

```lua
vim.opt.runtimepath:append('~/wc/algo-synth/syntax/vim')
```

`ftdetect/song.vim` sets the `song` filetype for `*.song`.

### tree-sitter (nvim-treesitter `main`)

```lua
vim.filetype.add({ extension = { song = 'song' } })

vim.api.nvim_create_autocmd('User', { pattern = 'TSUpdate', callback = function()
  require('nvim-treesitter.parsers').song = {
    install_info = { path = '~/wc/algo-synth/syntax/tree-sitter-song', queries = 'queries' },
  }
end })

vim.api.nvim_create_autocmd('FileType', { pattern = 'song', callback = function()
  vim.treesitter.start()
end })
```

Then `:TSInstall song`. The SuperCollider injection needs a `supercollider`
parser; without one the code is coloured as song tokens.

### Language server

```sh
cargo install --path syntax/song-lsp
```

```lua
vim.lsp.config('song_lsp', {
  cmd = { 'song-lsp' },
  filetypes = { 'song' },
  root_markers = { 'Cargo.toml', '.git' },
})
vim.lsp.enable('song_lsp')
```

Neovim uses the server's semantic tokens as they arrive, on top of whichever
highlighting is on.

## Zed

1. `cargo install --path syntax/song-lsp`, so `song-lsp` is on your PATH.
2. Command palette, **zed: install dev extension**, and pick `syntax/zed/`.
   Zed fetches the grammar from GitHub at the commit in `extension.toml`
   (`rev`), so that commit has to be pushed.
3. To have the server's tokens on top of tree-sitter's, in `settings.json`:

```json
"languages": { "Song": { "semantic_tokens": "combined" } }
```

`languages/song/*.scm` is a copy of `tree-sitter-song/queries/`;
`make syntax-test` fails when they differ. The grammar moves to a repository
of its own when the extension is published.

## The classes

| Engine (`song::lex::Class`) | Vim group | tree-sitter capture | LSP token type |
|---|---|---|---|
| keyword: `clip`, `scene`, `ramp`, `drums`… | `Keyword` | `@keyword` | `keyword` |
| name | `Identifier` | `@variable` | `variable` |
| number | `Number` | `@number` | `number` |
| note: `c4`, `f#3`, `c:m7`, `bVII` | `String` | `@string.special` | `string` |
| pad: `bd`, `sn` | `Type` | `@type` | `type` |
| step: `x`, `X` | `Special` | `@constant` | `enumMember` |
| rest: `.`, `~`, `r` | `NonText` | `@comment.rest` | `comment` |
| call: `euclid(` | `Function` | `@function.call` | `function` |
| param: `kit.Cutoff` | `PreProc` | `@property` | `property` |
| punct | `Delimiter` | `@punctuation.delimiter` | `operator` |
| comment | `Comment` | `@comment` | `comment` |

## Changing them

A new keyword or notation goes in `lex.rs` first, then in `vim/syntax/song.vim`
and `tree-sitter-song/` (run `tree-sitter generate` there, then copy the
queries to `zed/languages/song/`). `make syntax-test` needs `tree-sitter`,
`nvim` and a C compiler; `song-lsp`'s own tests run with `cargo test`.
