//! Layout algorithm implementation.
//!
//! The only thing the layout algorithm *actually* has to decide is whether each
//! group breaks or not. The algorithm is as follows.
//!
//! 1. Measure the width of each element recursively. Elements which span
//!    multiple lines are treated as being of infinite width.
//!
//! 2. Mark groups as broken recursively: for each group, if at its current
//!    position, it would overflow the maximum column length, break it, and
//!    recurse into it.

use unicode_width::UnicodeWidthStr;

use crate::Cursor;
use crate::Doc;
use crate::If;
use crate::Measure;
use crate::Options;
use crate::Tag;
use crate::TagInfo;

impl Doc<'_> {
  pub(crate) fn do_layout(&self, opts: &Options) {
    for (t, c) in self.cursor() {
      measure(t, c);
    }

    LayoutState { opts, indent: 0, column: 0 }.do_layout(self.cursor());
  }
}

struct LayoutState<'a> {
  opts: &'a Options,

  /// The column to start the next line at.
  indent: usize,

  /// The next column that we would be writing at.
  column: usize,
}

impl LayoutState<'_> {
  /// Advances state for rendering a tag within a broken group.
  fn do_layout(&mut self, cursor: Cursor) {
    for (tag, cursor) in cursor {
      let cond = tag.cond != Some(If::Flat);

      let mut m = tag.measure.get();
      m.column = self.column;
      match &tag.tag {
        Tag::Text(text) => match text.rfind("\n") {
          Some(nl) => self.column = self.indent + text[nl..].width(),
          None => self.column += m.width.unwrap(),
        },

        Tag::Space => self.column += 1,
        Tag::Break(0) => {}
        Tag::Break(_) => self.column = self.indent,

        Tag::Group(max) => {
          let mut width =
            m.width.filter(|w| self.column + w <= self.opts.max_columns);

          if width.is_some_and(|w| w > *max) {
            width = None;
          }

          if let Some(w) = width {
            // Don't need to do layout here: everything already fits.
            self.column += w;
          } else {
            m.width = None;

            self.do_layout(cursor);
          }
        }

        Tag::Indent(columns) => {
          if cond {
            let prev = self.indent;
            self.indent = self.indent.saturating_add_signed(*columns);
            self.do_layout(cursor);
            self.indent = prev;
          }
        }
      }
      tag.measure.set(m);
    }
  }
}

/// Calculates the width of each element if it was laid out in one line.
fn measure(tag: &TagInfo, cursor: Cursor) {
  let tag_width = match &tag.tag {
    _ if tag.cond == Some(If::Broken) => Some(0),

    Tag::Text(text) => (!text.contains("\n")).then(|| text.width()),
    Tag::Space => Some(1),
    Tag::Break(_) => None,

    _ => Some(0),
  };

  let width = cursor
    .map(|(t, c)| {
      measure(t, c);
      t.measure.get().width
    })
    .fold(tag_width, |a, b| a?.checked_add(b?));

  tag.measure.set(Measure { width, column: 0 });
}
