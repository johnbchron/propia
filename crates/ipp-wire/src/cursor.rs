use crate::span::Spanned;

use super::Span;

pub(crate) struct Cursor<'a> {
  buf: &'a [u8],
  /// base offset to use in spans
  base: u32,
  /// position within the current buffer
  pos: u32,
}

pub(crate) struct CursorMark(u32);

impl<'a> Cursor<'a> {
  /// Creates a new cursor. Returns `None` if `base + buf.len() > U32::MAX`.
  pub fn new(buf: &'a [u8], base: u32) -> Option<Self> {
    if base as u64 + buf.len() as u64 > u32::MAX as u64 {
      return None;
    }
    Some(Cursor { buf, base, pos: 0 })
  }

  /// The current absolute offset.
  pub fn offset(&self) -> u32 {
    self.base + self.pos
  }

  /// The current position within the buffer.
  pub fn pos(&self) -> u32 {
    self.pos
  }

  /// The bytes remaining in the buffer.
  pub fn remaining(&self) -> u32 {
    self.buf.len() as u32 - self.pos
  }

  /// Takes bytes from the buffer, returning `None` if there aren't enough.
  pub fn take(&mut self, n: usize) -> Option<(&'a [u8], Span)> {
    if n as u32 > self.remaining() {
      return None;
    }

    let start = self.offset();
    self.pos += n as u32;

    let slice = &self.buf[self.pos as usize - n..self.pos as usize];
    let span = Span::new(start, self.offset());

    Some((slice, span))
  }

  /// Reads a `u8` from the buffer and advances the cursor.
  pub fn u8(&mut self) -> Option<Spanned<u8>> {
    let (b, span) = self.take(1)?;
    Some(Spanned { value: b[0], span })
  }

  /// Reads a `u16` from the buffer and advances the cursor.
  pub fn u16(&mut self) -> Option<Spanned<u16>> {
    let (b, span) = self.take(2)?;
    Some(Spanned {
      value: u16::from_be_bytes(b.try_into().unwrap()),
      span,
    })
  }

  /// Reads a `u32` from the buffer and advances the cursor.
  pub fn u32(&mut self) -> Option<Spanned<u32>> {
    let (b, span) = self.take(4)?;
    Some(Spanned {
      value: u32::from_be_bytes(b.try_into().unwrap()),
      span,
    })
  }

  /// Creates a `CursorMark` (fencepost) at the current offset.
  pub fn mark(&self) -> CursorMark {
    CursorMark(self.offset())
  }

  /// Returns the span from the given mark until now.
  pub fn since_mark(&self, mark: CursorMark) -> Span {
    Span::new(mark.0, self.offset())
  }
}
