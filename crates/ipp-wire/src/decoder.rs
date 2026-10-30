mod decode;

use self::decode::{decode_attribute_or_delimiter, decode_header};
use crate::{
  DecodeError, DecodeErrorKind, DelimiterTag, TokenRef, cursor::Cursor,
};

/// The output of a run of [`Decoder::decode`].
pub enum Step<'a> {
  /// Processing on the given buffer yielded a complete [`Token`](crate::Token).
  /// For the next call, slice out the first `consumed` bytes.
  Token {
    /// The yielded [`Token`](crate::Token).
    token:    TokenRef<'a>,
    /// The number of bytes consumed, which should be excluded from the next
    /// call.
    consumed: u32,
  },
  /// More bytes are needed before a token can be yielded; none of the buffer
  /// bytes are considered to be consumed. Call again once `buf.len() >=
  /// min_len`.
  NeedMore {
    /// At least this many bytes (total) are needed in the buffer before a
    /// [`Token`](crate::Token) can be yielded.
    min_len: u32,
  },
  /// The attribute section is over; document data starts at `buf[0]`. The
  /// absolute (across buffers) offset of the data block is can be found via
  /// [`Decoder::offset`]. Returned on every call from here on.
  Body,
  /// A previous call returned an error.
  Errored,
}

/// An IPP wire format (RFC 8010) decoder.
///
/// This decoder is built on a sans-IO design, meaning that it doesn't touch or
/// drive any IO. To drive the decoder, call [`Decoder::decode`] with the data
/// you have. Set `eof` if you know that no more data is coming. If it doesn't
/// error, it will give back a [`Step`]. There are 4 possible output states
/// encoded in [`Step`]:
/// - [`Step::Token`](crate::Token): Enough data to decode a
///   [`Token`](crate::Token) was present, and a single [`Token`](crate::Token)
///   was yielded. `consumed` is the number of bytes that token covered, so call
///   [`decode`](Decoder::decode) again with `buf[consumed..]`.
/// - [`Step::NeedMore`]: Not enough data to decode a [`Token`](crate::Token)
///   was present, so acquire more data and call [`decode`](Decoder::decode)
///   again when at least `min_len` bytes are present in the buffer. If you
///   reach the end of your input, set `eof`.
/// - [`Step::Body`]: Every last token has been yielded from the attribute data
///   of the IPP message, and the rest of the input (`buf[..]`) is body data. If
///   you need the offset since the start of decoding, that's available via
///   [`Decoder::offset`]. This state is fused, meaning that every subsequent
///   call to [`decode`](Decoder::decode) will also return [`Step::Body`].
/// - [`Step::Errored`]: A previous call to [`decode`](Decoder::decode) yielded
///   an error. This state is fused, meaning that every subsequent call to
///   [`decode`](Decoder::decode) will also return [`Step::Errored`].
pub struct Decoder {
  state:  State,
  offset: u32,
}

/// Internal state for the decoder.
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
  /// Creates a new [`Decoder`].
  #[must_use]
  pub fn new() -> Self {
    Self {
      state:  State::Header,
      offset: 0,
    }
  }

  #[must_use]
  /// Returns the absolute offset the decoder is at, i.e. the total number of
  /// bytes it has reported `consumed`.
  pub fn offset(&self) -> u32 { self.offset }

  /// Runs the decoder on a chunk of data. See the docs on [`Decoder`] for
  /// instructions.
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

// A failure reported by the decoder functions.
enum Stall {
  Incomplete { min_len: u32, at_eof: DecodeError },
  Invalid(DecodeError),
}

/// Helper function for the decoder functions, reporting that more bytes are
/// needed.
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

/// Helper function for the decoder functions, reporting a structural decoding
/// error.
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
