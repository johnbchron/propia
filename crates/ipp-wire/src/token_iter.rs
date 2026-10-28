use std::{iter::FusedIterator, str::Utf8Error};

use crate::{
  Span, Token,
  decoder::{Decoder, Step},
  span::Spanned,
};

pub struct TokenIterator<'a> {
  input: &'a [u8],
  pos: usize,
  decoder: Decoder,
  done: bool,
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("decoding error at offset {offset}: {message}")]
pub struct DecodeError {
  pub offset: usize,
  pub message: &'static str,
  pub kind: DecodeErrorKind,
}

#[derive(Clone, Debug)]
pub enum DecodeErrorKind {
  Truncated { needed: usize, available: usize },
  InvalidName { name_error: Utf8Error },
  NoEndOfAttributes,
  InvalidExtensionLength { value_len: usize },
  OffsetOverflow,
}

impl<'a> TokenIterator<'a> {
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
    match self.decoder.decode(&self.input[self.pos..], true) {
      Ok(Step::Token { token, consumed }) => {
        self.pos += consumed;
        Some(Ok(token))
      }
      Ok(Step::Body) => {
        self.done = true;
        let span = Span::new(self.pos as u32, self.input.len() as u32);
        Some(Ok(Token::Data {
          data: Spanned {
            value: &self.input[self.pos..],
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
