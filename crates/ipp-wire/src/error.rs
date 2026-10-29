#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
  pub offset: u32,
  pub message: &'static str,
  pub kind: DecodeErrorKind,
}

impl std::error::Error for DecodeError {}

impl std::fmt::Display for DecodeError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

#[derive(Debug)]
pub enum EncodeError {
  NameTooLong { length: usize },
  ValueTooLong { length: usize },
}
