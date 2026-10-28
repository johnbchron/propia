use crate::{Span, Spanned};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<'a> {
  Header {
    span: Span,
    version: (Spanned<u8>, Spanned<u8>),
    operation_or_status: Spanned<u16>,
    request_id: Spanned<u32>,
  },
  Delimiter {
    tag: Spanned<DelimiterTag>,
  },
  Value {
    span: Span,
    tag: Spanned<ValueTag>,
    name: Option<Spanned<&'a [u8]>>,
    value: Spanned<&'a [u8]>,
  },
  Data {
    data: Spanned<&'a [u8]>,
  },
}

impl Token<'_> {
  pub fn span(&self) -> Span {
    match self {
      Token::Header { span, .. } => *span,
      Token::Delimiter { tag } => tag.span,
      Token::Value { span, .. } => *span,
      Token::Data { data } => data.span,
    }
  }
}

/// Only in the range `0x00..=0x0F`
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DelimiterTag(pub u8);

impl DelimiterTag {
  pub const OPERATION: Self = Self(0x01);
  pub const JOB: Self = Self(0x02);
  pub const END: Self = Self(0x03);
  pub const PRINTER: Self = Self(0x04);
  pub const UNSUPPORTED: Self = Self(0x05);
  // there are others that are out of scope; this is just RFC 8010
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueTag {
  /// An RFC 8010-defined value type, falling in the range `0x10..=0xFF`
  Known(u8),
  /// A value type defined by an extension. It's a `0x7F` with the real type in the first 4 value bytes.
  Extension { kind: Spanned<u32> },
}
