use ipp_wire::{DecodedToken, DelimiterTag, Span, ValueTag};

use super::{AttrIdx, AttrRow, GroupRow, Inner, Message, ValueIdx, ValueRow};
use crate::Diagnostic;

/// The fields of a message header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
  /// The version, as `(major, minor)`.
  pub version:             (u8, u8),
  /// The operation of a request or the status of a response.
  pub operation_or_status: u16,
  /// The request ID.
  pub request_id:          u32,
}

impl Message {
  /// Returns the header, unless the message was truncated before it finished.
  pub fn header(&self) -> Option<Header> {
    match self.0.tokens.first()? {
      DecodedToken::Header {
        version,
        operation_or_status,
        request_id,
        ..
      } => Some(Header {
        version:             (version.0.value, version.1.value),
        operation_or_status: operation_or_status.value,
        request_id:          request_id.value,
      }),
      _ => None,
    }
  }

  /// Iterates through the attribute groups, in message order.
  pub fn groups(&self) -> impl ExactSizeIterator<Item = GroupRef<'_>> {
    let msg = &*self.0;
    msg.groups.iter().map(move |row| GroupRef { msg, row })
  }

  /// Iterates over the groups with a given tag. A response can hold several,
  /// such as one job group per job.
  pub fn groups_tagged(
    &self,
    tag: DelimiterTag,
  ) -> impl Iterator<Item = GroupRef<'_>> {
    self.groups().filter(move |g| g.tag() == Some(tag))
  }

  /// Returns the problems found while structuring the message.
  #[must_use]
  pub fn diagnostics(&self) -> &[Diagnostic] { &self.0.diagnostics }
}

/// An isolated [`DecodedTokenIdx::Value`](ipp_wire::DecodedTokenIdx::Value).
struct ValueToken {
  tag:   ValueTag,
  name:  Span,
  value: Span,
}

impl Inner {
  /// Extracts a [`ValueToken`] from the message token stream. Only call on
  /// `Value` tokens.
  fn value_token(&self, i: u32) -> ValueToken {
    match self.tokens[i as usize] {
      DecodedToken::Value {
        tag, name, value, ..
      } => ValueToken {
        tag:   tag.value,
        name:  name.span,
        value: value.span,
      },
      _ => panic!("value_token() was called on a token that was not a value"),
    }
  }

  /// Produces an [`AttrRef`] view.
  fn attr(&self, index: AttrIdx) -> AttrRef<'_> {
    AttrRef {
      msg: self,
      row: &self.attrs[index.0 as usize],
    }
  }

  fn value(&self, index: ValueIdx) -> ValueRef<'_> {
    ValueRef {
      msg: self,
      row: &self.values[index.0 as usize],
    }
  }
}

/// A view of an attribute group.
#[derive(Clone, Copy)]
pub struct GroupRef<'m> {
  msg: &'m Inner,
  row: &'m GroupRow,
}

impl<'m> GroupRef<'m> {
  /// Returns the group's tag. An implicit group, opened by an attribute with no
  /// preceding delimiter, has none.
  #[must_use]
  pub fn tag(self) -> Option<DelimiterTag> {
    match self.msg.tokens[self.row.tokens.start as usize] {
      DecodedToken::Delimiter { tag } => Some(tag.value),
      _ => None,
    }
  }

  /// Iterates through the group's attributes, in message order.
  pub fn attrs(self) -> impl ExactSizeIterator<Item = AttrRef<'m>> {
    let msg = self.msg;
    self.row.attrs.iter().map(move |&index| msg.attr(index))
  }

  /// Returns the first attribute in the group with the given name.
  pub fn get(self, name: impl AsRef<[u8]>) -> Option<AttrRef<'m>> {
    self.get_all(name).next()
  }

  /// Iterates through all the attributes in the group with the given name.
  pub fn get_all(
    self,
    name: impl AsRef<[u8]>,
  ) -> impl Iterator<Item = AttrRef<'m>> {
    self.attrs().filter(move |a| a.name() == name.as_ref())
  }
}

/// A view of an attribute.
#[derive(Clone, Copy)]
pub struct AttrRef<'m> {
  msg: &'m Inner,
  row: &'m AttrRow,
}

impl<'m> AttrRef<'m> {
  pub fn name(self) -> &'m [u8] {
    let token = self.msg.value_token(self.row.tokens.start);

    if !token.name.is_empty() {
      self.msg.buffer.slice(token.name)
    } else if token.tag == ValueTag::MEMBER_ATTR_NAME {
      self.msg.buffer.slice(token.value)
    } else {
      &[]
    }
  }

  /// Iterates through the attribute's values, in message order.
  pub fn values(self) -> impl ExactSizeIterator<Item = ValueRef<'m>> {
    let msg = self.msg;
    self.row.values.iter().map(move |&index| msg.value(index))
  }

  /// Returns the value at this position.
  #[must_use]
  pub fn value(self, index: usize) -> Option<ValueRef<'m>> {
    let index = *self.row.values.get(index)?;
    Some(self.msg.value(index))
  }
}

/// A view of a value.
#[derive(Clone, Copy)]
pub struct ValueRef<'m> {
  msg: &'m Inner,
  row: &'m ValueRow,
}

impl<'m> ValueRef<'m> {
  /// Returns the value's tag.
  #[must_use]
  pub fn tag(self) -> ValueTag {
    self.msg.value_token(self.row.tokens.start).tag
  }

  /// Returns the raw value bytes, without any extension type prefix.
  #[must_use]
  pub fn bytes(self) -> &'m [u8] {
    let token = self.msg.value_token(self.row.tokens.start);
    self.msg.buffer.slice(token.value)
  }

  /// Indicates whether this value is a collection, which may be empty.
  #[must_use]
  pub fn is_collection(self) -> bool { self.tag() == ValueTag::BEG_COLLECTION }

  /// Iterates through a collection's members. Empty for anything else.
  pub fn members(self) -> impl ExactSizeIterator<Item = AttrRef<'m>> {
    let msg = self.msg;
    self.row.members.iter().map(move |&index| msg.attr(index))
  }

  /// Returns the first member with this name.
  pub fn member(self, name: impl AsRef<[u8]>) -> Option<AttrRef<'m>> {
    self.members().find(|a| a.name() == name.as_ref())
  }
}
