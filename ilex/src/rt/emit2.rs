use std::iter;
use std::ptr;

use byteyarn::yarn;
use byteyarn::Yarn;
use byteyarn::YarnBox;

use crate::f;
use crate::file::Span;
use crate::file::Span2;
use crate::plural;
use crate::report::Expected;
use crate::rt;
use crate::rt::lexer::Lexer;
use crate::rt::DigitBlocks;
use crate::rule;
use crate::rule::Affixes;
use crate::rule::Any;
use crate::rule::BracketKind;
use crate::rule::Comment;
use crate::rule::Quoted;
use crate::spec::Lexeme;
use crate::spec::Spec;

use super::dfa::Lexeme2;
use super::unicode::is_xid;

pub fn emit(lexer: &mut Lexer) {
  // Start by searching for the longest matches using the DFA.
  let dfa = lexer.spec().dfa();
  let Some(mut match_) = dfa.search(lexer) else {
    return;
  };

  let start = lexer.cursor();
  let end = start + match_.len;
  let span = lexer.span(start..end);
  let text = span.text();
  let end = end + match_.extra;

  // Now we have to decide which of `candidates` is the best one, i.e.,
  // the one with no errors. The following things are explicitly *not*
  // checked by the DFA:
  //
  // - Trailing XID characters on some XID-ended tokens.
  // - Minimum identifier length.
  // - Valid identifier characters (i.e. ASCII-only-ness).
  // - Valid digit and separator locations in digit literals.
  // - Valid number of digit blocks; only the max is checked in the DFA.
  //
  // Once we filter out based on that, we break ties by picking the one with
  // the smallest lexeme index; bracket opens the corresponding bracket close,
  // so that if '|', '|' is a type of bracket, || will parse correctly.
  //
  // TODO(mcyoung): Document first-wins semantics?
  match_.candidates.sort_unstable();

  // Find the first candidate that has no errors. If we can't find one, we'll
  // assume the first candidate on the list is a good enough choice for
  // generating diagnostics.
  //
  // Note also that trailing XID characters does not disqualify any of these
  // choices; that is independent of which token we decide to create.
  let mut best = None;
  'verify: for &c in &match_.candidates {
    let [.., range, _] = find_affixes_partial(span, lexer.spec(), c);

    // NOTE: We only need to find the first lexeme that is valid. If it's not
    // valid, we will diagnose that in the next stage.
    match lexer.spec().rule(c.lexeme) {
      Any::Bracket(bracket)
      | Any::Comment(Comment { bracket, .. })
      | Any::Quoted(Quoted { bracket, .. }) => {
        if let BracketKind::CxxLike { ident_rule, open, close } = &bracket.kind
        {
          let [_, range, _] = if !c.is_close {
            range.split_around(open.0.len(), open.1.len())
          } else {
            range.split_around(close.0.len(), close.1.len())
          };

          let [_, name, _] = find_affixes(range, &ident_rule.affixes);
          if name.text().chars().count() < ident_rule.min_len {
            continue 'verify;
          }

          if ident_rule.ascii_only {
            for c in name.text().chars() {
              if !c.is_ascii()
                && !ident_rule.extra_continues.contains(c)
                && !ident_rule.extra_starts.contains(c)
              {
                continue 'verify;
              }
            }
          }
        }
      }
      Any::Ident(rule) => {
        if text.chars().count() < rule.min_len {
          continue 'verify;
        }
        if rule.ascii_only {
          for c in text.chars() {
            if !c.is_ascii()
              && !rule.extra_continues.contains(c)
              && !rule.extra_starts.contains(c)
            {
              continue 'verify;
            }
          }
        }
      }
      Any::Digital(rule) => {
        if text.is_empty() {
          continue 'verify;
        }

        let mut text = text;
        let mut digits = &rule.mant;
        let mut digit_blocks = 0;
        let mut digits_in_block = 0;
        let mut last_was_sep = false;
        'digits: while let Some(c) = text.chars().next() {
          if !rule.separator.is_empty() {
            if let Some(rest) = text.strip_prefix(rule.separator.as_str()) {
              if digits_in_block == 0 {
                let ok = if digit_blocks != 0 {
                  rule.corner_cases.around_point
                } else if ptr::eq(digits, &rule.mant) {
                  rule.corner_cases.prefix
                } else {
                  rule.corner_cases.around_exp
                };

                if !ok {
                  continue 'verify;
                }
              }

              text = rest;
              last_was_sep = true;
              continue;
            }
          }

          if let Some(rest) = text.strip_prefix(rule.point.as_str()) {
            if last_was_sep && !rule.corner_cases.around_point {
              continue 'verify;
            }

            text = rest;
            digit_blocks += 1;
            digits_in_block = 0;
            last_was_sep = false;
            continue;
          }

          if c.is_digit(digits.radix as u32) {
            text = &text[c.len_utf8()..];
            last_was_sep = false;
            digits_in_block += 1;
            continue;
          }

          for (pre, exp) in &rule.exps {
            if let Some(rest) = text.strip_prefix(pre.as_str()) {
              if last_was_sep && !rule.corner_cases.around_exp {
                continue 'verify;
              }

              text = rest;
              digit_blocks = 0;
              digits_in_block = 0;
              last_was_sep = false;
              digits = exp;
              continue 'digits;
            }
          }

          // Encountered an unexpected character; bail.
          continue 'verify;
        }
      }
      _ => {}
    }

    // We found a good one.
    best = Some(c);
    break;
  }

  let best = best.unwrap_or(match_.candidates[0]);
  let [sign_span, prefix, range, suffix] =
    find_affixes_partial(span, lexer.spec(), best);
  let text = range.text();

  let mirrored = match lexer.spec().rule(best.lexeme) {
    Any::Bracket(bracket)
    | Any::Comment(Comment { bracket, .. })
    | Any::Quoted(Quoted { bracket, .. }) => match &bracket.kind {
      BracketKind::Paired(open, _) if best.is_close => Some(open.aliased()),
      BracketKind::Paired(_, close) => Some(close.aliased()),
      BracketKind::RustLike { open, close, .. } => {
        let (remove, replace) =
          if !best.is_close { (open, close) } else { (close, open) };

        let [_, mid, _] = range.split_around(remove.0.len(), remove.1.len());
        Some(yarn!("{}{}{}", replace.0, mid.text(), replace.1))
      }
      BracketKind::CxxLike { ident_rule, open, close, .. } => {
        let (remove, replace) =
          if !best.is_close { (open, close) } else { (close, open) };

        let [_, mid, _] = range.split_around(remove.0.len(), remove.1.len());
        let [_, name, _] = find_affixes(mid, &ident_rule.affixes);

        let text = name.text();
        let count = text.chars().count();
        if count < ident_rule.min_len {
          lexer
            .builtins()
            .ident_too_small(ident_rule.min_len, count, name);
        }

        for c in text.chars() {
          if !c.is_ascii()
            && !ident_rule.extra_continues.contains(c)
            && !ident_rule.extra_starts.contains(c)
          {
            lexer.builtins().non_ascii_in_ident(best.lexeme, name);
            break;
          }
        }

        Some(yarn!("{}{}{}", replace.0, mid.text(), replace.1))
      }
    },
    _ => None,
  };

  let mut emitted = true;
  if best.is_close {
    let Some(opener) = &mirrored else {
      bug!("found is_close Lexeme2 corresponding to rule without brackets")
    };

    let found = if let Some(name) = lexer.spec().rule_name(best.lexeme) {
      Expected::Name(name.to_box())
    } else {
      Expected::Literal(YarnBox::new(text))
    };

    lexer.builtins().unopened(opener, found, span);
    lexer.add_token(rt::UNEXPECTED, end - start, None);
    emitted = false;
  } else {
    // Now we have repeat the process from the 'verify, but now we know what kind
    // of token we're going to create.

    let rule = lexer.spec().rule(best.lexeme);
    if !matches!(rule, Any::Comment(..)) {
      // Diagnose a \ that is not followed by only spaces and comments.
      if let Some(cancel) = lexer.line_end_cancel.take() {
        let cancel = cancel.get(lexer.file());
        lexer
          .report()
          .builtins(lexer.spec())
          .unexpected(cancel.text(), best.lexeme, cancel)
          .note(f!(
            "expected `{}` to be followed by a new line",
            cancel.text()
          ));
      }
    }

    match rule {
      Any::Keyword(..) => lexer.add_token(best.lexeme, range.len(), None),

      Any::LineEnd(..) if text == "\n" => {
        lexer.add_token(best.lexeme, range.len(), None)
      }
      Any::LineEnd(..) => {
        // The cancel is always inserted as whitespace.
        lexer.add_token(rt::WHITESPACE, range.len(), None);
        lexer.line_end_cancel = Some(range.span2())
      }

      Any::Bracket(..) => {
        // Construct the closer.
        lexer.push_closer(
          best.lexeme.cast(),
          mirrored.clone().unwrap().immortalize(),
        );
        lexer.add_token(
          best.lexeme,
          range.len(),
          Some(rt::Kind::Offset { cursor: 0, meta: 0 }),
        );
      }

      #[allow(clippy::almost_swapped)]
      Any::Comment(rule) => {
        // Comments aren't real tokens.
        emitted = false;

        let mut cursor = end;
        // The span we created only contains the open bracket for the comment.
        // We still need to lex the comment to the end.
        let mut depth = 1;
        let close = mirrored.clone().unwrap().immortalize();
        while let Some(c) = lexer.text(cursor..).chars().next() {
          if rule.can_nest && lexer.text(cursor..).starts_with(text) {
            depth += 1;
            cursor += text.len();
          } else if lexer.text(cursor..).starts_with(close.as_str()) {
            depth -= 1;
            cursor += close.len();
            if depth == 0 {
              break;
            }
          } else {
            cursor += c.len_utf8();
          }
        }

        // The EOF marker is just a funny newline, right?
        if close != "\n" && depth != 0 {
          lexer
            .builtins()
            .unclosed(span, &close, Lexeme::eof(), lexer.eof());
        }

        // Crop off an ending \n so that it can get turned into whitespace or
        // a line end token, as appropriate.
        if close == "\n" && depth == 0 {
          cursor -= 1;
        }

        lexer.add_token(best.lexeme, cursor - lexer.cursor(), None);
      }

      Any::Ident(rule) => {
        let count = text.chars().count();
        if count < rule.min_len {
          lexer.builtins().ident_too_small(rule.min_len, count, span);
        }
        if rule.ascii_only {
          for c in text.chars() {
            if !c.is_ascii()
              && !rule.extra_continues.contains(c)
              && !rule.extra_starts.contains(c)
            {
              lexer.builtins().non_ascii_in_ident(best.lexeme, range);
              break;
            }
          }
        }

        lexer.add_token(rt::PREFIX, prefix.len(), None);
        lexer.add_token(best.lexeme, range.len(), None);
        lexer.add_token(rt::SUFFIX, suffix.len(), None);
      }

      Any::Digital(rule) => {
        lexer.add_token(rt::PREFIX, prefix.len(), None);
        lexer.add_token(
          best.lexeme,
          sign_span.len() + range.len(),
          Some(rt::Kind::Digital(rt::Digital::default())),
        );
        lexer.add_token(rt::SUFFIX, suffix.len(), None);

        let sign_text = sign_span.text();
        let sign = (!sign_text.is_empty()).then(|| {
          let Some((_, value)) =
            rule.mant.signs.iter().find(|(text, _)| text == sign_text)
          else {
            bug!("could not find appropriate sign for Digital rule");
          };

          (*value, sign_span.span2())
        });

        let mut chunks = vec![DigitBlocks {
          prefix: Span2::default(),
          sign,
          blocks: Vec::new(),
          which_exp: !0,
        }];

        if !prefix.is_empty() {
          chunks[0].prefix = prefix.span2();
        }

        let mut offset = 0;
        let mut text = text;

        let mut digits = &rule.mant;
        let mut block_start = 0;
        let mut last_was_sep = false;
        let sep = rule.separator.as_str();
        'digits: while let Some(c) = text.chars().next() {
          let chunk = chunks.last_mut().unwrap();

          if !sep.is_empty() {
            if let Some(rest) = text.strip_prefix(sep) {
              if block_start == offset {
                let ok = if !chunk.blocks.is_empty() {
                  rule.corner_cases.around_point
                } else if ptr::eq(digits, &rule.mant) {
                  rule.corner_cases.prefix
                } else {
                  rule.corner_cases.around_exp
                };

                if !ok {
                  lexer.builtins().unexpected(
                    Expected::Name(yarn!("digit separator")),
                    best.lexeme,
                    range.subspan(offset..offset + sep.len()),
                  );
                }
              }

              text = rest;
              offset += rule.separator.len();
              last_was_sep = true;
              continue;
            }
          }

          if let Some(rest) = text.strip_prefix(rule.point.as_str()) {
            if last_was_sep && !rule.corner_cases.around_point {
              lexer.builtins().unexpected(
                Expected::Name(yarn!("digit separator")),
                best.lexeme,
                range.subspan(offset..offset + sep.len()),
              );
            }

            chunk
              .blocks
              .push(range.subspan(block_start..offset).span2());
            text = rest;
            offset += rule.point.len();
            block_start = offset;
            last_was_sep = false;
            continue;
          }

          for (i, (pre, exp)) in rule.exps.iter().enumerate() {
            if let Some(rest) = text.strip_prefix(pre.as_str()) {
              if last_was_sep && !rule.corner_cases.around_exp {
                lexer.builtins().unexpected(
                  Expected::Name(yarn!("digit separator")),
                  best.lexeme,
                  range.subspan(offset..offset + sep.len()),
                );
              }

              chunk
                .blocks
                .push(range.subspan(block_start..offset).span2());

              let prefix = range.subspan(offset..offset + pre.len());
              text = rest;
              offset += pre.len();

              let sign = exp
                .signs
                .iter()
                .filter(|(y, _)| rest.starts_with(y.as_str()))
                .max_by_key(|(y, _)| y.len())
                .map(|(y, s)| {
                  let sign = range.subspan(offset..offset + y.len());
                  text = &text[y.len()..];
                  offset += y.len();
                  (*s, sign.span2())
                });

              chunks.push(DigitBlocks {
                prefix: Span2::default(),
                sign,
                blocks: Vec::new(),
                which_exp: i,
              });

              if !prefix.is_empty() {
                chunks.last_mut().unwrap().prefix = prefix.span2();
              }

              digits = exp;
              block_start = offset;
              last_was_sep = false;
              continue 'digits;
            }
          }

          text = &text[c.len_utf8()..];
          offset += c.len_utf8();
        }

        if last_was_sep && !rule.corner_cases.suffix {
          lexer.builtins().unexpected(
            Expected::Name(yarn!("digit separator")),
            best.lexeme,
            range.subspan(offset - sep.len()..),
          );
        }

        chunks
          .last_mut()
          .unwrap()
          .blocks
          .push(range.subspan(block_start..).span2());
        let mant = chunks.remove(0);

        let Some(rt::Kind::Digital(meta)) = lexer
          .stream_mut()
          .last_meta_mut()
          .and_then(|m| m.kind.as_mut())
        else {
          bug!("missing rt::Digital in digital token");
        };
        meta.digits = mant;
        meta.exponents = chunks;

        let Some(rt::Kind::Digital(meta)) =
          lexer.stream().last_meta().and_then(|m| m.kind.as_ref())
        else {
          bug!("missing rt::Digital in digital token");
        };

        // This happens later so we have access to the full spans of
        // the digit blocks.
        for chunk in iter::once(&meta.digits).chain(&meta.exponents) {
          let digits = rule
            .exps
            .get(chunk.which_exp)
            .map(|(_, e)| e)
            .unwrap_or(&rule.mant);

          let chunk_span = Span::union(
            chunk
              .prefix(lexer.file())
              .into_iter()
              .chain(chunk.blocks(lexer.file())),
          );

          if (chunk.blocks.len() as u32) < digits.min_chunks {
            lexer
              .report()
              .error(f!(
                "expected at least {} `{}`{}",
                digits.min_chunks - 1,
                rule.point,
                plural(digits.min_chunks - 1)
              ))
              .at(chunk_span);
          }

          for block in chunk.blocks(lexer.file()) {
            let mut text = block.text();

            // FIXME: The is_some() here should not be necessary.
            if range.is_empty() && chunk.prefix(lexer.file()).is_some() {
              let prefix = chunk.prefix(lexer.file()).unwrap();
              lexer
                .builtins()
                .expected(
                  [Expected::Name(yarn!("digits after `{}`", prefix.text(),))],
                  match lexer.text(range.end()..).chars().next() {
                    Some(c) => Expected::Literal(Yarn::from(c)),
                    None => Expected::Lexeme(Lexeme::eof().any()),
                  },
                  range,
                )
                .saying(prefix, "because of this prefix");
            }

            while let Some(c) = text.chars().next() {
              let cursor = range.end() - text.len();
              if !rule.separator.is_empty() {
                if let Some(rest) = text.strip_prefix(rule.separator.as_str()) {
                  text = rest;
                  continue;
                }
              }

              text = &text[c.len_utf8()..];
              if !c.is_digit(digits.radix as u32) {
                lexer.builtins().unexpected(
                  Expected::Literal(c.into()),
                  lexer.stream().last_token(),
                  lexer.span(cursor..cursor + c.len_utf8()),
                )
                .remark(
                  chunk_span,
                  f!(
                    "because this value is {} (base {}), digits should be within '0'..='{:x}'",
                    digits.radix_name(), digits.radix, digits.radix - 1,
                  ),
                );
              }
            }
          }
        }
      }

      Any::Quoted(rule) => {
        let close = mirrored.clone().unwrap().immortalize();

        let mut chunk_start = end;
        let mut cursor = end;
        let mut marks = vec![chunk_start as u32];
        let uq_end = loop {
          if lexer.text(cursor..).starts_with(close.as_str()) {
            let end = cursor;
            cursor += close.len();
            if end > chunk_start {
              marks.push(end as u32);
            }

            break Some(end);
          }

          let rest = lexer.text(cursor..);
          let (esc, rule) = match rule.escapes.longest_prefix(rest) {
            Some(e) => e,
            None => match rest.chars().next() {
              Some(c) => {
                cursor += c.len_utf8();
                continue;
              }
              None => break None,
            },
          };

          // Push unconditionally: this ensures that chunks of text are always
          // between escapes, even if the literal chunks are empty.
          marks.push(cursor as u32);

          let esc_start = cursor;
          cursor += esc.len();
          let esc_end = cursor;
          let mark = match rule {
            rule::Escape::Invalid => {
              lexer.builtins().invalid_escape(
                lexer.span(esc_start..cursor),
                "invalid escape sequence",
              );
              [cursor; 3]
            }

            rule::Escape::Basic => [cursor; 3],

            rule::Escape::Fixed(chars) => {
              let arg_start = cursor;
              let mut count = 0;
              for _ in 0..*chars {
                // TRICKY: We have just skipped over \x. If we were to take *any*
                // characters, we would lex `"\x" ` as being `\x` with arg `" `.
                // So, we want to check for a closer on *every* loop iteration, and
                // break out if we *see* it: we should not consume it.
                if lexer.text(cursor..).starts_with(close.as_str()) {
                  break;
                }

                match lexer.text(cursor..).chars().next() {
                  Some(c) => cursor += c.len_utf8(),
                  None => break,
                }
                count += 1;
              }

              if count != *chars {
                lexer.builtins().invalid_escape(
                  lexer.span(esc_start..cursor),
                  f!(
                    "expected exactly {chars} character{} here",
                    plural(*chars)
                  ),
                );
              }

              [arg_start, cursor, cursor]
            }

            rule::Escape::Bracketed(open, close) => 'delim: {
              if !lexer.text(cursor..).starts_with(open.as_str()) {
                lexer.builtins().invalid_escape(
                  lexer.span(esc_start..cursor),
                  f!("expected a `{open}`"),
                );
                break 'delim [cursor; 3];
              } else {
                cursor += open.len()
              }

              let arg_start = cursor;
              let Some(len) = lexer.text(..cursor).find(close.as_str()) else {
                lexer.builtins().invalid_escape(
                  lexer.span(esc_start..cursor),
                  f!("expected a `{close}`"),
                );
                break 'delim [arg_start, cursor, cursor];
              };
              cursor += len + close.len();
              [arg_start, arg_start + len, cursor]
            }
          };

          marks.push(esc_end as u32);
          marks.extend(mark.iter().map(|&x| x as u32));
          chunk_start = cursor;
        };

        if uq_end.is_none() {
          lexer
            .builtins()
            .unclosed(span, &close, Lexeme::eof(), lexer.eof());
        }

        // We have to parse the suffix ourselves explicitly!
        let suf = rule
          .affixes
          .suffixes()
          .iter()
          .filter(|y| lexer.text(cursor..).starts_with(y.as_str()))
          .map(|y| y.len())
          .max()
          .unwrap_or_else(|| {
            let found = match lexer.text(cursor..).chars().next() {
              Some(n) => Expected::Literal(n.into()),
              None => Lexeme::eof().into(),
            };

            lexer.builtins().expected(
              rule
                .affixes
                .suffixes()
                .iter()
                .map(|y| Expected::Literal(y.aliased())),
              found,
              lexer.span(cursor..cursor),
            );

            0
          });

        lexer.add_token(rt::PREFIX, prefix.len(), None);
        lexer.add_token(
          best.lexeme,
          cursor - lexer.cursor(),
          Some(rt::Kind::Quoted(rt::Quoted { marks })),
        );
        lexer.add_token(rt::SUFFIX, suf, None);
      }
    }
  }

  // Now that we've lexed all we can, we need to take care of two error
  // conditions. First, overparsing: if `match_.extra` is too long, some
  // extra characters need to be diagnosed. Second, if self.cursor() points
  // just past an XID character, we need to skip all XID characters that follow
  // and diagnose that.

  if match_.extra > 0 {
    let expected = if emitted {
      Expected::Token(lexer.stream().last_token())
    } else if let Some(mirrored) = &mirrored {
      if best.is_close {
        Expected::Literal(yarn!("{mirrored} ... {text}"))
      } else {
        Expected::Literal(yarn!("{text} ... {mirrored}"))
      }
    } else {
      Expected::Lexeme(best.lexeme)
    };

    let start = start + match_.len;
    lexer
      .builtins()
      .extra_chars(expected, lexer.span(start..start + match_.extra));
  }

  let rest = lexer.text(lexer.cursor()..);
  let prev = lexer.text(..lexer.cursor()).chars().next_back();
  if prev.is_some_and(is_xid) {
    let xids = rest.find(|c| !is_xid(c)).unwrap_or(rest.len());
    if xids > 0 {
      let start = lexer.cursor();
      lexer.add_token(rt::UNEXPECTED, xids, None);

      let expected = if emitted {
        Expected::Token(lexer.stream().last_token())
      } else if let Some(mirrored) = &mirrored {
        if best.is_close {
          Expected::Literal(yarn!("{mirrored} ... {text}"))
        } else {
          Expected::Literal(yarn!("{text} ... {mirrored}"))
        }
      } else {
        Expected::Lexeme(best.lexeme)
      };

      lexer
        .builtins()
        .extra_chars(expected, lexer.span(start..start + xids));
    }
  }
}

/// Extracts the affixes from `text`.
fn find_affixes_partial<'a>(
  range: Span<'a>,
  spec: &Spec,
  best: Lexeme2,
) -> [Span<'a>; 4] {
  let text = range.text();
  let ep = range.file().span(0..0);
  match spec.rule(best.lexeme) {
    Any::Ident(rule) => {
      let [pre, range, suf] = find_affixes(range, &rule.affixes);
      [ep, pre, range, suf]
    }
    Any::Digital(rule) => {
      let sign = rule
        .mant
        .signs
        .iter()
        .filter(|(y, _)| text.starts_with(y.as_str()))
        .map(|(y, _)| y.len())
        .max()
        .unwrap_or(0);
      let (sign, range) = range.split_at(sign);

      let [pre, range, suf] = find_affixes(range, &rule.affixes);
      [sign, pre, range, suf]
    }
    Any::Quoted(rule) if !best.is_close => {
      let (pre, range) = find_prefix(range, &rule.affixes);
      [ep, pre, range, ep]
    }
    Any::Quoted(rule) => {
      let (range, suf) = find_suffix(range, &rule.affixes);
      [ep, ep, range, suf]
    }
    _ => [ep, ep, range, ep],
  }
}

/// Extracts the affixes from `text`.
fn find_affixes<'a>(range: Span<'a>, affixes: &Affixes) -> [Span<'a>; 3] {
  let (prefix, range) = find_prefix(range, affixes);
  let (range, suffix) = find_suffix(range, affixes);
  [prefix, range, suffix]
}

fn find_prefix<'a>(range: Span<'a>, affixes: &Affixes) -> (Span<'a>, Span<'a>) {
  let text = range.text();
  let prefix = affixes
    .prefixes()
    .iter()
    .filter(|y| text.starts_with(y.as_str()))
    .map(|y| y.len())
    .max()
    .unwrap_or_else(|| bug!("could not find matching prefix post-DFA"));
  range.split_at(prefix)
}

fn find_suffix<'a>(range: Span<'a>, affixes: &Affixes) -> (Span<'a>, Span<'a>) {
  let text = range.text();
  let suffix = affixes
    .suffixes()
    .iter()
    .filter(|y| text.ends_with(y.as_str()))
    .map(|y| y.len())
    .max()
    .unwrap_or_else(|| bug!("could not find matching suffix post-DFA"));
  range.split_at(text.len() - suffix)
}
