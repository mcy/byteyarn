//! Output implementation for YAML.

use std::fmt;

use allman::If;
use allman::Tag;
use byteyarn::YarnRef;

use crate::doc::Doc;
use crate::doc::DocOptions;
use crate::doc::Value;

pub struct Args<'a> {
  pub root: bool,
  pub in_list: bool,
  pub options: &'a DocOptions,
}

pub fn build<'t>(args: Args, doc: &'t Doc<'t>, out: &mut allman::Doc<'t>) {
  let is_array = doc.entries.iter().all(|(k, _)| k.is_none());
  if is_array {
    out.tag_with(Tag::Group(args.options.max_array_width), |out| {
      out.tag_if("[", If::Flat);
      if !args.root {
        out.tag_if(Tag::Break(1), If::Broken);
      }
      for (i, (_, entry)) in doc.entries.iter().enumerate() {
        if i > 0 {
          out.tag_if(",", If::Flat);
          out.tag_if(Tag::Space, If::Flat);
        }

        out.tag_if("-", If::Broken);
        out.tag_if(Tag::Space, If::Broken);
        //out.tag_with(Tag::Indent(args.options.tab_width as isize), |out| {
        value(Args { root: false, in_list: true, ..args }, entry, out);
        //});

        out.tag_if(Tag::Break(1), If::Broken);
      }
      out.tag_if("]", If::Flat);
    });
  } else {
    out.tag_with(Tag::Group(args.options.max_object_width), |out| {
      let in_map = !args.root && !args.in_list;
      if in_map {
        out.tag_if(Tag::Break(1), If::Broken);
      }
      out
        .tag_if("{", If::Flat)
        .tag_with(Tag::Indent(args.options.tab_width as isize), |out| {
          for (i, (key, entry)) in doc.entries.iter().enumerate() {
            if i > 0 {
              out.tag_if(",", If::Flat);
              out.tag_if(Tag::Space, If::Flat);
            }

            let key_bytes = key.as_deref().unwrap_or_default().as_bytes();
            let ident = is_ident(key_bytes);

            if let Some(ident) = ident {
              out.tag(ident.to_box());

              let mut entry = entry;
              while let Value::Doc(d) = entry {
                let [(Some(k), v)] = d.entries.as_slice() else { break };
                let Some(ident) = is_ident(k.as_bytes()) else { break };

                out.tag(".").tag(ident.to_box());
                entry = v;
              }
            } else {
              out.tag(Escape(key_bytes).to_string());
            }
            out.tag(":").tag(Tag::Space);

            value(Args { root: false, in_list: false, ..args }, entry, out);
            out.tag_if(Tag::Break(1), If::Broken);
          }
        })
        .tag_if("}", If::Flat);
    });
  }
}

fn value<'t>(args: Args, v: &'t Value<'t>, out: &mut allman::Doc<'t>) {
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
      if is_raw_string(v.as_ref()) {
        out.tag("|").tag(Tag::Break(1)).tag_with(
          Tag::Indent(args.options.tab_width as isize),
          |out| {
            out.tag(v.as_ref().to_utf8().unwrap().to_box());
          },
        );
        return;
      }
      out.tag(Escape(v).to_string());
    }
    Value::Doc(v) => build(args, v, out),
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
            write!(f, "\\x{b:02x}")?;
          }
          continue;
        }
      };

      for c in chunk.chars() {
        match c {
          '\0' => write!(f, "\\0")?,
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

fn is_raw_string(data: YarnRef<[u8]>) -> bool {
  data.to_utf8().is_ok_and(|s| {
    s.contains("\n") && s.chars().all(|c| c == '\n' || !c.is_control())
  })
}

fn is_ident(data: &[u8]) -> Option<YarnRef<str>> {
  fn is_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '-'
  }
  fn is_continue(c: char) -> bool {
    is_start(c) || c.is_numeric()
  }

  let s = YarnRef::from(data).to_utf8().ok()?;

  let mut chars = s.chars();
  let is_ident = chars.next().is_some_and(is_start) && chars.all(is_continue);
  is_ident.then_some(s)
}
