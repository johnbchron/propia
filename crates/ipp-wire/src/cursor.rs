use super::Span;
use crate::span::Spanned;

/// A cursor that reads bytes and emits spans. Only accepts buffers less than 4
/// GiB.
pub struct Cursor<'a> {
  buf:  &'a [u8],
  /// base offset to use in spans
  base: u32,
  /// position within the current buffer
  pos:  u32,
}

/// A mark, or fencepost, that can be arbitrarily used as the start of a `Span`.
/// Call [`Cursor::since_mark`] to close the `Span` with the current [`Cursor`]
/// position.
#[derive(Clone, Copy)]
pub struct CursorMark(u32);

impl<'a> Cursor<'a> {
  /// Creates a new cursor. Returns `None` if `base + buf.len() > U32::MAX`.
  #[must_use]
  pub fn new(buf: &'a [u8], base: u32) -> Option<Self> {
    if u64::from(base) + buf.len() as u64 > u64::from(u32::MAX) {
      return None;
    }
    Some(Cursor { buf, base, pos: 0 })
  }

  /// The current absolute offset.
  #[must_use]
  pub fn offset(&self) -> u32 { self.base + self.pos }

  /// The current position within the buffer.
  #[must_use]
  pub fn pos(&self) -> u32 { self.pos }

  /// The bytes remaining in the buffer.
  #[must_use]
  pub fn remaining(&self) -> u32 {
    u32::try_from(self.buf.len())
      .expect("input proven to be shorter than u32::MAX")
      - self.pos
  }

  /// Takes bytes from the buffer, returning `None` if there aren't enough.
  #[must_use]
  pub fn take(&mut self, n: u32) -> Option<(&'a [u8], Span)> {
    if n > self.remaining() {
      return None;
    }

    let start = self.offset();
    self.pos += n;

    let slice = &self.buf[self.pos as usize - n as usize..self.pos as usize];
    let span = Span::new(start, self.offset());

    Some((slice, span))
  }

  /// Reads a `u8` from the buffer and advances the cursor.
  #[must_use]
  pub fn u8(&mut self) -> Option<Spanned<u8>> {
    let (b, span) = self.take(1)?;
    Some(Spanned { value: b[0], span })
  }

  /// Reads a `u16` from the buffer and advances the cursor.
  #[must_use]
  pub fn u16(&mut self) -> Option<Spanned<u16>> {
    let (b, span) = self.take(2)?;
    Some(Spanned {
      value: u16::from_be_bytes(b.try_into().unwrap()),
      span,
    })
  }

  /// Reads a `u32` from the buffer and advances the cursor.
  #[must_use]
  pub fn u32(&mut self) -> Option<Spanned<u32>> {
    let (b, span) = self.take(4)?;
    Some(Spanned {
      value: u32::from_be_bytes(b.try_into().unwrap()),
      span,
    })
  }

  /// Creates a `CursorMark` (fencepost) at the current offset.
  #[must_use]
  pub fn mark(&self) -> CursorMark { CursorMark(self.offset()) }

  /// Returns the span from the given mark until now.
  #[must_use]
  pub fn since_mark(&self, mark: CursorMark) -> Span {
    Span::new(mark.0, self.offset())
  }
}
