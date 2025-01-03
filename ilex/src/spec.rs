//! Lexer specifications.

use std::cmp::Ordering;
use std::fmt;
use std::fmt::Display;
use std::hash::Hash;
use std::marker::PhantomData;

use byteyarn::yarn;
use byteyarn::Yarn;
use byteyarn::YarnBox;
use byteyarn::YarnRef;

use crate::report::Expected;
use crate::rt;
use crate::rule;
use crate::rule::Comment;
use crate::rule::Rule;

/// An ID for a lexeme that a [`Spec`][crate::Spec] can capture.
///
/// Methods on [`SpecBuilder`][crate::SpecBuilder] will return lexemes that can
/// be used to distinguish what rule a [`Token`][crate::token::Token] came from.
#[repr(transparent)]
pub struct Lexeme<Rule> {
  id: i32,
  _ph: PhantomData<Rule>,
}

impl Lexeme<rule::Eof> {
  /// Returns the unique lexeme for the end-of-file marker.
  pub fn eof() -> Self {
    Self::new(i32::MAX)
  }
}

impl<R> Lexeme<R> {
  /// Erases the type of this lexeme.
  pub fn any(self) -> Lexeme<rule::Any> {
    Lexeme::new(self.id)
  }

  /// Returns whether this is the EOF lexeme.
  pub fn is_eof(self) -> bool {
    self == Lexeme::eof()
  }

  /// Returns whether this is an auxiliary token that users should never
  /// actually see.
  pub(crate) fn is_aux(self) -> bool {
    self.id < 0
  }

  /// Returns whether this lexeme can have comments attached to it.
  pub(crate) fn can_have_comments(self, spec: &Spec) -> bool {
    !self.is_aux()
      && (self.is_eof()
        || !matches!(spec.rule(self.any()), rule::Any::Comment(_)))
  }

  /// Converts this lexeme into an index.
  pub(crate) fn index(self) -> usize {
    self.id as usize
  }

  /// Changes the type of this lexeme.
  pub(crate) fn cast<S>(self) -> Lexeme<S> {
    Lexeme::new(self.id)
  }

  /// Creates a new lexeme.
  pub(crate) const fn new(id: i32) -> Self {
    Self { id, _ph: PhantomData }
  }
}

impl<R> fmt::Debug for Lexeme<R> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "#{}", self.id)
  }
}

/// A lexer specification.
///
/// This is a compiled, immutable object that describes how to lex a particular
/// language. The [`Spec::builder()`] function returns a builder for
pub struct Spec {
  builder: SpecBuilder,
  dfa: rt::Dfa,
}

impl Spec {
  /// Returns a new [`SpecBuilder`].
  pub fn builder() -> SpecBuilder {
    SpecBuilder::default()
  }

  /// Gets the rule corresponding a lexeme.
  ///
  /// Behavior given another spec's lexeme is unspecified.
  ///
  /// # Panics
  ///
  /// May panic if the given lexeme came from another spec.
  pub fn rule<R: Rule>(&self, lexeme: Lexeme<R>) -> &R {
    R::try_from_ref(&self.builder.rules[lexeme.index()]).unwrap()
  }

  /// Returns the name of a rule corresponding to a particular lexeme, if it has
  /// one.
  pub(crate) fn rule_name(
    &self,
    lexeme: Lexeme<rule::Any>,
  ) -> Option<YarnRef<str>> {
    Some(self.builder.names[lexeme.index()].as_ref()).filter(|n| !n.is_empty())
  }

  /// Returns the name of a rule corresponding to a particular lexeme, if it has
  /// one.
  pub(crate) fn rule_name_or(
    &self,
    lexeme: Lexeme<rule::Any>,
    or: impl Display,
  ) -> Expected {
    self
      .rule_name(lexeme)
      .map(|y| Expected::Name(y.to_box()))
      .unwrap_or(Expected::Literal(or.to_string().into()))
  }

  /// Returns the underlying DFAs for this spec.
  pub(crate) fn dfa(&self) -> &rt::Dfa {
    &self.dfa
  }
}

/// A builder for constructing a [`Spec`].
#[derive(Default)]
pub struct SpecBuilder {
  pub(crate) rules: Vec<rule::Any>,
  pub(crate) names: Vec<Yarn>,
}

impl SpecBuilder {
  /// Compiles a new [`Spec`] out of this builder.
  ///
  /// The process of building a [`Spec`] includes validation and sorting of
  /// the lexing rules; the resulting object is immutable, so ideally it should
  /// be constructed once and re-used.
  ///
  /// # Panics
  ///
  /// Panics if any of the invariants of a [`Spec`] are violated, or if any rule
  /// combinations are ambiguous (e.g., they have the same prefix).
  pub fn compile(self) -> Spec {
    let dfa = rt::compile(&self.rules);
    Spec { builder: self, dfa }
  }

  /// Adds a new rule to the [`Spec`] being built.
  ///
  /// When parsing the next token, the `ilex` lexer will select the longest
  /// matching token, giving priority to tokens *added first*.
  ///
  /// ```
  /// # use ilex::*;
  /// use ilex::rule;
  /// let mut builder = Spec::builder();
  /// let ident = builder.rule(rule::Ident::new().prefix("%"));
  ///
  /// let str = builder.rule(
  ///   rule::Quoted::new('"')
  ///     .prefix("r")
  ///     .add_rust_escapes()
  /// );
  /// let spec = builder.compile();
  /// ```
  pub fn rule<R: Rule>(&mut self, rule: R) -> Lexeme<R> {
    self.named_rule("", rule)
  }

  /// Adds a new named rule to the [`Spec`] being built.
  ///
  /// This is similar to [`SpecBuilder::rule()`], but diagnostics involving
  /// the returned [`Lexeme`] will use the given name, instead of a generated
  /// one.
  pub fn named_rule<R: Rule>(
    &mut self,
    name: impl Into<Yarn>,
    rule: R,
  ) -> Lexeme<R> {
    if self.rules.len() == (i32::MAX as usize) {
      panic!(
        "ilex: grammars with more than {} lexemes are unsupported",
        i32::MAX
      )
    }

    self.names.push(name.into());
    self.rules.push(rule.into());
    Lexeme::new(self.rules.len() as i32 - 1)
  }

  #[doc(hidden)]
  pub fn __macro_rule<R: Rule>(
    &mut self,
    name: Option<&'static str>,
    rule: impl Into<R>,
  ) -> Lexeme<R> {
    match name {
      Some(name) => self.named_rule(name, rule.into()),
      None => self.rule(rule.into()),
    }
  }
}

impl<R> Clone for Lexeme<R> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<R> Copy for Lexeme<R> {}

impl<R, S> PartialEq<Lexeme<S>> for Lexeme<R> {
  fn eq(&self, other: &Lexeme<S>) -> bool {
    self.id == other.id
  }
}
impl<R> Eq for Lexeme<R> {}
impl<R> PartialOrd for Lexeme<R> {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}
impl<R> Ord for Lexeme<R> {
  fn cmp(&self, other: &Self) -> Ordering {
    self.id.cmp(&other.id)
  }
}

impl<R> Hash for Lexeme<R> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    i32::hash(&self.id, state)
  }
}

/// Converts a lexeme into a string, for printing as a diagnostic.
impl Lexeme<rule::Any> {
  pub(crate) fn to_yarn(self, spec: &Spec) -> YarnBox<str> {
    if self == Lexeme::eof().any() {
      return yarn!("<eof>");
    }

    if let Some(name) = &spec.rule_name(self) {
      return name.to_box();
    }

    match spec.rule(self) {
      rule::Any::Keyword(rule) => yarn!("`{}`", rule.value),
      rule::Any::Bracket(d)
      | rule::Any::Comment(Comment { bracket: d, .. }) => match &d.kind {
        rule::BracketKind::Paired(open, close) => {
          yarn!(
            "`{open} ...{}{}`",
            // This little hack is so line comments get a reasonable generated
            // name. :)
            if close == "\n" { "" } else { " " },
            if close == "\n" { "" } else { close },
          )
        }
        rule::BracketKind::RustLike {
          repeating,
          open: (o1, o2),
          close: (c1, c2),
          ..
        } => yarn!("`{o1}{repeating}{o2} ... {c1}{repeating}{c2}`"),
        rule::BracketKind::CxxLike {
          open: (o1, o2), close: (c1, c2), ..
        } => yarn!("`{o1}<ident>{o2} ... {c1}<ident>{c2}`"),
      },

      rule::Any::Ident(tok) => {
        yarn!("{}identifier", tok.affixes)
      }
      rule::Any::Quoted(tok) => {
        yarn!("{}string", tok.affixes)
      }
      rule::Any::Digital(tok) => {
        yarn!("{}number", tok.affixes)
      }
    }
  }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __spec__ {
  (
    #[spec]
    $(#[$meta:meta])*
    $vis:vis struct $name:ident {$(
      $(#[$($fmeta:tt)*])*
      $fvis:vis $rule:ident: $ty:ty
    ),* $(,)?}
  ) => {
    $(#[$meta])*
    #[derive($crate::derive_hack)]
    $vis struct $name {
      #[doc(hidden)]
      __spec: $crate::Spec,
      $(
        $(#[$($fmeta)*])*
        $fvis $rule: $ty
      ),*
    }

    impl $name {
      /// Returns this spec's value.
      ///
      /// If necessary, compiles it for the first time.
      pub fn get() -> &'static Self {
        static SPEC: std::sync::OnceLock<$name> = std::sync::OnceLock::new();
        SPEC.get_or_init(|| {
          let mut spec = $crate::Spec::builder();
          Self {
            $($rule: $crate::__spec__!(
              @impl "call-meta"
              $(#[$($fmeta)*])*;
              spec, $ty, $rule, name: None, rule: stringify!($rule)
            ),)*
            __spec: spec.compile(),
          }
        })
      }

      /// Returns the underlying compiled spec.
      pub fn spec(&self) -> &$crate::Spec {
        &self.__spec
      }
    }
  };

  (@impl "call-meta"
    #[named($arg:literal)]
    $(#[$($rest:tt)+])*;
    $spec:ident, $ty:ty, $ident:ident, name: $name:expr, rule: $rule:expr
  ) => {
    $crate::__spec__!(@impl "call-meta"
      $(#[$($rest)+])*;
      $spec, $ty, $ident, name: Some($arg), rule: $rule
    )
  };

  (@impl "call-meta"
    #[named]
    $(#[$($rest:tt)+])*;
    $spec:ident, $ty:ty, $ident:ident, name: $name:expr, rule: $rule:expr
  ) => {
    $crate::__spec__!(@impl "call-meta"
      $(#[$($rest)+])*;
      $spec, $ty, $ident, name: Some(stringify!($ident)), rule: $rule
    )
  };

  (@impl "call-meta"
    #[rule($($arg:tt)*)]
    $(#[$($rest:tt)+])*;
    $spec:ident, $ty:ty, $ident:ident, name: $name:expr, rule: $rule:expr
  ) => {
    $crate::__spec__!(@impl "call-meta"
      $(#[$($rest)+])*;
      $spec, $ty, $ident, name: $name, rule: ($($arg)*)
    )
  };

  (@impl "call-meta"
    #[$($meta:tt)+]
    $(#[$($rest:tt)+])*;
    $spec:ident, $ty:ty, $ident:ident, name: $name:expr, rule: $rule:expr
  ) => {
    $crate::__spec__!(@impl "call-meta"
      $(#[$($rest)+])*;
      $spec, $ty, $ident, name: $name, rule: $rule
    )
  };

  (@impl "call-meta"
    ;
    $spec:ident, $ty:ty, $ident:ident, name: $name:expr, rule: $rule:expr
  ) => {
    $spec.__macro_rule($name, $rule)
  };
}
