//! Verifies that structurally-generated tokens encode and decode back to the
//! same tokens.

#![feature(iterator_try_collect)]
#![no_main]

use arbitrary::Arbitrary;
use ipp_wire::{
  DecodeIterator, DelimiterTag, Token, encode_token_to_vec, encoded_len,
};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
  header: ((u8, u8), u16, u32),
  middle: Vec<Token<Vec<u8>>>,
  data: Vec<u8>,
}

fuzz_target!(|input: Input| {
  let total_encoded_len: usize = input.middle.iter().map(encoded_len).sum();
  if total_encoded_len >= u32::MAX as usize {
    panic!("fuzz input is too long");
  }

  let mut out = Vec::with_capacity(total_encoded_len);
  let mut accepted_input_tokens = Vec::with_capacity(input.middle.len());

  let header = Token::<Vec<u8>>::Header {
    version: input.header.0,
    operation_or_status: input.header.1,
    request_id: input.header.2,
  };
  encode_token_to_vec(&header, &mut out).expect("header encoding failed");
  accepted_input_tokens.push(header);

  for input_token in &input.middle {
    match input_token {
      Token::Header { .. } => continue,
      Token::Delimiter {
        tag: DelimiterTag::END_OF_ATTRIBUTES_TAG,
      } => continue,
      Token::Data { .. } => continue,
      _ => (),
    }

    if let Ok(()) = encode_token_to_vec(input_token, &mut out) {
      accepted_input_tokens.push(input_token.clone());
    }
  }

  let end_of_attributes = Token::<Vec<u8>>::Delimiter {
    tag: DelimiterTag::END_OF_ATTRIBUTES_TAG,
  };
  let data = Token::Data { data: input.data };
  encode_token_to_vec(&end_of_attributes, &mut out)
    .expect("end-of-attributes token encoding failed");
  accepted_input_tokens.push(end_of_attributes);
  encode_token_to_vec(&data, &mut out).expect("data token encoding failed");
  accepted_input_tokens.push(data);

  let output_tokens = DecodeIterator::new(&out)
    .unwrap()
    .map(|r| r.map(|t| t.forget_spans().map_generic(ToOwned::to_owned)))
    .try_collect::<Vec<_>>()
    .expect("decoding failed");

  assert_eq!(accepted_input_tokens, output_tokens);
});
