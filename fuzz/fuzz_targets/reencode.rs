#![no_main]

use ipp_wire::{DecodeIterator, encode_token};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  let mut out = Vec::new();
  let mut end = 0;
  for item in DecodeIterator::new(data).unwrap() {
    let Ok(token) = item else { break };
    let before = out.len();
    encode_token(&token, &mut out).expect("decoded tokens are encodable");
    let span = token.span();
    assert_eq!(
      &out[before..],
      &data[span.start as usize..span.end as usize],
      "re-encoding changed {token:?}"
    );
    end = span.end;
  }
  // on success the Data token runs to the end, so this is the whole input
  assert_eq!(out, &data[..end as usize]);
});
