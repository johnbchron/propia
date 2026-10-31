use crate::{Span, Spanned};

/// A single chunk of data within an IPP message, annotated with [`Span`]s.
///
/// This type is meant to be functional rather than to correspond to some
/// specific spec-defined concept. Tokens are meant to be structurally decoded
/// without a complex state machine, so header fields are grouped and not all
/// semantic concepts are represented.
///
/// Every field is annotated with the span representing the location it
/// originated from, so this type is also not meant to be constructed outside of
/// the context of decoding a buffer. If you need to construct arbitrary tokens
/// or encode tokens, use [`Token`].
///
/// Fixed-length fields are copied into the token, and variable-length fields
/// are generic on their representation. When emitted from the
/// [`Decoder`](crate::Decoder), that generic is `&[u8]`, but those slices (and
/// their lifetime) can be stripped out with [`detach`](DecodedTokenRef::detach)
/// or converted to some other type with
/// [`map_generic`](DecodedToken::map_generic). This property allows one, for
/// example, to decode tokens from byte slices as they come in and then strip
/// the references to allow dropping the buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodedToken<B> {
  /// The header token, containing the fields always found at the beginning of
  /// the message.
  Header {
    /// The outer span, containing the whole header.
    span:                Span,
    /// The `version-number`, as described in Section 3.1.1 of the RFC.
    version:             (Spanned<u8>, Spanned<u8>),
    /// The `operation-id` or `status-code`, as described in Section 3.1.1 of
    /// the RFC.
    operation_or_status: Spanned<u16>,
    /// The `request-id`, as described in Section 3.1.1 of the RFC.
    request_id:          Spanned<u32>,
  },
  /// A delimiter tag token, containing just a single byte which delimits some
  /// state change within the message. Described by Section 3.5.1 of the RFC.
  Delimiter {
    /// The tag in question.
    tag: Spanned<DelimiterTag>,
  },
  /// A single value of an attribute, as described by Section 3.1.3 of the RFC.
  ///
  /// Specifically, either an `attribute-with-one-value` field or an
  /// `additional-value` field. If the `name` field of this variant is
  /// `Some(...)`, it's an `attribute-with-one-value`, and if not, it's a
  /// `additional-value`.
  Value {
    /// The outer span, containing the whole attribute value.
    span:  Span,
    /// The value tag delimiting this attribute value, as described by Sections
    /// 3.1.4, 3.1.5, and 3.5.2 of the RFC.
    tag:   Spanned<ValueTag>,
    /// The name field of the attribute value, as described by Section 3.7 of
    /// the RFC.
    name:  Option<Spanned<B>>,
    /// The value field of the attribute value, as described by Section 3.9 of
    /// the RFC.
    value: Spanned<B>,
  },
  /// This token represents the data portion of the IPP message, as described by
  /// Section 3.10 of the RFC. Currently, it's only ever emitted by
  /// [`DecodeIterator`](crate::DecodeIterator), as the
  /// [`Decoder`](crate::Decoder) will emit a [`Step::Body`](crate::Step) to
  /// indicate that the data body has been reached, instead of emitting a `Data`
  /// token.
  Data {
    /// The data portion of the IPP message.
    data: Spanned<B>,
  },
}

impl<B> DecodedToken<B> {
  /// Returns the outer [`Span`] which covers the whole token.
  #[must_use]
  pub fn span(&self) -> Span {
    match self {
      DecodedToken::Header { span, .. } | DecodedToken::Value { span, .. } => {
        *span
      }
      DecodedToken::Delimiter { tag } => tag.span,
      DecodedToken::Data { data } => data.span,
    }
  }

  /// Maps the backing variable-length type to a different type.
  pub fn map_generic<U, F>(self, f: F) -> DecodedToken<U>
  where
    F: Copy + Fn(Spanned<B>) -> Spanned<U>,
  {
    match self {
      DecodedToken::Header {
        span,
        version,
        operation_or_status,
        request_id,
      } => DecodedToken::Header {
        span,
        version,
        operation_or_status,
        request_id,
      },
      DecodedToken::Delimiter { tag } => DecodedToken::Delimiter { tag },
      DecodedToken::Value {
        span,
        tag,
        name,
        value,
      } => DecodedToken::Value {
        span,
        tag,
        name: name.map(f),
        value: f(value),
      },
      DecodedToken::Data { data } => DecodedToken::Data { data: f(data) },
    }
  }

  /// Drops all span information referring to the original buffer and returns a
  /// [`Token`].
  pub fn forget_spans(self) -> Token<B> {
    match self {
      DecodedToken::Header {
        version,
        operation_or_status,
        request_id,
        ..
      } => Token::Header {
        version:             (version.0.value, version.1.value),
        operation_or_status: operation_or_status.value,
        request_id:          request_id.value,
      },
      DecodedToken::Delimiter { tag } => Token::Delimiter { tag: tag.value },
      DecodedToken::Value {
        tag, name, value, ..
      } => Token::Value {
        tag:   tag.value,
        name:  name.map(|n| n.value),
        value: value.value,
      },
      DecodedToken::Data { data } => Token::Data { data: data.value },
    }
  }
}

/// A [`DecodedToken`] backed by a byte slice for variable-length fields.
pub type DecodedTokenRef<'a> = DecodedToken<&'a [u8]>;

/// A [`DecodedToken`] type without any backing data for variable-length fields.
pub type DecodedTokenIdx = DecodedToken<()>;

impl<'a> DecodedTokenRef<'a> {
  /// Converts a [`DecodedTokenRef`] to a [`DecodedTokenIdx`].
  pub fn detach(self) -> DecodedTokenIdx { self.map_generic(|s| s.map(|_| ())) }
}

impl DecodedTokenIdx {
  /// Converts a [`DecodedTokenIdx`] to a [`DecodedTokenRef`], given a byte
  /// buffer, using the contained spans.
  pub fn attach<'a>(self, buf: &'a [u8]) -> DecodedTokenRef<'a> {
    self.map_generic(|s| Spanned {
      value: &buf[s.span.start as usize..s.span.end as usize],
      span:  s.span,
    })
  }
}

/// A single chunk of data within an IPP message, annotated with [`Span`]s.
///
/// This type is meant to be functional rather than to correspond to some
/// specific spec-defined concept. Tokens are meant to be structurally decoded
/// without a complex state machine, so header fields are grouped and not all
/// semantic concepts are represented.
///
/// This is the span-stripped version of [`DecodedToken`], which is suitable for
/// being constructed arbitrarily. Encode to bytes with
/// [`encode_token`](crate::encode_token)
///
/// Fixed-length fields are copied into the token, and variable-length fields
/// are generic on their representation. When emitted from the
/// [`Decoder`](crate::Decoder), that generic is `&[u8]`, but those slices (and
/// their lifetime) can be stripped out with [`detach`](DecodedTokenRef::detach)
/// or converted to some other type with
/// [`map_generic`](DecodedToken::map_generic). This property allows one, for
/// example, to decode tokens from byte slices as they come in and then strip
/// the references to allow dropping the buffers.
pub enum Token<B> {
  /// The header token, containing the fields always found at the beginning of
  /// the message.
  Header {
    /// The `version-number`, as described in Section 3.1.1 of the RFC.
    version:             (u8, u8),
    /// The `operation-id` or `status-code`, as described in Section 3.1.1 of
    /// the RFC.
    operation_or_status: u16,
    /// The `request-id`, as described in Section 3.1.1 of the RFC.
    request_id:          u32,
  },
  /// A delimiter tag token, containing just a single byte which delimits some
  /// state change within the message. Described by Section 3.5.1 of the RFC.
  Delimiter {
    /// The tag in question.
    tag: DelimiterTag,
  },
  /// A single value of an attribute, as described by Section 3.1.3 of the RFC.
  ///
  /// Specifically, either an `attribute-with-one-value` field or an
  /// `additional-value` field. If the `name` field of this variant is
  /// `Some(...)`, it's an `attribute-with-one-value`, and if not, it's a
  /// `additional-value`.
  Value {
    /// The value tag delimiting this attribute value, as described by Sections
    /// 3.1.4, 3.1.5, and 3.5.2 of the RFC.
    tag:   ValueTag,
    /// The name field of the attribute value, as described by Section 3.7 of
    /// the RFC.
    name:  Option<B>,
    /// The value field of the attribute value, as described by Section 3.9 of
    /// the RFC.
    value: B,
  },
  /// This token represents the data portion of the IPP message, as described by
  /// Section 3.10 of the RFC. Currently, it's only ever emitted by
  /// [`DecodeIterator`](crate::DecodeIterator), as the
  /// [`Decoder`](crate::Decoder) will emit a [`Step::Body`](crate::Step) to
  /// indicate that the data body has been reached, instead of emitting a `Data`
  /// token.
  Data {
    /// The data portion of the IPP message.
    data: B,
  },
}

impl<B> Token<B> {
  /// Maps the backing variable-length type to a different type.
  pub fn map_generic<U, F>(self, f: F) -> Token<U>
  where
    F: Copy + Fn(B) -> U,
  {
    match self {
      Token::Header {
        version,
        operation_or_status,
        request_id,
      } => Token::Header {
        version,
        operation_or_status,
        request_id,
      },
      Token::Delimiter { tag } => Token::Delimiter { tag },
      Token::Value { tag, name, value } => Token::Value {
        tag,
        name: name.map(f),
        value: f(value),
      },
      Token::Data { data } => Token::Data { data: f(data) },
    }
  }
}

/// A [`Token`] backed by a byte slice for variable-length fields.
pub type TokenRef<'a> = Token<&'a [u8]>;

/// A [`Token`] type without any backing data for variable-length fields.
pub type TokenIdx = Token<()>;

impl<'a> TokenRef<'a> {
  /// Converts a [`TokenRef`] to a [`TokenIdx`].
  pub fn detach(self) -> TokenIdx { self.map_generic(|_| ()) }
}

/// A delimiter tag. Only in the range `0x00..=0x0F`
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DelimiterTag(pub u8);

#[allow(missing_docs)]
impl DelimiterTag {
  pub const OPERATION_ATTRIBUTES_TAG: Self = Self(0x01);
  pub const JOB_ATTRIBUTES_TAG: Self = Self(0x02);
  pub const END_OF_ATTRIBUTES_TAG: Self = Self(0x03);
  pub const PRINTER_ATTRIBUTES_TAG: Self = Self(0x04);
  pub const UNSUPPORTED_ATTRIBUTES_TAG: Self = Self(0x05);
  // there are others defined in other specs; this is just RFC 8010
}

/// A tag that delimits the start of a [`DecodedToken::Value`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueTag {
  /// An RFC 8010-defined value type, falling in the range `0x10..=0xFF`. Not
  /// all values in that range are valid,
  Standard(u8),
  /// A value type defined by an extension. It's a `0x7F` with the real type in
  /// the first 4 value bytes.
  Extension {
    /// The 4-byte extension type.
    kind: Spanned<u32>,
  },
}

/// `"out-of-band"` values for the `"value-tag"` field, defined by Section
/// 3.5.2 of RFC 8010.
#[allow(missing_docs)]
impl ValueTag {
  pub const UNSUPPORTED: Self = ValueTag::Standard(0x10);
  pub const UNKNOWN: Self = ValueTag::Standard(0x12);
  pub const NO_VALUE: Self = ValueTag::Standard(0x13);
}

/// `integer` values for the `"value-tag"` field, defined by Section 3.5.2 of
/// RFC 8010.
#[allow(missing_docs)]
impl ValueTag {
  // 0x20: Unassigned integer data type
  pub const INTEGER: Self = ValueTag::Standard(0x21);
  pub const BOOLEAN: Self = ValueTag::Standard(0x22);
  pub const ENUM: Self = ValueTag::Standard(0x23);
  // 0x24..0x2f: Unassigned integer data types
}

/// `octetString` values for the `"value-tag"` field, defined by Section 3.5.2
/// of RFC 8010.
#[allow(missing_docs)]
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
#[allow(missing_docs)]
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
