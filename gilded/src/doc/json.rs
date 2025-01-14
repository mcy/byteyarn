//! Output implementation for JSON.

use std::fmt;

use byteyarn::YarnRef;
use allman::If;
use allman::Tag;

use crate::doc::Doc;
use crate::doc::DocOptions;
use crate::doc::Value;

pub fn build<'t>(options: &DocOptions, doc: &Doc<'t>, out: &mut allman::Doc<'t>) {
  let is_array = doc.entries.iter().all(|(k, _)| k.is_none());
  if is_array {
    out.tag_with(Tag::Group(options.max_array_width), |out| {
      out
        .tag("[")
        .tag_with(Tag::Indent(options.tab_width as isize), |out| {
          for (i, (_, entry)) in doc.entries.iter().enumerate() {
            if i > 0 {
              out.tag(",");
              out.tag_if(Tag::Space, If::Flat);
            }
            out.tag_if("\n", If::Broken);
            value(options, entry, out);
          }
        })
        .tag_if("\n", If::Broken)
        .tag("]");
    });
  } else {
    out.tag_with(Tag::Group(options.max_object_width), |out| {
      out
        .tag("{")
        .tag_with(Tag::Indent(options.tab_width as isize), |out| {
          for (i, (key, entry)) in doc.entries.iter().enumerate() {
            if i > 0 {
              out.tag(",");
              out.tag_if(Tag::Space, If::Flat);
            }
            out
              .tag_if("\n", If::Broken)
              .tag(
                Escape(key.as_deref().unwrap_or_default().as_bytes())
                  .to_string(),
              )
              .tag(":")
              .tag(Tag::Space);
            value(options, entry, out);
          }
        })
        .tag_if("\n", If::Broken)
        .tag("}");
    });
  }
}

fn value<'t>(options: &DocOptions, v: &Value<'t>, out: &mut allman::Doc<'t>) {
  match v {
    Value::Bool(v) => {
      out.tag(v.to_string());
    }
    Value::Int(v) => {
      out.tag(v.to_string());
    }
    Value::UInt(v) => {
      out.tag(v.to_string());
    }
    Value::Fp(v) => {
      out.tag(v.to_string());
    }
    Value::String(v) => {
      out.tag(Escape(v).to_string());
    }
    Value::Doc(v) => build(options, v, out),
  }
}

/// A displayable that prints the given data as a JSON string.
pub struct Escape<'a>(&'a [u8]);

impl fmt::Display for Escape<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "\"")?;
    for chunk in YarnRef::new(self.0).utf8_chunks() {
      let chunk = match chunk {
        Ok(s) => s,
        Err(e) => {
          for b in e {
            write!(f, "<{b:02x}>")?;
          }
          continue;
        }
      };

      for c in chunk.chars() {
        match c {
          '\n' => write!(f, "\\n")?,
          '\r' => write!(f, "\\r")?,
          '\t' => write!(f, "\\t")?,
          '\\' => write!(f, "\\\\")?,
          '\"' => write!(f, "\\\"")?,
          c if !c.is_control() => write!(f, "{c}")?,
          c => {
            for u in c.encode_utf16(&mut [0, 0]) {
              write!(f, "\\u{u:04x}")?;
            }
          }
        }
      }
    }

    write!(f, "\"")
  }
}
