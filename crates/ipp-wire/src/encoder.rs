use crate::{
  DecodedToken, DecodedTokenIdx, DelimiterTag, EncodeError, Span, Spanned,
  Token, ValueTag,
};

/// Encodes a [`Token`] to a `&mut Vec<u8>`.
///
/// Any token that can be encoded normally will be encoded; errors will only be
/// thrown for structural problems with a token.
///
/// # Errors
/// If an error is thrown while encoding, partial writes are cleaned up.
pub fn encode_token_to_vec<B: AsRef<[u8]>>(
  token: &Token<B>,
  out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
  let encoded_len = encoded_len(token);
  let before_len = out.len();
  out.resize(before_len + encoded_len, 0);

  let after_len = out.len();
  let out_slice = &mut out[before_len..after_len];

  match encode_token(token, out_slice) {
    Ok(_n) => Ok(()),
    Err(e) => {
      out.truncate(before_len);
      Err(e)
    }
  }
}

/// Returns how many bytes it would take to encode a token.
#[must_use]
pub fn encoded_len<B: AsRef<[u8]>>(token: &Token<B>) -> usize {
  match token {
    Token::Header { .. } => 8,
    Token::Delimiter { .. } => 1,
    Token::Value { tag, name, value } => {
      let name_len = name.as_ref().len();
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
///
/// # Errors
/// If the buffer is too small for the token to be encoded,
/// [`EncodeError::BufferTooSmall`] will be returned and nothing will be
/// written. For any other error, a partial token may be written; partial write
/// cleanup is the responsibility of the caller.
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
      encode_delimiter(*tag, out)?;
    }
    Token::Value {
      tag, name, value, ..
    } => {
      encode_value(*tag, name, value, out)?;
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

fn encode_delimiter(
  tag: DelimiterTag,
  mut out: &mut [u8],
) -> Result<(), EncodeError> {
  // anything 0x10 or higher will be read back as a value tag
  if let 0x10.. = tag.0 {
    return Err(EncodeError::InvalidDelimiterTag { tag });
  }

  put(&mut out, &[tag.0]);

  Ok(())
}

fn encode_value<B: AsRef<[u8]>>(
  tag: ValueTag,
  name: &B,
  value: &B,
  mut out: &mut [u8],
) -> Result<(), EncodeError> {
  match tag {
    // this would be read back as a delimiter tag
    ValueTag::Standard(0x00..=0x0F) => {
      return Err(EncodeError::InvalidValueTag { tag });
    }
    // this should be a ValueTag::Extension, but we don't have the type
    ValueTag::Standard(0x7F) => {
      return Err(EncodeError::InvalidValueTag { tag });
    }
    _ => (),
  }

  // calculate name length and value length, exiting early if needed
  let name_len = name.as_ref().len();
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
  put(&mut out, name.as_ref());

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

/// Encodes a [`Token`] to a `&mut Vec<u8>`, and recomputes the spans according
/// to the output buffer.
///
/// Any token that can be encoded normally will be encoded; errors will only be
/// thrown for structural problems with a token.
///
/// # Errors
/// If an error is thrown while encoding, partial writes are cleaned up.
#[allow(clippy::missing_panics_doc)]
pub fn encode_token_to_vec_spanned<B: AsRef<[u8]>>(
  token: &Token<B>,
  base: u32,
  out: &mut Vec<u8>,
) -> Result<DecodedTokenIdx, EncodeError> {
  // starting offset: base + the bytes already in out
  let start = u64::from(base) + out.len() as u64;

  // eager check for offset overflow
  if start + encoded_len(token) as u64 > u64::from(u32::MAX) {
    return Err(EncodeError::OffsetOverflow);
  }
  let start = u32::try_from(start).unwrap();

  // encode the token but remove the bytes if an error occurs
  encode_token_to_vec(token, out)?;

  // helpers for computing spans
  let at = |from: u32, len: u32| Span::new(start + from, start + from + len);
  let val = |span| Spanned { value: (), span };

  // compute spans from token
  Ok(match token {
    Token::Header {
      version,
      operation_or_status,
      request_id,
    } => DecodedToken::Header {
      span:                at(0, 8),
      version:             (
        Spanned {
          value: version.0,
          span:  at(0, 1),
        },
        Spanned {
          value: version.1,
          span:  at(1, 1),
        },
      ),
      operation_or_status: Spanned {
        value: *operation_or_status,
        span:  at(2, 2),
      },
      request_id:          Spanned {
        value: *request_id,
        span:  at(4, 4),
      },
    },
    Token::Delimiter { tag } => DecodedToken::Delimiter {
      tag: Spanned {
        value: *tag,
        span:  at(0, 1),
      },
    },
    Token::Value { tag, name, value } => {
      let name_len = u32::try_from(name.as_ref().len()).unwrap();
      let value_len = u32::try_from(value.as_ref().len()).unwrap();
      let mut pos = 3 + name_len + 2;
      let extension_type_span =
        matches!(tag, ValueTag::Extension { .. }).then(|| {
          let span = at(pos, 4);
          pos += 4;
          span
        });
      DecodedToken::Value {
        span: at(0, pos + value_len),
        tag: Spanned {
          value: *tag,
          span:  at(0, 1),
        },
        extension_type_span,
        name: val(at(3, name_len)),
        value: val(at(pos, value_len)),
      }
    }
    Token::Data { data } => DecodedToken::Data {
      data: val(at(0, u32::try_from(data.as_ref().len()).unwrap())),
    },
  })
}
