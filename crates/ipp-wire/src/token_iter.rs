use crate::{DelimiterTag, Token, ValueTag, cursor::Cursor};

pub struct TokenIterator<'a> {
  pub(crate) cursor: Cursor<'a>,
  pub(crate) state: TokenIteratorState,
}

impl<'a> TokenIterator<'a> {
  pub fn new(input: &'a [u8]) -> Self {
    Self {
      cursor: Cursor { input, pos: 0 },
      state: TokenIteratorState::Start,
    }
  }
}

pub(crate) enum TokenIteratorState {
  Start,
  AfterHeader,
  AfterAttributes,
  End,
}

impl<'a> Iterator for TokenIterator<'a> {
  type Item = Token<'a>;

  fn next(&mut self) -> Option<Self::Item> {
    match self.state {
      TokenIteratorState::Start => {
        // need to pull out the header
        let mark = self.cursor.mark();
        let (major_version, _) = self.cursor.read_u8()?;
        let (minor_version, _) = self.cursor.read_u8()?;
        let (operation_or_status, _) = self.cursor.read_u16()?;
        let (request_id, _) = self.cursor.read_u32()?;
        let span = self.cursor.span_from_mark(mark)?;

        let header = Token::Header {
          span,
          version: (major_version, minor_version),
          operation_or_status,
          request_id,
        };

        self.state = TokenIteratorState::AfterHeader;
        Some(header)
      }
      TokenIteratorState::AfterHeader => {
        // next byte is either a value tag or delimeter tag
        match self.cursor.read_u8()? {
          // delimiter tag
          (b @ 0x00..=0x0F, tag_span) => {
            let tag = DelimiterTag(b);

            if tag == DelimiterTag::END {
              self.state = TokenIteratorState::AfterAttributes
            }

            Some(Token::Delimiter {
              tag,
              span: tag_span,
            })
          }
          // value tag with extension value type, starts an attribute value
          (0x7F, tag_span) => {
            let mark = self.cursor.mark();

            let (name_len, _) = self.cursor.read_u16()?;
            let (name, name_span) = self.cursor.take(name_len as usize)?;
            let (value_len, _) = self.cursor.read_u16()?;

            // value type is first 4 bytes of value
            let (kind, kind_span) = self.cursor.read_u32()?;
            let value_len = value_len.checked_sub(4)?;
            let (value, value_span) = self.cursor.take(value_len as usize)?;

            let span = self.cursor.span_from_mark(mark)?;

            let tag = ValueTag::Extension { kind, kind_span };
            Some(Token::Value {
              span,
              tag,
              tag_span,
              name_span,
              name,
              value_span,
              value,
            })
          }
          // normal value tag, starts an attribute value
          (b, tag_span) => {
            let mark = self.cursor.mark();

            let (name_len, _) = self.cursor.read_u16()?;
            let (name, name_span) = self.cursor.take(name_len as usize)?;

            let (value_len, _) = self.cursor.read_u16()?;
            let (value, value_span) = self.cursor.take(value_len as usize)?;

            let span = self.cursor.span_from_mark(mark)?;

            let tag = ValueTag::Known(b);
            Some(Token::Value {
              span,
              tag,
              tag_span,
              name_span,
              name,
              value_span,
              value,
            })
          }
        }
      }
      TokenIteratorState::AfterAttributes => {
        // rest of the bytes are data
        let (data, span) = self.cursor.take_rest()?;

        self.state = TokenIteratorState::End;

        Some(Token::Data { span, data })
      }
      TokenIteratorState::End => None,
    }
  }
}
