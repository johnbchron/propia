//! Contains types and functions useful for conveniently assembling messages
//! from inbound data.

use std::{
  error::Error,
  fmt,
  future::poll_fn,
  pin::Pin,
  task::{Context, Poll, ready},
};

use bytes::{Buf, Bytes};

use futures_core::{Stream, TryStream};

use ipp_message::{
  Assembled, Assembler, EndState, Limits, Message, Progress,
  edit::{Edit, EditError},
};

use crate::{
  chain::Chain,
  outbound::{Outbound, Unterminated},
};

/// Represents a parsed inbound message and job data stream.
///
/// This is produced by [`read`](Inbound::read). [`Inbound`] allows inspection,
/// and a [`Message`] is always decoded.
///
/// If an error is encountered in decoding or the [`Limits`] supplied to
/// [`read`](Inbound::read) are exceeded, the [`Message`] is still decoded but
/// it is likely to be unterminated, meaning it is missing its
/// `end-of-attributes` tag. Attempting to edit an unterminated message would
/// disrupt the boundary between attribute data and job data on the next read,
/// so [`Inbound`] will only transition on to [`Split`] if the message is
/// terminated.
///
/// Regardless of the end state of decoding, an [`Inbound`] can be turned into
/// an [`Outbound`] with [`into_verbatim`](Inbound::into_verbatim).
pub struct Inbound<S> {
  message: Message,
  end: EndState,
  rest: Bytes,
  stream: Option<S>,
  tail_len: Option<u64>,
}

impl<S> fmt::Debug for Inbound<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Inbound")
      .field("message", &self.message)
      .field("end", &self.end)
      .field("rest", &format_args!("{} bytes", self.rest.len()))
      .field("stream", &format_args!("..."))
      .field("tail_len", &self.tail_len)
      .finish()
  }
}

impl<S> Inbound<S> {
  /// Produces an [`Inbound`] from a [`TryStream`]. The `Err` variant can be
  /// anything, and the `Ok` variant can be anything that implements
  /// [`bytes::Buf`].
  pub async fn read(
    mut stream: S,
    limits: Limits,
  ) -> Result<Self, Interrupted<S::Error>>
  where
    S: TryStream + Unpin,
    S::Ok: Buf,
  {
    let mut assembler = Assembler::new(limits);

    loop {
      let poller = poll_fn(|cx| Pin::new(&mut stream).try_poll_next(cx));

      match poller.await {
        // stream has ended
        None => {
          let Assembled { message, rest, end } = assembler.finish();
          return Ok(Inbound {
            message,
            end,
            rest,
            stream: None,
            tail_len: None,
          });
        }

        // stream errored; likely terminated
        Some(Err(error)) => {
          let Assembled {
            message,
            rest: _,
            end: _,
          } = assembler.finish();
          return Err(Interrupted { error, message });
        }

        // got next chunk
        Some(Ok(mut chunk)) => {
          match assembler.push(&chunk.copy_to_bytes(chunk.remaining())) {
            Progress::More(a) => {
              assembler = a;
            }
            Progress::Done(Assembled { message, rest, end }) => {
              return Ok(Inbound {
                message,
                end,
                rest,
                stream: Some(stream),
                tail_len: None,
              });
            }
          }
        }
      }
    }
  }

  /// Sets the length hint of the stream, or clears it with `None`.
  ///
  /// `total` is the length of the whole stream in bytes, including the message
  /// that was already read. This makes it suitable for passing a
  /// `Content-Length`. If `total` is less than what was already read, it's
  /// considered to be wrong and the hint is cleared.
  #[must_use]
  pub fn with_len(self, total: Option<u64>) -> Self {
    let seen = self.message.encoded_len() as u64 + self.rest.len() as u64;
    let tail_len = total.and_then(|l| l.checked_sub(seen));
    Self { tail_len, ..self }
  }

  /// Returns a reference to the decoded [`Message`].
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns a reference to the [`EndState`] of the message decoding.
  pub fn end(&self) -> &EndState {
    &self.end
  }

  /// Returns the undecoded remainder bytes if decoding failed or the limits
  /// were hit. This can also contain job data.
  pub fn undecoded(&self) -> &[u8] {
    match self.end {
      EndState::Complete => &[],
      EndState::Failed(_) | EndState::LimitExceeded => &self.rest,
    }
  }

  /// Transforms this stream into an [`Outbound`] as is, ready to be streamed
  /// out to a client.
  pub fn into_verbatim(self) -> Outbound<S> {
    let Self {
      message,
      end: _,
      rest,
      stream,
      tail_len,
    } = self;
    Outbound(Chain {
      heads: [message.to_bytes(), rest],
      stream,
      tail_len,
    })
  }

  /// Returns the message and document split from each other, if the boundary
  /// between the attribute data and job data is known. Requires the message to
  /// be terminated.
  pub fn split(self) -> Result<Split<S>, Self> {
    // only Ok if end is Complete
    match self {
      Self {
        message,
        end: EndState::Complete,
        rest,
        stream,
        tail_len,
      } => Ok(Split {
        message,
        document: Document(Chain {
          heads: [rest, Bytes::new()],
          stream,
          tail_len,
        }),
      }),
      s => Err(s),
    }
  }
}

/// A message and its job data.
pub struct Split<S> {
  message: Message,
  document: Document<S>,
}

impl<S> fmt::Debug for Split<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Split")
      .field("message", &self.message)
      .field("document", &self.document)
      .finish()
  }
}

impl<S> Split<S> {
  /// Returns a reference to the [`Message`].
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Performs an edit to the message with the given closure.
  pub fn edit<F>(&mut self, f: F) -> Result<(), EditError>
  where
    F: for<'m> FnOnce(&'m Message, &mut Edit<'m>),
  {
    let mut edit = self.message.edit();

    // perform edits
    f(&self.message, &mut edit);

    let new = edit.commit()?;
    self.message = new;

    Ok(())
  }

  /// Replaces the message with given message. Fails if the new message is
  /// unterminated.
  pub fn set_message(&mut self, m: Message) -> Result<(), Unterminated> {
    if !m.is_terminated() {
      return Err(Unterminated);
    }

    self.message = m;

    Ok(())
  }

  /// Transforms this message and job data stream into an [`Outbound`] stream.
  pub fn into_outbound(self) -> Outbound<S> {
    let Self {
      message,
      document:
        Document(Chain {
          heads: [rest, _],
          stream,
          tail_len,
        }),
    } = self;

    Outbound(Chain {
      heads: [message.to_bytes(), rest],
      stream,
      tail_len,
    })
  }

  /// Breaks this type into its parts.
  pub fn into_parts(self) -> (Message, Document<S>) {
    (self.message, self.document)
  }
}

/// The document data stream for a message.
pub struct Document<S>(Chain<S>);

impl<S> Document<S> {
  /// The total byte length of the outbound stream, if known.
  #[allow(clippy::len_without_is_empty)]
  pub fn len(&self) -> Option<u64> {
    self.0.len()
  }
}

impl<S> fmt::Debug for Document<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("Document").field(&self.0).finish()
  }
}

impl<S> Stream for Document<S>
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

/// Returned when an inbound stream is interrupted.
#[derive(Debug)]
pub struct Interrupted<E> {
  /// The error returned by the stream.
  pub error: E,
  /// The message decoded up to the point of interruption.
  pub message: Message,
}

impl<E> fmt::Display for Interrupted<E> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "message stream was interrupted")
  }
}

impl<E: Error + 'static> Error for Interrupted<E> {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    Some(&self.error)
  }
}
