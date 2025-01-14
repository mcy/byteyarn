//! `proc2decl` exists for one reason only: because proc macros are a toxic
//! ecosystem.
//!
//! Sometimes, you want to use an attribute to define a macro. Unfortunately,
//! Rust does not support declarative macros (also called macros-by-example)
//! for attributes, for reasons that essentially boil down to cookie-licking.
//!
//! This crate exists for one purpose only, and that is ot facilitate writing
//! declarative macros that an attribute converts into.
//!
//! # How Uo Use
//!
//! 1. Define the macro-by-example you wish to use as the main implementation of
//!    your attribute or derive.
//!
//! 2. Crate a proc-macro crate. This is where the documentation for your
//!    attribute will need to live. Your actual crate should depend on this
//!    crate.
//!
//! 3. Use [`bridge!()`] to define your bridge proc macros. These
//!    macro calls should be documented, since their doc comments are the ones
//!    that will appear in rustdoc for your macros.
//!
//! 4. `pub use` the macros in your actual crate.
//!
//! Proc macros suck!

pub extern crate proc_macro;

use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::Component;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use nu_glob::Pattern;
use proc_macro::Delimiter;
use proc_macro::Group;
use proc_macro::Ident;
use proc_macro::Literal;
use proc_macro::Punct;
use proc_macro::Spacing;
use proc_macro::Span;
use proc_macro::TokenStream;
use proc_macro::TokenTree;
use walkdir::WalkDir;

/// Defines a new attribute or derive proc macro that forwards to the given
/// function-like macro.
///
/// # Attribute Macros
///
/// The tokens passed to `$macro!()` will be `#[$name(...)]` containing the
/// arguments of the attribute, followed by the item passed to the macro by
/// rustc. Like all other attribute macros, it will replace the annotated
/// item with the result of evaluating the macro, in this case a call to the
/// actual macro-by-example that implements it.
///
/// ```ignore
/// macro_rules! __impl {
///   (#[my_macro] const $name:ident: $ty:ty = $expr:expr;) => {/* ... */}
/// }
///
/// proc2decl::bridge! {
///   // My cool macro.
///   macro #[my_macro] => my_crate::__impl;
/// }
/// ```
///
/// # Derive Macros
///
/// The tokens passed to `$macro!()` will be the item passed to the macro by
/// rustc. Like all other derive macros, it will insert the result of evaluating
/// the macro immediately after the annotated item, in this case a call to the
/// actual macro-by-example that implements it.
///
/// The `$attrs` are the names of inert helper attributes to define for
/// the derive.
///
/// ```ignore
/// macro_rules! __impl {
///   (struct $name:ident {}) => {/* ... */}
/// }
///
/// proc2decl::bridge! {
///   // My cool macro.
///   macro #[derive(MyMacro)], #[helper] => my_crate::__impl;
/// }
/// ```
#[macro_export]
macro_rules! bridge {
  (
    $(#[$attr:meta])*
    macro #[$name:ident] => $crate_:ident::$macro:ident;
  ) => {
    $(#[$attr])*
    #[proc_macro_attribute]
    pub fn $name(
      attr: $crate::proc_macro::TokenStream,
      item: $crate::proc_macro::TokenStream,
    ) -> $crate::proc_macro::TokenStream {
      use $crate::proc_macro::*;
      let span = Span::call_site();

      $crate::attr_bridge(
        stringify!($name),
        stringify!($crate_),
        stringify!($macro),
        span,
        attr,
        item,
      )
    }
  };

  (
    $(#[$attr:meta])*
    macro #[derive($name:ident)] $(, #[$attrs:ident])* => $crate_:ident::$macro:ident
  ) => {
    $(#[$attr])*
    #[proc_macro_derive($name, attributes($($attrs,)*))]
    pub fn $name(
      item: $crate::proc_macro::TokenStream,
    ) -> $crate::proc_macro::TokenStream {
      use $crate::proc_macro::*;
      let span = Span::call_site();

      $crate::derive_bridge(
        stringify!($name),
        stringify!($crate_),
        stringify!($macro),
        span,
        item,
      )
    }
  };
}

/// Defines a new attribute proc macro that finds files matching a glob and
/// forwards the directory structure to the given function-like macro, in such
/// a way that a corresponding module structure can be defined using the
/// directory structure.
///
/// The resulting attribute should be called as #[my_attr("glob", ...)], where
/// `glob` is a glob relative to the root of the crate the attribute appears in.
/// The glob will not match files across symlinks.
///
/// The expanded-to macro will be called with the annotated item, followed by
/// token trees in the following form:
///
/// ```ignore
/// foo {
///   bar {
///     baz("foo/bar/baz.txt", b"contents")
///     empty("foo/bar/empty.txt", b"contents")
///   }
///   bar2 {
///     boing("foo/bar2/boing.txt", b"contents")
///   }
/// }
/// ```
///
/// Any directories whose names contain identifiers that are not valid Rust
/// identifiers will be ignored.
#[macro_export]
macro_rules! fs_bridge {
  (
    $(#[$attr:meta])*
    macro #[$name:ident] => $crate_:ident::$macro:ident;
  ) => {
    $(#[$attr])*
    #[proc_macro_attribute]
    pub fn $name(
      attr: $crate::proc_macro::TokenStream,
      item: $crate::proc_macro::TokenStream,
    ) -> $crate::proc_macro::TokenStream {
      use $crate::proc_macro::*;
      let span = Span::call_site();

      $crate::dir_bridge(
        stringify!($name),
        stringify!($crate_),
        stringify!($macro),
        span,
        attr,
        item,
      )
    }
  };
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[doc(hidden)]
pub fn derive_bridge(
  _name: &str,
  crate_: &str,
  macro_: &str,
  span: Span,
  item: TokenStream,
) -> TokenStream {
  let extern_ =
    format!("__extern{}_{}__", COUNTER.fetch_add(1, Ordering::Relaxed), crate_);

  stream([
    // extern crate $crate as __extern_$crate__;
    Ident::new("extern", span).into(),
    Ident::new("crate", span).into(),
    Ident::new(crate_, span).into(),
    Ident::new("as", span).into(),
    Ident::new(&extern_, span).into(),
    Punct::new(';', Spacing::Alone).into(),
    // __extern_$crate__::$macro! { attr item }
    Ident::new(&extern_, span).into(),
    Punct::new(':', Spacing::Joint).into(),
    Punct::new(':', Spacing::Alone).into(),
    Ident::new(macro_, span).into(),
    Punct::new('!', Spacing::Alone).into(),
    Group::new(Delimiter::Brace, item).into(),
  ])
}

#[doc(hidden)]
pub fn attr_bridge(
  name: &str,
  crate_: &str,
  macro_: &str,
  span: Span,
  args: TokenStream,
  mut item: TokenStream,
) -> TokenStream {
  if !args.is_empty() {
    item = stream2(
      [
        // #[name(args)]
        Punct::new('#', Spacing::Alone).into(),
        Group::new(
          Delimiter::Bracket,
          stream([
            Ident::new(name, span).into(),
            Group::new(Delimiter::Parenthesis, args).into(),
          ]),
        )
        .into(),
      ],
      item,
    );
  } else {
    item = stream2(
      [
        // #[name]
        Punct::new('#', Spacing::Alone).into(),
        Group::new(Delimiter::Bracket, stream([Ident::new(name, span).into()]))
          .into(),
      ],
      item,
    );
  }

  derive_bridge(name, crate_, macro_, span, item)
}

#[doc(hidden)]
pub fn dir_bridge(
  name: &str,
  crate_: &str,
  macro_: &str,
  span: Span,
  args: TokenStream,
  item: TokenStream,
) -> TokenStream {
  let Some(TokenTree::Literal(lit)) = args.clone().into_iter().next() else {
    panic!("#[{crate_}::{name}] requires a glob as its first argument");
  };

  // TODO(mcyoung): support all Rust string literals.
  let lit = lit.to_string();
  if !lit.starts_with('"') || !lit.starts_with('"') || lit.contains('\\') {
    panic!("#[{crate_}::{name}] only supports single-quoted string literals without escapes");
  }
  let glob = match Pattern::new(&lit[1..lit.len() - 1]) {
    Ok(p) => p,
    Err(e) => {
      panic!("#[{crate_}::{name}] requires a glob as its first argument: {e}")
    }
  };

  struct File {
    path: String,
    components: Vec<usize>,
    contents: Vec<u8>,
  }

  let mut names = Vec::new();
  let mut table = HashMap::new();
  let mut push_name = |name: &OsStr| -> Option<usize> {
    let utf8 = name.to_str()?;
    if !is_valid_ident(utf8) {
      return None;
    }

    Some(*table.entry(utf8.to_string()).or_insert_with_key(|k| {
      let n = names.len();
      names.push(k.clone());
      n
    }))
  };

  let mut files = Vec::new();
  let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
  'walk: for entry in WalkDir::new(&root) {
    let entry = match entry {
      Ok(p) => p,
      Err(e) => panic!("directory walk failed: {e}"),
    };

    let path = entry.path();
    if path.is_dir() {
      continue 'walk;
    }

    let rel = path.strip_prefix(&root).unwrap();
    if !glob.matches_path(rel) {
      continue 'walk;
    }

    let mut components = Vec::new();
    if let Some(parent) = rel.parent() {
      for component in parent.components() {
        let Component::Normal(component) = component else {
          continue 'walk;
        };
        let Some(name) = push_name(component) else {
          continue 'walk;
        };
        components.push(name);
      }
    }

    let Some(name) = push_name(path.file_stem().unwrap()) else {
      continue 'walk;
    };
    components.push(name);

    let Some(utf8) = path.as_os_str().to_str() else {
      continue 'walk;
    };

    let contents = match fs::read(path) {
      Ok(bytes) => bytes,
      Err(e) => panic!("could not open file: {e}"),
    };

    files.push(File {
      path: utf8.to_string(),
      components,
      contents,
    });
  }
  files.sort_by(|a, b| Ord::cmp(&a.components, &b.components));

  let mut mod_stack: Vec<Vec<TokenTree>> = vec![item.into_iter().collect()];
  let mut dir_stack = &[][..];
  for file in &files {
    let dir = &file.components[..file.components.len() - 1];
    let [_, remove, add] = common_prefix(dir_stack, dir);
    for &i in remove {
      let items = mod_stack.pop().unwrap();
      mod_stack.last_mut().unwrap().extend_from_slice(&[
        Ident::new(&names[i], span).into(),
        Group::new(Delimiter::Brace, items.into_iter().collect()).into(),
      ]);
    }
    for _ in add {
      mod_stack.push(Vec::new());
    }
    dir_stack = dir;

    let name = &names[*file.components.last().unwrap()];
    mod_stack.last_mut().unwrap().extend_from_slice(&[
      Ident::new(name, span).into(),
      Group::new(
        Delimiter::Parenthesis,
        stream([
          Literal::string(&file.path).into(),
          Punct::new(',', Spacing::Alone).into(),
          Literal::byte_string(&file.contents).into(),
        ]),
      )
      .into(),
    ]);
  }

  attr_bridge(
    name,
    crate_,
    macro_,
    span,
    args,
    mod_stack.swap_remove(0).into_iter().collect(),
  )
}

fn common_prefix<'a, T: PartialEq>(a: &'a [T], b: &'a [T]) -> [&'a [T]; 3] {
  for (i, (x, y)) in a.iter().zip(b).enumerate() {
    if x != y {
      return [&a[..i], &a[i..], &b[i..]];
    }
  }
  [a, &[], &[]]
}

fn is_valid_ident(name: &str) -> bool {
  use unicode_xid::UnicodeXID as _;
  // See https://doc.rust-lang.org/reference/identifiers.html
  name.chars().enumerate().all(|(i, c)| {
    if i == 0 {
      c == '_' || c.is_xid_start()
    } else {
      c.is_xid_continue()
    }
  })
}

fn stream<const N: usize>(tt: [TokenTree; N]) -> TokenStream {
  tt.into_iter().collect()
}

fn stream2<const N: usize>(tt: [TokenTree; N], ts: TokenStream) -> TokenStream {
  tt.into_iter().chain(ts).collect()
}
