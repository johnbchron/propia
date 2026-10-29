use crate::{
  DecodeError, DecodeErrorKind, DelimiterTag, Span, Spanned, TokenRef,
  ValueTag, cursor::Cursor,
};

use super::{Stall, invalid, need};

pub(super) fn decode_header<'a>(
  cur: &mut Cursor<'a>,
) -> Result<TokenRef<'a>, Stall> {
  // open the span for the header
  let mark = cur.mark();

  // read the version
  let major_version = cur
    .u8()
    .ok_or_else(|| need(cur, 1, "truncated before header major version"))?;
  let minor_version = cur
    .u8()
    .ok_or_else(|| need(cur, 1, "truncated before header minor version"))?;

  // an operation ID for a request and a status code for a response
  let operation_or_status = cur
    .u16()
    .ok_or_else(|| need(cur, 2, "truncated before header operation/status"))?;

  // correlating ID
  let request_id = cur
    .u32()
    .ok_or_else(|| need(cur, 4, "truncated before header request-id"))?;

  // end the header span
  let span = cur.since_mark(mark);

  let header = TokenRef::Header {
    span,
    version: (major_version, minor_version),
    operation_or_status,
    request_id,
  };

  Ok(header)
}

pub(super) fn decode_attribute_or_delimiter<'a>(
  cur: &mut Cursor<'a>,
) -> Result<TokenRef<'a>, Stall> {
  // pull the first byte to see what it is
  let tag = cur.u8().ok_or_else(|| Stall::Incomplete {
    min_len: 1,
    at_eof: DecodeError {
      offset: cur.offset(),
      message: "message ended before end-of-attributes tag",
      kind: DecodeErrorKind::NoEndOfAttributes,
    },
  })?;

  match tag.value {
    // delimiter tag
    0x00..=0x0F => Ok(TokenRef::Delimiter {
      tag: tag.map(DelimiterTag),
    }),
    // value tag: starts an attribute value
    _ => decode_attribute(cur, tag),
  }
}

pub(super) fn decode_attribute<'a>(
  cur: &mut Cursor<'a>,
  tag: Spanned<u8>,
) -> Result<TokenRef<'a>, Stall> {
  // open the span for the attribute
  let span_start = tag.span.start;

  // consume the name
  let name_len = cur
    .u16()
    .ok_or_else(|| need(cur, 2, "truncated before attribute name length"))?
    .value;
  let (name_bytes, name_span) =
    cur.take(u32::from(name_len)).ok_or_else(|| {
      need(cur, u32::from(name_len), "truncated before attribute name")
    })?;
  let name = if name_bytes.is_empty() {
    None
  } else {
    Some(Spanned {
      value: name_bytes,
      span: name_span,
    })
  };

  // get the length of the value
  let mut value_len = cur
    .u16()
    .ok_or_else(|| need(cur, 2, "truncated before attribute value length"))?
    .value;

  // if it's an extension value type, extract the value type first
  let tag = match tag.value {
    ValueTag::EXTENSION_TYPE_MAGIC_BYTE => {
      // value type is first 4 bytes of value
      value_len = value_len.checked_sub(4).ok_or_else(|| {
        invalid(
          cur,
          "attribute value ends within extension type bytes",
          DecodeErrorKind::InvalidExtensionLength {
            value_len: u32::from(value_len),
          },
        )
      })?;
      let kind = cur.u32().ok_or_else(|| {
        need(cur, 4, "truncated before value tag extension type")
      })?;
      tag.map(|_| ValueTag::Extension { kind })
    }
    _ => tag.map(ValueTag::Standard),
  };

  // extract the value itself
  let (value_bytes, value_span) =
    cur.take(u32::from(value_len)).ok_or_else(|| {
      need(
        cur,
        u32::from(value_len),
        "truncated before attribute value",
      )
    })?;
  let value = Spanned {
    value: value_bytes,
    span: value_span,
  };

  // end the attribute span
  let span = Span::new(span_start, cur.offset());

  Ok(TokenRef::Value {
    span,
    tag,
    name,
    value,
  })
}
