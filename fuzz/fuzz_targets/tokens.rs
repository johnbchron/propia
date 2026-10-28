#![no_main]

use ipp_wire::{Span, Spanned, Token, TokenIterator, ValueTag};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  let mut next = 0u32;
  for item in TokenIterator::new(data).unwrap() {
    let token = match item {
      Ok(token) => token,
      Err(e) => {
        assert!(e.offset >= next && e.offset as usize <= data.len());
        return;
      }
    };
    assert_eq!(token.span().start, next, "gap or overlap at {token:?}");
    check_token(data, &token);
    next = token.span().end;
  }
  // a successful parse always ends with a Data token running to the end
  assert_eq!(next as usize, data.len(), "tokens don't cover input");
});

fn slice(data: &[u8], span: Span) -> &[u8] {
  &data[span.start as usize..span.end as usize]
}

fn check_bytes(data: &[u8], outer: Span, s: Spanned<&[u8]>) {
  assert_eq!(s.value, slice(data, s.span));
  assert!(outer.start <= s.span.start && s.span.end <= outer.end);
}

fn check_token(data: &[u8], token: &Token) {
  match *token {
    Token::Header { span, .. } => assert_eq!(span.len(), 8),
    Token::Delimiter { tag } => assert_eq!(tag.span.len(), 1),
    Token::Value {
      span,
      tag,
      name,
      value,
    } => {
      assert_eq!(tag.span.start, span.start);
      assert_eq!(value.span.end, span.end);
      if let Some(name) = name {
        check_bytes(data, span, name);
      }
      if let ValueTag::Extension { kind } = tag.value {
        assert_eq!(kind.span.len(), 4);
      }
      check_bytes(data, span, value);
    }
    Token::Data { data: d } => check_bytes(data, d.span, d),
  }
}
