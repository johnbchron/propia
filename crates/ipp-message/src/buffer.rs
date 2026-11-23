use std::sync::Arc;

use bytes::Bytes;
use ipp_wire::Span;

/// A buffer constructed from a collection of [`Bytes`].
#[derive(Clone)]
pub(crate) struct Buffer {
  segments: Arc<[Segment]>,
}

impl Buffer {
  pub(crate) fn new(bytes: Bytes) -> Self {
    Self {
      segments: Arc::new([Segment { offset: 0, bytes }]),
    }
  }

  /// The first offset after the last segment.
  pub(crate) fn end(&self) -> u32 {
    let last = self.segments.last().expect("a buffer always has a segment");
    last.offset
      + u32::try_from(last.bytes.len()).expect("byte sections are len-checked")
  }

  /// Returns a buffer with `bytes` added at [`end`](Self::end).
  pub(crate) fn append(&self, bytes: Bytes) -> Self {
    if bytes.is_empty() {
      return self.clone();
    }
    let offset = self.end();
    let tail = Segment { offset, bytes };
    Self {
      // an empty segment would share its offset with the new one
      segments: self
        .segments
        .iter()
        .filter(|s| !s.bytes.is_empty())
        .cloned()
        .chain([tail])
        .collect(),
    }
  }

  fn locate_span(&self, span: Span) -> (&Segment, usize, usize) {
    let index = self.segments.partition_point(|s| s.offset <= span.start) - 1;
    let segment = &self.segments[index];

    let start = (span.start - segment.offset) as usize;
    let end = (span.end - segment.offset) as usize;

    (segment, start, end)
  }

  /// Slices into a buffer. The span cannot cross segment boundaries. Panics on
  /// out-of-bounds or a segment boundary.
  pub(crate) fn slice(&self, span: Span) -> &[u8] {
    let (segment, start, end) = self.locate_span(span);
    &segment.bytes[start..end]
  }

  /// Slices into a buffer, as Bytes. The span cannot cross segment boundaries.
  /// Panics on out-of-bounds or a segment boundary.
  pub(crate) fn slice_bytes(&self, span: Span) -> Bytes {
    let (segment, start, end) = self.locate_span(span);
    segment.bytes.slice(start..end)
  }

  /// Returns whether the start of a segment falls on a given offset.
  pub(crate) fn is_segment_start(&self, offset: u32) -> bool {
    self
      .segments
      .binary_search_by_key(&offset, |s| s.offset)
      .is_ok()
  }
}

#[derive(Clone)]
pub(crate) struct Segment {
  offset: u32,
  bytes: Bytes,
}
