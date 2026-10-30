use std::{error::Error, fmt};

/// An error encountered in decoding [`Token`](crate::Token)s.
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
    /// The bytes needed to yield the token.
    needed:    u32,
    /// The bytes actually available.
    available: u32,
  },
  /// The `end-of-attributes-tag` was never found.
  NoEndOfAttributes,
  /// A value tag indicated an extension type, but the value bytes were not long
  /// enough for the extension type bytes.
  InvalidExtensionLength {
    /// The length of the value bytes.
    value_len: u32,
  },
  /// The message attribute data exceeded 4 GiB.
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
    }
  }
}
