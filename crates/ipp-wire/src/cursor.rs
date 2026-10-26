use super::Span;

pub(crate) struct Cursor<'a> {
  pub(crate) input: &'a [u8],
  pub(crate) pos: usize,
}

pub struct CursorMark(usize);

impl<'a> Cursor<'a> {
  pub fn read_u8(&mut self) -> Option<(u8, Span)> {
    let b = *self.input.get(self.pos)?;
    let span = Span {
      start: self.pos as u32,
      len: 1,
    };
    self.pos += 1;
    Some((b, span))
  }

  pub fn read_u16(&mut self) -> Option<(u16, Span)> {
    let s = self.input.get(self.pos..self.pos + 2)?;
    let span = Span {
      start: self.pos as u32,
      len: 2,
    };
    self.pos += 2;
    Some((u16::from_be_bytes([s[0], s[1]]), span))
  }

  pub fn read_u32(&mut self) -> Option<(u32, Span)> {
    let s = self.input.get(self.pos..self.pos + 4)?;
    let span = Span {
      start: self.pos as u32,
      len: 4,
    };
    self.pos += 4;
    Some((u32::from_be_bytes([s[0], s[1], s[2], s[3]]), span))
  }

  pub fn take(&mut self, n: usize) -> Option<(&'a [u8], Span)> {
    let end = self.pos.checked_add(n)?;
    if end > self.input.len() {
      return None;
    }

    let slice = &self.input[self.pos..end];
    let s = Span {
      start: self.pos as u32,
      len: n as u32,
    };

    self.pos = end;

    Some((slice, s))
  }

  pub fn take_rest(&mut self) -> Option<(&'a [u8], Span)> {
    let end = self.input.len().checked_sub(1)?;
    let len = end.checked_sub(self.pos)?;
    self.take(len)
  }

  pub fn mark(&self) -> CursorMark {
    CursorMark(self.pos)
  }

  pub fn span_from_mark(&self, mark: CursorMark) -> Option<Span> {
    let len = self.pos.checked_sub(mark.0)?;
    Some(Span {
      start: mark.0 as u32,
      len: len as u32,
    })
  }
}
