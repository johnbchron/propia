#![no_main]

use ipp_wire::{DecodeError, Decoder, Step, TokenIterator, TokenRef};
use libfuzzer_sys::fuzz_target;

struct Outcome<'a> {
  tokens: Vec<TokenRef<'a>>,
  error: Option<DecodeError>,
  body_start: Option<u32>,
}

fn stream(data: &[u8], exact: bool) -> Outcome<'_> {
  let mut dec = Decoder::new();
  let (mut start, mut avail) = (0usize, 0usize);
  let mut tokens = Vec::new();
  loop {
    let eof = avail == data.len();
    let buf = &data[start..avail];
    match dec.decode(buf, eof) {
      Ok(Step::Token { token, consumed }) => {
        assert!(consumed > 0 && consumed as usize <= buf.len());
        start += consumed as usize;
        tokens.push(token);
      }
      Ok(Step::NeedMore { min_len }) => {
        assert!(!eof);
        assert!(min_len as usize > buf.len(), "NeedMore would loop forever");
        avail = match exact {
          true => (start + min_len as usize).min(data.len()),
          false => avail + 1,
        };
      }
      Ok(Step::Body) => {
        assert_eq!(dec.offset() as usize, start);
        assert!(matches!(dec.decode(&data[start..], true), Ok(Step::Body)));
        return Outcome {
          tokens,
          error: None,
          body_start: Some(start as u32),
        };
      }
      Ok(Step::Done) => unreachable!("Done before any error"),
      Err(error) => {
        assert!(matches!(dec.decode(buf, eof), Ok(Step::Done)));
        return Outcome {
          tokens,
          error: Some(error),
          body_start: None,
        };
      }
    }
  }
}

fuzz_target!(|data: &[u8]| {
  let mut expected = Outcome {
    tokens: vec![],
    error: None,
    body_start: None,
  };
  for item in TokenIterator::new(data).unwrap() {
    match item {
      Ok(TokenRef::Data { data }) => {
        expected.body_start = Some(data.span.start)
      }
      Ok(token) => expected.tokens.push(token),
      Err(e) => expected.error = Some(e),
    }
  }

  for exact in [false, true] {
    let got = stream(data, exact);
    assert_eq!(got.tokens, expected.tokens);
    assert_eq!(got.error, expected.error);
    assert_eq!(got.body_start, expected.body_start);
  }
});
