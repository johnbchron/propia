mod debug;
mod view;

use std::{range::Range, sync::Arc};

use ipp_wire::{DecodedToken, DecodedTokenIdx, DelimiterTag, ValueTag};

pub use self::view::*;
use crate::{Assembled, Assembler, Diagnostic, Kind, Limits, buffer::Buffer};

/// An IPP message, structurally decoded.
///
/// `Message`s are produced by the [`Assembler`](crate::Assembler). See its docs
/// for how to produce a message from bytes.
///
/// ```
/// use std::convert::identity;
/// use ipp_message::{Assembled, AttrRef, GroupRef, Limits, Message, ValueRef, EndState};
/// use ipp_wire::DelimiterTag;
///
/// const BYTES: &[u8] =
///   include_bytes!("../../../corpora/ipp-messages-initial/good/1.ipp");
///
/// // quickly assemble a message
/// let Assembled { message, end, .. } = Message::from_slice(BYTES, Limits::default());
/// assert!(matches!(end, EndState::Complete));
///
/// // iterator over attribute groups
/// let mut groups = message.groups();
///
/// // get the first attribute group
/// let op_attrs: GroupRef = groups.next().unwrap();
/// assert_eq!(op_attrs.tag(), Some(DelimiterTag::OPERATION_ATTRIBUTES_TAG));
/// // look at an attribute within the group
/// let charset_attr: AttrRef = op_attrs.get(b"attributes-charset").unwrap();
/// // attributes can have multiple values
/// let charset_val: ValueRef = charset_attr.value(0).unwrap();
/// assert_eq!(charset_val.bytes(), b"utf-8");
///
/// // we want to assert that there is an entry in `media-col` that is at least
/// // 21.5cm wide and 27cm tall
///
/// // this verifier takes a collection value, returning None if an entry is malformed
/// // the entry we're looking for looks like this:
/// // [v]: collection {
/// //   media-size: collection {
/// //     x-dimension: integer
/// //     y-dimension: integer
/// //   }
/// // }
/// fn eligible_media(v: ValueRef) -> Option<bool> {
///   let media_size_attr = v.member(b"media-size")?;
///   let media_size_col = media_size_attr.value(0)?;
///
///   let x_attr = media_size_col.member(b"x-dimension")?;
///   let x_val = x_attr.value(0)?;
///   let x_bytes: [u8; 4] = x_val.bytes().try_into().ok()?;
///   let x = i32::from_be_bytes(x_bytes);
///   // a `dimension` type is in hundredths of a millimeter
///   if x < 21500 { return Some(false); }
///
///   let y_attr = media_size_col.member(b"y-dimension")?;
///   let y_val = y_attr.value(0)?;
///   let y_bytes: [u8; 4] = y_val.bytes().try_into().ok()?;
///   let y = i32::from_be_bytes(y_bytes);
///   if y < 27000 { return Some(false); }
///
///   Some(true)
/// }
///
/// // find the first `job-attributes` group
/// let job_attrs_group = message.groups_tagged(DelimiterTag::JOB_ATTRIBUTES_TAG).next().unwrap();
/// let media_col_attr = job_attrs_group.get(b"media-col").unwrap();
/// let eligible_media_type = media_col_attr.values()
///   // filter to just eligible media
///   .filter(|v| eligible_media(*v) == Some(true))
///   // filter and map for media that has media-type
///   .filter_map(|v| v.member(b"media-type")?.value(0).map(|v| v.bytes()))
///   .next();
/// assert_eq!(eligible_media_type, Some(b"plain".as_ref()));
/// ```
///
/// # Characteristics of `Message`
///
/// ## Minimal & Permissive Decoding
/// `Message` is only "structurally" decoded, which means that the minimum
/// amount of work is done to understand the message, but enough to move around
/// the attribute data tree. Tokens are grouped into groups, attributes, and
/// values, but data in names and values is not decoded or checked.
///
/// This minimal decoding is for two reasons:
/// 1. To make decoding as **fast** and **efficient** as possible.
/// 2. To make decoding as **permissive** as possible, allowing the widest range
///    of both well-formed and malformed messages to be decoded.
///
/// `ipp-message` makes as few assumptions about the correctness or value
/// semantics of a given message as possible, to allow usage with the widest
/// selection of clients, printers, and applications.
///
/// ## Assembling is a Total Function
/// Assembling is a *total function*, meaning it is made to always succeed on
/// any number and selection of bytes (up to your set
/// [`Limits`](crate::Limits), which ends assembling with
/// [`EndState::LimitExceeded`](crate::EndState)).
///
/// Assembling even succeeds on zero bytes. If there are too few bytes for the
/// header, [`Message::header`] will return `None`. If token decoding fails at
/// some point (provided by [`ipp_wire::Decoder`]), assembling will stop there
/// and a [`DecodeError`](ipp_wire::DecodeError) will be returned
/// within [`EndState`](crate::EndState) alongside the `Message` constructed
/// from the already-decoded tokens.
///
/// That is not to say, however, that a sequence of bytes cannot be considered
/// malformed when interpreted as an IPP message; for all the various
/// *structural* defects that such a message can have, [`Diagnostic`]s are
/// returned by the [`Message::diagnostics`] method, but importantly none of
/// these are fatal to assembling.
///
/// ## The Data Tree and Copy-Free Access
///
/// An IPP message is composed of a header, a number of attribute groups, an
/// `end-of-attributes` tag, and then job data. Attribute groups are accessed by
/// [`Message::groups`] or [`Message::groups_tagged`], yielding [`GroupRef`].
/// Each attribute group contains zero or more attributes, accessed by
/// [`GroupRef::get`] or [`GroupRef::attrs`], yielding [`AttrRef`]. Each
/// attribute can contain zero or more values, accessed by [`AttrRef::value`] or
/// [`AttrRef::values`], yielding [`ValueRef`]. Values are usually the bottom of
/// the tree, but they can also be a *collection value*, meaning they contain a
/// collection of attributes, accessed by [`ValueRef::member`] or
/// [`ValueRef::members`].
///
/// The `tokens()` method on any of these views return the token range they
/// cover. All the views borrow the message so that access is copy-free.
///
/// ## Cloning Behavior
///
/// `Message` is internally backed by [`Arc`], so cloning is cheap.
///
/// # Editing a message
/// `Message` itself is immutable; all edits to a `Message` starts with a call
/// to [`edit`](Message::edit), which returns an [`Edit`](crate::Edit).
/// [`Edit`](crate::Edit) borrows from `Message` and is a builder for a round of
/// edits that are executed by [`commit`](crate::Edit::commit), which produces a
/// new `Message`.

#[derive(Clone)]
pub struct Message(pub(crate) Arc<Inner>);

impl Message {
  /// Builds a [`Message`] from a buffer and its stream of tokens. This is
  /// infalliable given that the tokens have already been decoded, and if there
  /// are problems, a [`Diagnostic`] will be recorded.
  pub(crate) fn build(buffer: Buffer, tokens: Vec<DecodedTokenIdx>) -> Self {
    let builder = Builder::default().run(&tokens);

    Self(Arc::new(Inner {
      buffer,
      tokens: tokens.into_boxed_slice(),
      groups: builder.groups.into_boxed_slice(),
      attrs: builder.attrs.into_boxed_slice(),
      values: builder.values.into_boxed_slice(),
      diagnostics: builder.diagnostics.into_boxed_slice(),
    }))
  }

  /// [Assembles](crate::Assembler) a `Message` from a single slice.
  ///
  /// Builds an [`Assembler`] with `limits`, pushes the slice to it, and
  /// finalizes it.
  pub fn from_slice(slice: &[u8], limits: Limits) -> Assembled {
    match Assembler::new(limits).push(slice) {
      crate::Progress::More(assembler) => assembler.finish(),
      crate::Progress::Done(assembled) => assembled,
    }
  }
}

pub(crate) struct Inner {
  pub(crate) buffer: Buffer,
  pub(crate) tokens: Box<[DecodedTokenIdx]>,
  groups:            Box<[GroupRow]>,
  pub(crate) attrs:  Box<[AttrRow]>,
  pub(crate) values: Box<[ValueRow]>,
  diagnostics:       Box<[Diagnostic]>,
}

#[derive(Clone, Copy)]
struct GroupIdx(u32);

#[derive(Clone, Copy)]
pub(crate) struct AttrIdx(pub(crate) u32);

#[derive(Clone, Copy)]
pub(crate) struct ValueIdx(pub(crate) u32);

pub(crate) struct GroupRow {
  pub(crate) tokens: Range<u32>,
  attrs:             Box<[AttrIdx]>,
}

pub(crate) struct AttrRow {
  pub(crate) tokens: Range<u32>,
  pub(crate) values: Box<[ValueIdx]>,
}

pub(crate) struct ValueRow {
  pub(crate) tokens:  Range<u32>,
  pub(crate) members: Box<[AttrIdx]>,
}

#[derive(Default)]
struct Builder {
  groups:      Vec<GroupRow>,
  attrs:       Vec<AttrRow>,
  values:      Vec<ValueRow>,
  diagnostics: Vec<Diagnostic>,

  /// The current open group.
  group: Option<OpenGroup>,
  /// The innermost open attribute.
  attr:  Option<OpenAttr>,
  /// Currently open attributes.
  stack: Vec<Frame>,
}

impl Builder {
  /// Build the message structure using a slice of tokens.
  fn run(mut self, tokens: &[DecodedTokenIdx]) -> Self {
    // use zip to get u32s out of the box
    for (i, token) in (0u32..).zip(tokens) {
      match token {
        DecodedToken::Header { .. } if i == 0 => {}
        DecodedToken::Delimiter { tag } => self.delimiter(i, tag.value),
        DecodedToken::Value { tag, name, .. } => {
          self.value(i, tag.value, !name.span.is_empty());
        }
        _ => self.diag(i, Kind::UnexpectedToken),
      }
    }

    self.close_everything(tokens.len() as u32);
    self
  }

  /// Handle a delimiter tag token.
  fn delimiter(&mut self, i: u32, tag: DelimiterTag) {
    // tags either start a group or end the message, so any prior group closes
    self.close_everything(i);

    // if it's the end tag, we're already closed, but otherwise open a group
    if tag != DelimiterTag::END_OF_ATTRIBUTES_TAG {
      self.open_group(i);
    }
  }

  fn close_everything(&mut self, end: u32) {
    // the stack should be empty, but close every open frame
    while let Some(frame) = self.stack.last() {
      let start = self.values[frame.value.0 as usize].tokens.start;
      self.diag(start, Kind::UnclosedCollection);
      self.close_collection(end, end);
    }

    self.close_attr(end);
    self.close_group(end);
  }

  /// Handle a value token.
  fn value(&mut self, i: u32, tag: ValueTag, named: bool) {
    let nested = !self.stack.is_empty();

    // check first for control value tags
    match tag {
      // end the current collection
      ValueTag::END_COLLECTION if nested => {
        return self.close_collection(i, i + 1);
      }
      ValueTag::END_COLLECTION => self.diag(i, Kind::StrayEndCollection),

      // new member attribute of the current collection
      ValueTag::MEMBER_ATTR_NAME if nested => {
        // if named, the member attribute would be malformed
        if named {
          self.diag(i, Kind::NamedMemberAttrName);
        }
        // close the previous attribute and start the current one
        self.close_attr(i);
        return self.open_attr(i);
      }
      ValueTag::MEMBER_ATTR_NAME => {
        self.diag(i, Kind::MemberNameOutsideCollection)
      }

      // move on for anything else
      _ => {}
    }

    // determine the attribute to drop the value in
    if named {
      // way out of place
      if nested {
        self.diag(i, Kind::NamedValueInCollection);
      }

      // this is just a base attribute in current group
      self.close_attr(i);
      self.open_attr(i);
    } else if self.attr.is_none() {
      // continuation without a predecessor
      self.diag(i, Kind::ValueWithoutAttribute);
      self.open_attr(i);
    }

    // push the value and add it to the current attribute
    let row = self.push_value(i);
    let attr = self.attr.as_mut().expect("attr opened above");
    attr.values.push(row);

    // start a collection if needed
    if tag == ValueTag::BEG_COLLECTION {
      let outer = self.attr.take().expect("attr opened above");
      self.stack.push(Frame {
        value: row,
        outer,
        members: Vec::new(),
      });
    }
  }

  /// CLose the current collection, taking the end of the last attribute and the
  /// end collection token indices separately.
  fn close_collection(&mut self, attr_end: u32, value_end: u32) {
    self.close_attr(attr_end);

    // end the collection value
    let frame = self
      .stack
      .pop()
      .expect("caller checked for an open collection");
    let row = &mut self.values[frame.value.0 as usize];
    row.tokens.end = value_end;
    row.members = frame.members.into_boxed_slice();

    // reopen the outer attribute
    self.attr = Some(frame.outer);
  }

  /// Log a diagnostic.
  fn diag(&mut self, token: u32, kind: Kind) {
    self.diagnostics.push(Diagnostic { token, kind });
  }

  /// Opens a group.
  fn open_group(&mut self, i: u32) {
    // create a group
    let row = GroupIdx(self.groups.len() as u32);
    self.groups.push(GroupRow {
      tokens: Range { start: i, end: i },
      attrs:  Box::default(),
    });

    // set it as the current open group
    self.group = Some(OpenGroup {
      row,
      attrs: Vec::new(),
    });
  }

  /// Closes a group.
  fn close_group(&mut self, end: u32) {
    let Some(open) = self.group.take() else {
      return;
    };

    let row = &mut self.groups[open.row.0 as usize];
    row.tokens.end = end;
    row.attrs = open.attrs.into_boxed_slice();
  }

  /// Opens an attribute.
  fn open_attr(&mut self, i: u32) {
    debug_assert!(self.attr.is_none(), "caller should close the previous attr");

    // a collection must be in a group
    if self.group.is_none() {
      self.diag(i, Kind::AttributeBeforeGroup);
      self.open_group(i);
    }

    let row = AttrIdx(self.attrs.len() as u32);
    self.attrs.push(AttrRow {
      tokens: Range { start: i, end: i },
      values: Box::default(),
    });

    // ascribe it to the current collection if there is one, otherwise the group
    match self.stack.last_mut() {
      Some(frame) => frame.members.push(row),
      None => self
        .group
        .as_mut()
        .expect("group opened above")
        .attrs
        .push(row),
    }

    self.attr = Some(OpenAttr {
      row,
      values: Vec::new(),
    });
  }

  /// Closes an attribute.
  fn close_attr(&mut self, end: u32) {
    let Some(open) = self.attr.take() else {
      return;
    };

    let row = &mut self.attrs[open.row.0 as usize];
    row.tokens.end = end;
    row.values = open.values.into_boxed_slice();
  }

  /// Pushes a value, marking the given token and the next.
  fn push_value(&mut self, i: u32) -> ValueIdx {
    let row = ValueIdx(self.values.len() as u32);
    self.values.push(ValueRow {
      tokens:  Range {
        start: i,
        end:   i + 1,
      },
      members: Box::default(),
    });
    row
  }
}

struct OpenGroup {
  row:   GroupIdx,
  attrs: Vec<AttrIdx>,
}

struct OpenAttr {
  row:    AttrIdx,
  values: Vec<ValueIdx>,
}

struct Frame {
  /// The value row of the open collection
  value:   ValueIdx,
  /// The attribute to resume after the collection value closes
  outer:   OpenAttr,
  /// Members of the collection so far
  members: Vec<AttrIdx>,
}
