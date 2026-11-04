use std::sync::Arc;

use bytes::Bytes;

pub(crate) struct Buffer {
  segments: Arc<[Segment]>,
}

impl Buffer {
  pub(crate) fn new(bytes: Bytes) -> Self {
    Self {
      segments: Arc::new([Segment { offset: 0, bytes }]),
    }
  }
}

pub(crate) struct Segment {
  offset: u32,
  bytes:  Bytes,
}
