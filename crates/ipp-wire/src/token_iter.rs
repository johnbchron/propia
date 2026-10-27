use std::{iter::FusedIterator, str::Utf8Error};

use crate::{
  DelimiterTag, Span, Token, ValueTag, cursor::Cursor, span::Spanned,
};

pub struct TokenIterator<'a> {
  cursor: Cursor<'a>,
  state: TokenIteratorState,
}

pub(crate) enum TokenIteratorState {
  Start,
  Attributes,
  Data,
  End,
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
}

impl<'a> TokenIterator<'a> {
  pub fn new(input: &'a [u8]) -> Option<Self> {
    Some(Self {
      cursor: Cursor::new(input)?,
      state: TokenIteratorState::Start,
    })
  }

  fn error(
    &mut self,
    message: &'static str,
    kind: DecodeErrorKind,
  ) -> DecodeError {
    self.state = TokenIteratorState::End;
    DecodeError {
      offset: self.cursor.pos(),
      message,
      kind,
    }
  }

  fn truncated_error(
    &mut self,
    message: &'static str,
    needed: usize,
  ) -> DecodeError {
    self.error(
      message,
      DecodeErrorKind::Truncated {
        needed,
        available: self.cursor.remaining(),
      },
    )
  }

  fn decode_header(&mut self) -> Result<Token<'a>, DecodeError> {
    // open the span for the header
    let mark = self.cursor.mark();

    // read the version
    let major_version = self.cursor.u8().ok_or_else(|| {
      self.truncated_error("truncated before header major version", 1)
    })?;
    let minor_version = self.cursor.u8().ok_or_else(|| {
      self.truncated_error("truncated before header minor version", 1)
    })?;

    // an operation ID for a request and a status code for a response
    let operation_or_status = self.cursor.u16().ok_or_else(|| {
      self.truncated_error(
        "truncated before header operation-id or status-code",
        2,
      )
    })?;

    // correlating ID
    let request_id = self.cursor.u32().ok_or_else(|| {
      self.truncated_error("truncated before header request-id", 4)
    })?;

    // end the header span
    let span = self.cursor.since_mark(mark);

    let header = Token::Header {
      span,
      version: (major_version, minor_version),
      operation_or_status,
      request_id,
    };

    self.state = TokenIteratorState::Attributes;
    Ok(header)
  }

  fn decode_delimiter_tag(
    &mut self,
    tag: Spanned<u8>,
  ) -> Result<Token<'a>, DecodeError> {
    let tag = tag.map(DelimiterTag);

    if tag.value == DelimiterTag::END {
      self.state = TokenIteratorState::Data
    }

    Ok(Token::Delimiter { tag })
  }

  fn decode_attribute_or_delimiter(
    &mut self,
  ) -> Result<Token<'a>, DecodeError> {
    // pull the first byte to see what it is
    let tag = self.cursor.u8().ok_or_else(|| {
      self.error(
        "message ended before end-of-attributes tag",
        DecodeErrorKind::NoEndOfAttributes,
      )
    })?;

    match tag.value {
      // delimiter tag
      0x00..=0x0F => self.decode_delimiter_tag(tag),
      // value tag: starts an attribute value
      _ => self.decode_attribute(tag),
    }
  }

  fn decode_attribute(
    &mut self,
    tag: Spanned<u8>,
  ) -> Result<Token<'a>, DecodeError> {
    // open the span for the attribute
    let span_start = tag.span.start;

    // consume the name
    let name_len = self
      .cursor
      .u16()
      .ok_or_else(|| {
        self.truncated_error("truncated before attribute name length", 2)
      })?
      .value;
    let (name_bytes, name_span) =
      self.cursor.take(name_len as usize).ok_or_else(|| {
        self.truncated_error("truncated before attribute name", name_len as _)
      })?;
    let name = Spanned {
      value: str::from_utf8(name_bytes).map_err(|e| {
        self.error(
          "attribute name is not valid utf-8",
          DecodeErrorKind::InvalidName { name_error: e },
        )
      })?,
      span: name_span,
    };

    // get the length of the value
    let mut value_len = self
      .cursor
      .u16()
      .ok_or_else(|| {
        self.truncated_error("truncated before attribute value length", 2)
      })?
      .value;

    // if it's an extension value type, extract the value type first
    let tag = match tag.value {
      0x7F => {
        // value type is first 4 bytes of value
        value_len = value_len.checked_sub(4).ok_or_else(|| {
          self.error(
            "attribute value ends within extension type bytes",
            DecodeErrorKind::InvalidExtensionLength {
              value_len: value_len as _,
            },
          )
        })?;
        let kind = self.cursor.u32().ok_or_else(|| {
          self.truncated_error("truncated before value tag extension type", 4)
        })?;
        tag.map(|_| ValueTag::Extension { kind })
      }
      _ => tag.map(ValueTag::Known),
    };

    // extract the value itself
    let (value_bytes, value_span) =
      self.cursor.take(value_len as usize).ok_or_else(|| {
        self.truncated_error("truncated before attribute value", value_len as _)
      })?;
    let value = Spanned {
      value: value_bytes,
      span: value_span,
    };

    // end the attribute span
    let span = Span::new(span_start, self.cursor.pos() as u32);

    Ok(Token::Value {
      span,
      tag,
      name,
      value,
    })
  }

  fn decode_data(&mut self) -> Token<'a> {
    // the rest of the message bytes are the message data
    let (data_bytes, span) = self.cursor.rest();

    let data = Spanned {
      value: data_bytes,
      span,
    };

    self.state = TokenIteratorState::End;

    Token::Data { data }
  }
}

impl<'a> Iterator for TokenIterator<'a> {
  type Item = Result<Token<'a>, DecodeError>;

  fn next(&mut self) -> Option<Self::Item> {
    match self.state {
      TokenIteratorState::Start => Some(self.decode_header()),
      TokenIteratorState::Attributes => {
        Some(self.decode_attribute_or_delimiter())
      }
      TokenIteratorState::Data => Some(Ok(self.decode_data())),
      TokenIteratorState::End => None,
    }
  }
}

impl FusedIterator for TokenIterator<'_> {}
