use crate::span::Spanned;

use super::Span;

pub(crate) struct Cursor<'a> {
  buf: &'a [u8],
  pos: usize,
  end: usize,
}

pub struct CursorMark(usize);

impl<'a> Cursor<'a> {
  pub fn new(buf: &'a [u8]) -> Option<Self> {
    if buf.len() > u32::MAX as usize {
      return None;
    }
    Some(Cursor {
      buf,
      pos: 0,
      end: buf.len(),
    })
  }

  pub fn pos(&self) -> usize {
    self.pos
  }

  pub fn remaining(&self) -> usize {
    self.end - self.pos
  }

  pub fn take(&mut self, n: usize) -> Option<(&'a [u8], Span)> {
    if n > self.remaining() {
      return None;
    }
    let start = self.pos;
    self.pos += n;
    Some((
      &self.buf[start..self.pos],
      Span::new(start as u32, self.pos as u32),
    ))
  }

  pub fn u8(&mut self) -> Option<Spanned<u8>> {
    let (b, span) = self.take(1)?;
    Some(Spanned { value: b[0], span })
  }

  pub fn u16(&mut self) -> Option<Spanned<u16>> {
    let (b, span) = self.take(2)?;
    Some(Spanned {
      value: u16::from_be_bytes(b.try_into().unwrap()),
      span,
    })
  }

  pub fn u32(&mut self) -> Option<Spanned<u32>> {
    let (b, span) = self.take(4)?;
    Some(Spanned {
      value: u32::from_be_bytes(b.try_into().unwrap()),
      span,
    })
  }

  pub fn rest(&mut self) -> (&'a [u8], Span) {
    self
      .take(self.remaining())
      .expect("remaining bytes always fit")
  }

  pub fn mark(&self) -> CursorMark {
    CursorMark(self.pos)
  }

  pub fn since_mark(&self, mark: CursorMark) -> Span {
    Span::new(mark.0 as u32, self.pos as u32)
  }
}
