// The line kinds of a song (#482), told by their shape as the Vim syntax
// (syntax/vim/syntax/song.vim) tells them. The engine knows a clip's track;
// a grammar does not, so an indented line is a lane when it is a pad and
// steps or a pad and a call, and notes otherwise.

#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <string.h>

enum TokenType {
  LINE_START,
  MODULAR_START,
  LANE_INDENT,
  CALL_LANE_INDENT,
  NOTES_INDENT,
  CODE_INDENT,
  LINE_END,
  ERROR_SENTINEL,
};

// Enough of a line to tell its kind; a longer line is read by its start.
#define LINE_MAX 256

void *tree_sitter_song_external_scanner_create(void) { return NULL; }
void tree_sitter_song_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_song_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}
void tree_sitter_song_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

static bool is_space(int32_t c) { return c == ' ' || c == '\t'; }
static bool is_lower(char c) { return c >= 'a' && c <= 'z'; }
static bool is_digit(char c) { return c >= '0' && c <= '9'; }
static bool is_alpha(char c) { return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_'; }
// Not the terminator, which `strchr` finds in any string.
static bool is_step(char c) { return c && strchr("xXofd234.", c) != NULL; }

// The rest of the line from the lexer's position, without consuming it.
static unsigned read_line(TSLexer *lexer, char *buf) {
  unsigned n = 0;
  while (!lexer->eof(lexer) && lexer->lookahead != '\n' && n < LINE_MAX - 1) {
    int32_t c = lexer->lookahead;
    buf[n++] = c < 128 ? (char)c : '?';
    lexer->advance(lexer, false);
  }
  buf[n] = '\0';
  return n;
}

// `setting <name> = Modular`, the start of a line of code (ADR-0024).
static bool is_modular(const char *s) {
  if (strncmp(s, "setting", 7) != 0 || !is_space(s[7])) return false;
  s += 7;
  while (is_space(*s)) s++;
  if (!*s || is_space(*s)) return false;
  while (*s && !is_space(*s) && *s != '=') s++;
  while (is_space(*s)) s++;
  if (*s != '=') return false;
  s++;
  while (is_space(*s)) s++;
  return strncmp(s, "Modular", 7) == 0 && !is_alpha(s[7]) && !is_digit(s[7]);
}

// A lane: a pad, then steps and rests (and maybe a comment), or a call.
static int lane_kind(const char *s) {
  if (!is_lower(*s)) return -1;
  while (is_lower(*s) || is_digit(*s)) s++;
  if (!is_space(*s)) return -1;
  while (is_space(*s)) s++;
  if (is_step(*s)) {
    while (is_step(*s) || is_space(*s)) s++;
    return *s == '\0' || *s == '#' ? LANE_INDENT : -1;
  }
  if (is_alpha(*s)) {
    while (is_alpha(*s) || is_digit(*s) || *s == '.') s++;
    return *s == '(' ? CALL_LANE_INDENT : -1;
  }
  return -1;
}

bool tree_sitter_song_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid) {
  (void)payload;
  // In error recovery every token is valid; let the internal lexer handle it.
  if (valid[ERROR_SENTINEL]) return false;

  if (lexer->get_column(lexer) == 0) {
    if (is_space(lexer->lookahead)) {
      while (is_space(lexer->lookahead)) lexer->advance(lexer, false);
      if (lexer->eof(lexer) || lexer->lookahead == '\n' || lexer->lookahead == '\r') return false;
      lexer->mark_end(lexer);
      if (valid[CODE_INDENT]) {
        lexer->result_symbol = CODE_INDENT;
        return true;
      }
      char buf[LINE_MAX];
      read_line(lexer, buf);
      int kind = lane_kind(buf);
      if (kind < 0) kind = NOTES_INDENT;
      if (!valid[kind]) return false;
      lexer->result_symbol = (TSSymbol)kind;
      return true;
    }
    if (lexer->eof(lexer) || lexer->lookahead == '\n' || lexer->lookahead == '\r') return false;
    if (!valid[LINE_START] && !valid[MODULAR_START]) return false;
    // Zero-width: the line's own tokens follow.
    lexer->mark_end(lexer);
    char buf[LINE_MAX];
    read_line(lexer, buf);
    lexer->result_symbol = is_modular(buf) && valid[MODULAR_START] ? MODULAR_START : LINE_START;
    return valid[lexer->result_symbol];
  }

  if (valid[LINE_END]) {
    while (is_space(lexer->lookahead) || lexer->lookahead == '\r') lexer->advance(lexer, true);
    if (lexer->lookahead == '\n') {
      lexer->advance(lexer, false);
      lexer->mark_end(lexer);
      lexer->result_symbol = LINE_END;
      return true;
    }
    if (lexer->eof(lexer)) {
      lexer->mark_end(lexer);
      lexer->result_symbol = LINE_END;
      return true;
    }
  }
  return false;
}
