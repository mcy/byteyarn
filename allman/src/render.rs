use std::io;
use std::io::Write;
use std::mem;

use crate::If;
use crate::Options;
use crate::Tag;

/// An indentation-aware pretty-printer.
pub struct Printer<'a> {
  out: &'a mut dyn io::Write,
  indent: usize,
  space: bool,
  newlines: usize,
}

impl<'a> Printer<'a> {
  /// Returns a new printer with the given output and options.
  pub fn new(out: &'a mut dyn io::Write) -> Self {
    Self {
      out,
      indent: 0,
      space: false,
      newlines: 0,
    }
  }

  /// Updates the indentation level with the given diff.
  pub fn with_indent<R>(
    &mut self,
    diff: isize,
    body: impl FnOnce(&mut Self) -> R,
  ) -> R {
    let prev = self.indent;
    self.indent = self.indent.saturating_add_signed(diff);
    let r = body(self);
    self.indent = prev;
    r
  }

  /// Writes indentation, if necessary.
  pub fn write_indent(&mut self) -> io::Result<()> {
    if mem::take(&mut self.newlines) == 0 {
      return Ok(());
    }

    self.write_spaces(self.indent)
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

  pub fn render(
    &mut self,
    cursor: crate::Cursor,
    _options: &Options,
    parent_is_broken: bool,
  ) -> io::Result<()> {
    for (tag, cursor) in cursor {
      let cond = match tag.cond {
        Some(If::Broken) => parent_is_broken,
        Some(If::Flat) => !parent_is_broken,
        None => true,
      };

      match &tag.tag {
        Tag::Text(text) => {
          if cond {
            write!(self, "{text}")?;
          }
        }

        Tag::Space => self.space |= cond,
        Tag::Break(n) => {
          if cond {
            for _ in self.newlines..*n {
              writeln!(self)?;
            }
          }
        }

        Tag::Group(..) => {
          let m = tag.measure.get();
          self.render(cursor, _options, m.width.is_none())?;
        }

        Tag::Indent(columns) => {
          if cond {
            self.with_indent(*columns, |p| {
              p.render(cursor, _options, parent_is_broken)
            })?;
          }
        }
      }
    }

    Ok(())
  }
}

impl io::Write for Printer<'_> {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }

    if mem::take(&mut self.space) && !buf.starts_with(b"\n") {
      self.write_all(b" ")?;
    }

    for line in buf.split_inclusive(|&b| b == b'\n') {
      if line == b"\n" {
        self.newlines += 1;
        self.out.write_all(line)?;
        continue;
      }

      self.write_indent()?;
      self.out.write_all(line)?;
      if line.ends_with(b"\n") {
        self.newlines = 1;
      }
    }
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    self.out.flush()
  }
}
