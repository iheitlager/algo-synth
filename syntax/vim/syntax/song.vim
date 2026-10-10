" Vim syntax for algo-synth song files (#482).
"
" The engine's lexer (crates/dsp/src/song/lex.rs) is the reference: the same
" eleven classes, coloured by regex. It knows a clip's track kind; this does
" not, so an indented line is read by its shape: a pad and steps (`bd x..X`)
" or a pad and a call (`ch euclid(3,8)`) is a lane, anything else notes, and
" the body under `setting … = Modular` is SuperCollider code.

if exists('b:current_syntax')
  finish
endif

syn case match

" Comments: a `#` at the start of a line or after a space, so `c#4` is a note.
syn match songComment /\%(^\|\s\)\@1<=#.*$/ contains=@Spell

" Tokens, valid on any line. At one position the later match wins, so the
" general name comes first and what refines it after.
syn match songName    /\<[A-Za-z_][A-Za-z0-9_#]*/ contained
syn match songNumber  /\%([A-Za-z0-9_#]\)\@1<!-\?\.\?\d[0-9.]*/ contained
syn match songParam   /\<[A-Za-z_]\w*\%(\.[A-Za-z]\w*\)\+/ contained
syn match songCall    /\<[A-Za-z_][A-Za-z0-9_.]*\ze(/ contained
" A keyword after the first, unless it names a track (`drums.Send1`) or a call.
syn match songWord    /\<\%(live\|bars\|ramp\|drums\|synth\|sampler\|voicing\)\>\%(\.\a\|(\)\@!/ contained
syn match songPunct   /[=:"\[\]<>(),*@?&!/+]/ contained
syn match songRest    /\~/ contained

syn cluster songTokens contains=songName,songNumber,songParam,songCall,songWord,songPunct,songRest

" Notes: pitches (`c4`, `f#3`, `bb2`, `c-1`), chord roots before a colon with
" their quality (`c:m7`, `bb3:sus4`), roman numerals (`bVII`, `viio7`) and the
" `r` rest.
syn match songNote    /\%([A-Za-z0-9_#]\)\@1<![a-g][#b]\?-\?\d\+\>/ contained
syn match songNote    /\%([A-Za-z0-9_#]\)\@1<![a-g][#b]\?\d\?\ze:\a/ contained
syn match songNote    /\%(\<[a-g][#b]\?\d\?:\)\@<=\a\w*/ contained
syn match songNote    /\<b\?\%([iv]\{1,4}\|[IV]\{1,4}\)\%(o7\|o\|7\|maj7\)\?\>/ contained
syn match songNoteRest /\<r\>/ contained

" Lines. A top-level line starts with a keyword: today's words (ADR-0031) and
" the old `frag` and `section` a song written before still uses.
syn match songKeyword /^\%(tempo\|swing\|scale\|setting\|track\|samples\|strip\|group\|master\|clip\|auto\|snapshot\|mod\|scene\|arrange\|loop\|frag\|section\)\>/ contained
syn match songLine    /^\S.*$/ contains=songKeyword,@songTokens,songComment

syn match songNotes   /^\s\+\S.*$/ contains=@songTokens,songNote,songNoteRest,songComment
syn match songLane    /^\s\+[a-z][a-z0-9]*\s\+[xXofd2-4.][xXofd2-4. ]*\%(#.*\)\?$/ contains=songPad,songStep,songLaneRest,songComment
syn match songLane    /^\s\+[a-z][a-z0-9]*\s\+[A-Za-z_][A-Za-z0-9_.]*(.*$/ contains=songPad,@songTokens,songComment
syn match songStep    /[xXofd2-4]\+/ contained
syn match songLaneRest /\.\+/ contained
syn match songPad     /^\s\+\zs[a-z][a-z0-9]*/ contained

" A Modular setting: its header, then the indented code up to the next
" top-level line, coloured with the same tokens as the engine colours it. The
" code has no song comments: `#` there is SuperCollider's.
syn match songModular /^setting\s\+\S\+\s*=\s*Modular\>.*$/ contains=songKeyword,@songTokens,songComment nextgroup=songCode skipnl
syn region songCode start=/^\s/ end=/^\ze\S/ contained contains=@songTokens

" The web editor's palette (web/src/components/CodeArea.vue), in Vim's groups.
hi def link songKeyword  Keyword
hi def link songWord     Keyword
hi def link songName     Identifier
hi def link songNumber   Number
hi def link songNote     String
hi def link songPad      Type
hi def link songStep     Special
hi def link songRest     NonText
hi def link songNoteRest NonText
hi def link songLaneRest NonText
hi def link songCall     Function
hi def link songParam    PreProc
hi def link songPunct    Delimiter
hi def link songComment  Comment

let b:current_syntax = 'song'
