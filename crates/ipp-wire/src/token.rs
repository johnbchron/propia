use crate::{Span, Spanned};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<B> {
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
    name: Option<Spanned<B>>,
    value: Spanned<B>,
  },
  Data {
    data: Spanned<B>,
  },
}

impl<B> Token<B> {
  #[must_use]
  pub fn span(&self) -> Span {
    match self {
      Token::Header { span, .. } | Token::Value { span, .. } => *span,
      Token::Delimiter { tag } => tag.span,
      Token::Data { data } => data.span,
    }
  }
}

pub type TokenRef<'a> = Token<&'a [u8]>;

pub type TokenIdx = Token<()>;

impl<'a> TokenRef<'a> {
  pub fn detach(self) -> TokenIdx {
    match self {
      TokenRef::Header {
        span,
        version,
        operation_or_status,
        request_id,
      } => TokenIdx::Header {
        span,
        version,
        operation_or_status,
        request_id,
      },
      TokenRef::Delimiter { tag } => TokenIdx::Delimiter { tag },
      TokenRef::Value {
        span,
        tag,
        name,
        value,
      } => TokenIdx::Value {
        span,
        tag,
        name: name.map(|s| s.map(|_| ())),
        value: value.map(|_| ()),
      },
      TokenRef::Data { data } => TokenIdx::Data {
        data: data.map(|_| ()),
      },
    }
  }
}

impl TokenIdx {
  pub fn attach<'a>(self, buf: &'a [u8]) -> TokenRef<'a> {
    match self {
      TokenIdx::Header {
        span,
        version,
        operation_or_status,
        request_id,
      } => TokenRef::Header {
        span,
        version,
        operation_or_status,
        request_id,
      },
      TokenIdx::Delimiter { tag } => TokenRef::Delimiter { tag },
      TokenIdx::Value {
        span,
        tag,
        name,
        value,
      } => TokenRef::Value {
        span,
        tag,
        name: name.map(|s| Spanned {
          value: &buf[s.span.start as usize..s.span.end as usize],
          span: s.span,
        }),
        value: Spanned {
          value: &buf[value.span.start as usize..value.span.end as usize],
          span: value.span,
        },
      },
      TokenIdx::Data { data } => TokenRef::Data {
        data: Spanned {
          value: &buf[data.span.start as usize..data.span.end as usize],
          span: data.span,
        },
      },
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
