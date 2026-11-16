use bytes::Bytes;
use ipp_wire::{DecodedToken, DelimiterTag};

use super::{
  Attr, Edit, Fate, Group, Insert, Payload, Value, headed_attribute,
};
use crate::{AttrRef, GroupRef, Header, ValueRef};

impl<'m> Edit<'m> {
  /// Overwrites the header (or sets if not present).
  pub fn set_header(&mut self, header: Header) { self.header = Some(header); }

  /// Add a new attribute group to the message.
  pub fn push_group(&mut self, group: Group) {
    let tokens = &self.msg.0.tokens;

    // whether or not the message ends with an end-of-attributes-tag
    let well_formed_end = matches!(
      tokens.last(),
      Some(DecodedToken::Delimiter { tag })
        if tag.value == DelimiterTag::END_OF_ATTRIBUTES_TAG
    );

    // place the group before the end tag if it's there
    let pos = tokens.len() - usize::from(well_formed_end);

    self.inserts.push(Insert {
      pos:     pos as u32,
      payload: Payload::Group(group),
    });
  }

  /// Inserts a new group before the given group.
  pub fn insert_group_before(&mut self, anchor: GroupRef<'m>, group: Group) {
    self.check(anchor.msg);
    self.inserts.push(Insert {
      pos:     anchor.row.tokens.start,
      payload: Payload::Group(group),
    });
  }

  /// Removes a group and all its attributes.
  pub fn remove_group(&mut self, group: GroupRef<'m>) {
    self.check(group.msg);
    self.group_removals.push(group);
  }

  /// Pushes an attribute at the end of the given group.
  pub fn push_attr(&mut self, group: GroupRef<'m>, attr: Attr) {
    self.check(group.msg);
    self.inserts.push(Insert {
      pos:     group.row.tokens.end,
      payload: Payload::Top(attr),
    });
  }

  /// Pushes a member attribute to the given collection.
  pub fn push_member(&mut self, collection: ValueRef<'m>, attr: Attr) {
    self.check(collection.msg);
    // the value must be a collection, otherwise the message would be malformed.
    assert!(collection.is_collection(), "not a collection");

    let msg = collection.msg;
    // the last member's end, not `tokens.end - 1`, as the collection may be
    // unclosed
    let pos = collection
      .row
      .members
      .last()
      .map_or(collection.row.tokens.start + 1, |m| {
        msg.attrs[m.0 as usize].tokens.end
      });
    self.inserts.push(Insert {
      pos,
      payload: Payload::Member(attr),
    });
  }

  /// Inserts an attribute, referencing the anchor for top-level vs. member form
  fn neighbour(anchor: AttrRef<'m>, pos: u32, attr: Attr) -> Insert {
    Insert {
      pos,
      payload: if headed_attribute(anchor) {
        Payload::Top(attr)
      } else {
        Payload::Member(attr)
      },
    }
  }

  /// Inserts an attribute before the given attribute.
  pub fn insert_before(&mut self, anchor: AttrRef<'m>, attr: Attr) {
    self.check(anchor.msg);
    let i = Self::neighbour(anchor, anchor.row.tokens.start, attr);
    self.inserts.push(i);
  }

  /// Inserts an attribute after the given attribute.
  pub fn insert_after(&mut self, anchor: AttrRef<'m>, attr: Attr) {
    self.check(anchor.msg);
    let i = Self::neighbour(anchor, anchor.row.tokens.end, attr);
    self.inserts.push(i);
  }

  /// Removes the given attribute.
  pub fn remove(&mut self, attr: AttrRef<'m>) {
    let slot = self.slot(attr);
    slot.fates.fill_with(|| Fate::Drop);
    slot.pushed.clear();
    slot.removed = true;
  }

  /// Sets the name of the given attribute.
  pub fn rename(&mut self, attr: AttrRef<'m>, name: impl Into<Bytes>) {
    self.slot(attr).name = Some(name.into());
  }

  /// Sets the values of the given attribute, dropping all previous values.
  pub fn set_values(
    &mut self,
    attr: AttrRef<'m>,
    values: impl IntoIterator<Item = Value>,
  ) {
    let slot = self.slot(attr);
    slot.fates.fill_with(|| Fate::Drop);
    slot.pushed = values.into_iter().collect();
  }

  /// Replaces the value at the given index within the given attribute.
  pub fn replace_value(
    &mut self,
    attr: AttrRef<'m>,
    index: usize,
    value: Value,
  ) {
    self.slot(attr).fates[index] = Fate::Replace(value);
  }

  /// Removes the value at the given index within the given attribute.
  pub fn remove_value(&mut self, attr: AttrRef<'m>, index: usize) {
    self.slot(attr).fates[index] = Fate::Drop;
  }

  /// Uses a closure to determine whether to retain or drop values of the given
  /// attribute. If the closure returns `true` for a value, that value will be
  /// retained.
  pub fn retain_values(
    &mut self,
    attr: AttrRef<'m>,
    mut keep: impl FnMut(ValueRef<'m>) -> bool,
  ) {
    let slot = self.slot(attr);
    for (i, value) in attr.values().enumerate() {
      if !keep(value) {
        slot.fates[i] = Fate::Drop;
      }
    }
  }

  /// Pushes a value at the end of the given attribute.
  pub fn push_value(&mut self, attr: AttrRef<'m>, value: Value) {
    self.slot(attr).pushed.push(value);
  }
}
