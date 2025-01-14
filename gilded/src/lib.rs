//! `gilded` - Easy-peesy golden testing. 👑
//!
//! # Why Golden Testing?
//!
//! A "golden test" is a test that transforms data in some way, and validates it
//! by diffing it against an expected result: the "golden".
//!
//! This is especially useful for testing scenarios that consume an input file
//! (say, a source code file, for testing a compiler) and generate structured,
//! diffable textual output (such as JSON or CSV data, or even a `Debug`).
//!
//! Golden tests are best for cases where the output must be deterministic, and
//! where capturing fine-grained detail is valuable.
//!
//! Because they simply compare the result to an expected value byte-for-byte,
//! changes can quickly regenerate the test output by using the output of the
//! test itself. Diffs can be examined in code review directly.
//!
//! # Defining a Test
//!
//! A `gilded` test is defined like so:
//!
//! ```
//! #[gilded::test("testdata/**/*.txt")]
//! fn my_test(test: &gilded::Test) {
//!   // ...
//! }
//! ```
//!
//! `my_test` will be run as a separate unit test for every file (relative to
//! the crate root) which matches the glob passed to the attribute. The input
//! file's path and contents can be accessed through the [`Test`] accessors.
//!
//! To specify a test output, use [`Test::output()`]. This specifies the
//! file extension for the golden, and its computed contents. The extension is
//! used to construct the path of the result. If the input is `foo/bar.txt`, and
//! the extension for this output is `csv`, the output will be read/written to
//! `foo/bar.csv`.
//!
//! Panicking within the test body will fail the test as normal, tests should
//! not contain output assertions; those are handled by the framework.
//!
//! # Generating Goldens
//!
//! Once the test is created, simply set the `GILDED_REGENERATE` environment
//! variable: `GILDED_REGENERATE=1 cargo test`.
//!
//! To regenerate a specific test, simply pass its name as a filter to the test.
//! See `cargo test -- --help` for available flags.`

use std::env;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::str;

use camino::Utf8Path;

pub use gilded_attr::test;

mod doc;
pub use doc::*;

/// The environment variable that is checked to decide whether or not to
/// regenerate goldens.
pub const REGENERATE: &str = "GILDED_REGENERATE";

/// A golden test suite, corresponding to a single invocation of the
/// [`#[gilded::test]`][test] macro.
pub struct Suite {
  name: &'static str,
  crate_root: &'static Path,
  test_root: &'static Utf8Path,
  run: fn(&mut Test),
}

impl Suite {
  /// Returns the name of this test suite (i.e., the name of the function that
  /// `#[gilded::test]` was applied to).
  pub fn name(&self) -> &str {
    self.name
  }

  /// Constructs a new test suite.
  #[doc(hidden)]
  pub fn new(
    name: &'static str,
    crate_root: &'static str,
    run: fn(&mut Test),
    paths: &[&'static str],
  ) -> Suite {
    let crate_root = Path::new(crate_root);

    let Some(mut common_prefix) = paths.first().copied() else {
      return Suite {
        name,
        crate_root,
        run,
        test_root: Utf8Path::new(""),
      };
    };

    common_prefix = Utf8Path::new(common_prefix)
      .parent()
      .map(Utf8Path::as_str)
      .unwrap_or("");

    let sep = std::path::MAIN_SEPARATOR;
    for path in &paths[1..] {
      let common = common_prefix.split_inclusive(sep);
      let chunks = path.split_inclusive(sep);

      let len = common
        .zip(chunks)
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len())
        .sum();

      common_prefix = &common_prefix[..len];
    }

    common_prefix = common_prefix.trim_end_matches(sep);
    Suite {
      name,
      crate_root,
      run,
      test_root: Utf8Path::new(common_prefix),
    }
  }

  /// Executes a test in this test suite with the given data. Panics to signal
  /// test failure.
  ///
  /// This is the function called in the body of a generated test function.
  #[doc(hidden)]
  #[track_caller]
  pub fn run(&'static self, path: &'static str, text: &'static [u8]) {
    let root = self.crate_root.join(self.test_root);
    let path = Utf8Path::new(path);
    let file = self.crate_root.join(path);
    let lock = root.join("GILDED_CHANGED");
    let lock_name = self.test_root.join("GILDED_CHANGED");

    // TODO: make sure this is normalized to being a Unix path on Windows.
    let name = path.strip_prefix(self.test_root).unwrap();

    let mut test = Test {
      suite: self,
      path: name,
      text,
      outputs: Vec::new(),
    };
    (self.run)(&mut test);

    let regen = env::var_os(REGENERATE).is_some();
    assert!(
      regen || !lock.exists(),
      "golden files have changed: verify changes and then delete {lock_name}",
    );
    if regen {
      eprintln!("{}", lock.display());
      File::create(lock).unwrap();
    }

    let mut failed = false;
    for (extn, text) in &test.outputs {
      let file = file.with_extension(extn);
      let name = name.with_extension(extn);

      if regen {
        if text.is_empty() {
          if file.exists() {
            fs::remove_file(file).unwrap();
          }
        } else {
          fs::write(file, text).unwrap();
        }

        continue;
      }

      let mut want = String::new();
      if file.exists() {
        want = fs::read_to_string(file).unwrap()
      }

      if text == &*want {
        continue;
      }

      let fmt = diffy::PatchFormatter::new().with_color();
      let patch = diffy::create_patch(text, &want);
      let patch = fmt.fmt_patch(&patch);
      eprintln!("mismatch for {name}:\n{patch}\n");
      failed = true;
    }

    assert!(!failed, "golden output did not match test output");
    assert!(
      !regen,
      "golden files have changed: verify changes and then delete {lock_name}",
    )
  }
}

/// A handle for a single golden test case.
pub struct Test<'t> {
  suite: &'t Suite,

  path: &'t Utf8Path,
  text: &'t [u8],

  outputs: Vec<(String, String)>,
}

impl<'t> Test<'t> {
  /// Returns the test suite this test case belongs to.
  pub fn suite(&self) -> &'t Suite {
    self.suite
  }

  /// Returns a path for the test input.
  ///
  /// This path will be unique among test outputs, and will be the same
  /// regardless of platform. However, it need not correspond to the actual
  /// path used to read and write the test data.
  pub fn path(&self) -> &'t Utf8Path {
    self.path
  }

  /// Returns the textual content of the test input.
  pub fn text(&self) -> &'t [u8] {
    self.text
  }

  /// Outputs a result for this test.
  ///
  /// A test may have many results, each of which has the same path as the input
  /// with an extra extension. For example, for a `foo.txt` input, the output
  /// might be `foo.txt.stderr`, in which case `extension` would be `stderr`.
  pub fn output(&mut self, extension: &str, result: String) {
    self.outputs.push((extension.into(), result));
  }
}

/// Implementation macro for `#[gilded::test]`.
#[doc(hidden)]
#[macro_export]
macro_rules! __test__ {
  (
    #[test($($_:tt)*)]
    $(#[$attr:meta])*
    fn $name:ident($($args:tt)*) { $($body:tt)* }
    $($tt:tt)*
  ) => {
    #[cfg(test)]
    mod $name {
      use super::*;
      pub static __SUITE__: ::std::sync::LazyLock<$crate::Suite> =
        ::std::sync::LazyLock::new(|| $crate::Suite::new(
          stringify!($name),
          env!("CARGO_MANIFEST_DIR"),
          |$($args)*| -> () { $($body)* },
          &$crate::__test__!(@paths[] $($tt)*),
        ));

      $crate::__test__! { @tests $(#[$attr])* $($tt)* }
    }
  };

  (
    @tests
    $(#[$attr:meta])*
    $mod:ident { $(inner:tt)* }
    $($outer:tt)*
  ) => {
    mod $mod {
      use super::__SUITE__;
      $crate::__test__! { @tests $(#[$attr])* $(inner)* }
    }
    $crate::__test__! { @tests $(#[$attr])* $(outer)* }
  };

  (
    @tests
    $(#[$attr:meta])*
    $test:ident($path:expr, $text:expr)
    $($tt:tt)*
  ) => {
    $(#[$attr])*
    #[::std::prelude::rust_2021::test]
    fn $test() { __SUITE__.run($path, $text) }
    $crate::__test__! { @tests $(#[$attr])* $($tt)* }
  };

  (@tests $(#[$attr:meta])*) => {};

  (
    @paths[$($e:expr,)*]
    $mod:ident { $(inner:tt)* }
    $($outer:tt)*
  ) => {
    $crate::__test__!(@paths[$($e,)*] $(inner)* $(outer)*)
  };

  (
    @paths[$($e:expr,)*]
    $test:ident($path:expr, $text:expr)
    $($tt:tt)*
  ) => {
    $crate::__test__!(@paths[$($e,)* $path,] $($tt)*)
  };

  (@paths $e:expr) => { $e };
}
