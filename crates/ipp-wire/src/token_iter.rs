use crate::{DelimiterTag, Token, ValueTag, cursor::Cursor, span::Spanned};

pub struct TokenIterator<'a> {
  pub(crate) cursor: Cursor<'a>,
  pub(crate) state: TokenIteratorState,
}

impl<'a> TokenIterator<'a> {
  pub fn new(input: &'a [u8]) -> Option<Self> {
    Some(Self {
      cursor: Cursor::new(input)?,
      state: TokenIteratorState::Start,
    })
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
        let major_version = self.cursor.u8()?;
        let minor_version = self.cursor.u8()?;
        let operation_or_status = self.cursor.u16()?;
        let request_id = self.cursor.u32()?;
        let span = self.cursor.since_mark(mark)?;

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
        let tag = self.cursor.u8()?;
        // next byte is either a value tag or delimeter tag
        match tag.value {
          // delimiter tag
          0x00..=0x0F => {
            let tag = tag.map(DelimiterTag);

            if tag.value == DelimiterTag::END {
              self.state = TokenIteratorState::AfterAttributes
            }

            Some(Token::Delimiter { tag })
          }
          // value tag with extension value type, starts an attribute value
          0x7F => {
            let mark = self.cursor.mark();

            let name_len = self.cursor.u16()?.value;
            let (name_bytes, name_span) =
              self.cursor.take(name_len as usize)?;
            let name = Spanned {
              value: str::from_utf8(name_bytes)
                .expect("value name is not utf8"),
              span: name_span,
            };

            let value_len = self.cursor.u16()?.value;
            // value type is first 4 bytes of value
            let kind = self.cursor.u32()?;
            let value_len = value_len.checked_sub(4)?;
            let (value_bytes, value_span) =
              self.cursor.take(value_len as usize)?;
            let value = Spanned {
              value: value_bytes,
              span: value_span,
            };

            let span = self.cursor.since_mark(mark)?;

            let tag = tag.map(|_| ValueTag::Extension { kind });

            Some(Token::Value {
              span,
              tag,
              name,
              value,
            })
          }
          // normal value tag, starts an attribute value
          _ => {
            let mark = self.cursor.mark();

            let name_len = self.cursor.u16()?.value;
            let (name_bytes, name_span) =
              self.cursor.take(name_len as usize)?;
            let name = Spanned {
              value: str::from_utf8(name_bytes)
                .expect("value name is not utf8"),
              span: name_span,
            };

            let value_len = self.cursor.u16()?.value;
            let (value_bytes, value_span) =
              self.cursor.take(value_len as usize)?;
            let value = Spanned {
              value: value_bytes,
              span: value_span,
            };

            let span = self.cursor.since_mark(mark)?;

            let tag = tag.map(ValueTag::Known);

            Some(Token::Value {
              span,
              tag,
              name,
              value,
            })
          }
        }
      }
      TokenIteratorState::AfterAttributes => {
        // rest of the bytes are data
        let (data_bytes, span) = self.cursor.rest();
        let data = Spanned {
          value: data_bytes,
          span,
        };

        self.state = TokenIteratorState::End;

        Some(Token::Data { data })
      }
      TokenIteratorState::End => None,
    }
  }
}
