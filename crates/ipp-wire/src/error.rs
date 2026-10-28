use std::str::Utf8Error;

#[derive(Clone, Debug, thiserror::Error)]
#[error("decoding error at offset {offset}: {message}")]
pub struct DecodeError {
  pub offset: usize,
  pub message: &'static str,
  pub kind: DecodeErrorKind,
}

#[derive(Clone, Debug)]
pub enum DecodeErrorKind {
  Truncated { needed: usize, available: usize },
  InvalidName { name_error: Utf8Error },
  NoEndOfAttributes,
  InvalidExtensionLength { value_len: usize },
  OffsetOverflow,
}
