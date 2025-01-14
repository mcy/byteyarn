//! Token types.
//!
//! A token is created during lexing, and records information about a matched
//! [`Rule`][crate::rule::Rule] (or failure to match).
//!
//! It is idiomatic to `use ilex::token;` and refer to types in this module with
//! a `token` prefix. E.g., `token::Any`, `token::Ident`, and `token::Stream`.
//!
//! The token types themselves are thin immutable references into a
//! [`token::Stream`][crate::token::Stream], and should be passed around by
//! value. They all implement [`Token`].

use std::fmt;
use std::iter;
use std::num::NonZeroU32;
use std::ops::RangeBounds;
use std::panic::Location;

use byteyarn::yarn;
use byteyarn::YarnBox;
use num_traits::Bounded;

use crate::f;
use crate::file::Context;
use crate::file::Span;
use crate::file::Spanned;
use crate::fp;
use crate::report::Report;
use crate::rt;
use crate::rt::DigitBlocks;
use crate::rule;
use crate::spec::Lexeme;
use crate::spec::Spec;
use crate::Never;
use crate::WrongKind;

mod stream;
pub mod summary;

pub use stream::switch::switch;
pub use stream::switch::Switch;
pub use stream::Comments;
pub use stream::Cursor;
pub use stream::Stream;

/// A token ID.
///
/// An [`Id`] is a lightweight handle to some token, which can be converted
/// back into that token using the corresponding [`Stream`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Id(pub(crate) NonZeroU32);

impl Id {
  fn idx(self) -> usize {
    self.0.get() as usize - 1
  }

  fn prev(self) -> Option<Id> {
    NonZeroU32::new(self.0.get() - 1).map(Self)
  }

  fn next(self) -> Option<Id> {
    self.0.checked_add(1).map(Self)
  }
}

/// A token type. All types in [`ilex::token`][crate::token] implement this
/// trait.
pub trait Token<'lex>:
  Copy + Spanned<'lex> + fmt::Debug + TryFrom<Any<'lex>> + Into<Any<'lex>>
{
  /// The token this rule was parsed from.
  type Rule: rule::Rule;

  /// The ID of this token.
  fn id(self) -> Id;

  /// The token stream that owns this token.
  fn stream(self) -> &'lex Stream<'lex>;

  /// The context that owns this token.
  fn context(self) -> &'lex Context {
    self.stream().context()
  }

  /// The spec that lexed this token.
  fn spec(self) -> &'lex Spec {
    self.stream().spec()
  }

  /// Returns this token's [`Lexeme`].
  fn lexeme(self) -> Lexeme<Self::Rule> {
    self.stream().lookup_token(self.id()).lexeme.cast()
  }

  /// Returns an iterator over the attacked to this token.
  fn comments(self) -> Comments<'lex> {
    let stream = self.stream();
    Comments {
      stream,
      comments: stream
        .lookup_meta(self.id())
        .map(|m| m.comments.as_slice())
        .unwrap_or(&[])
        .iter(),
    }
  }

  /// The rule inside of [`Token::spec()`] that this token refers to.
  ///
  /// Returns `None` for [`Eof`].
  fn rule(self) -> Option<&'lex Self::Rule> {
    let lexeme = self.lexeme();
    if lexeme.any() == Lexeme::eof().any() {
      return None;
    }

    Some(self.spec().rule(lexeme))
  }

  // /// Returns this token's raw [`LexerId`], if it has one.
  // fn lexer_id(self) -> Option<LexerId>;

  /// This exist tot work around a type checker limitation around Result::unwrap().
  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self;
}

/// A type-erased [`Token`].
///
/// This includes all possible token types, plus the "unexpected" tokens that
/// represent unexpected characters encountered and skipped.
#[derive(Copy, Clone)]
#[allow(missing_docs)]
pub enum Any<'lex> {
  Eof(Eof<'lex>),
  Keyword(Keyword<'lex>),
  Bracket(Bracket<'lex>),
  Ident(Ident<'lex>),
  Digital(Digital<'lex>),
  Quoted(Quoted<'lex>),
}

impl<'lex> Token<'lex> for Any<'lex> {
  type Rule = rule::Any;

  fn id(self) -> Id {
    match self {
      Self::Eof(tok) => tok.id(),
      Self::Bracket(tok) => tok.id(),
      Self::Keyword(tok) => tok.id(),
      Self::Ident(tok) => tok.id(),
      Self::Digital(tok) => tok.id(),
      Self::Quoted(tok) => tok.id(),
    }
  }

  fn stream(self) -> &'lex Stream<'lex> {
    match self {
      Self::Eof(tok) => tok.stream(),
      Self::Bracket(tok) => tok.stream(),
      Self::Keyword(tok) => tok.stream(),
      Self::Ident(tok) => tok.stream(),
      Self::Digital(tok) => tok.stream(),
      Self::Quoted(tok) => tok.stream(),
    }
  }

  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self {
    any
  }
}

impl<'lex> Any<'lex> {
  /// The bare name shown for whatever this token is in `fmt::Debug`.
  pub(crate) fn debug_name(self) -> &'static str {
    match self {
      Any::Eof(_) => "Eof",
      Any::Keyword(_) => "Keyword",
      Any::Bracket(_) => "Bracket",
      Any::Ident(_) => "Ident",
      Any::Digital(_) => "Digital",
      Any::Quoted(_) => "Quoted",
    }
  }

  /// Converts this token into an [`Eof`] if it is one.
  pub fn eof(self) -> Result<Eof<'lex>, WrongKind> {
    match self {
      Self::Eof(tok) => Ok(tok),
      _ => Err(WrongKind { want: "Eof", got: self.debug_name() }),
    }
  }

  /// Converts this token into a [`Keyword`] if it is one.
  pub fn keyword(self) -> Result<Keyword<'lex>, WrongKind> {
    match self {
      Self::Keyword(tok) => Ok(tok),
      _ => Err(WrongKind { want: "Keyword", got: self.debug_name() }),
    }
  }

  /// Converts this token into a [`Bracket`] if possible.
  pub fn bracket(self) -> Result<Bracket<'lex>, WrongKind> {
    match self {
      Self::Bracket(tok) => Ok(tok),
      _ => Err(WrongKind { want: "Bracket", got: self.debug_name() }),
    }
  }

  /// Converts this token into an [`Ident`] if it is one.
  pub fn ident(self) -> Result<Ident<'lex>, WrongKind> {
    match self {
      Self::Ident(tok) => Ok(tok),
      _ => Err(WrongKind { want: "Ident", got: self.debug_name() }),
    }
  }

  /// Converts this token into a [`Digital`] if it is one.
  pub fn digital(self) -> Result<Digital<'lex>, WrongKind> {
    match self {
      Self::Digital(tok) => Ok(tok),
      _ => Err(WrongKind { want: "Digital", got: self.debug_name() }),
    }
  }

  /// Converts this token into a [`Quoted`] if it is one.
  pub fn quoted(self) -> Result<Quoted<'lex>, WrongKind> {
    match self {
      Self::Quoted(tok) => Ok(tok),
      _ => Err(WrongKind { want: "Quoted", got: self.debug_name() }),
    }
  }
}

impl fmt::Debug for Any<'_> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match self {
      Self::Eof(tok) => write!(f, "token::{tok:?}"),
      Self::Keyword(tok) => write!(f, "token::{tok:?}"),
      Self::Ident(tok) => write!(f, "token::{tok:?}"),
      Self::Digital(tok) => write!(f, "token::{tok:?}"),
      Self::Quoted(tok) => write!(f, "token::{tok:?}"),

      Self::Bracket(tok) => {
        f.write_str("token::")?;
        fmt::Debug::fmt(tok, f)
      }
    }
  }
}

impl<'lex> Spanned<'lex> for Any<'lex> {
  fn span(&self) -> Span<'lex> {
    match self {
      Self::Eof(tok) => tok.span(),
      Self::Keyword(tok) => tok.span(),
      Self::Bracket(tok) => tok.span(),
      Self::Ident(tok) => tok.span(),
      Self::Quoted(tok) => tok.span(),
      Self::Digital(tok) => tok.span(),
    }
  }
}

/// A special token for the end of a file.
///
/// The main purpose of this token is to hold any comment spans that are after
/// all the "real" tokens. For example, a file consisting of only comments will
/// have a single `Eof` token after lexing, and will include all of those
/// comments within.
#[derive(Copy, Clone)]
pub struct Eof<'lex> {
  stream: &'lex Stream<'lex>,
  id: Id,
}

impl<'lex> Token<'lex> for Eof<'lex> {
  type Rule = rule::Eof;

  fn id(self) -> Id {
    self.id
  }

  fn stream(self) -> &'lex Stream<'lex> {
    self.stream
  }

  fn context(self) -> &'lex Context {
    self.stream.context()
  }

  fn spec(self) -> &'lex Spec {
    self.stream.spec()
  }

  fn lexeme(self) -> Lexeme<Self::Rule> {
    Lexeme::eof()
  }

  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self {
    any.try_into().unwrap()
  }
}

impl<'lex> From<Eof<'lex>> for Any<'lex> {
  fn from(value: Eof<'lex>) -> Self {
    Any::Eof(value)
  }
}

impl<'lex> TryFrom<Any<'lex>> for Eof<'lex> {
  type Error = WrongKind;
  fn try_from(value: Any<'lex>) -> Result<Self, Self::Error> {
    value.eof()
  }
}

impl fmt::Debug for Eof<'_> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "Eof({:?})", self.span())
  }
}

impl<'lex> Spanned<'lex> for Eof<'lex> {
  fn span(&self) -> Span<'lex> {
    self.stream.lookup_span_no_affix(self.id)
  }
}

/// A keyword, i.e., an exact well-known string, such as `+`, `class`, and
/// `#define`.
///
/// Keywords are similar to identifiers, but their content is always the same
/// fixed string.
#[derive(Copy, Clone)]
pub struct Keyword<'lex> {
  stream: &'lex Stream<'lex>,
  id: Id,
}

impl<'lex> Token<'lex> for Keyword<'lex> {
  type Rule = rule::Keyword;

  fn id(self) -> Id {
    self.id
  }

  fn stream(self) -> &'lex Stream<'lex> {
    self.stream
  }

  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self {
    any.try_into().unwrap()
  }
}

impl<'lex> From<Keyword<'lex>> for Any<'lex> {
  fn from(value: Keyword<'lex>) -> Self {
    Any::Keyword(value)
  }
}

impl<'lex> TryFrom<Any<'lex>> for Keyword<'lex> {
  type Error = WrongKind;
  fn try_from(value: Any<'lex>) -> Result<Self, Self::Error> {
    value.keyword()
  }
}

impl fmt::Debug for Keyword<'_> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "Keyword({:?})", self.span())
  }
}

impl<'lex> Spanned<'lex> for Keyword<'lex> {
  fn span(&self) -> Span<'lex> {
    self.stream.lookup_span_no_affix(self.id)
  }
}

/// A bracket pair, such as matching  `( ... )`, which contains a substream
/// of tokens.
///
/// Skipping over a bracket skips over all tokens in it, since `ilex` uses token
/// *trees*, like Rust does.
#[derive(Copy, Clone)]
pub struct Bracket<'lex> {
  open: Id,
  close: Id,
  contents: Cursor<'lex>,
}

impl<'lex> Bracket<'lex> {
  /// Returns this token's open delimiter.
  pub fn open(self) -> Span<'lex> {
    self.contents.stream().lookup_span_no_affix(self.open)
  }

  /// Returns this token's close delimiter.
  pub fn close(self) -> Span<'lex> {
    self.contents.stream().lookup_span_no_affix(self.close)
  }

  /// Returns this token's quote delimiters.
  pub fn delimiters(self) -> [Span<'lex>; 2] {
    [self.open(), self.close()]
  }

  /// Returns a cursor over this bracket's internal tokens (not including the
  /// delimiters themselves!).
  ///
  /// `Bracket` is also [`IntoIterator`].
  pub fn contents(self) -> Cursor<'lex> {
    self.contents
  }
}

impl<'lex> Token<'lex> for Bracket<'lex> {
  type Rule = rule::Bracket;

  fn id(self) -> Id {
    self.open
  }

  fn stream(self) -> &'lex Stream<'lex> {
    self.contents().stream()
  }

  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self {
    any.try_into().unwrap()
  }
}

impl<'lex> From<Bracket<'lex>> for Any<'lex> {
  fn from(value: Bracket<'lex>) -> Self {
    Any::Bracket(value)
  }
}

impl<'lex> TryFrom<Any<'lex>> for Bracket<'lex> {
  type Error = WrongKind;
  fn try_from(value: Any<'lex>) -> Result<Self, Self::Error> {
    value.bracket()
  }
}

impl<'lex> IntoIterator for Bracket<'lex> {
  type Item = Any<'lex>;
  type IntoIter = Cursor<'lex>;
  fn into_iter(self) -> Self::IntoIter {
    self.contents()
  }
}

impl fmt::Debug for Bracket<'_> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    f.debug_struct("Bracket")
      .field("delimiters", &f!("({:?}, {:?})", self.open(), self.close()))
      .field("contents", &self.contents)
      .finish()
  }
}

impl<'lex> Spanned<'lex> for Bracket<'lex> {
  fn span(&self) -> Span<'lex> {
    let [a, b] = self.delimiters();
    self.contents.stream().file().span(a.start()..b.end())
  }
}

/// A identifier, i.e., a self-delimiting word like `foo` or `黒猫`.
#[derive(Copy, Clone)]
pub struct Ident<'lex> {
  stream: &'lex Stream<'lex>,
  id: Id,
}

impl<'lex> Ident<'lex> {
  /// Returns this token's name span.
  pub fn name(self) -> Span<'lex> {
    self.stream.lookup_span_no_affix(self.id)
  }

  /// Returns this token's prefix.
  pub fn prefix(self) -> Option<Span<'lex>> {
    self.stream.lookup_prefix(self.id)
  }

  /// Checks whether this identifier has a particular prefix.
  pub fn has_prefix(&self, expected: &str) -> bool {
    self.prefix().is_some_and(|s| s.text() == expected)
  }

  /// Returns this token's suffix.
  pub fn suffix(&self) -> Option<Span<'lex>> {
    self.stream.lookup_suffix(self.id)
  }

  /// Checks whether this identifier has a particular prefix.
  pub fn has_suffix(&self, expected: &str) -> bool {
    self.suffix().is_some_and(|s| s.text() == expected)
  }
}

impl<'lex> Token<'lex> for Ident<'lex> {
  type Rule = rule::Ident;

  fn id(self) -> Id {
    self.id
  }

  fn stream(self) -> &'lex Stream<'lex> {
    self.stream
  }

  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self {
    any.try_into().unwrap()
  }
}

impl<'lex> From<Ident<'lex>> for Any<'lex> {
  fn from(value: Ident<'lex>) -> Self {
    Any::Ident(value)
  }
}

impl<'lex> TryFrom<Any<'lex>> for Ident<'lex> {
  type Error = WrongKind;
  fn try_from(value: Any<'lex>) -> Result<Self, Self::Error> {
    value.ident()
  }
}

impl fmt::Debug for Ident<'_> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    if self.prefix().is_none() && self.suffix().is_none() {
      return write!(f, "Ident({:?})", self.name());
    }

    let mut f = f.debug_struct("Ident");
    f.field("span", &self.span()).field("name", &self.name());

    if let Some(prefix) = self.prefix() {
      f.field("prefix", &prefix);
    }

    if let Some(suffix) = self.suffix() {
      f.field("suffix", &suffix);
    }

    f.finish()
  }
}

impl<'lex> Spanned<'lex> for Ident<'lex> {
  fn span(&self) -> Span<'lex> {
    self.stream.lookup_span_with_affixes(self.id)
  }
}

/// A digital literal.
///
/// See [`rule::Digital`] for an explanation on what a "digital" is (briefly,
/// they are generalized number literals).
///
/// Note that this crate does not provide general number parsing services;
/// parsing numbers is a complex and subtle undertaking, particularly when it
/// comes to floats. For that, consider reading the source code of the
/// `rustc_apfloat` library, or another softfloat library.
///
/// However, we do provide parsing for a few common cases: decimal, binary,
/// octal, and hexadecimal. For floats, this means that the radix of the
/// mantissa is decimal, binary, octal, or hex, the radix of the exponent is
/// decimal, and the base of the exponent is 10 (for decimal) or 2 (for the
/// others).
#[derive(Copy, Clone)]
pub struct Digital<'lex> {
  stream: &'lex Stream<'lex>,
  id: Id,
  meta: &'lex rt::Digital,
  idx: usize,
}

impl<'lex> Digital<'lex> {
  /// Returns the radix that this digital literal's digits were parsed in.
  pub fn radix(self) -> u8 {
    self.digit_rule().radix
  }

  /// Returns the leading sign of this digital literal, if it had any.
  pub fn sign(self) -> Option<Sign> {
    self.rt_blocks().sign.map(|(s, _)| s)
  }

  /// Checks if there was an explicit positive sign.
  pub fn is_positive(self) -> bool {
    self.sign() == Some(Sign::Pos)
  }

  /// Checks if there was an explicit positive sign.
  pub fn is_negative(self) -> bool {
    self.sign() == Some(Sign::Neg)
  }

  /// Returns the span corresponding to [`Digital::sign()`].
  pub fn sign_span(self) -> Option<Span<'lex>> {
    self.rt_blocks().sign(self.file())
  }

  /// Returns the point-separated digit chunks of this digital literal.
  pub fn digit_blocks(self) -> impl Iterator<Item = Span<'lex>> + 'lex {
    self.rt_blocks().blocks(self.file())
  }

  /// Returns the exponents of this digital literal, if it any.
  ///
  /// Calling `exponents()` on any of the returned tokens will yield all
  /// exponents that follow.
  pub fn exponents(self) -> impl Iterator<Item = Digital<'lex>> {
    (self.idx..self.meta.exponents.len()).map(move |idx| Self {
      stream: self.stream,
      id: self.id,
      meta: self.meta,
      idx: idx + 1,
    })
  }

  /// Returns this token's prefix.
  pub fn prefix(self) -> Option<Span<'lex>> {
    if self.idx > 0 {
      return self.rt_blocks().prefix(self.file());
    }

    self.stream.lookup_prefix(self.id)
  }

  /// Checks whether this identifier has a particular prefix.
  pub fn has_prefix(&self, expected: &str) -> bool {
    self.prefix().is_some_and(|s| s.text() == expected)
  }

  /// Returns this token's suffix.
  pub fn suffix(&self) -> Option<Span<'lex>> {
    if self.idx > 0 {
      // Exponent tokens never have a suffix.
      return None;
    }

    self.stream.lookup_suffix(self.id)
  }

  /// Checks whether this identifier has a particular prefix.
  pub fn has_suffix(&self, expected: &str) -> bool {
    self.suffix().is_some_and(|s| s.text() == expected)
  }

  /// Parses this token as an integer.
  ///
  /// More than one digit block, or any exponents, will be diagnosed as an
  /// error.
  ///
  /// Parse failures become diagnostics, and an unspecified value is provided
  /// for a failed integer.
  #[track_caller]
  pub fn to_int<N>(self, range: impl RangeBounds<N>, report: &Report) -> N
  where
    N: Bounded + PartialOrd + FromRadix + fmt::Display,
  {
    for extra in self.digit_blocks().skip(1) {
      report.builtins(self.spec()).unexpected(
        "extra digits",
        self.lexeme(),
        extra,
      );
    }

    for extra in self.exponents() {
      report
        .builtins(self.spec())
        .unexpected("exponent", self.lexeme(), extra);
    }

    self.to_ints(range, report).drain(..).next().unwrap()
  }

  /// Parses the blocks of this digital literal as a sequence of integers;
  /// this ignores any exponents that follow.
  ///
  /// Parse failures become diagnostics, and an unspecified value is provided
  /// for a failed integer.
  #[track_caller]
  pub fn to_ints<N>(self, range: impl RangeBounds<N>, report: &Report) -> Vec<N>
  where
    N: Bounded + PartialOrd + FromRadix + fmt::Display,
  {
    let rule = self.rule().unwrap();
    let radix = self.radix();
    let here = Location::caller();

    self
      .digit_blocks()
      .map(|span| {
        let text = span.text();
        let buf;
        let text =
          if !rule.separator.is_empty() && text.contains(&*rule.separator) {
            buf = text.replace(&*rule.separator, "");
            buf.as_str()
          } else {
            text
          };

        let mut value = N::from_radix(text, radix, &rule.separator);
        if self.is_negative() {
          value = value.and_then(N::checked_neg);
        }

        if value.is_none() || value.as_ref().is_some_and(|v| !range.contains(v))
        {
          report
            .builtins(self.spec())
            .literal_out_of_range(
              Any::from(self),
              span,
              &range,
              &N::min_value(),
              &N::max_value(),
            )
            .reported_at(here);
        }

        value.unwrap_or(N::max_value())
      })
      .collect()
  }

  /// Parses this token as a float. `Fp` can be any of the float types defined
  /// in [`ilex::fp`][crate::fp].
  ///
  /// This function supports floats with a mantissa radix that is either 10 or
  /// a power of two. In the former case, the base for the exponent is 10;
  /// otherwise, it is 2.
  ///
  /// Any other radix will produce `Err`. It is highly recommended to
  /// `unwrap()` the result, since a return value of `Exotic` is almost
  /// certainly a bug.
  ///
  /// Parse failures and overflow to infinity will generate diagnostics.
  #[track_caller]
  pub fn to_float<Fp: fp::Parse>(
    self,
    range: impl RangeBounds<Fp>,
    report: &Report,
  ) -> Result<Fp, fp::Exotic> {
    let fp: Fp = self.parse_fp(report, false)?;

    if !fp.__is_finite() || !range.contains(&fp) {
      report.builtins(self.spec()).literal_out_of_range(
        self,
        self,
        &range,
        &Fp::__min(),
        &Fp::__max(),
      );
    }

    Ok(fp)
  }

  /// Parses this token as a float, with no rounding. `Fp` can be any of the
  /// float types defined in [`token::fp`][crate::fp].
  ///
  /// This function is like [`Digital::to_float()`], except that it also
  /// generates a diagnostic if rounding while converting to base 2.
  #[track_caller]
  pub fn to_float_exact<Fp: fp::Parse>(
    self,
    range: impl RangeBounds<Fp>,
    report: &Report,
  ) -> Result<Fp, fp::Exotic> {
    let fp: Fp = self.parse_fp(report, true)?;

    if !fp.__is_finite() || !range.contains(&fp) {
      report.builtins(self.spec()).literal_out_of_range(
        self,
        self,
        &range,
        &Fp::__min(),
        &Fp::__max(),
      );
    }

    Ok(fp)
  }

  fn digit_rule(self) -> &'lex rule::Digits {
    let rule = self.rule().unwrap();
    if self.idx == 0 {
      &rule.mant
    } else {
      &rule.exps[self.rt_blocks().which_exp].1
    }
  }

  fn rt_blocks(&self) -> &'lex DigitBlocks {
    if self.idx == 0 {
      return &self.meta.digits;
    }
    &self.meta.exponents[self.idx - 1]
  }
}

/// A sign for a [`Digital`] literal.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Sign {
  /// Positive.
  Pos,
  /// Negative.
  Neg,
}

impl Default for Sign {
  fn default() -> Self {
    Self::Pos
  }
}

/// A base 2 integer type of portable size that can be parsed from any radix.
pub trait FromRadix: Sized {
  /// Parses a value from` data`, given it's in a particular radix.
  ///
  /// The result must be exact: truncation or rounding are not permitted.
  /// Occurrences thereof must be signaled by returning `None`.
  ///
  /// The implementation may assume that `radix` is in `2..=16`, and that the
  /// the only bytes that occur in `data` are `0..=9`, `a..=f`, and `A..=F`, as
  /// would be implied by the value of `radix`; also, the byte sequence in
  /// `sep` may appear, which should be ignored.
  fn from_radix(data: &str, radix: u8, sep: &str) -> Option<Self>;

  /// Equivalent to `std`'s [`checked_neg()`][i32::checked_neg()].
  fn checked_neg(self) -> Option<Self>;
}

macro_rules! impl_radix {
  ($($ty:ty,)*) => {$(
    impl FromRadix for $ty {
      fn from_radix(
        mut data: &str,
        radix: u8,
        sep: &str,
      ) -> Option<Self> {
        let mut total: Self = 0;
        let mut count = 0;
        while !data.is_empty() {
          if !sep.is_empty() {
            if let Some(rest) = data.strip_prefix(sep) {
              data = rest;
              continue;
            }
          }

          let next = data.chars().next().unwrap();
          data = &data[next.len_utf8()..];

          let digit = next.to_digit(radix as _).unwrap_or_else(|| {
            bug!("an invalid digit slipped past the lexer: {:?}", next)
          });

          total = total
            .checked_mul(radix as Self)
            .and_then(|i| i.checked_add(digit as Self))?;

          count += 1;
        }

        if count == 0 {
          return None;
        }

        Some(total)
      }

      fn checked_neg(self) -> Option<Self> {
        self.checked_neg()
      }
    }
  )*};
}

impl_radix! {
  i8, i16, i32, i64, i128,
  u8, u16, u32, u64, u128,
}

impl<'lex> Token<'lex> for Digital<'lex> {
  type Rule = rule::Digital;

  fn id(self) -> Id {
    self.id
  }

  fn stream(self) -> &'lex Stream<'lex> {
    self.stream
  }

  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self {
    any.try_into().unwrap()
  }
}

impl<'lex> From<Digital<'lex>> for Any<'lex> {
  fn from(value: Digital<'lex>) -> Self {
    Any::Digital(value)
  }
}

impl<'lex> TryFrom<Any<'lex>> for Digital<'lex> {
  type Error = WrongKind;
  fn try_from(value: Any<'lex>) -> Result<Self, Self::Error> {
    value.digital()
  }
}

impl fmt::Debug for Digital<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    let mut f = f.debug_struct("Digital");
    f.field("span", &self.span())
      .field("radix", &self.radix())
      // TODO: Get rid of this collect.
      .field("digits", &self.digit_blocks().collect::<Vec<_>>());

    if let Some(sign) = self.sign() {
      f.field("sign", &sign);
    }

    if let Some(prefix) = self.prefix() {
      f.field("prefix", &prefix);
    }

    if let Some(suffix) = self.suffix() {
      f.field("suffix", &suffix);
    }

    if let Some(exp) = self.exponents().next() {
      f.field("exponent", &exp);
    }

    f.finish()
  }
}

impl<'lex> Spanned<'lex> for Digital<'lex> {
  fn span(&self) -> Span<'lex> {
    self.stream.lookup_span_with_affixes(self.id)
  }
}

/// A quoted literal.
#[derive(Copy, Clone)]
pub struct Quoted<'lex> {
  stream: &'lex Stream<'lex>,
  id: Id,
  meta: &'lex rt::Quoted,
}

impl<'lex> Quoted<'lex> {
  /// Returns this token's open delimiter.
  pub fn open(self) -> Span<'lex> {
    self.delimiters()[0]
  }

  /// Returns this token's close delimiter.
  pub fn close(self) -> Span<'lex> {
    self.delimiters()[1]
  }

  /// Returns this token's quote delimiters.
  pub fn delimiters(self) -> [Span<'lex>; 2] {
    let span = self.stream.lookup_span_no_affix(self.id);
    [
      self
        .stream
        .file()
        .span(span.start()..*self.meta.marks.first().unwrap() as usize),
      self
        .stream
        .file()
        .span(*self.meta.marks.last().unwrap() as usize..span.end()),
    ]
  }

  /// Returns the raw content of this token.
  ///
  /// There are two kinds of content: either a literal span of Unicode scalars
  /// (represented as a [`Span`] pointing to those characters) or a single
  /// escape, potentially with some side data.
  ///
  /// It is up to the user of the library to decode these two content types into
  /// strings. [`Quoted::to_utf8()`] helps with the common case of doing this for
  /// UTF-8 strings.
  pub fn raw_content(self) -> impl Iterator<Item = Content<Span<'lex>>> + 'lex {
    let file = self.stream.file();
    let mut next = self.meta.marks[0];
    let mut is_escape = false;
    let mut marks = &self.meta.marks[1..];

    iter::from_fn(move || loop {
      return match is_escape {
        false => {
          let start = next;
          let &[end, ref rest @ ..] = marks else {
            return None;
          };

          next = end;
          marks = rest;
          is_escape = true;

          if start == end {
            continue;
          }

          let span = file.span(start as usize..end as usize);
          Some(Content::Lit(span))
        }
        true => {
          let start = next;
          let &[esc_end, data_start, data_end, end, ref rest @ ..] = marks
          else {
            return None;
          };

          next = end;
          marks = rest;
          is_escape = false;

          let span = file.span(start as usize..esc_end as usize);
          let data = (data_start != data_end)
            .then(|| file.span(data_start as usize..data_end as usize));
          Some(Content::Esc(span, data))
        }
      };
    })
  }

  /// Returns the unique single literal content of this token, if it is unique.
  pub fn literal(self) -> Option<Span<'lex>> {
    if self.meta.marks.len() > 2 {
      return None;
    }
    let start = *self.meta.marks.first().unwrap();
    let end = *self.meta.marks.last().unwrap();
    Some(self.stream.file().span(start as usize..end as usize))
  }

  /// Constructs a UTF-8 string in the "obvious way", using this token and a
  /// mapping function for escapes.
  pub fn to_utf8(
    self,
    mut decode_esc: impl FnMut(Span, Option<Span<'lex>>, &mut String),
  ) -> String {
    let total = self
      .raw_content()
      .map(|c| match c {
        Content::Lit(sp) => sp.text().len(),
        Content::Esc(..) => 1,
      })
      .sum();

    let mut buf = String::with_capacity(total);
    for chunk in self.raw_content() {
      match chunk {
        Content::Lit(sp) => buf.push_str(sp.text()),
        Content::Esc(sp, data) => decode_esc(sp, data, &mut buf),
      }
    }
    buf
  }

  /// Returns this token's prefix.
  pub fn prefix(self) -> Option<Span<'lex>> {
    self.stream.lookup_prefix(self.id)
  }

  /// Checks whether this identifier has a particular prefix.
  pub fn has_prefix(&self, expected: &str) -> bool {
    self.prefix().is_some_and(|s| s.text() == expected)
  }

  /// Returns this token's suffix.
  pub fn suffix(&self) -> Option<Span<'lex>> {
    self.stream.lookup_suffix(self.id)
  }

  /// Checks whether this identifier has a particular prefix.
  pub fn has_suffix(&self, expected: &str) -> bool {
    self.suffix().is_some_and(|s| s.text() == expected)
  }
}

/// A piece of a quoted literal.
///
/// The "span type" is configurable; this type is used by multiple parts of
/// the library.
#[derive(Copy, Clone, Debug)]
pub enum Content<Span> {
  /// A literal chunk, i.e. UTF-8 text directly from the source file.
  Lit(Span),

  /// An escape sequence, which may have associated data (e.g. the `NN` from a
  /// `\xNN`).
  Esc(Span, Option<Span>),
}

impl<Span> Content<Span> {
  /// Literal contents.
  pub fn lit(chunk: impl Into<Span>) -> Self {
    Self::Lit(chunk.into())
  }

  /// Escaped contents.
  pub fn esc(chunk: impl Into<Span>) -> Self {
    Self::Esc(chunk.into(), None)
  }

  /// Escaped contents.
  pub fn esc_with_data(chunk: impl Into<Span>, data: impl Into<Span>) -> Self {
    Self::Esc(chunk.into(), Some(data.into()))
  }
}

impl<'lex> Token<'lex> for Quoted<'lex> {
  type Rule = rule::Quoted;

  fn id(self) -> Id {
    self.id
  }

  fn stream(self) -> &'lex Stream<'lex> {
    self.stream
  }

  #[doc(hidden)]
  fn from_any(any: Any<'lex>) -> Self {
    any.try_into().unwrap()
  }
}

impl<'lex> From<Quoted<'lex>> for Any<'lex> {
  fn from(value: Quoted<'lex>) -> Self {
    Any::Quoted(value)
  }
}

impl<'lex> TryFrom<Any<'lex>> for Quoted<'lex> {
  type Error = WrongKind;
  fn try_from(value: Any<'lex>) -> Result<Self, Self::Error> {
    value.quoted()
  }
}

impl fmt::Debug for Quoted<'_> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    let mut f = f.debug_struct("Quoted");
    f.field("span", &self.span())
      .field("delimiters", &self.delimiters())
      // TODO: get rid of this collect().
      .field("content", &self.raw_content().collect::<Vec<_>>());

    if let Some(prefix) = self.prefix() {
      f.field("prefix", &prefix);
    }

    if let Some(suffix) = self.suffix() {
      f.field("suffix", &suffix);
    }

    f.finish()
  }
}

impl<'lex> Spanned<'lex> for Quoted<'lex> {
  fn span(&self) -> Span<'lex> {
    self.stream.lookup_span_with_affixes(self.id)
  }
}

impl<'lex> Token<'lex> for Never {
  type Rule = Never;

  fn id(self) -> Id {
    self.from_nothing_anything()
  }

  fn stream(self) -> &'lex Stream<'lex> {
    self.from_nothing_anything()
  }

  fn from_any(any: Any<'_>) -> Self {
    any.try_into().unwrap()
  }
}

impl TryFrom<Any<'_>> for Never {
  type Error = WrongKind;

  fn try_from(value: Any<'_>) -> Result<Self, Self::Error> {
    Err(WrongKind { want: "Never", got: value.debug_name() })
  }
}

impl From<Never> for Any<'_> {
  fn from(value: Never) -> Self {
    value.from_nothing_anything()
  }
}

/// Converts a lexeme into a string, for printing as a diagnostic.
impl<'lex> Any<'lex> {
  pub(crate) fn to_yarn(self) -> YarnBox<'lex, str> {
    let spec = self.spec();
    if let Some(name) = spec.rule_name(self.lexeme()) {
      return name.to_box();
    }

    let (pre, suf, kind) = match self {
      Any::Eof(_) => return yarn!("<eof>"),
      Any::Keyword(tok) => return yarn!("`{}`", tok.text()),
      Any::Bracket(d) => {
        return yarn!("`{} ... {}`", d.open().text(), d.close().text());
      }

      Any::Quoted(tok) => {
        let pre = tok.prefix().map(|s| s.text()).unwrap_or("");
        let suf = tok.suffix().map(|s| s.text()).unwrap_or("");
        let open = tok.open().text();
        let close = tok.close().text();
        return yarn!("`{pre}{open}...{close}{suf}`");
      }

      Any::Ident(tok) => (tok.prefix(), tok.suffix(), "identifier"),
      Any::Digital(tok) => (tok.prefix(), tok.suffix(), "number"),
    };

    match (pre.map(|s| s.text()), suf.map(|s| s.text())) {
      (Some(pre), Some(suf)) => {
        yarn!("`{pre}`-prefixed, `{suf}`-suffixed {kind}")
      }
      (Some(pre), None) => yarn!("`{pre}`-prefixed {kind}"),
      (None, Some(suf)) => yarn!("`{suf}`-suffixed {kind}"),
      (None, None) => yarn!("{kind}"),
    }
  }
}
