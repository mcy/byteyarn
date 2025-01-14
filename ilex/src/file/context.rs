use std::fs;
use std::sync::Arc;
use std::sync::RwLock;

use camino::Utf8Path;

use crate::f;
use crate::file::File;
use crate::file::CTX_FOR_SPAN_DEBUG;
use crate::report;
use crate::report::Fatal;
use crate::report::Report;

#[cfg(doc)]
use crate::Span;

/// A source context, which owns source code files.
///
/// A `Context` contains the full text of all the loaded source files, which
/// [`Span`]s ultimately refer to.
#[derive(Default)]
pub struct Context {
  state: Arc<RwLock<State>>,
}

#[derive(Default)]
pub struct State {
  // Each file is laid out as the length of the text, followed by the text data,
  // followed by the path.
  //
  // TODO(mcyoung): Be smarter about this and use something something concurrent
  // vector? We don't need to have all this stuff behind a lock I think.
  files: Vec<(usize, String)>,
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Context {
  /// Creates a new source context.
  pub fn new() -> Self {
    Self::default()
  }

  pub(crate) fn copy(&self) -> Context {
    Self { state: self.state.clone() }
  }

  /// Sets this thread to use this [`Context`] in `fmt::Debug`.
  ///
  /// By default, `dbg!(some_span)` produces a string like `"<elided>"`, since
  /// spans do not know what context they came from. This function sets a thread
  /// local that `<SpanId as fmt::Debug>` looks at when printing; this is useful
  /// for when dumping e.g. an AST when debugging.
  ///
  /// Returns an RAII type that undoes the effects of this function when leaving
  /// scope, so that if the caller also called this function, it doesn't get
  /// clobbered.
  #[must_use = "Context::use_for_debugging_spans() returns an RAII object"]
  pub fn use_for_debugging_spans(&self) -> impl Drop {
    struct Replacer(Option<Context>);
    impl Drop for Replacer {
      fn drop(&mut self) {
        CTX_FOR_SPAN_DEBUG.with(|v| *v.borrow_mut() = self.0.take())
      }
    }

    Replacer(CTX_FOR_SPAN_DEBUG.with(|v| v.replace(Some(self.copy()))))
  }

  /// Creates a new [`Report`] based on this context.
  pub fn new_report(&self) -> Report {
    Report::new(self, Default::default())
  }

  /// Creates a new [`Report`] based on this context, with the specified
  /// options.
  pub fn new_report_with(&self, options: report::Options) -> Report {
    Report::new(self, options)
  }

  /// Adds a new file to this source context.
  pub fn new_file<'a>(
    &self,
    path: impl Into<&'a Utf8Path>,
    text: impl Into<String>,
  ) -> File {
    let mut text = text.into();
    text.push(' '); // This space only exists to be somewhere for an EOF span
                    // to point to in diagnostics; user code will never see
                    // it.
    let len = text.len();
    text.push_str(path.into().as_str());

    let idx = {
      let mut state = self.state.write().unwrap();
      state.files.push((len, text));
      state.files.len() - 1
    };

    self.file(idx).unwrap()
  }

  /// Adds a new file to this source context, validating that it is valid
  /// UTF-8.
  pub fn new_file_from_bytes<'a>(
    &self,
    path: impl Into<&'a Utf8Path>,
    text: impl Into<Vec<u8>>,
    report: &Report,
  ) -> Result<File, Fatal> {
    let path = path.into();
    let text = String::from_utf8(text.into()).map_err(|e| {
      let n = e.utf8_error().valid_up_to();
      let b = e.as_bytes()[n];

      report
        .error(f!("input file `{path}` was not valid UTF-8"))
        .note(f!("encountered non-UTF-8 byte {b:#02x} at offset {n}"));
      report.fatal().unwrap()
    })?;

    Ok(self.new_file(path, text))
  }

  /// Adds a new file to this source context by opening `name` and reading it
  /// from the file system.
  pub fn open_file<'a>(
    &self,
    path: impl Into<&'a Utf8Path>,
    report: &Report,
  ) -> Result<File, Fatal> {
    let path = path.into();

    let bytes = match fs::read(path) {
      Ok(bytes) => bytes,
      Err(e) => {
        report.error(f!("could not open input file `{path}`: {e}"));
        return report.fatal();
      }
    };

    self.new_file_from_bytes(path, bytes, report)
  }

  /// Gets the `idx`th file in this source context.
  pub fn file(&self, idx: usize) -> Option<File> {
    let state = self.state.read().unwrap();
    let (len, text) = state.files.get(idx)?;
    let text = unsafe {
      // SAFETY: The pointer to the file's text is immutable and pointer-stable,
      // so we can safely extend its lifetime here.
      &*(text.as_str() as *const str)
    };

    Some(File { len: *len, text, ctx: self, idx })
  }

  /// Gets the number of files currently tracked by this source context.
  pub fn file_count(&self) -> usize {
    self.state.read().unwrap().files.len()
  }
}
