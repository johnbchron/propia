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

impl Iterator for TokenIterator<'_> {
  type Item = Token;

  fn next(&mut self) -> Option<Self::Item> {
    match self.state {
      TokenIteratorState::Start => {
        // need to pull out the header
        // 2 bytes for version, 2 for operation/status, 4 for request ID
        const HEADER_LENGTH: usize = 8;

        let Some(span) = self.cursor.take(HEADER_LENGTH) else {
          tracing::debug!(
            "could not read header; input less than {HEADER_LENGTH} bytes"
          );
          return None;
        };
        let header = Token::Header { span };

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

            Some(Token::Delimiter { tag, tag_span })
          }
          // value tag with extension value type, starts an attribute value
          (0x7F, tag_span) => {
            let (name_len, _) = self.cursor.read_u16()?;
            let name = self.cursor.take(name_len as usize)?;
            let (value_len, _) = self.cursor.read_u16()?;

            // value type is first 4 bytes of value
            let (kind, kind_span) = self.cursor.read_u32()?;
            let value_len = value_len.checked_sub(4)?;

            let value = self.cursor.take(value_len as usize)?;

            let tag = ValueTag::Extension { kind, kind_span };
            Some(Token::Value {
              tag,
              tag_span,
              name,
              value,
            })
          }
          // normal value tag, starts an attribute value
          (b, tag_span) => {
            let (name_len, _) = self.cursor.read_u16()?;
            let name = self.cursor.take(name_len as usize)?;
            let (value_len, _) = self.cursor.read_u16()?;
            let value = self.cursor.take(value_len as usize)?;

            let tag = ValueTag::Known(b);
            Some(Token::Value {
              tag,
              tag_span,
              name,
              value,
            })
          }
        }
      }
      TokenIteratorState::AfterAttributes => {
        // rest of the bytes are data
        let span = self.cursor.take_rest()?;

        self.state = TokenIteratorState::End;

        Some(Token::Data { span })
      }
      TokenIteratorState::End => None,
    }
  }
}
