use std::fmt;

use super::Message;

/// The max depth to display while debug printing.
const MAX_DEPTH: usize = 16;

/// The max for bytes to display when debug printing a field.
const MAX_BYTES: usize = 64;

impl fmt::Debug for Message {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self.header() {
      Some(header) => write!(
        f,
        "Message {}.{} op/status {:#06x} id {}",
        header.version.0,
        header.version.1,
        header.operation_or_status,
        header.request_id
      ),
      None => write!(f, "Message <no header>"),
    }
  }
}
