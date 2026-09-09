#include "tree_sitter/parser.h"
#include <stdbool.h>
#include <stdint.h>
#include <string.h>

// Keep this order synchronized with grammar.js's externals. Lookahead decisions
// are recorded in the selected opening token, so no mutable state is needed.
enum TokenType {
  TEXT, LABEL_OPEN, EXPRESSION_OPEN, CONDITIONAL_OPEN, SEQUENCE_OPEN,
  BLOCK_OPEN, ARGUMENT_OPEN, PATH_DOT, TUNNEL_WITH_TARGET,
  SEQUENCE_MODIFIER, CONDITIONAL_BRANCH_START, SEQUENCE_BLOCK_OPEN,
  INLINE_TEXT, ERROR_SENTINEL, TUNNEL_END, MISSING_BRACE
};
void *tree_sitter_ink_external_scanner_create(void) { return NULL; }
void tree_sitter_ink_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_ink_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload; (void)buffer; return 0;
}
void tree_sitter_ink_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  (void)payload; (void)buffer; (void)length;
}
static void advance(TSLexer *lexer) { lexer->advance(lexer, false); }
static bool word_char(int32_t c) {
  return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
    (c >= '0' && c <= '9') || c == '_' || c > 127;
}
static bool emit(TSLexer *lexer, const bool *valid, enum TokenType kind) {
  if (!valid[kind]) return false;
  lexer->result_symbol = kind;
  return true;
}

// Look ahead to the first top-level delimiter. mark_end stays after '{', so
// the parser subsequently reads the body as expressions or narrative normally.
static bool scan_brace(TSLexer *lexer, const bool *valid) {
  advance(lexer);
  lexer->mark_end(lexer);
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') advance(lexer);
  bool modifier = lexer->lookahead == '&' || lexer->lookahead == '!' || lexer->lookahead == '~';
  bool colon = false, pipe = false, quoted = false;
  unsigned depth = 0;
  char header[32] = {0}; unsigned n = 0;
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    if (c == '\r' || c == '\n') {
      bool sequence = modifier || !strcmp(header, "stopping:") ||
        !strcmp(header, "cycle:") || !strcmp(header, "shuffle:") ||
        !strcmp(header, "once:") || !strcmp(header, "shufflestopping:") ||
        !strcmp(header, "shuffleonce:");
      return emit(lexer, valid, sequence ? SEQUENCE_BLOCK_OPEN : BLOCK_OPEN);
    }
    if (c == '\\') {
      advance(lexer); if (!lexer->eof(lexer)) advance(lexer); continue;
    }
    if (c == '"') quoted = !quoted;
    if (!quoted) {
      if (c == '}' && depth == 0) break;
      if (c == '{' || c == '(' || c == '[') depth++;
      else if ((c == '}' || c == ')' || c == ']') && depth) depth--;
      else if (depth == 0 && c == ':' && !pipe) colon = true;
      else if (depth == 0 && c == '|') {
        advance(lexer);
        if (lexer->lookahead == '|') { advance(lexer); continue; }
        pipe = true;
        continue;
      }
    }
    if (n < sizeof(header) - 1 && c != ' ' && c != '\t') header[n++] = c < 128 ? (char)c : '?';
    advance(lexer);
  }
  return emit(lexer, valid, colon && !modifier ? CONDITIONAL_OPEN :
    (pipe || modifier) ? SEQUENCE_OPEN : EXPRESSION_OPEN);
}

static bool scan_text_tail(TSLexer *lexer, bool consumed) {
  while (!lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    if (c == '\r' || c == '\n' || c == '{' || c == '}' || c == '[' ||
        c == ']' || c == '#' || c == '|' || c == '\\') break;
    if (c == '/' || c == '<' || c == '-') {
      advance(lexer);
      if ((c == '/' && (lexer->lookahead == '/' || lexer->lookahead == '*')) ||
          (c == '<' && (lexer->lookahead == '>' || lexer->lookahead == '-')) ||
          (c == '-' && lexer->lookahead == '>')) break;
    } else {
      advance(lexer);
    }
    consumed = true;
    lexer->mark_end(lexer);
  }
  return consumed;
}

bool tree_sitter_ink_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid) {
  (void)payload;
  if (valid[ERROR_SENTINEL]) return false;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' || lexer->lookahead == 0xFEFF) lexer->advance(lexer, true);
  // Close an unfinished block before its final newline when the next line is
  // a declaration. Keep the newline for the surrounding content rule.
  if (valid[MISSING_BRACE] && (lexer->eof(lexer) || lexer->lookahead == '\r' || lexer->lookahead == '\n')) {
    lexer->mark_end(lexer);
    if (lexer->eof(lexer)) return emit(lexer, valid, MISSING_BRACE);
    if (lexer->lookahead == '\r') advance(lexer);
    if (lexer->lookahead == '\n') advance(lexer);
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') advance(lexer);
    return (lexer->lookahead == '=' || lexer->eof(lexer)) && emit(lexer, valid, MISSING_BRACE);
  }
  if (valid[ARGUMENT_OPEN] && lexer->lookahead == '(') {
    advance(lexer); return emit(lexer, valid, ARGUMENT_OPEN);
  }
  if (valid[PATH_DOT] && lexer->lookahead == '.') {
    advance(lexer); return emit(lexer, valid, PATH_DOT);
  }
  if (valid[SEQUENCE_MODIFIER] && (lexer->lookahead == '&' || lexer->lookahead == '!' || lexer->lookahead == '~')) {
    advance(lexer); return emit(lexer, valid, SEQUENCE_MODIFIER);
  }
  if (valid[TUNNEL_END] && lexer->lookahead == '-') {
    advance(lexer); if (lexer->lookahead != '>') return false;
    advance(lexer); lexer->mark_end(lexer);
    if (lexer->lookahead == '-') return false;
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') advance(lexer);
    return !word_char(lexer->lookahead) && emit(lexer, valid, TUNNEL_END);
  }
  if (lexer->lookahead == '-' && (valid[TUNNEL_WITH_TARGET] || valid[CONDITIONAL_BRANCH_START])) {
    advance(lexer);
    if (lexer->lookahead == '>' && valid[TUNNEL_WITH_TARGET]) {
      advance(lexer); if (lexer->lookahead != '-') return false;
      advance(lexer); if (lexer->lookahead != '>') return false;
      advance(lexer); lexer->mark_end(lexer);
      while (lexer->lookahead == ' ' || lexer->lookahead == '\t') advance(lexer);
      return word_char(lexer->lookahead) && emit(lexer, valid, TUNNEL_WITH_TARGET);
    }
    if (valid[CONDITIONAL_BRANCH_START]) {
      lexer->mark_end(lexer);
      bool quoted = false;
      while (!lexer->eof(lexer) && lexer->lookahead != '\r' && lexer->lookahead != '\n') {
        if (lexer->lookahead == '\\') { advance(lexer); if (!lexer->eof(lexer)) advance(lexer); continue; }
        if (lexer->lookahead == '"') quoted = !quoted;
        if (!quoted && lexer->lookahead == ':') return emit(lexer, valid, CONDITIONAL_BRANCH_START);
        advance(lexer);
      }
      return false;
    }
    if (!valid[INLINE_TEXT]) return false;
    lexer->mark_end(lexer);
    lexer->result_symbol = INLINE_TEXT;
    return scan_text_tail(lexer, true);
  }
  if (valid[LABEL_OPEN] && lexer->lookahead == '(') {
    advance(lexer); return emit(lexer, valid, LABEL_OPEN);
  }
  if (lexer->lookahead == '{' && (valid[EXPRESSION_OPEN] || valid[CONDITIONAL_OPEN] ||
      valid[SEQUENCE_OPEN] || valid[BLOCK_OPEN] || valid[SEQUENCE_BLOCK_OPEN])) return scan_brace(lexer, valid);
  if (!valid[TEXT] && !valid[INLINE_TEXT]) return false;
  bool line_start = valid[TEXT];
  int32_t first = lexer->lookahead;
  if (!first || first == '\r' || first == '\n') return false;
  if (line_start && (first == '=' || first == '~' || first == '*' || first == '+' || first == '-')) return false;
  bool consumed = false;
  // Match complete reserved words only at line starts. After choices, inline
  // expressions, and tags these same words are ordinary narrative text.
  if (line_start && first >= 'A' && first <= 'Z') {
    char word[16]; unsigned n = 0;
    while (word_char(lexer->lookahead) && n < sizeof(word) - 1) {
      word[n++] = lexer->lookahead < 128 ? (char)lexer->lookahead : '?'; advance(lexer);
    }
    word[n] = 0;
    if (!word_char(lexer->lookahead) &&
        (!strcmp(word, "VAR") || !strcmp(word, "CONST") || !strcmp(word, "LIST") ||
         !strcmp(word, "INCLUDE") || !strcmp(word, "EXTERNAL") ||
         (!strcmp(word, "TODO") && lexer->lookahead == ':'))) return false;
    consumed = n > 0;
    lexer->mark_end(lexer);
  }
  lexer->result_symbol = line_start ? TEXT : INLINE_TEXT;
  return scan_text_tail(lexer, consumed);
}
