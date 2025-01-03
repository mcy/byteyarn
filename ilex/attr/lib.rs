//! Implementation detail of `ilex`.

use proc_macro::TokenStream;

// This helper exists only to make the #[spec] field attributes inert.
#[doc(hidden)]
#[proc_macro_derive(derive_hack, attributes(named, rule))]
pub fn derive(_: TokenStream) -> TokenStream {
  TokenStream::new()
}

proc2decl::bridge! {
  /// Generates a lexer spec struct.
  ///
  /// This macro generates the type of struct described in the
  /// [crate documentation][crate]. The syntax is as follows.
  ///
  /// ```ignore
  /// use ilex::rule::Keyword;
  /// use ilex::Lexeme;
  ///
  /// /// My cool spec.
  /// #[ilex::spec]
  /// struct MySpec {
  ///   #[named("...")]
  ///   #[rule(/* ... */)]
  ///   dollar: Lexeme<Keyword> = "$",
  /// }
  /// ```
  ///
  /// The type of each field must be a [`Lexeme`] with a [`Rule`] type as its
  /// parameter. There are two special attributes that can follow.
  ///
  /// - `#[named]` makes the rule into a *named* rule. This name can be used by
  ///   diagnostics, and corresponds to calling `Spec::named_rule()`.
  ///
  /// - `#[rule]` is the value to use to construct the rule, which must be
  ///   `Into<R>`, where `R` is the type inside `Lexeme` (so, above, the rule
  ///   value must be `Into<Keyword>`). By default, this value is the name of the
  ///   rule, to make the common case of declaring a keyword as simple as writing
  ///   `nullptr: Lexeme<Keyword>`, assuming Rust itself doesn't already use that
  ///   keyword.
  ///
  /// Note that *order matters* for the fields: when breaking a tie between two
  /// potential tokens of the same length, the first one in the struct will win.
  /// In practice, this means you should put keywords before identifiers.
  ///
  /// Additionally, the following functions will be defined for the `MySpec` type.
  ///
  /// ```
  /// # struct Spec;
  /// # struct MySpec;
  /// # fn norun(_: i32) {
  /// impl MySpec {
  ///   /// Gets the global instance of this spec.
  ///   pub fn get() -> &'static Self {
  ///     // ...
  /// #   todo!()
  ///   }
  ///
  ///   /// Gets the actual compiled spec.
  ///   pub fn spec(&self) -> &Spec {
  ///     // ...
  /// #   todo!()
  ///   }
  /// }
  /// # }
  /// ```
  ///
  // God cross-trait links suck.
  /// [`Lexeme`]: https://docs.rs/ilex/latest/ilex/struct.Lexeme.html
  /// [`Rule`]: https://docs.rs/ilex/latest/ilex/rule/trait.Rule.html
  /// [crate]: https://docs.rs/ilex
  macro #[spec] => ilex::__spec__;
}

