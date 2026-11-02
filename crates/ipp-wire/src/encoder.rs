use crate::{DelimiterTag, EncodeError, Token, ValueTag};

/// Encodes a [`Token`] to a `&mut Vec<u8>`.
///
/// Any token that can be encoded normally will be encoded; errors will only be
/// thrown for structural problems with a token.
#[allow(clippy::missing_errors_doc)]
pub fn encode_token_to_vec<B: AsRef<[u8]>>(
  token: &Token<B>,
  out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
  let encoded_len = encoded_len(token);
  let before_len = out.len();
  out.resize(before_len + encoded_len, 0);
  let after_len = out.len();
  encode_token(token, &mut out[before_len..after_len]).map(|_| ())
}

/// Returns the encoded length of a token. Will not reject an invalid token.
#[must_use]
pub fn encoded_len<B: AsRef<[u8]>>(token: &Token<B>) -> usize {
  match token {
    Token::Header { .. } => 8,
    Token::Delimiter { .. } => 1,
    Token::Value { tag, name, value } => {
      let name_len = name.as_ref().map_or(0, |b| b.as_ref().len());
      let is_extended_type = matches!(tag, ValueTag::Extension { .. });
      let extended_type_len = if is_extended_type { 4 } else { 0 };
      1 + 2 + name_len + 2 + extended_type_len + value.as_ref().len()
    }
    Token::Data { data } => data.as_ref().len(),
  }
}

#[allow(clippy::mut_mut)]
fn put(out: &mut &mut [u8], bytes: &[u8]) {
  out
    .split_off_mut(..bytes.len())
    .expect("buffer too small")
    .copy_from_slice(bytes);
}

/// Encodes a [`Token`] as bytes into a mutable slice.
///
/// The slice must be large enough for the token being encoded; call
/// [`encoded_len`] to get the encoded length. Any token that can be encoded
/// normally will be encoded; errors will only be thrown for structural problems
/// with a token.
#[allow(clippy::missing_errors_doc)]
pub fn encode_token<B: AsRef<[u8]>>(
  token: &Token<B>,
  mut out: &mut [u8],
) -> Result<usize, EncodeError> {
  let (needed, available) = (encoded_len(token), out.len());
  if needed > available {
    return Err(EncodeError::BufferTooSmall { needed, available });
  }

  match token {
    Token::Header {
      version,
      operation_or_status,
      request_id,
      ..
    } => {
      encode_header(*version, *operation_or_status, *request_id, out);
    }
    Token::Delimiter { tag } => {
      encode_delimiter(*tag, out);
    }
    Token::Value {
      tag, name, value, ..
    } => {
      encode_value(*tag, name.as_ref(), value, out)?;
    }
    Token::Data { data } => {
      put(&mut out, data.as_ref());
    }
  }

  Ok(needed)
}

fn encode_header(
  version: (u8, u8),
  operation_or_status: u16,
  request_id: u32,
  mut out: &mut [u8],
) {
  let op = operation_or_status.to_be_bytes();
  let r_id = request_id.to_be_bytes();

  let yielded_slice = [
    version.0, version.1, op[0], op[1], r_id[0], r_id[1], r_id[2], r_id[3],
  ];

  put(&mut out, &yielded_slice);
}

fn encode_delimiter(tag: DelimiterTag, mut out: &mut [u8]) {
  put(&mut out, &[tag.0]);
}

fn encode_value<B: AsRef<[u8]>>(
  tag: ValueTag,
  name: Option<&B>,
  value: &B,
  mut out: &mut [u8],
) -> Result<(), EncodeError> {
  // this should be a ValueTag::Extension, but we don't have the extension type
  if let ValueTag::Standard(0x7F) = tag {
    return Err(EncodeError::InvalidValueTag { tag });
  }

  // calculate name length and value length, exiting early if needed
  let name_len = name.map_or(0, |s| s.as_ref().len());
  let name_len = u16::try_from(name_len)
    .map_err(|_| EncodeError::NameTooLong { length: name_len })?
    .to_be_bytes();

  let value_bytes = value.as_ref();
  let mut value_len = value_bytes.len();
  if matches!(tag, ValueTag::Extension { .. }) {
    value_len += 4;
  }
  let value_len = u16::try_from(value_len)
    .map_err(|_| EncodeError::ValueTooLong { length: value_len })?
    .to_be_bytes();

  let tag_byte = match tag {
    ValueTag::Standard(b) => b,
    ValueTag::Extension { .. } => ValueTag::EXTENSION_TYPE_MAGIC_BYTE,
  };

  // tag and name len
  let initial = [tag_byte, name_len[0], name_len[1]];
  put(&mut out, &initial);

  // name
  let name_slice = name.map_or_default(AsRef::as_ref);
  put(&mut out, name_slice);

  // value len
  put(&mut out, &value_len);

  // extension type bytes if needed
  if let ValueTag::Extension { kind } = tag {
    let extension_type_bytes = kind.to_be_bytes();
    put(&mut out, &extension_type_bytes);
  }

  // value
  put(&mut out, value_bytes);

  Ok(())
}
