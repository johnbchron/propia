use ipp_wire::TokenIterator;

const MESSAGE_1_BYTES: &[u8] =
  include_bytes!("../../../corpora/ipp-messages-initial/good/1.ipp");
const MESSAGE_2_BYTES: &[u8] =
  include_bytes!("../../../corpora/ipp-messages-initial/good/2.ipp");

fn main() {
  println!("message 1:");
  for token in TokenIterator::new(MESSAGE_1_BYTES).unwrap() {
    println!("  token: {token:?}");
  }

  println!("message 2:");
  for token in TokenIterator::new(MESSAGE_2_BYTES).unwrap() {
    println!("  token: {token:?}");
  }
}
