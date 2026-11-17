// #![deny(missing_docs)]

//! # `ipp-message` - IPP message decoding and editing (RFC 8010)
//!
//! `ipp-message` enables parsing an IPP message datastructure from a stream of
//! bytes, editing the message, and then re-encoding the message back to bytes.

mod assembler;
mod buffer;
mod diagnostic;
mod edit;
mod message;

pub use self::{assembler::*, diagnostic::*, edit::*, message::*};
