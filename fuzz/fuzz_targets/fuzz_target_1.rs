#![no_main]

use ipp_wire::TokenIterator;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  let _tokens = TokenIterator::new(data)
    .expect("failed to create iterator")
    .collect::<Vec<_>>();
});
