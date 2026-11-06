use bytes::{Bytes, BytesMut};
use ipp_wire::{
  DecodeError, DecodedTokenIdx, DecodedTokenRef, Decoder, DelimiterTag, Step,
};

use crate::{Message, buffer::Buffer};

/// Assembles an IPP [`Message`] from a byte stream.
pub struct Assembler {
  decoder: Decoder,
  buf:     BytesMut,
  /// Position of the decoder (tokens already decoded).
  pos:     usize,
  /// Last wanted by the decoder.
  wanted:  usize,
  /// Decoded tokens.
  tokens:  Vec<DecodedTokenIdx>,
  /// User-defined limits for message assembling.
  limits:  Limits,
}

impl Assembler {
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

          if self.tokens.len() >= self.limits.max_tokens as usize {
            return Some(EndState::LimitExceeded);
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
    let bytes = self.buf.split_to(self.pos).freeze();
    let rest = self.buf.freeze();

    // build and return the message
    let message = Message::build(Buffer::new(bytes), self.tokens);
    Assembled { message, rest, end }
  }

  /// Finish the assembler and produce a message.
  pub fn finish(mut self) -> Assembled {
    let end = self.run(true).expect("eof never needs more");
    self.assemble(end)
  }
}

/// Limits to be used in assembling an IPP [`Message`].
#[derive(Clone, Debug)]
pub struct Limits {
  /// The maximum number of bytes allowed in a message's attribute data.
  pub max_bytes:  u32,
  /// The maximum number of tokens allowed in a message's attribute data.
  pub max_tokens: u32,
}

impl Default for Limits {
  fn default() -> Self {
    Self {
      max_bytes:  u32::MAX,
      max_tokens: u32::MAX,
    }
  }
}

/// The progress of message assembly after pushing a slice of bytes.
pub enum Progress {
  More(Assembler),
  Done(Assembled),
}

/// The output of assembling a [`Message`] from a byte stream.
#[derive(Debug)]
pub struct Assembled {
  pub message: Message,
  /// All ingested bytes following the last token.
  pub rest:    Bytes,
  /// The ending state of the Assembler.
  pub end:     EndState,
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
