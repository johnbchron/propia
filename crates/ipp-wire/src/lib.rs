#![warn(missing_docs)]

//! # ipp-wire
//! `ipp-wire` aides in decoding and encoding messages in the IPP wire format
//! described by [RFC 8010][1], specifically the conversion between raw bytes
//! and collections of tokens. All decoded data is spanned.
//!
//! A [`Token`]/[`DecodedToken`] is loosely defined as a small flat chunk of
//! bytes within an IPP message which can be intrensically identified, i.e.
//! based on its own data, rather than its surroundings or context. Further
//! contextual and stateful interpretation of tokens is needed to gain full [RFC
//! 8010][1] semantics. A [`DecodedToken`] contains spans while a [`Token`] does
//! not, but it can be created arbitrarily.
//!
//! ## Decoding
//! The primary interface is the [`Decoder`], which is a sans-IO style decoder.
//! [`DecodeIterator`] is offered as a convenience on top of [`Decoder`] when
//! message data is available as a slice.
//!
//! ## Encoding
//! Encode [`Token`]s with [`encode_token`]/[`encoded_len`], which is also
//! sans-IO. [`encode_token_to_vec`] is provided as a convenience.
//!
//! [1]: https://www.rfc-editor.org/info/rfc8010/

mod cursor;
mod decode_iter;
mod decoder;
mod encoder;
mod error;
mod span;
mod token;

pub use self::{
  cursor::*, decode_iter::*, decoder::*, encoder::*, error::*, span::*,
  token::*,
};
