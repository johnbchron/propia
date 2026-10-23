use ipp_wire::TokenIterator;

const MESSAGE_1_BYTES: &[u8] =
  include_bytes!("../../../corpora/initial/goodMessage1.ipp");
const MESSAGE_2_BYTES: &[u8] =
  include_bytes!("../../../corpora/initial/goodMessage2.ipp");

fn main() {
  println!("message 1:");
  for token in TokenIterator::new(MESSAGE_1_BYTES) {
    println!("  token: {token:?}");
  }

  println!("message 2:");
  for token in TokenIterator::new(MESSAGE_2_BYTES) {
    println!("  token: {token:?}");
  }
}
