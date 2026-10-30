#![warn(missing_docs)]

//! # ipp-wire
//! `ipp-wire` aides in decoding and encoding messages in the IPP wire format
//! described by [RFC 8010][1], specifically the conversion between raw bytes
//! and collections of [`Token`]s. All data is spanned.
//!
//! A [`Token`] is loosely defined as a small flat chunk of data within an IPP
//! message which can be intrensically identified, i.e. based on its own data,
//! rather than its surroundings or context. Further contextual and stateful
//! interpretation of [`Token`]s is needed to gain full [RFC 8010][1] semantics.
//!
//! ## Decoding
//! The primary interface is the [`Decoder`], which is a sans-IO style decoder.
//! [`TokenIterator`] is offered as a convenience on top of [`Decoder`] when
//! message data is available as a slice.
//!
//! ## Encoding
//! Currently the only encoding interface is [`encode_token`], which writes a
//! single [`Token`] as bytes to a mutable [`Vec`].
//!
//! [1]: https://www.rfc-editor.org/info/rfc8010/

mod cursor;
mod decoder;
mod encoder;
mod error;
mod span;
mod token;
mod token_iter;

pub use self::{
  cursor::*, decoder::*, encoder::*, error::*, span::*, token::*, token_iter::*,
};
