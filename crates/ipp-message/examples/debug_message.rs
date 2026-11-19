//! Buffers in an IPP message from stdin, assembles a message from the bytes,
//! and then pretty prints it.

use std::io::{self, Read};

use bytes::Bytes;
use ipp_message::{Assembled, Assembler, Limits, Progress};

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

fn commas(n: usize) -> String {
  let s = n.to_string();
  let mut out = String::with_capacity(s.len() + s.len() / 3);
  for (i, c) in s.chars().enumerate() {
    if i > 0 && (s.len() - i) % 3 == 0 {
      out.push(',');
    }
    out.push(c);
  }
  out
}

fn main() -> io::Result<()> {
  let Assembled { message, rest, end } = assemble(io::stdin().lock())?;
  eprintln!("end state: {end:?}, trailing bytes: {}", rest.len());
  eprintln!(
    "total bytes: {} bytes, total tokens: {} tokens",
    commas(message.chunks().map(|c| c.len()).sum::<usize>()),
    commas(message.tokens().len())
  );

  println!("{message:+#?}");
  Ok(())
}
