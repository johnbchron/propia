use crate::{DelimiterTag, EncodeError, Spanned, Token, ValueTag};

/// Encodes a [`Token`] as bytes into a mutable `Vec<u8>`.
pub fn encode_token<B: AsRef<[u8]>>(
  token: &Token<B>,
  out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
  match token {
    Token::Header {
      version,
      operation_or_status,
      request_id,
      ..
    } => encode_header(version, operation_or_status, request_id, out),
    Token::Delimiter { tag } => encode_delimiter(tag, out),
    Token::Value {
      tag, name, value, ..
    } => encode_value(tag, name.as_ref(), value, out),
    Token::Data { data } => {
      out.extend_from_slice(data.value.as_ref());
      Ok(())
    }
  }
}

fn encode_header(
  version: &(Spanned<u8>, Spanned<u8>),
  operation_or_status: &Spanned<u16>,
  request_id: &Spanned<u32>,
  out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
  let op = operation_or_status.value.to_be_bytes();
  let r_id = request_id.value.to_be_bytes();

  let yielded_slice = [
    version.0.value,
    version.1.value,
    op[0],
    op[1],
    r_id[0],
    r_id[1],
    r_id[2],
    r_id[3],
  ];

  out.reserve(yielded_slice.len());

  out.extend_from_slice(&yielded_slice);

  Ok(())
}

fn encode_delimiter(
  tag: &Spanned<DelimiterTag>,
  out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
  out.push(tag.value.0);

  Ok(())
}

fn encode_value<B: AsRef<[u8]>>(
  tag: &Spanned<ValueTag>,
  name: Option<&Spanned<B>>,
  value: &Spanned<B>,
  out: &mut Vec<u8>,
) -> Result<(), EncodeError> {
  let name_len = name.map(|s| s.value.as_ref().len()).unwrap_or(0);
  let name_len = u16::try_from(name_len)
    .map_err(|_| EncodeError::NameTooLong { length: name_len })?
    .to_be_bytes();

  let tag_byte = match tag.value {
    ValueTag::Standard(b) => b,
    ValueTag::Extension { .. } => ValueTag::EXTENSION_TYPE_MAGIC_BYTE,
  };

  // tag and name len
  let initial = [tag_byte, name_len[0], name_len[1]];
  let name_slice = name.map(|s| s.value.as_ref()).unwrap_or_default();

  // reserve tag, name_len, and name together to conserve allocations
  out.reserve(initial.len() + name_slice.len());
  out.extend_from_slice(&initial);
  out.extend_from_slice(name_slice);

  let value_bytes = value.value.as_ref();
  match tag.value {
    ValueTag::Standard(_) => {
      // encoded value length is normal value length
      let value_len = u16::try_from(value_bytes.len())
        .map_err(|_| EncodeError::ValueTooLong {
          length: value_bytes.len(),
        })?
        .to_be_bytes();

      // reserve space for value length and value bytes
      out.reserve(2 + value_bytes.len());
      out.extend_from_slice(&value_len);
    }
    ValueTag::Extension { kind } => {
      // include the extension type as the first 4 bytes of the value
      let value_len = u16::try_from(value_bytes.len() + 4)
        .map_err(|_| EncodeError::ValueTooLong {
          length: value_bytes.len() + 4,
        })?
        .to_be_bytes();
      let extension_type_bytes = kind.value.to_be_bytes();

      // reserve space for value length, extension type, and value bytes
      out.reserve(2 + 4 + value_bytes.len());
      out.extend_from_slice(&value_len);
      out.extend_from_slice(&extension_type_bytes);
    }
  }
  out.extend_from_slice(value_bytes);

  Ok(())
}
