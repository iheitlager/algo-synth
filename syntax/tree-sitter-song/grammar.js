/**
 * tree-sitter grammar for algo-synth song files (#482).
 *
 * The engine's lexer (crates/dsp/src/song/lex.rs) is the reference. A song
 * is lines; the external scanner (src/scanner.c) tells what each line is by
 * its shape, as the Vim syntax does: a top-level line, a Modular setting's
 * header and its indented SuperCollider code, a drum lane (`bd x...X...` or
 * `ch euclid(3,8)`) or a line of notes. Within a line the grammar is a flat
 * run of tokens; the queries tell a pitch from a name.
 *
 * @license Apache-2.0
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const KEYWORDS = [
  'tempo', 'swing', 'scale', 'setting', 'track', 'samples', 'strip', 'group', 'master',
  'clip', 'auto', 'snapshot', 'mod', 'scene', 'arrange', 'loop',
  // The words of a song written before ADR-0031, which still parse.
  'frag', 'section',
];
// The words after a line's keyword that are keywords too (lex.rs WORDS).
const WORDS = ['live', 'bars', 'ramp', 'drums', 'synth', 'sampler', 'voicing'];

const IDENT = /[A-Za-z_]([A-Za-z0-9_#]|-[0-9])*/.source;

module.exports = grammar({
  name: 'song',

  externals: $ => [
    $._line_start,
    $._modular_start,
    $._lane_indent,
    $._call_lane_indent,
    $._notes_indent,
    $._code_indent,
    $._line_end,
    $._error_sentinel,
  ],

  extras: _ => [/[ \t\r]/],

  word: $ => $.name,

  rules: {
    source_file: $ => repeat(choice($.line, $.modular, $.lane, $.notes, '\n')),

    line: $ => seq($._line_start, optional($.keyword), repeat($._token), optional($.comment), $._line_end),

    // `setting <name> = Modular …` and the code under it, blank lines and all.
    modular: $ => prec.right(seq(
      $._modular_start, $.keyword, repeat($._token), optional($.comment), $._line_end,
      repeat(choice($.code, '\n')),
    )),
    // SuperCollider has no song comments: a `#` there is its own.
    code: $ => seq($._code_indent, repeat($._token), $._line_end),

    lane: $ => choice(
      seq($._lane_indent, $.pad, repeat(choice($.step, $.dots)), optional($.comment), $._line_end),
      seq($._call_lane_indent, $.pad, repeat($._token), optional($.comment), $._line_end),
    ),

    notes: $ => seq($._notes_indent, repeat($._token), optional($.comment), $._line_end),

    _token: $ => choice($.call, $.name, $.param, $.word, $.number, $.punct, $.rest, $.other),

    keyword: _ => choice(...KEYWORDS),
    word: _ => choice(...WORDS),

    name: _ => new RegExp(IDENT),
    // `kit.Cutoff`, `master.P2Return`.
    param: _ => new RegExp(`${IDENT}(\\.[A-Za-z]([A-Za-z0-9_#]|-[0-9])*)+`),
    // `euclid(`, `perlin.slow(`: the name is the call, the bracket notation.
    call: $ => seq(field('name', choice($.name, $.param)), alias(token.immediate('('), $.punct)),

    number: _ => /[-.]?[0-9][0-9.]*/,
    punct: _ => /[=:"\[\]<>(),*@?&!\/+]/,
    rest: _ => '~',
    // Anything else, uncoloured, so no text is an error.
    other: _ => token(prec(-1, /[^\s]/)),

    pad: _ => /[a-z][a-z0-9]*/,
    step: _ => /[xXofd2-4]+/,
    dots: _ => /\.+/,

    comment: _ => token(seq('#', /[^\n]*/)),
  },
});
