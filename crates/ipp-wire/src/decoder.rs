mod decode;

use self::decode::{decode_attribute_or_delimiter, decode_header};
use crate::{
  DecodeError, DecodeErrorKind, DelimiterTag, TokenRef, cursor::Cursor,
};

pub enum Step<'a> {
  /// Processing yielded complete token. The next call's `buf` must start at
  /// `buf[consumed..]`.
  Token {
    token:    TokenRef<'a>,
    consumed: u32,
  },
  /// More bytes are needed before a token can be yielded; nothing was consumed.
  /// Call again once `buf.len() >= min_len`.
  NeedMore { min_len: u32 },
  /// The attribute section is over; document data starts at `buf[0]`
  /// (absolute offset `Decoder::offset()`). Returned on every call from here
  /// on.
  Body,
  /// A previous call returned an error.
  Errored,
}

pub struct Decoder {
  state:  State,
  offset: u32,
}

enum State {
  Header,
  Attributes,
  Body,
  Failed,
}

impl Default for Decoder {
  fn default() -> Self { Self::new() }
}

impl Decoder {
  #[must_use]
  pub fn new() -> Self {
    Self {
      state:  State::Header,
      offset: 0,
    }
  }

  #[must_use]
  pub fn offset(&self) -> u32 { self.offset }

  pub fn decode<'a>(
    &mut self,
    buf: &'a [u8],
    eof: bool,
  ) -> Result<Step<'a>, DecodeError> {
    match &self.state {
      State::Body => return Ok(Step::Body),
      State::Failed => return Ok(Step::Errored),
      _ => (),
    }

    let mut cur = Cursor::new(buf, self.offset).ok_or_else(|| {
      self.state = State::Failed;
      DecodeError {
        offset:  self.offset,
        message: "buffer exceeds 4 GiB",
        kind:    DecodeErrorKind::OffsetOverflow,
      }
    })?;

    let attempt = match self.state {
      State::Header => decode_header(&mut cur),
      State::Attributes => decode_attribute_or_delimiter(&mut cur),
      _ => unreachable!("fused states already checked"),
    };

    match attempt {
      Ok(token) => {
        // state transitions
        match token {
          TokenRef::Header { .. } => self.state = State::Attributes,
          TokenRef::Delimiter { tag }
            if tag.value == DelimiterTag::END_OF_ATTRIBUTES_TAG =>
          {
            self.state = State::Body;
          }
          _ => {}
        }
        let consumed = cur.pos();
        self.offset += consumed;
        Ok(Step::Token { token, consumed })
      }
      Err(Stall::Incomplete { min_len, .. }) if !eof => {
        Ok(Step::NeedMore { min_len })
      }
      Err(Stall::Incomplete { at_eof: e, .. } | Stall::Invalid(e)) => {
        self.state = State::Failed;
        Err(e)
      }
    }
  }
}

enum Stall {
  Incomplete { min_len: u32, at_eof: DecodeError },
  Invalid(DecodeError),
}

#[must_use]
fn need(cur: &Cursor, n: u32, message: &'static str) -> Stall {
  Stall::Incomplete {
    min_len: cur.pos() + n,
    at_eof:  DecodeError {
      offset: cur.offset(),
      message,
      kind: crate::DecodeErrorKind::Truncated {
        needed:    n,
        available: cur.remaining(),
      },
    },
  }
}

#[must_use]
fn invalid(
  cur: &Cursor,
  message: &'static str,
  kind: DecodeErrorKind,
) -> Stall {
  Stall::Invalid(DecodeError {
    offset: cur.offset(),
    message,
    kind,
  })
}
