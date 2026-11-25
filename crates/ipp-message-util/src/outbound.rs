//! Writing messages to outbound streams.
//!
//! An [`Outbound`] can be built from scratch with [`Outbound::new`], or come
//! from a message that was read, with
//! [`Inbound::into_verbatim`](crate::inbound::Inbound::into_verbatim) or
//! [`Split::into_outbound`](crate::inbound::Split::into_outbound).

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

/// A byte stream of a [`Message`] followed by its job data, ready to be sent
/// over the wire.
///
/// The message is yielded first, and then the job data as it arrives. After the
/// stream ends or yields an error, it only yields `None`.
pub struct Outbound<S>(pub(crate) Chain<S>);

impl<S> fmt::Debug for Outbound<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("Outbound").field(&self.0).finish()
  }
}

impl<S> Outbound<S> {
  /// Joins a [`Message`] and a stream of job data.
  ///
  /// `document_len` is the length of the job data in bytes, if known. It should
  /// not include the message.
  ///
  /// For a message without job data, pass an empty stream and `Some(0)`.
  ///
  /// # Errors
  ///
  /// Returns [`Unterminated`] if the message is unterminated.
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

/// Indicates that a message was unterminated, meaning its last token was not an
/// `end-of-attributes` tag.
///
/// If job data were attached to an unterminated message, the job data would be
/// misinterpreted as being part of the message. See also
/// [`Message::is_terminated`].
#[derive(Debug)]
pub struct Unterminated;

impl fmt::Display for Unterminated {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "the message was unterminated")
  }
}

impl Error for Unterminated {}
