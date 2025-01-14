//! Implementation of `Stream::summary()`.

use gilded::Doc;

use crate::file::Span;
use crate::file::Spanned;
use crate::token::Any;
use crate::token::Cursor;
use crate::token::Stream;

use crate::token::Sign;
use crate::token::Token;

use super::Content;

impl Stream<'_> {
  /// Returns a string that summarizes the contents of this token stream.
  pub fn summary(&self) -> String {
    self.cursor().summary().to_string(&Default::default())
  }
}

impl<'a> Cursor<'a> {
  fn summary(&self) -> Doc<'a> {
    Doc::new().push({ *self }.map(|token| {
      let doc = Doc::new()
        .entry("lexeme", token.lexeme().index())
        .entry("span", span2doc(token.span()));

      match token {
        Any::Eof(..) => Doc::single("eof", doc),
        Any::Keyword(..) => Doc::single("keyword", doc),
        Any::Bracket(tok) => Doc::single(
          "bracket",
          doc
            .array("delims", tok.delimiters().into_iter().map(span2doc))
            .entry("contents", tok.contents().summary()),
        ),

        Any::Ident(tok) => Doc::single(
          "ident",
          doc
            .entry("prefix", tok.prefix().map(span2doc))
            .entry("suffix", tok.suffix().map(span2doc))
            .entry("name", span2doc(tok.name())),
        ),

        Any::Digital(tok) => Doc::single(
          "ident",
          doc
            .entry("prefix", tok.prefix().map(span2doc))
            .entry("suffix", tok.suffix().map(span2doc))
            .entry("radix", tok.radix())
            .entry("sign", tok.sign().map(sign2str))
            .array("blocks", tok.digit_blocks().map(span2doc))
            .array(
              "exponents",
              tok.exponents().map(|exp| {
                Doc::new()
                  .entry("span", span2doc(exp.span()))
                  .entry("prefix", exp.prefix().map(span2doc))
                  .entry("radix", exp.radix())
                  .entry("sign", exp.sign().map(sign2str))
                  .array("blocks", exp.digit_blocks().map(span2doc))
              }),
            ),
        ),

        Any::Quoted(tok) => Doc::single(
          "quoted",
          doc
            .entry("prefix", tok.prefix().map(span2doc))
            .entry("suffix", tok.suffix().map(span2doc))
            .array("delims", tok.delimiters().into_iter().map(span2doc))
            .array(
              "contents",
              tok.raw_content().map(|c| match c {
                Content::Lit(lit) => Doc::single("text", span2doc(lit)),
                Content::Esc(esc, data) => Doc::new()
                  .entry("esc", span2doc(esc))
                  .entry("data", data.map(span2doc)),
              }),
            ),
        ),
      }
    }))
  }
}

fn span2doc(span: Span) -> Doc {
  Doc::new()
    .array("span", [span.start(), span.end()])
    .entry("text", span.text())
}

fn sign2str(s: Sign) -> &'static str {
  match s {
    Sign::Pos => "+",
    Sign::Neg => "-",
  }
}
