use std::assert_matches;

use ipp_model::{Assembled, Assembler, EndState, Limits, Progress};

fn main() {
  const BYTES: &[u8] =
    include_bytes!("../../../corpora/ipp-messages-initial/good/1.ipp");

  let assembler = Assembler::new(Limits::default());
  let assembled = match assembler.push(BYTES) {
    Progress::More(_) => {
      panic!("assembler should have finished with the full message")
    }
    Progress::Done(assembled) => assembled,
  };

  let Assembled { message, rest, end } = assembled;
  assert!(rest.is_empty(), "there is no job data to end up in `rest`");
  assert_matches!(end, EndState::Complete);

  println!("{message:#?}");
}
