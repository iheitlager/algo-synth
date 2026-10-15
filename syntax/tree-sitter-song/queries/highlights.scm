; The engine's classes (crates/dsp/src/song/lex.rs) as tree-sitter captures.
; General first: a later pattern wins over an earlier one.

(name) @variable
(param) @property
(number) @number
(punct) @punctuation.delimiter
(rest) @comment.rest
(keyword) @keyword
(word) @keyword
(comment) @comment

(call name: (_) @function.call)

; Lanes: the pad, its hits and its rests.
(pad) @type
(step) @constant
(dots) @comment.rest

; Notes: pitches (`c4`, `f#3`, `c-1`), chord roots (`c`, `bb3`), roman numerals
; (`VI`, `bVII`, `viio7`), a chord's quality after its root (`c:m7`), and the
; `r` rest.
(notes
  (name) @string.special
  (#match? @string.special "^[a-g][#b]?-?[0-9]+$"))
(notes
  (name) @string.special
  (#match? @string.special "^[a-g][#b]?[0-9]?$"))
(notes
  (name) @string.special
  (#match? @string.special "^b?([iv]{1,4}|[IV]{1,4})(o7|o|7|maj7)?$"))
(notes
  (name) @_root
  .
  (punct) @_colon
  .
  (name) @string.special
  (#match? @_root "^[a-g][#b]?-?[0-9]*$")
  (#eq? @_colon ":"))
(notes
  (name) @comment.rest
  (#eq? @comment.rest "r"))
; A beat line's pads (#659), `"bd sd [hh oh]"`: a pad word in a line of
; notes, which no note is; `cb` too, the cowbell before a C-flat root, written B here.
(notes
  (name) @type
  (#match? @type "^(bd|sn|cb|cp|ch|oh|lt|mt|ht|rs|cl|ma|cy|lc|mc|hc|cr|rd|sd|hh|rim|p(1[0-5]|[0-9]))$"))
