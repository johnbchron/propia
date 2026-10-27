use crate::{DelimiterTag, Token, ValueTag, cursor::Cursor, span::Spanned};

pub struct TokenIterator<'a> {
  pub(crate) cursor: Cursor<'a>,
  pub(crate) state: TokenIteratorState,
}

pub(crate) enum TokenIteratorState {
  Start,
  Attributes,
  Data,
  End,
}

impl<'a> TokenIterator<'a> {
  pub fn new(input: &'a [u8]) -> Option<Self> {
    Some(Self {
      cursor: Cursor::new(input)?,
      state: TokenIteratorState::Start,
    })
  }

  fn decode_header(&mut self) -> Option<Token<'a>> {
    // open the span for the header
    let mark = self.cursor.mark();

    // read the version
    let major_version = self.cursor.u8()?;
    let minor_version = self.cursor.u8()?;

    // an operation ID for a request and a status code for a response
    let operation_or_status = self.cursor.u16()?;

    // correlating ID
    let request_id = self.cursor.u32()?;

    // end the header span
    let span = self.cursor.since_mark(mark);

    let header = Token::Header {
      span,
      version: (major_version, minor_version),
      operation_or_status,
      request_id,
    };

    self.state = TokenIteratorState::Attributes;
    Some(header)
  }

  fn decode_delimiter_tag(&mut self, tag: Spanned<u8>) -> Option<Token<'a>> {
    let tag = tag.map(DelimiterTag);

    if tag.value == DelimiterTag::END {
      self.state = TokenIteratorState::Data
    }

    Some(Token::Delimiter { tag })
  }

  fn decode_attribute_or_delimiter(&mut self) -> Option<Token<'a>> {
    // pull the first byte to see what it is
    let tag = self.cursor.u8()?;

    match tag.value {
      // delimiter tag
      0x00..=0x0F => self.decode_delimiter_tag(tag),
      // value tag: starts an attribute value
      _ => self.decode_attribute(tag),
    }
  }

  fn decode_attribute(&mut self, tag: Spanned<u8>) -> Option<Token<'a>> {
    // open the span for the attribute
    let mark = self.cursor.mark();

    // consume the name
    let name_len = self.cursor.u16()?.value;
    let (name_bytes, name_span) = self.cursor.take(name_len as usize)?;
    let name = Spanned {
      value: str::from_utf8(name_bytes).expect("value name is not utf8"),
      span: name_span,
    };

    // get the length of the value
    let mut value_len = self.cursor.u16()?.value;

    // if it's an extension value type, extract the value type first
    let tag = match tag.value {
      0x7F => {
        // value type is first 4 bytes of value
        let kind = self.cursor.u32()?;
        value_len = value_len.checked_sub(4)?;
        tag.map(|_| ValueTag::Extension { kind })
      }
      _ => tag.map(ValueTag::Known),
    };

    // extract the value itself
    let (value_bytes, value_span) = self.cursor.take(value_len as usize)?;
    let value = Spanned {
      value: value_bytes,
      span: value_span,
    };

    // end the attribute span
    let span = self.cursor.since_mark(mark);

    Some(Token::Value {
      span,
      tag,
      name,
      value,
    })
  }

  fn decode_data(&mut self) -> Option<Token<'a>> {
    // the rest of the message bytes are the message data
    let (data_bytes, span) = self.cursor.rest();

    let data = Spanned {
      value: data_bytes,
      span,
    };

    self.state = TokenIteratorState::End;

    Some(Token::Data { data })
  }
}

impl<'a> Iterator for TokenIterator<'a> {
  type Item = Token<'a>;

  fn next(&mut self) -> Option<Self::Item> {
    match self.state {
      TokenIteratorState::Start => self.decode_header(),
      TokenIteratorState::Attributes => self.decode_attribute_or_delimiter(),
      TokenIteratorState::Data => self.decode_data(),
      TokenIteratorState::End => None,
    }
  }
}
