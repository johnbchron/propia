#[derive(Clone, Debug, thiserror::Error)]
#[error("decoding error at offset {offset}: {message}")]
pub struct DecodeError {
  pub offset: u32,
  pub message: &'static str,
  pub kind: DecodeErrorKind,
}

#[derive(Clone, Debug)]
pub enum DecodeErrorKind {
  Truncated { needed: u32, available: u32 },
  NoEndOfAttributes,
  InvalidExtensionLength { value_len: u32 },
  OffsetOverflow,
}
