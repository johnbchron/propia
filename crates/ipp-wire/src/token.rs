use crate::{Span, Spanned};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token<B> {
  Header {
    span:                Span,
    version:             (Spanned<u8>, Spanned<u8>),
    operation_or_status: Spanned<u16>,
    request_id:          Spanned<u32>,
  },
  Delimiter {
    tag: Spanned<DelimiterTag>,
  },
  Value {
    span:  Span,
    tag:   Spanned<ValueTag>,
    name:  Option<Spanned<B>>,
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

  pub fn map_generic<U, F>(self, f: F) -> Token<U>
  where
    F: Copy + Fn(Spanned<B>) -> Spanned<U>,
  {
    match self {
      Token::Header {
        span,
        version,
        operation_or_status,
        request_id,
      } => Token::Header {
        span,
        version,
        operation_or_status,
        request_id,
      },
      Token::Delimiter { tag } => Token::Delimiter { tag },
      Token::Value {
        span,
        tag,
        name,
        value,
      } => Token::Value {
        span,
        tag,
        name: name.map(f),
        value: f(value),
      },
      Token::Data { data } => Token::Data { data: f(data) },
    }
  }
}

pub type TokenRef<'a> = Token<&'a [u8]>;

pub type TokenIdx = Token<()>;

pub type TokenOwned = Token<Vec<u8>>;

impl<'a> TokenRef<'a> {
  pub fn detach(self) -> TokenIdx { self.map_generic(|s| s.map(|_| ())) }

  pub fn owned(self) -> TokenOwned {
    self.map_generic(|span| span.map(|s| s.to_vec()))
  }
}

impl TokenIdx {
  pub fn attach<'a>(self, buf: &'a [u8]) -> TokenRef<'a> {
    self.map_generic(|s| Spanned {
      value: &buf[s.span.start as usize..s.span.end as usize],
      span:  s.span,
    })
  }
}

/// Only in the range `0x00..=0x0F`
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DelimiterTag(pub u8);

impl DelimiterTag {
  pub const OPERATION_ATTRIBUTES_TAG: Self = Self(0x01);
  pub const JOB_ATTRIBUTES_TAG: Self = Self(0x02);
  pub const END_OF_ATTRIBUTES_TAG: Self = Self(0x03);
  pub const PRINTER_ATTRIBUTES_TAG: Self = Self(0x04);
  pub const UNSUPPORTED_ATTRIBUTES_TAG: Self = Self(0x05);
  // there are others defined in other specs; this is just RFC 8010
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueTag {
  /// An RFC 8010-defined value type, falling in the range `0x10..=0xFF`. Not
  /// all values in that range are valid,
  Standard(u8),
  /// A value type defined by an extension. It's a `0x7F` with the real type in
  /// the first 4 value bytes.
  Extension { kind: Spanned<u32> },
}

/// `"out-of-band"` values for the `"value-tag"` field, defined by Section
/// 3.5.2 of RFC 8010.
impl ValueTag {
  pub const UNSUPPORTED: Self = ValueTag::Standard(0x10);
  pub const UNKNOWN: Self = ValueTag::Standard(0x12);
  pub const NO_VALUE: Self = ValueTag::Standard(0x13);
}

/// `integer` values for the `"value-tag"` field, defined by Section 3.5.2 of
/// RFC 8010.
impl ValueTag {
  // 0x20: Unassigned integer data type
  pub const INTEGER: Self = ValueTag::Standard(0x21);
  pub const BOOLEAN: Self = ValueTag::Standard(0x22);
  pub const ENUM: Self = ValueTag::Standard(0x23);
  // 0x24..0x2f: Unassigned integer data types
}

/// `octetString` values for the `"value-tag"` field, defined by Section 3.5.2
/// of RFC 8010.
impl ValueTag {
  pub const OCTETSTRING_WITH_AN_UNSPECIFIED_FORMAT: Self =
    ValueTag::Standard(0x30);
  pub const DATETIME: Self = ValueTag::Standard(0x31);
  pub const RESOLUTION: Self = ValueTag::Standard(0x32);
  pub const RANGE_OF_INTEGER: Self = ValueTag::Standard(0x33);
  pub const BEG_COLLECTION: Self = ValueTag::Standard(0x34);
  pub const TEXT_WITH_LANGUAGE: Self = ValueTag::Standard(0x35);
  pub const NAME_WITH_LANGUAGE: Self = ValueTag::Standard(0x36);
  pub const END_COLLECTION: Self = ValueTag::Standard(0x37);
  // 0x38..0x3f: Unassigned octetString data types
}

/// `String` values for the `"value-tag"` field, defined by Section 3.5.2 of RFC
/// 8010.
impl ValueTag {
  // 0x40: Unassigned character-string data type
  pub const TEXT_WITHOUT_LANGUAGE: Self = ValueTag::Standard(0x41);
  pub const NAME_WITHOUT_LANGUAGE: Self = ValueTag::Standard(0x42);
  // 0x43: Unassigned character-string data type
  pub const KEYWORD: Self = ValueTag::Standard(0x44);
  pub const URI: Self = ValueTag::Standard(0x45);
  pub const URI_SCHEME: Self = ValueTag::Standard(0x46);
  pub const CHARSET: Self = ValueTag::Standard(0x47);
  pub const NATURAL_LANGUAGE: Self = ValueTag::Standard(0x48);
  pub const MIME_MEDIA_TYPE: Self = ValueTag::Standard(0x49);
  pub const MEMBER_ATTR_NAME: Self = ValueTag::Standard(0x4a);
  // 0x4b..0x5f: Unassigned character-string data types
}

impl ValueTag {
  /// Signifies that the first 4 bytes of the value field are to be interpreted
  /// as the type, as extended in specifications following RFC 8010
  pub const EXTENSION_TYPE_MAGIC_BYTE: u8 = 0x7f;
}
