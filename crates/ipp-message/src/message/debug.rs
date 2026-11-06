use std::fmt;

use ipp_wire::{DelimiterTag, ValueTag};

use super::{AttrRef, GroupRef, Message, ValueRef};

/// The max depth to display while debug printing.
const MAX_DEPTH: usize = 16;

/// The max for bytes to display when debug printing a field.
const MAX_BYTES: usize = 64;

impl fmt::Debug for Message {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    paint(f, HEADER, |f| match self.header() {
      Some(h) => write!(
        f,
        "Message {}.{} op/status {:#06x} id {}",
        h.version.0, h.version.1, h.operation_or_status, h.request_id
      ),
      None => write!(f, "Message <no header>"),
    })?;

    let diagnostics = self.diagnostics();

    if !f.alternate() {
      let groups = self.groups().len();
      let problems = diagnostics.len();
      return write!(f, " ({groups} groups, {problems} diagnostics)");
    }

    for group in self.groups() {
      newline(f, 1)?;
      write_group(f, group, 1)?;
    }

    if !diagnostics.is_empty() {
      newline(f, 1)?;
      paint(f, PROBLEM, |f| f.write_str("diagnostics"))?;

      for diagnostic in diagnostics {
        newline(f, 2)?;
        f.write_str("token ")?;
        paint(f, NUMBER, |f| write!(f, "{}", diagnostic.token))?;
        f.write_str(": ")?;
        paint(f, PROBLEM, |f| write!(f, "{}", diagnostic.kind))?;
      }
    }

    Ok(())
  }
}

impl fmt::Debug for AttrRef<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if f.alternate() {
      return write_attr(f, *self, 0);
    }

    write_name(f, self.name())?;
    write!(f, " ({} values)", self.values().len())
  }
}

impl fmt::Debug for GroupRef<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if f.alternate() {
      return write_group(f, *self, 0);
    }

    write_group_tag(f, self.tag())?;
    write!(f, " ({} attrs)", self.attrs().len())
  }
}

fn write_group(
  f: &mut fmt::Formatter<'_>,
  group: GroupRef<'_>,
  depth: usize,
) -> fmt::Result {
  write_group_tag(f, group.tag())?;

  for attr in group.attrs() {
    newline(f, depth + 1)?;
    write_attr(f, attr, depth + 1)?;
  }

  Ok(())
}

fn write_group_tag(
  f: &mut fmt::Formatter<'_>,
  tag: Option<DelimiterTag>,
) -> fmt::Result {
  paint(f, GROUP, |f| match tag {
    Some(DelimiterTag::OPERATION_ATTRIBUTES_TAG) => {
      f.write_str("operation-attributes")
    }
    Some(DelimiterTag::JOB_ATTRIBUTES_TAG) => f.write_str("job-attributes"),
    Some(DelimiterTag::PRINTER_ATTRIBUTES_TAG) => {
      f.write_str("printer-attributes")
    }
    Some(DelimiterTag::UNSUPPORTED_ATTRIBUTES_TAG) => {
      f.write_str("unsupported-attributes")
    }
    Some(DelimiterTag(byte)) => write!(f, "group {byte:#04x}"),
    None => f.write_str("<implicit group>"),
  })
}

fn write_attr(
  f: &mut fmt::Formatter<'_>,
  attr: AttrRef<'_>,
  depth: usize,
) -> fmt::Result {
  write_name(f, attr.name())?;

  let mut values = attr.values();

  match values.len() {
    0 => paint(f, NOTE, |f| f.write_str(" <no values>")),
    1 => {
      f.write_str(": ")?;
      values.try_for_each(|value| write_value(f, value, depth))
    }
    _ => values.try_for_each(|value| {
      newline(f, depth + 1)?;
      write_value(f, value, depth + 1)
    }),
  }
}

fn write_value(
  f: &mut fmt::Formatter<'_>,
  value: ValueRef<'_>,
  depth: usize,
) -> fmt::Result {
  write_scalar(f, value)?;

  let members = value.members();

  if depth >= MAX_DEPTH && members.len() > 0 {
    return paint(f, NOTE, |f| {
      write!(f, " <{} members hidden>", members.len())
    });
  }

  for member in members {
    newline(f, depth + 1)?;
    write_attr(f, member, depth + 1)?;
  }

  Ok(())
}

fn write_scalar(
  f: &mut fmt::Formatter<'_>,
  value: ValueRef<'_>,
) -> fmt::Result {
  let tag = value.tag();
  let bytes = value.bytes();

  paint(f, TAG, |f| match tag {
    ValueTag::Extension { kind } => write!(f, "extension {kind:#010x}"),
    ValueTag::Standard(byte) => match value_tag_name(tag) {
      Some(name) => f.write_str(&name),
      None => write!(f, "{byte:#04x}"),
    },
  })?;

  match (tag, bytes) {
    (ValueTag::BEG_COLLECTION, _) => Ok(()),
    (ValueTag::INTEGER | ValueTag::ENUM, &[a, b, c, d]) => {
      f.write_str(" ")?;
      paint(f, NUMBER, |f| {
        write!(f, "{}", i32::from_be_bytes([a, b, c, d]))
      })
    }
    (ValueTag::BOOLEAN, &[0]) => {
      f.write_str(" ")?;
      paint(f, NUMBER, |f| f.write_str("false"))
    }
    (ValueTag::BOOLEAN, &[1]) => {
      f.write_str(" ")?;
      paint(f, NUMBER, |f| f.write_str("true"))
    }
    _ if value_tag_is_text(tag) => {
      f.write_str(" ")?;
      paint(f, TEXT, |f| {
        f.write_str("\"")?;
        write_escaped(f, bytes)?;
        f.write_str("\"")
      })
    }
    (_, []) => Ok(()),
    _ => {
      f.write_str(" ")?;
      paint(f, RAW, |f| {
        f.write_str("0x")?;
        write_hex(f, bytes)
      })
    }
  }
}

fn value_tag_is_text(tag: ValueTag) -> bool {
  matches!(tag, ValueTag::Standard(0x40..=0x5f))
}

fn value_tag_name(tag: ValueTag) -> Option<String> {
  Some(match tag {
    ValueTag::UNSUPPORTED => "unsupported".into(),
    ValueTag::UNKNOWN => "unknown".into(),
    ValueTag::NO_VALUE => "no-value".into(),
    ValueTag::INTEGER => "integer".into(),
    ValueTag::BOOLEAN => "boolean".into(),
    ValueTag::ENUM => "enum".into(),
    ValueTag::OCTETSTRING_WITH_AN_UNSPECIFIED_FORMAT => "octetString".into(),
    ValueTag::DATETIME => "dateTime".into(),
    ValueTag::RESOLUTION => "resolution".into(),
    ValueTag::RANGE_OF_INTEGER => "rangeOfInteger".into(),
    ValueTag::BEG_COLLECTION => "collection".into(),
    ValueTag::TEXT_WITH_LANGUAGE => "textWithLanguage".into(),
    ValueTag::NAME_WITH_LANGUAGE => "nameWithLanguage".into(),
    ValueTag::END_COLLECTION => "endCollection".into(),
    ValueTag::TEXT_WITHOUT_LANGUAGE => "text".into(),
    ValueTag::NAME_WITHOUT_LANGUAGE => "name".into(),
    ValueTag::KEYWORD => "keyword".into(),
    ValueTag::URI => "uri".into(),
    ValueTag::URI_SCHEME => "uriScheme".into(),
    ValueTag::CHARSET => "charset".into(),
    ValueTag::NATURAL_LANGUAGE => "naturalLanguage".into(),
    ValueTag::MIME_MEDIA_TYPE => "mimeMediaType".into(),
    ValueTag::MEMBER_ATTR_NAME => "memberAttrName".into(),
    ValueTag::Standard(byte) => format!("unknown-value-type ({byte:#04x})"),
    _ => return None,
  })
}

fn newline(f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
  f.write_str("\n")?;
  (0..depth).try_for_each(|_| f.write_str("  "))
}

fn write_name(f: &mut fmt::Formatter<'_>, name: &[u8]) -> fmt::Result {
  if name.is_empty() {
    return paint(f, NAME, |f| f.write_str("<unnamed>"));
  }

  paint(f, NAME, |f| write_escaped(f, name))
}

fn write_escaped(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
  let (shown, hidden) = bytes.split_at(bytes.len().min(MAX_BYTES));
  write!(f, "{}", shown.escape_ascii())?;
  write_hidden(f, hidden)
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
  let (shown, hidden) = bytes.split_at(bytes.len().min(MAX_BYTES));
  shown.iter().try_for_each(|byte| write!(f, "{byte:02x}"))?;
  write_hidden(f, hidden)
}

fn write_hidden(f: &mut fmt::Formatter<'_>, hidden: &[u8]) -> fmt::Result {
  if hidden.is_empty() {
    return Ok(());
  }

  write!(f, "…+{}", hidden.len())
}

const HEADER: &str = "1";
const GROUP: &str = "1;36";
const NAME: &str = "1";
const TAG: &str = "2";
const TEXT: &str = "32";
const NUMBER: &str = "33";
const RAW: &str = "35";
const NOTE: &str = "2";
const PROBLEM: &str = "1;31";

fn paint(
  f: &mut fmt::Formatter<'_>,
  style: &str,
  body: impl FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
) -> fmt::Result {
  if !f.sign_plus() {
    return body(f);
  }

  write!(f, "\x1b[{style}m")?;
  body(f)?;
  f.write_str("\x1b[0m")
}
