use std::{error::Error, fmt};

use crate::ValueTag;

/// An error encountered in decoding [`DecodedToken`](crate::DecodedToken)s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
  /// The (absolute) byte offset the error occured at.
  pub offset:  u32,
  /// A human-readable message describing the error.
  pub message: &'static str,
  /// The variety of error encountered.
  pub kind:    DecodeErrorKind,
}

impl Error for DecodeError {}

impl fmt::Display for DecodeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "decoding error at offset {}: {}",
      self.offset, self.message
    )
  }
}

/// The kind of decoding error encountered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeErrorKind {
  /// The document ended mid-token.
  Truncated {
    /// The size of the field being read when the buffer ran out.
    needed:    u32,
    /// The bytes available after the cursor.
    available: u32,
  },
  /// The `end-of-attributes-tag` was never found.
  NoEndOfAttributes,
  /// A value tag indicated an extension type, but the value bytes were not long
  /// enough for the extension type bytes.
  InvalidExtensionLength {
    /// The declared length of the value field.
    value_len: u32,
  },
  /// The total size of buffers passed exceeded 4 GiB before message attribute
  /// data concluded.
  OffsetOverflow,
}

/// An error encountered in encoding [`Token`](crate::Token)s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeError {
  /// The length of the name was too large to fit in a `u16`.
  NameTooLong {
    /// The length of the name.
    length: usize,
  },
  /// The length of the value was too large to fit in a `u16`.
  ValueTooLong {
    /// The length of the value.
    length: usize,
  },
  /// The value tag is invalid.
  InvalidValueTag {
    /// The invalid value tag.
    tag: ValueTag,
  },
}

impl Error for EncodeError {}

impl fmt::Display for EncodeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      EncodeError::NameTooLong { length } => {
        write!(f, "encoding error: attribute name too long: len = {length}")
      }
      EncodeError::ValueTooLong { length } => {
        write!(
          f,
          "encoding error: attribute value too long: len = {length}"
        )
      }
      EncodeError::InvalidValueTag { tag } => {
        write!(f, "the value tag is invalid: {tag:?}")
      }
    }
  }
}
