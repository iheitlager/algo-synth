# 0046: The song editor: a `.song` file, and the lexer and completer on a second wasm instance on the main thread

**Status:** Accepted · **Date:** 2026-10-17 · **Amends:** 0001

## Context

ADR-0012 makes the song text, parsed and printed by the engine. Three issues built the editor around that text, and none recorded the decisions they took:

- #105 (PR #196): save and open the song as a file, and keep the last one between sessions. No server.
- #203 (PR #207): a Strudel-like editor with line numbers, syntax highlighting and the parse error marked on its line and column.
- #595: an autocompleter, so live editing is right the first time. The language keeps growing: pattern methods, scale modes, chord qualities, generators, parameter names, and the song's own tracks and clips.

ADR-0001 puts every musical decision in Rust, and its single engine runs on the audio thread. A highlighter or a completer needs the grammar on every keystroke. Run in TypeScript, it is a second copy of the grammar. Run in the worklet, it costs the audio thread time it does not have.

## Decision

**The song is saved as `<name>.song`, the text the engine prints; the editor is a textarea over a highlighted `<pre>`, coloured and completed by the engine's own lexer and completer, run on a second `dsp.wasm` instance on the main thread.**

- **A `.song` file.** Save writes `song.text`, the song as the engine printed it after its last load, as `<name>.song`, named after the song or MIDI file opened (`songFileName` in `web/src/audio/songfile.ts`). Where the browser has the File System Access API, Save writes back into the file opened (#465); elsewhere it downloads. Open… takes a `.song` beside a MIDI file and a setup; a file that does not parse shows its text and error, and the playing song plays on.
- **The last song in `localStorage`** under `algo-synth:song`, loaded on power-on. A convenience: storage that throws changes nothing, an empty song never replaces a kept one, and the file is the real save.
- **The lexer is Rust.** `crates/dsp/src/song/lex.rs` classifies the text a line at a time with the parser's context: the line's keyword, and whether an indented line is a drum lane or notes by its clip's track kind. It does not check the song; errors come from `parse`. `crates/dsp/src/modular/lex.rs` does the same for a Modular synth's SuperCollider code (#329), with the same classes.
- **The completer is Rust.** `crates/dsp/src/song/complete.rs` returns the words that fit at the cursor from the lists the parser reads, with a class, a one-line detail, the usage of the call the cursor is in and a preview of what it gives. It reads the text, not the song, so a text with an error still completes. `crates/dsp/src/modular/complete.rs` completes SuperCollider code (#647).
- **Both are total.** Any text and cursor give spans or a list, maybe empty, never a panic.
- **Their own C ABI, on their own thread-local state.** `lex_buf(len)` sizes the text buffer, `lex()` or `sc_lex()` returns the span count, `lex_ptr()` holds `(start, len, class)` triples of `u32`. `complete(cursor)` or `sc_complete(cursor)` reads the same buffer and leaves UTF-8 lines at `complete_ptr()`. They touch no engine, so they need no `init`. Text is bounded by `song::MAX_TEXT`.
- **Offsets in UTF-16 units**, as the view indexes its strings. Spans are ordered, never overlap, and leave whitespace and unknown text plain.
- **A second instance on the main thread.** `AudioEngine.start` compiles `dsp.wasm` once, hands the module to the worklet, and `wasmLexers` (`web/src/audio/lex.ts`) instantiates it a second time on the main thread for both lexers and both completers. Typing never touches the audio thread.
- **No editor dependency.** `CodeArea.vue` is a transparent textarea over a `<pre>` painted from the spans, with a line-number gutter, the error's line and column marked, Tab indenting and the completion list. `SongEditor.vue` and the Modular `CodeEditor.vue` share it. The web dependencies stay `vue` and two `@fontsource` fonts.

#105 kept the synth setup as its own JSON file. That no longer holds: ADR-0018 moves the session into the song, and ADR-0027 drops Save setup.

## Consequences

- Highlighting and completion never drift from the grammar: a new keyword, method or mode is coloured and offered when the parser reads it (ADR-0019, ADR-0031, ADR-0035).
- ADR-0001's "one engine per wasm instance" gains a second kind of instance: one with no engine, used only through the lexer exports. The module stays one, with no imports.
- The view keeps a table of class names (`CLASSES` in `lex.ts`) in the order of `song::lex::Class`; a Rust test holds them equal.
- Lexing and completing allocate, on the main thread, never in `render` (ADR-0002). The lexer handles 325 KB in about 7 ms.
- The editor has no multiple cursors, folding or search; a textarea gives typing, undo and selection for free.

## Alternatives considered

- **Lexing in TypeScript**: no second instance, but a second grammar that drifts from the parser, and a musical decision in the view against ADR-0001.
- **Lexing in the worklet**, on the engine that plays: one instance, but every keystroke costs the audio thread, and the spans come back over the port, not from a call.
- **CodeMirror or Monaco**: folding, search and multiple cursors, but a large dependency where the view has none beyond Vue, and a grammar of its own to keep in step with the parser.
