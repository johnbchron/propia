#![warn(missing_docs)]

//! # `ipp-wire` - An RFC 8010 tokenizer (and encoder)
//!
//! `ipp-wire` aides in decoding and encoding messages in the **IPP wire
//! format** described by [RFC 8010][1], specifically the conversion between raw
//! bytes and collections of tokens. Further contextual and stateful
//! interpretation of tokens is needed to gain full [RFC 8010][1] semantics. All
//! decoded data is spanned.
//!
//! A [`Token`]/[`DecodedToken`] is loosely defined as a small contiguous chunk
//! of bytes within an IPP message which can be intrensically identified, i.e.
//! based on its own data, rather than its surroundings or context. A
//! [`DecodedToken`] contains [`Span`]s while a [`Token`] does not, but that
//! means an arbitrary [`Token`] can be created more easily.
//!
//! ## Decoding
//! The primary decoding interface is the [`Decoder`], which is a sans-IO style
//! decoder. This means that the decoder is a state machine that will yield
//! tokens when fed bytes. This makes it possible to easily write a variety of
//! adapters on top of it, including async or streaming interfaces.
//!
//! [`DecodeIterator`] is offered as a convenience adapter on top of [`Decoder`]
//! for when message data is already buffered.
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
