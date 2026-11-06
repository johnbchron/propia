//! Buffers in an IPP message from stdin, assembles a message from the bytes,
//! and then pretty prints it.

use std::io::{self, Read};

use bytes::Bytes;
use ipp_model::{Assembled, Assembler, Limits, Progress};

fn assemble(mut input: impl Read) -> io::Result<Assembled> {
  let mut assembler = Assembler::new(Limits::default());
  let mut chunk = [0u8; 8192];

  loop {
    let n = match input.read(&mut chunk) {
      Ok(0) => return Ok(assembler.finish()),
      Ok(n) => n,
      Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
      Err(e) => return Err(e),
    };

    match assembler.push(&chunk[..n]) {
      Progress::More(next) => assembler = next,
      Progress::Done(Assembled { message, rest, end }) => {
        let mut rest = rest.to_vec();
        input.read_to_end(&mut rest)?;
        let rest = Bytes::from_owner(rest);
        return Ok(Assembled { message, rest, end });
      }
    }
  }
}

fn main() -> io::Result<()> {
  let Assembled { message, rest, end } = assemble(io::stdin().lock())?;
  eprintln!("end state: {end:?}, trailing bytes: {}", rest.len());

  println!("{message:+#?}");
  Ok(())
}
