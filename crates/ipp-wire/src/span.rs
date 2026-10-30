use std::ops::Range;

/// Absolute byte range (half-open) into the original input buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
  /// The starting offset of the range.
  pub start: u32,
  /// The ending offset of the range.
  pub end:   u32,
}

impl Span {
  /// Creates a new [`Span`].
  #[must_use]
  pub fn new(start: u32, end: u32) -> Self {
    debug_assert!(start <= end, "inverted span {start}..{end}");
    Span { start, end }
  }

  /// Returns the length of the [`Span`].
  #[must_use]
  pub fn len(self) -> u32 { self.end - self.start }

  /// Indicates whether the span is zero-length.
  #[must_use]
  pub fn is_empty(self) -> bool { self.start == self.end }

  /// Returns the [`Span`] as a [`Range`].
  #[must_use]
  pub fn range(self) -> Range<u32> { self.start..self.end }
}

/// A value with its originating span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spanned<T> {
  /// The value itself.
  pub value: T,
  /// The span indicating where it originated from.
  pub span:  Span,
}

impl<T> Spanned<T> {
  /// Maps the spanned value from one type to another.
  #[must_use]
  pub fn map<U, F>(self, f: F) -> Spanned<U>
  where
    F: FnOnce(T) -> U,
  {
    Spanned {
      value: f(self.value),
      span:  self.span,
    }
  }
}
