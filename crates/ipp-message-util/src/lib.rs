#![deny(missing_docs)]

//! # `ipp-message-util` - Streaming adapters for IPP messages
//!
//! On the wire, an IPP request or response is a [`Message`] followed by job
//! data, and the job data can be far too large to buffer. This crate reads the
//! [`Message`] from the front of a stream, leaves the job data in the stream,
//! and joins the two again on the way out.
//!
//! - [`Inbound`] is a [`Message`] read from a stream, plus the rest of the
//!   stream.
//! - [`Split`] is a [`Message`] separated from its job data, so that it can be
//!   edited.
//! - [`Outbound`] is a byte stream of a [`Message`] followed by its job data.
//!
//! Here is an example of editing a message as it passes through:
//! ```
//! use std::convert::Infallible;
//!
//! use bytes::Bytes;
//! use futures::{TryStreamExt, executor::block_on, stream};
//! use ipp_message::{
//!   Limits,
//!   edit::{Attr, Value},
//! };
//! use ipp_message_util::inbound::Inbound;
//!
//! const MESSAGE: &[u8] =
//!   include_bytes!("../../../corpora/ipp-messages-initial/good/1.ipp");
//! const JOB_DATA: &[u8] = b"%PDF-1.7";
//!
//! # block_on(async {
//! // a stream of chunks, which don't need to line up with anything
//! let body = [MESSAGE, JOB_DATA].concat();
//! let chunks: Vec<_> = body
//!   .chunks(100)
//!   .map(|c| Ok::<_, Infallible>(Bytes::copy_from_slice(c)))
//!   .collect();
//! let body = stream::iter(chunks);
//!
//! // read the message, leaving the job data in the stream
//! let inbound = Inbound::read(body, Limits::default()).await.unwrap();
//!
//! // edit the message
//! let mut split = inbound.split().unwrap();
//! split
//!   .edit(|message, edit| {
//!     let op_attrs = message.groups().next().unwrap();
//!     edit.push_attr(op_attrs, Attr::new("copies", Value::integer(2)));
//!   })
//!   .unwrap();
//!
//! // stream out the edited message and the untouched job data
//! let outbound = split.into_outbound();
//! let bytes = outbound.try_collect::<Vec<_>>().await.unwrap().concat();
//! assert!(bytes.ends_with(JOB_DATA));
//! # });
//! ```
//!
//! ## Streams
//!
//! Any [`TryStream`] can be read from as long as it's [`Unpin`] and its items
//! implement [`Buf`]. Streams that aren't [`Unpin`] can be wrapped with
//! [`Box::pin`]. The error type can be anything, and is passed through.
//!
//! The streams this crate produces all yield [`Bytes`]. Job data that arrives
//! as [`Bytes`] in chunks after the message is passed through without copying.
//!
//! [`Buf`]: bytes::Buf
//! [`Bytes`]: bytes::Bytes
//! [`Inbound`]: inbound::Inbound
//! [`Message`]: ipp_message::Message
//! [`Outbound`]: outbound::Outbound
//! [`Split`]: inbound::Split
//! [`TryStream`]: futures_core::TryStream

pub mod inbound;
pub mod outbound;

mod chain;
