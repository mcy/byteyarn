//! Diagnostics and error reports.
//!
//! This module contains types for generating an *error report*: a collection of
//! diagnostics that describe why an operation failed in detail. Diagnostics
//! are basically fancy compiler errors: they use [`Span`]s to present faulty
//! input in context.
//!
//! The [`Report`] type is a reference-counted list of diagnostics, which is
//! typically passed by reference into functions, but can be copied to simplify
//! lifetimes, since it's reference-counted.

use std::fmt;
use std::io;
use std::panic;
use std::panic::Location;
use std::process;
use std::sync::Arc;

use crate::file::Context;
use crate::spec::Spec;

mod builtin;
mod diagnostic;
mod render;

pub use builtin::Builtins;
pub use builtin::Expected;
pub use diagnostic::Diagnostic;
use diagnostic::Kind;

#[cfg(doc)]
use crate::Span;

/// A collection of errors can may built up over the course of an operation.
///
/// To construct a report, see [`Context::new_report()`]. The context that
/// constructs a report is the only one whose [`Span`]s should be passed into
/// it; doing otherwise will result in unspecified output (or probably a panic).
pub struct Report {
  ctx: Context,
  state: Arc<render::State>,
}

/// Options for a [`Report`].
pub struct Options {
  /// Whether to color the output when rendered.
  pub color: bool,
  /// Whether to add a note to each diagnostic showing where in the source
  /// code it was reported. `ilex` makes a best-case effort to ensure this
  /// location is in *your* code.
  pub show_report_locations: bool,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      color: true,
      show_report_locations: cfg!(debug_assertions),
    }
  }
}

impl Report {
  pub(crate) fn copy(&self) -> Report {
    Self {
      ctx: self.ctx.copy(),
      state: self.state.clone(),
    }
  }

  /// Returns a wrapper for accessing commonly-used, built-in message types.
  ///
  /// See [`Builtins`].
  pub fn builtins<'a>(&'a self, spec: &'a Spec) -> Builtins<'a> {
    Builtins { report: self, spec }
  }

  /// Adds a new error to this report.
  ///
  /// The returned [`Diagnostic`] object can be used to add spans, notes, and
  /// remarks, to generate a richer diagnostic.
  #[track_caller]
  pub fn error(&self, message: impl fmt::Display) -> Diagnostic {
    self.new_diagnostic(Kind::Error, message.to_string())
  }

  /// Adds a new warning to this report.
  ///
  /// The returned [`Diagnostic`] object can be used to add spans, notes, and
  /// remarks, to generate a richer diagnostic.
  #[track_caller]
  pub fn warn(&self, message: impl fmt::Display) -> Diagnostic {
    self.new_diagnostic(Kind::Warning, message.to_string())
  }

  /// Adds a new top-level note to this report.
  ///
  /// The returned [`Diagnostic`] object can be used to add spans, notes, and
  /// remarks, to generate a richer diagnostic.
  #[track_caller]
  pub fn note(&self, message: impl fmt::Display) -> Diagnostic {
    self.new_diagnostic(Kind::Note, message.to_string())
  }

  #[track_caller]
  fn new_diagnostic(&self, kind: Kind, message: String) -> Diagnostic {
    Diagnostic::new(self.copy(), kind, message).reported_at(Location::caller())
  }

  /// Returns a [`Fatal`] regardless of whether this report contains any errors.
  pub fn fatal<T>(&self) -> Result<T, Fatal> {
    Err(Fatal(self.copy()))
  }

  /// If this report contains any errors, returns [`Err(Fatal)`][Fatal];
  /// otherwise, it returns `Ok(ok)`.
  ///
  /// This is a useful function for completing some operation that could have
  /// generated error diagnostics.
  ///
  /// See [`Fatal`].
  pub fn fatal_or<T>(&self, ok: T) -> Result<T, Fatal> {
    if !self.state.has_error() {
      return Ok(ok);
    }

    self.fatal()
  }

  /// Collates all of the "unsorted diagnostics" into the "sorted diagnostics",
  /// sorting them by thread id.
  ///
  /// This ensures that all diagnostics coming from a particular thread are
  /// together.
  pub fn collate(&self) {
    self.state.collate()
  }

  /// Writes out the contents of this diagnostic to `sink`.
  pub fn write_out(&self, sink: impl io::Write) -> io::Result<()> {
    render::finish(self, sink)
  }
  
  pub(crate) fn new(ctx: &Context, opts: Options) -> Self {
    Self {
      ctx: ctx.copy(),
      state: Arc::new(render::State::new(opts)),
    }
  }
}

/// An error type for making returning a [`Result`] that will trigger
/// diagnostics printing when unwrapped.
///
/// This is useful for functions that are [`Result`]-ey, like reading a file,
/// but which want to generate diagnostics, too.
pub struct Fatal(Report);

impl Fatal {
  /// Prints all diagnostics to stderr and terminates the program.
  pub fn terminate(self) -> ! {
    eprintln!("{self}");
    process::exit(1);
  }

  /// Panics with the [`Report`]'s diagnostics as the panic message.
  pub fn panic(self) -> ! {
    panic::panic_any(self.to_string())
  }
}

impl fmt::Debug for Fatal {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    render::render_fmt(&self.0, &self.0.state.opts, f)
  }
}

impl fmt::Display for Fatal {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(self, f)
  }
}
