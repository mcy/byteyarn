use std::fmt;
use std::io;
use std::mem;

use byteyarn::YarnRef;
use unicode_width::UnicodeWidthStr;

use crate::doc::DocFormat;
use crate::doc::DocOptions;

/// An indentation-aware pretty-printer.
pub struct Printer<'a> {
  out: &'a mut dyn io::Write,
  options: &'a DocOptions,
  indent: usize,
  at_newline: bool,
}

impl<'a> Printer<'a> {
  /// Returns a new printer with the given output and options.
  pub fn new(out: &'a mut dyn io::Write, options: &'a DocOptions) -> Self {
    Self {
      out,
      options,
      indent: 0,
      at_newline: true,
    }
  }

  /// Updates the indentation level with the given diff.
  pub fn indent(&mut self, diff: isize) {
    self.indent = self.indent.checked_add_signed(diff).unwrap();
  }

  /// Writes indentation, if necessary.
  pub fn write_indent(&mut self) -> io::Result<()> {
    if !mem::take(&mut self.at_newline) {
      return Ok(());
    }
    self.at_newline = false;
    self.write_spaces(self.indent * self.options.tab_width)
  }

  /// Writes len ASCII spaces to the output.
  pub fn write_spaces(&mut self, mut len: usize) -> io::Result<()> {
    const SPACES: &[u8; 32] = b"                                ";

    while len > SPACES.len() {
      self.out.write_all(SPACES)?;
      len -= SPACES.len();
    }
    self.out.write_all(&SPACES[..len])?;
    Ok(())
  }

  pub fn yaml_list_item(&mut self) -> io::Result<()> {
    writeln!(self)?;
    self
      .write_indent((self.indent * self.options.tab_width).saturating_sub(2))?;
    write!(self, "- ")
  }

  pub fn escaped_string(&mut self, data: YarnRef<[u8]>) -> io::Result<()> {
    let yaml = self.options.format == DocFormat::Yaml;

    if yaml {
      if let Some(ident) = is_ident(data) {
        return write!(self, "{ident}");
      }
    }

    write!(self, "\"")?;
    for chunk in data.utf8_chunks() {
      let chunk = match chunk {
        Ok(s) => s,
        Err(e) => {
          for b in e {
            write!(self, "\\x{b:02x}")?;
          }
          continue;
        }
      };
      for c in chunk.chars() {
        match c {
          '\0' if yaml => write!(self, "\\0")?,
          '\n' => write!(self, "\\n")?,
          '\r' => write!(self, "\\r")?,
          '\t' => write!(self, "\\t")?,
          '\\' => write!(self, "\\\\")?,
          '\"' => write!(self, "\\\"")?,
          c if !c.is_control() => write!(self, "{c}")?,
          c if yaml && c.is_ascii() => write!(self, "\\x{:02x}", c as u32)?,
          c => {
            for u in c.encode_utf16(&mut [0, 0]) {
              write!(self, "\\u{u:04x}")?;
            }
          }
        }
      }
    }
    write!(self, "\"")
  }

}

impl io::Write for Printer<'_> {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    for line in buf.split_inclusive(|&b| b == b'\n') {
      self.write_indent()?;
      self.out.write_all(line)?;
      if line.ends_with(b"\n") {
        self.at_newline = true;
      }
    }
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    self.out.flush()
  }
}

/// Returns the number of terminal columns that the printed output of `d` takes
/// up.
pub fn width(d: &dyn fmt::Display) -> usize {
  use fmt::Write;

  struct Counter(usize);
  impl Write for Counter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
      self.0 += s.width();
      Ok(())
    }
  }

  let mut counter = Counter(0);
  let _ = write!(&mut counter, "{}", d);
  counter.0
}