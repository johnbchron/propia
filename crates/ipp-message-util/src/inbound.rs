//! Reading messages from inbound streams.
//!
//! [`Inbound::read`] reads a [`Message`] from the front of a stream. The
//! resulting [`Inbound`] can then go one of two ways:
//!
//! - [`Inbound::split`] separates the message from its job data so that the
//!   message can be edited. This requires the message to be terminated, meaning
//!   it cannot have failed to decode or hit the decoding limits.
//! - [`Inbound::into_verbatim`] streams everything back out unchanged. This
//!   always works.

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

/// A [`Message`] read from the front of a stream, plus the rest of the stream.
///
/// This is produced by [`read`](Inbound::read). Assembling a message always
/// succeeds, so there is always a [`message`](Inbound::message) to inspect. How
/// assembling ended is described by [`end`](Inbound::end):
/// - [`EndState::Complete`] means the message is terminated by an
///   `end-of-attributes` tag, and everything after it is job data.
/// - [`EndState::Failed`] and [`EndState::LimitExceeded`] mean the message is
///   unterminated. It only contains what was decoded up to that point, and it's
///   unknown where the job data starts.
///
/// An unterminated message can't be [`split`](Inbound::split), because an edit
/// could change how the undecoded data following it is interpreted. However, an
/// `Inbound` can always be turned into an [`Outbound`] with
/// [`into_verbatim`](Inbound::into_verbatim). That makes it possible to forward
/// messages that aren't fully understood:
///
/// ```
/// use ipp_message::edit::EditError;
/// use ipp_message_util::{inbound::Inbound, outbound::Outbound};
///
/// fn forward<S>(inbound: Inbound<S>) -> Result<Outbound<S>, EditError> {
///   match inbound.split() {
///     Ok(mut split) => {
///       split.edit(|message, edit| {
///         // ...
///       })?;
///       Ok(split.into_outbound())
///     }
///     // this message can't be edited, so leave it as is
///     Err(inbound) => Ok(inbound.into_verbatim()),
///   }
/// }
/// ```
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
  /// Reads a [`Message`] from the front of a stream.
  ///
  /// The stream is polled until the message is complete, decoding fails, or
  /// `limits` is exceeded. Job data is only read if it shares a chunk with the
  /// end of the message, so at most [`Limits::max_bytes`] plus one chunk is
  /// buffered.
  ///
  /// # Errors
  ///
  /// Returns [`Interrupted`] if the stream yields an error before any of the
  /// above happens. Malformed data, exceeded limits, and streams that end early
  /// are not errors here; see [`end`](Inbound::end) for those.
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

  /// Returns a reference to the decoded [`Message`], which is unterminated
  /// unless [`end`](Inbound::end) is [`EndState::Complete`].
  pub fn message(&self) -> &Message {
    &self.message
  }

  /// Returns a reference to the [`EndState`], which describes how assembling
  /// the message ended.
  pub fn end(&self) -> &EndState {
    &self.end
  }

  /// Returns the bytes following the message that were read but couldn't be
  /// decoded. This is always empty if [`end`](Inbound::end) is
  /// [`EndState::Complete`].
  ///
  /// These bytes can include job data, and more undecoded data may still be in
  /// the stream.
  pub fn undecoded(&self) -> &[u8] {
    match self.end {
      EndState::Complete => &[],
      EndState::Failed(_) | EndState::LimitExceeded => &self.rest,
    }
  }

  /// Turns this into an [`Outbound`] that yields exactly the bytes of the
  /// original stream, regardless of how assembling ended.
  ///
  /// The bytes may be chunked differently than they arrived.
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

  /// Separates the message from its job data.
  ///
  /// # Errors
  ///
  /// Gives back the `Inbound` if [`end`](Inbound::end) is anything other than
  /// [`EndState::Complete`], because the boundary between the attribute data
  /// and job data is unknown.
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

/// A terminated [`Message`] and its job data.
///
/// This is produced by [`Inbound::split`]. The message can be edited with
/// [`edit`](Split::edit) or [`set_message`](Split::set_message), neither of
/// which can make it unterminated. Finish with
/// [`into_outbound`](Split::into_outbound) to stream the message and job data
/// back out, or with [`into_parts`](Split::into_parts) to take them separately.
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

  /// Edits the message with the given closure.
  ///
  /// The closure is given the current message and an [`Edit`] started from it,
  /// which is committed after the closure returns. Each call to this method is
  /// a separate round of edits, so the closure sees the results of prior calls.
  ///
  /// # Errors
  ///
  /// Returns an [`EditError`] if committing fails. The message is left as it
  /// was.
  ///
  /// # Panics
  ///
  /// Some methods on [`Edit`] can panic; see its docs.
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

  /// Replaces the message with the given message, keeping the job data.
  ///
  /// # Errors
  ///
  /// Returns [`Unterminated`] if the given message is unterminated. The current
  /// message is kept.
  pub fn set_message(&mut self, m: Message) -> Result<(), Unterminated> {
    if !m.is_terminated() {
      return Err(Unterminated);
    }

    self.message = m;

    Ok(())
  }

  /// Joins the message and its job data into an [`Outbound`].
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

  /// Breaks this into the message and its job data. These can be joined again
  /// with [`Outbound::new`].
  pub fn into_parts(self) -> (Message, Document<S>) {
    (self.message, self.document)
  }
}

/// A byte stream of the job data of a message.
///
/// This is produced by [`Split::into_parts`]. It yields any job data that was
/// read along with the message, and then the rest of the original stream. After
/// it ends or yields an error, it only yields `None`.
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

/// Returned by [`Inbound::read`] when the stream errors before the message was
/// fully read.
#[derive(Debug)]
pub struct Interrupted<E> {
  /// The error yielded by the stream.
  pub error: E,
  /// The message decoded up to the point of interruption, which is always
  /// unterminated. Any bytes after its last whole token are lost, so this can
  /// be used for inspection but not for forwarding.
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
