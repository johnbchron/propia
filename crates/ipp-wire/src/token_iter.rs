use std::iter::FusedIterator;

use crate::{
  DecodeError, Span, Token,
  decoder::{Decoder, Step},
  span::Spanned,
};

pub struct TokenIterator<'a> {
  input: &'a [u8],
  pos: u32,
  decoder: Decoder,
  done: bool,
}

impl<'a> TokenIterator<'a> {
  #[must_use]
  pub fn new(input: &'a [u8]) -> Option<Self> {
    if input.len() > u32::MAX as usize {
      return None;
    }
    Some(Self {
      input,
      pos: 0,
      decoder: Decoder::new(),
      done: false,
    })
  }
}

impl<'a> Iterator for TokenIterator<'a> {
  type Item = Result<Token<'a>, DecodeError>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.done {
      return None;
    }
    match self.decoder.decode(&self.input[self.pos as usize..], true) {
      Ok(Step::Token { token, consumed }) => {
        self.pos += consumed;
        Some(Ok(token))
      }
      Ok(Step::Body) => {
        self.done = true;
        let span = Span::new(
          self.pos,
          u32::try_from(self.input.len())
            .expect("input proven to be shorter than u32::MAX"),
        );
        Some(Ok(Token::Data {
          data: Spanned {
            value: &self.input[self.pos as usize..],
            span,
          },
        }))
      }
      Ok(Step::NeedMore { .. }) => {
        unreachable!("eof decoding never needs more")
      }
      Ok(Step::Done) => None,
      Err(e) => {
        self.done = true;
        Some(Err(e))
      }
    }
  }
}

impl FusedIterator for TokenIterator<'_> {}
