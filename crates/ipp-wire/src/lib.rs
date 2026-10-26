mod cursor;
mod token_iter;

pub use self::token_iter::TokenIterator;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
  pub start: u32,
  pub len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
  Header {
    span: Span,
  },
  Delimiter {
    tag: DelimiterTag,
    tag_span: Span,
  },
  Value {
    tag: ValueTag,
    tag_span: Span,
    name: Span,
    value: Span,
  },
  Data {
    span: Span,
  },
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
  Extension { kind: u32, kind_span: Span },
}
