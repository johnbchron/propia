//! Contains types and functions useful for conveniently assembling outbound
//! streams from message and document data.

use std::{
  error::Error,
  fmt,
  pin::Pin,
  task::{Context, Poll, ready},
};

use bytes::{Buf, Bytes};
use futures_core::{Stream, TryStream};
use ipp_message::Message;

use crate::chain::Chain;

/// An outbound message stream.
///
/// [`Outbound`] is a byte stream ready to be sent over the wire, containing a message and job data.
pub struct Outbound<S>(pub(crate) Chain<S>);

impl<S> fmt::Debug for Outbound<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("Outbound").field(&self.0).finish()
  }
}

impl<S> Outbound<S> {
  /// Constructs a new [`Outbound`] stream from a [`Message`] and a document stream.
  pub fn new(
    message: &Message,
    document: S,
    document_len: Option<u64>,
  ) -> Result<Self, Unterminated> {
    if !message.is_terminated() {
      return Err(Unterminated);
    }

    Ok(Self(Chain {
      heads: [message.to_bytes(), Bytes::new()],
      stream: Some(document),
      tail_len: document_len,
    }))
  }

  /// The total byte length of the outbound stream, if known.
  #[allow(clippy::len_without_is_empty)]
  pub fn len(&self) -> Option<u64> {
    self.0.len()
  }
}

impl<S> Stream for Outbound<S>
where
  S: TryStream + Unpin,
  S::Ok: Buf,
{
  type Item = Result<Bytes, S::Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    Poll::Ready(
      ready!(Pin::new(&mut self.0).try_poll_next(cx))
        .map(|r| r.map(|mut c| c.copy_to_bytes(c.remaining()))),
    )
  }
}

/// Indicates that a message was unterminated, meaning the last token was not an
/// `end-of-attributes`.
///
/// If an unterminated message was emitted from a stream, data following the
/// message would be misinterpreted as being part of the message.
#[derive(Debug)]
pub struct Unterminated;

impl fmt::Display for Unterminated {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "the message was unterminated")
  }
}

impl Error for Unterminated {}
