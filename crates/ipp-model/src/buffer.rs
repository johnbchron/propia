use std::sync::Arc;

use bytes::Bytes;
use ipp_wire::Span;

pub(crate) struct Buffer {
  segments: Arc<[Segment]>,
}

impl Buffer {
  pub(crate) fn new(bytes: Bytes) -> Self {
    Self {
      segments: Arc::new([Segment { offset: 0, bytes }]),
    }
  }

  /// Slices into a buffer. The span cannot cross segment boundaries. Panics on
  /// out-of-bounds or a segment boundary.
  pub(crate) fn slice(&self, span: Span) -> &[u8] {
    // find the segment to slice
    let index = self.segments.partition_point(|s| s.offset <= span.start) - 1;
    let segment = &self.segments[index];

    let start = (span.start - segment.offset) as usize;
    let end = (span.end - segment.offset) as usize;
    &segment.bytes[start..end]
  }
}

pub(crate) struct Segment {
  offset: u32,
  bytes:  Bytes,
}
