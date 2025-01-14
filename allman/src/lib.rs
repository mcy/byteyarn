//! `allman` 🗒️🖋️ - A code formatting and line reflowing toolkit.
//!
//! [`allman::Doc`][Doc] is a DOM-like structure that specifies how indentation,
//! like breaking, and reflowing should be handled. It is a tree of [`Tag`]s
//! that dictate layout information for the source code to format.
//!
//! For example, the Allman brace style (for which this crate is named) can
//! be implemented as follows:
//! 
//! ```
//! # use allman::*;
//! // flat: fn foo() { ... }
//! //
//! // broken:
//! // fn foo()
//! // {
//! //   // ...
//! // }
//! Doc::new()
//!   .tag("fn")
//!   .tag(Tag::Space)
//!   .tag("foo")
//!   .tag("(").tag(")")
//!   .tag_with(Tag::Group(40), |doc| {
//!     doc
//!       .tag_if(Tag::Space, If::Flat)
//!       .tag_if(Tag::Break(1), If::Broken)
//!       .tag("{")
//!       .tag_if(Tag::Space, If::Flat)
//!       .tag_if(Tag::Break(1), If::Broken)
//!       .tag_with(Tag::Indent(2), |doc| {
//!         // Brace contents here...
//!       })
//!       .tag_if(Tag::Space, If::Flat)
//!       .tag_if(Tag::Break(1), If::Broken)
//!       .tag("}");
//!   });
//! ```
//! 
//! When calling [`Doc::render()`], the layout algorithm will determine whether
//! [`Tag::Group`]s should be "broken", i.e., laid out with newlines inside.

use core::slice;
use std::cell::Cell;
use std::fmt;
use std::io;

use byteyarn::YarnBox;

mod layout;
mod render;

/// A source code document, which can be rendered as formatted text.
///
/// A [`Doc`] is analogous to an HTML DOM, which is text along with markup for
/// laying out that text. The difference being that rather than being converted
/// into raster graphics by a browser engine, a [`Doc`] is rendered as a text
/// file.
#[derive(Clone, Default)]
pub struct Doc<'text> {
  /// This is a flattened tree: each node specifies how many elements after it
  /// make up its children. The `Cursor` type implements walking this tree.
  tags: Vec<TagInfo<'text>>,
}

/// A condition that can be applied to a tag.
///
/// If a condition is set on a tag, and the condition is false, the tag is
/// treated as a no-op: its contents are not printed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum If {
  /// True when the containing group is printed on one line.
  Flat,
  /// True when the containing group does not fit on one line.
  Broken,
}

/// Options for [`Doc::render()`].
pub struct Options {
  /// The maximum number of columns in a line.
  pub max_columns: usize,
}

impl<'text> Doc<'text> {
  /// Returns a new, empty document.
  pub fn new() -> Self {
    Self::default()
  }

  /// Renders this document to the given writer.
  pub fn render(
    &self,
    out: &mut dyn io::Write,
    options: &Options,
  ) -> io::Result<()> {
    self.do_layout(options);
    render::Printer::new(out).render(self.cursor(), options, true)
  }

  /// Inserts a new self-closing tag into this doc.
  pub fn tag(&mut self, tag: impl Into<Tag<'text>>) -> &mut Self {
    self.tag_if_with(tag, None, |_| {})
  }

  /// Inserts a new tag into this doc. The given closure can be used to insert
  /// tags into it.
  ///
  /// # Panics
  ///
  /// Panics if children are inserted and [`Tag::can_have_children()`] is false.
  pub fn tag_with(
    &mut self,
    tag: impl Into<Tag<'text>>,
    body: impl FnOnce(&mut Self),
  ) -> &mut Self {
    self.tag_if_with(tag, None, body)
  }

  /// Inserts a new tag into this doc, with an optional condition.
  pub fn tag_if(
    &mut self,
    tag: impl Into<Tag<'text>>,
    cond: impl Into<Option<If>>,
  ) -> &mut Self {
    self.tag_if_with(tag, cond, |_| {})
  }

  /// Inserts a new tag into this doc, with an optional condition. The given
  /// closure can be used to insert tags into it.
  ///
  /// # Panics
  ///
  /// Panics if children are inserted and [`Tag::can_have_children()`] is false.
  pub fn tag_if_with(
    &mut self,
    tag: impl Into<Tag<'text>>,
    cond: impl Into<Option<If>>,
    body: impl FnOnce(&mut Self),
  ) -> &mut Self {
    let tag = tag.into();
    let compound = tag.can_have_children();

    let consolidate = matches!(
      (&tag, self.tags.last().map(|t| &t.tag)),
      (Tag::Space, Some(Tag::Space))
    );

    let idx = self.tags.len();
    self.tags.push(TagInfo {
      tag,
      len: 0,
      cond: cond.into(),
      measure: Cell::default(),
    });
    body(self);

    let len = self.tags.len() - idx - 1;
    assert!(
      compound || len == 0,
      "inserted children for {:?}",
      &self.tags[idx].tag
    );

    if consolidate {
      self.tags.pop();
    }

    self.tags[idx].len = len;
    self
  }

  fn cursor(&self) -> Cursor {
    Cursor { iter: self.tags.iter() }
  }
}

#[derive(Clone, Debug)]
struct TagInfo<'text> {
  tag: Tag<'text>,
  len: usize,
  cond: Option<If>,

  measure: Cell<Measure>,
}

#[derive(Copy, Clone, Default, Debug)]
struct Measure {
  /// The number of columns this tag takes up when it is formatted on one line.
  ///
  /// None if its width should be treated as infinite.
  width: Option<usize>,
  column: usize,
}

/// An element of a [`Doc`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Tag<'text> {
  /// Verbatim text. Line breaks inside of this text cause any groups that
  /// contain it to be broken.
  Text(YarnBox<'text, str>),

  /// Inserts a space, except if it would end a line. This is intended for
  /// ensuring lines do not have trailing whitespace. [`Tag::Text`] containing
  /// a space can be used to force a space at the end of a line.
  ///
  /// Consecutive space tags are consolidated into one.
  Space,

  /// Inserts the given number of newlines, and breaks the surrounding group.
  ///
  /// Consecutive breaks are consolidated into one. A `Break(0)` can be used
  /// to force a break without inserting an actual newline.
  Break(usize),

  /// A sequence of tags that may either be rendered as one line, or broken into
  /// multiple lines if it does not fit.
  ///
  /// The group will also break itself if it is wider than the given width;
  /// use [`usize::MAX`] to disable this.
  Group(usize),

  /// Change indentation by the given number of columns.
  Indent(isize),
}

impl Tag<'_> {
  /// Returns whether or not this tag can contain child tags.
  pub fn can_have_children(&self) -> bool {
    matches!(self, Self::Group(..) | Self::Indent(..))
  }
}

impl<'text, Y: Into<YarnBox<'text, str>>> From<Y> for Tag<'text> {
  fn from(yarn: Y) -> Self {
    Self::Text(yarn.into())
  }
}

/// A cursor over a piece of a [`Doc`].
struct Cursor<'a> {
  iter: slice::Iter<'a, TagInfo<'a>>,
}

impl<'a> Iterator for Cursor<'a> {
  type Item = (&'a TagInfo<'a>, Cursor<'a>);

  fn next(&mut self) -> Option<Self::Item> {
    let next = self.iter.next()?;
    if next.len == 0 {
      // Fast path that avoids an extra bounds check.
      return Some((next, Cursor { iter: [].iter() }));
    }

    let (contents, rest) = self.iter.as_slice().split_at(next.len);
    self.iter = rest.iter();
    Some((next, Cursor { iter: contents.iter() }))
  }
}

impl fmt::Debug for Doc<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fn fmt(
      indent: usize,
      cursor: Cursor,
      f: &mut fmt::Formatter,
    ) -> fmt::Result {
      for (tag, cursor) in cursor {
        write!(f, "{:<1$}", "\n", indent+1)?;
        match &tag.tag {
          Tag::Text(y) => write!(f, "<text>{y:?}</text>")?,
          Tag::Space => write!(f, "<space/>")?,
          Tag::Break(n) => write!(f, "<break count={n}/>")?,
          Tag::Group(w) => {
            if cursor.iter.as_slice().is_empty() {
              write!(f, "<group width={w}/>")?;
              continue;
            }

            write!(f, "<group width={w}>")?;
            fmt(indent + 2, cursor, f)?;
            write!(f, "</group>")?;
          }
          Tag::Indent(c) => {
            if cursor.iter.as_slice().is_empty() {
              write!(f, "<indent cols={c}/>")?;
              continue;
            }

            write!(f, "<indent cols={c}>")?;
            fmt(indent + 2, cursor, f)?;
            write!(f, "</indent>")?;
          }
        }
      }
      write!(f, "{:<1$}", "\n", indent-2+1)?;
      Ok(())
    }

    fmt(0, self.cursor(), f)
  }
}
