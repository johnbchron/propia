use std::ops::Range;

/// Absolute byte range into the original input buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
  pub start: u32,
  pub end: u32,
}

impl Span {
  pub fn new(start: u32, end: u32) -> Self {
    debug_assert!(start <= end, "inverted span {start}..{end}");
    Span { start, end }
  }

  pub fn len(self) -> u32 {
    self.end - self.start
  }

  pub fn is_empty(self) -> bool {
    self.start == self.end
  }

  pub fn range(self) -> Range<u32> {
    self.start..self.end
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
  pub value: T,
  pub span: Span,
}

impl<T> Spanned<T> {
  pub fn map<U, F>(self, f: F) -> Spanned<U>
  where
    F: FnOnce(T) -> U,
  {
    Spanned {
      value: f(self.value),
      span: self.span,
    }
  }
}
