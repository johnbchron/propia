use bytes::{Bytes, BytesMut};
use ipp_wire::{
  DecodeError, DecodedTokenIdx, DecodedTokenRef, Decoder, DelimiterTag, Step,
};

use crate::{Message, buffer::Buffer};

/// Assembles an IPP [`Message`] from a byte stream.
///
/// `Assembler` is a separate type from [`Message`] because it is a sans-IO
/// state machine, meaning it doesn't touch or drive any IO. Additionally, the
/// process of assembling a message is a *total function*, meaning it will
/// always succeed.
///
/// ## Assembling a message
/// Essentially, to assemble a message you feed the `Assembler` chunks of bytes
/// using [`push`](Assembler::push) until either **1)** the assembler tells you
/// it's done by returning [`Progress::Done`] (if not, it will return itself in
/// [`Progress::More`]), or **2)** you run out of bytes and you call
/// [`finish`](Assembler::finish) on it.
///
/// In either case, you end up with an [`Assembled`], which contains the
/// [`Message`], any leftover bytes (which are part of the job data), and an
/// [`EndState`], which indicates if assembling ended with an error or limit
/// breach.
///
/// Admittedly, the API shape here can make control flow a little difficult, but
/// it's for good reason. The `Assembler` only needs the attribute data at the
/// beginning of an IPP message, and the rest of the message (the job data)
/// should not be buffered. Thus it is desirable to parse just the first part of
/// a data stream, produce a [`Message`], and keep the rest of the stream
/// intact. This is enabled by the current design.
///
/// One could imagine shaping something like this into an Axum extractor, where
/// the extractor consumes the request body and returns something like
/// `Extractor((Message, EndState, [rest of the job data]))`.
///
/// Here is an example.
///
/// ```
/// use std::assert_matches;
///
/// use bytes::Bytes;
/// use ipp_message::{Assembled, Assembler, EndState, Header, Limits, Progress};
/// use ipp_wire::{DecodeError, DecodeErrorKind};
///
/// // takes a byte-stream-like and returns Assembled
/// fn simple_assemble<I: Iterator<Item = Bytes>>(iter: &mut I) -> Assembled {
///   let mut assembler = Assembler::new(Limits::default());
///
///   for bytes in iter {
///     assembler = match assembler.push(&bytes) {
///       Progress::More(assembler) => assembler,
///       Progress::Done(assembled) => return assembled,
///     }
///   }
///
///   // finish anyway if bytes ran out
///   return assembler.finish();
/// }
///
/// let bytes: [Bytes; 8] = core::array::from_fn(|_| {
///   Bytes::from_owner((0u8..).take(128).collect::<Vec<_>>())
/// });
/// let mut iter = bytes.into_iter();
///
/// // assemble the message
/// let Assembled { message, rest, end } = simple_assemble(&mut iter);
///
/// // reattach the any leftovers the assembler gave back
/// let _job_data_chunks = std::iter::once(rest).chain(iter);
///
/// // this is a garbage message, so it says an error occurred, but the message
/// // still exists and is readable because assembling always succeeds
/// assert_matches!(
///   end,
///   EndState::Failed(DecodeError {
///     kind: DecodeErrorKind::Truncated { .. },
///     ..
///   })
/// );
///
/// // header still works
/// assert_eq!(
///   message.header(),
///   Some(Header {
///     version:             (0, 1),
///     operation_or_status: u16::from_be_bytes([2, 3]),
///     request_id:          u32::from_be_bytes([4, 5, 6, 7]),
///   })
/// );
/// ```
pub struct Assembler {
  decoder: Decoder,
  buf: BytesMut,
  /// Position of the decoder (tokens already decoded).
  pos: usize,
  /// Last wanted by the decoder.
  wanted: usize,
  /// Decoded tokens.
  tokens: Vec<DecodedTokenIdx>,
  /// User-defined limits for message assembling.
  limits: Limits,
}

impl Assembler {
  /// Builds a new assembler with the given [`Limits`].
  #[must_use]
  pub fn new(limits: Limits) -> Self {
    Self {
      decoder: Decoder::new(),
      buf: BytesMut::new(),
      pos: 0,
      wanted: 0,
      tokens: Vec::new(),
      limits,
    }
  }

  /// Run the internal decoder on the next chunk of bytes.
  #[must_use]
  pub fn push(mut self, chunk: &[u8]) -> Progress {
    // capture the chunk
    self.buf.extend_from_slice(chunk);

    // don't try the decoder if the buffer doesn't have enough
    if self.buf.len() < self.wanted {
      return Progress::More(self);
    }

    // propagate run results
    match self.run(false) {
      Some(end) => Progress::Done(self.assemble(end)),
      None => Progress::More(self),
    }
  }

  /// Decode tokens from the bytes already captured.
  fn run(&mut self, eof: bool) -> Option<EndState> {
    loop {
      // don't consume more bytes than allowed
      let stop = self.buf.len().min(self.limits.max_bytes as usize);

      if self.tokens.len() >= self.limits.max_tokens as usize {
        return Some(EndState::LimitExceeded);
      }

      // attempt to decode a token from unprocessed bytes.
      match self.decoder.decode(&self.buf[self.pos..stop], eof) {
        // successful decode
        Ok(Step::Token { token, consumed }) => {
          // step forward the position
          self.pos += consumed as usize;

          // we're done if we've reached the end-of-attributes-tag
          let done = matches!(token, DecodedTokenRef::Delimiter { tag }
            if tag.value == DelimiterTag::END_OF_ATTRIBUTES_TAG);

          // save the token
          self.tokens.push(token.detach());

          if done {
            return Some(EndState::Complete);
          }
        }

        // more bytes are needed to decode a full token
        Ok(Step::NeedMore { min_len }) => {
          // keep track of how many bytes are needed
          self.wanted = self.pos + min_len as usize;

          // abort early if a token can't be decoded under the limits
          if self.wanted > self.limits.max_bytes as usize {
            return Some(EndState::LimitExceeded);
          }

          // exit the loop so more bytes can be gathered
          return None;
        }

        Err(e) => return Some(EndState::Failed(e)),

        // these are states we should never read from the decoder
        Ok(Step::Body | Step::Errored) => {
          unreachable!("we stop at end-of-attributes and on the first error")
        }
      }
    }
  }

  /// Build the message with the current buffer and given end state.
  fn assemble(mut self, end: EndState) -> Assembled {
    // separate consumed bytes from unconsumed bytes
    let rest = self.buf.split_off(self.pos).freeze();
    let bytes = if rest.is_empty() {
      self.buf.freeze()
    } else {
      // this is done to make sure rest's allocation is dropped if rest is dropped
      // worth a copy
      Bytes::copy_from_slice(&self.buf)
    };

    // build and return the message
    let message = Message::build(Buffer::new(bytes), self.tokens);
    Assembled { message, rest, end }
  }

  /// Finish the assembler and produce a message.
  #[allow(clippy::missing_panics_doc)]
  #[must_use]
  pub fn finish(mut self) -> Assembled {
    let end = self.run(true).expect("eof never needs more");
    self.assemble(end)
  }
}

/// Limits to be used in assembling an IPP [`Message`].
#[derive(Clone, Debug)]
pub struct Limits {
  /// The maximum number of bytes allowed in a message's attribute data.
  pub max_bytes: u32,
  /// The maximum number of tokens allowed in a message's attribute data.
  pub max_tokens: u32,
}

impl Default for Limits {
  fn default() -> Self {
    Self {
      max_bytes: 1 << 22,  // 4 MiB of attribute data
      max_tokens: 1 << 19, // 524,288 tokens
    }
  }
}

impl Limits {
  /// Sets the limits to the maximum values.
  #[must_use]
  pub fn unbounded() -> Self {
    Self {
      max_bytes: u32::MAX,
      max_tokens: u32::MAX,
    }
  }
}

/// The progress of message assembly after pushing a slice of bytes.
pub enum Progress {
  /// The assembler has not reached the `end-of-attributes` tag and is ready to
  /// ingest more data.
  More(Assembler),
  /// The assembler reached the `end-of-attributes` tag and has finished its
  /// decoding. Find the bytes of the last chunk that came after the
  /// `end-of-attributes` in [`Assembled::rest`].
  Done(Assembled),
}

/// The output of assembling a [`Message`] from a byte stream.
#[derive(Debug)]
pub struct Assembled {
  /// The assembled message.
  pub message: Message,
  /// All ingested bytes following the last token.
  pub rest: Bytes,
  /// The ending state of the Assembler.
  pub end: EndState,
}

/// Describes the end state of [`Message`] assembly, which is an infalliable
/// operation.
#[derive(Debug)]
pub enum EndState {
  /// A `Message` was produced.
  Complete,
  /// The decoder itself failed.
  Failed(DecodeError),
  /// Some set limit was exceeded and assembling was aborted.
  LimitExceeded,
}
