use std::{
  fmt,
  pin::Pin,
  task::{Context, Poll, ready},
};

use bytes::{Buf, Bytes};
use futures_core::{Stream, TryStream};

/// Generalized streaming type for this crate.
pub(crate) struct Chain<S> {
  /// Up to 2 known-sized chunks at the start.
  pub(crate) heads: [Bytes; 2],
  /// An optional streaming type following the heads.
  pub(crate) stream: Option<S>,
  /// A length hint for the stream.
  pub(crate) tail_len: Option<u64>,
}

impl<S> fmt::Debug for Chain<S> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("Chain").field(&format_args!("...")).finish()
  }
}

impl<S> Chain<S> {
  // Returns the length remaining in this stream, if known.
  pub(crate) fn remaining(&self) -> Option<u64> {
    let tail_len = match self.stream {
      Some(_) => self.tail_len?,
      None => 0,
    };

    let heads_len = (self.heads[0].len() + self.heads[1].len()) as u64;

    heads_len.checked_add(tail_len)
  }
}

impl<S> Stream for Chain<S>
where
  S: TryStream + Unpin,
  S::Ok: Buf,
{
  type Item = Result<Bytes, S::Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    // return a head chunk
    if let Some(head) = self.heads.iter_mut().find(|h| !h.is_empty()) {
      // empty it in place
      let head = std::mem::take(head);
      return Poll::Ready(Some(Ok(head)));
    }

    // get the stream
    let Some(stream) = self.stream.as_mut() else {
      return Poll::Ready(None);
    };

    // poll the stream and map the result to Bytes
    let next = ready!(Pin::new(stream).try_poll_next(cx))
      .map(|r| r.map(|mut c| c.copy_to_bytes(c.remaining())));

    // intercept result to update self
    match &next {
      // decrement tail_len by chunk length
      Some(Ok(chunk)) => {
        let len = chunk.len() as u64;
        self.tail_len = self.tail_len.and_then(|n| n.checked_sub(len));
      }
      // fuse the stream if errored or ended
      _ => self.stream = None,
    }

    Poll::Ready(next)
  }
}
