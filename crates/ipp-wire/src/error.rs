use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
  pub offset:  u32,
  pub message: &'static str,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeErrorKind {
  Truncated { needed: u32, available: u32 },
  NoEndOfAttributes,
  InvalidExtensionLength { value_len: u32 },
  OffsetOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EncodeError {
  NameTooLong { length: usize },
  ValueTooLong { length: usize },
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
