use ipp_wire::DecodeIterator;

const MESSAGE_1_BYTES: &[u8] =
  include_bytes!("../../../corpora/ipp-messages-initial/good/1.ipp");
const MESSAGE_2_BYTES: &[u8] =
  include_bytes!("../../../corpora/ipp-messages-initial/good/2.ipp");

fn main() {
  println!("message 1:");
  for token in DecodeIterator::new(MESSAGE_1_BYTES).unwrap() {
    println!("  token: {token:?}");
  }

  println!("message 2:");
  for token in DecodeIterator::new(MESSAGE_2_BYTES).unwrap() {
    println!("  token: {token:?}");
  }
}
