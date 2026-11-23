//! Contains types and functions useful for conveniently assembling messages
//! from inbound data.

use std::{error::Error, fmt, future::poll_fn, pin::Pin, task::Poll};

use bytes::{Buf, Bytes};

use futures_core::{Stream, TryStream};

use ipp_message::{Assembled, Assembler, EndState, Limits, Message, Progress};

/// Assembles a message from a stream.
///
/// This message takes any stream which yields `Result<B, E>` where `B: Buf`,
/// covering things like `&[u8]` and [`Bytes`].
///
/// If the function succeeds with `Ok(Split { .. })`, that only means that the
/// stream terminated without an error. Check `split.end` for the result of
/// message assembly.
///
/// When an `Err` item is encountered in the stream, message assembly stops and
/// [`Interrupted`] is returned.
pub async fn assemble<S>(
  mut stream: S,
  limits: Limits,
) -> Result<Split<S>, Interrupted<S::Error>>
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
        return Ok(Split {
          message,
          end,
          body: Body::new(Some(rest), None),
        });
      }

      // stream errored; likely terminated
      Some(Err(error)) => {
        return Err(Interrupted {
          error,
          assembled: assembler.finish(),
        });
      }

      // got next chunk
      Some(Ok(mut chunk)) => {
        match assembler.push(&chunk.copy_to_bytes(chunk.remaining())) {
          Progress::More(a) => {
            assembler = a;
          }
          Progress::Done(Assembled { message, rest, end }) => {
            return Ok(Split {
              message,
              end,
              body: Body::new(Some(rest), Some(stream)),
            });
          }
        }
      }
    }
  }
}

/// Returned from [`assemble`] when the stream terminates normally. Check
/// [`end`](Split::end) for the result of message assembly.
pub struct Split<S> {
  pub message: Message,
  pub end: EndState,
  pub body: Body<S>,
}

impl<S> fmt::Debug for Split<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Split")
      .field("message", &self.message)
      .field("end", &self.end)
      .field("body", &"...")
      .finish()
  }
}

/// A convenience wrapper around leftover bytes from message assembly plus the
/// rest of the inbound stream.
pub struct Body<S> {
  pub(crate) head: Option<Bytes>,
  pub(crate) stream: Option<S>,
}

impl<S> fmt::Debug for Body<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Body")
      .field("head", &self.head)
      .field("stream", &self.stream.as_ref().map(|_| "..."))
      .finish()
  }
}

impl<S> Stream for Body<S>
where
  S: TryStream + Unpin,
  S::Ok: Buf,
{
  type Item = Result<Bytes, S::Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    match (self.head.take(), self.stream.as_mut()) {
      // the head has bytes
      (Some(h), _) => Poll::Ready(Some(Ok(h))),

      // poll the stream
      (None, Some(stream)) => {
        match Pin::new(stream).try_poll_next(cx) {
          // return the poll result
          Poll::Ready(Some(res)) => {
            Poll::Ready(Some(res.map(|mut b| b.copy_to_bytes(b.remaining()))))
          }
          // nothing left; fuse the stream
          Poll::Ready(None) => {
            self.stream = None;
            Poll::Ready(None)
          }
          Poll::Pending => Poll::Pending,
        }
      }

      // nothing populated, we're done
      (None, None) => Poll::Ready(None),
    }
  }
}

impl<S> Body<S> {
  pub fn new(head: Option<Bytes>, stream: Option<S>) -> Self {
    Self {
      head: head.filter(|b| !b.is_empty()),
      stream,
    }
  }

  pub fn into_parts(self) -> (Option<Bytes>, Option<S>) {
    (self.head, self.stream)
  }
}

/// Returned from [`assemble`] when the stream is interrupted. Contains a
/// [`Message`] assembled from the bytes received.
#[derive(Debug)]
pub struct Interrupted<E> {
  pub error: E,
  pub assembled: Assembled,
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
