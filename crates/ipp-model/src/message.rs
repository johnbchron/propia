use std::{range::Range, sync::Arc};

use ipp_wire::{DecodedToken, DecodedTokenIdx, DelimiterTag, ValueTag};

use crate::{Diagnostic, Kind, buffer::Buffer};

pub struct Message(Arc<Inner>);

struct Inner {
  buffer:      Buffer,
  tokens:      Box<[DecodedTokenIdx]>,
  groups:      Box<[GroupRow]>,
  attrs:       Box<[AttrRow]>,
  values:      Box<[ValueRow]>,
  diagnostics: Box<[Diagnostic]>,
}

impl Message {
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
}

#[derive(Clone, Copy)]
struct GroupIdx(u32);

#[derive(Clone, Copy)]
struct AttrIdx(u32);

#[derive(Clone, Copy)]
struct ValueIdx(u32);

struct GroupRow {
  tokens: Range<u32>,
  attrs:  Box<[AttrIdx]>,
}

struct AttrRow {
  tokens: Range<u32>,
  values: Box<[ValueIdx]>,
}

struct ValueRow {
  tokens:  Range<u32>,
  members: Box<[AttrIdx]>,
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
