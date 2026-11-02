#![no_main]

use ipp_wire::{DecodeIterator, encode_token_to_vec};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  let mut out = Vec::new();
  let mut end = 0;

  for item in DecodeIterator::new(data).unwrap() {
    let Ok(decoded_token) = item else { break };
    let token = decoded_token.forget_spans();

    let before_len = out.len();
    encode_token_to_vec(&token, &mut out).unwrap();

    let span = decoded_token.span();
    assert_eq!(
      &out[before_len..],
      &data[span.start as usize..span.end as usize],
      "re-encoding changed {decoded_token:?}"
    );
    end = span.end;
  }
  // on success the Data token runs to the end, so this is the whole input
  assert_eq!(out, &data[..end as usize]);
});
