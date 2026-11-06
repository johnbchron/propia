use std::{cmp::Reverse, collections::BTreeMap};

use bytes::Bytes;
use ipp_wire::{
  DecodedToken, DelimiterTag, EncodeError, Token, ValueTag,
  encode_token_spanned,
};

use crate::{AttrRef, GroupRef, Header, Message, ValueRef, message::Inner};

/// A flat run of synthesized tokens.
type Run = Vec<Token<Bytes>>;

/// Shorthand for constructing an owned `Token`.
fn tok(tag: ValueTag, name: Bytes, value: Bytes) -> Token<Bytes> {
  Token::Value { tag, name, value }
}

/// An owned IPP Value.
#[derive(Clone, Debug)]
pub struct Value {
  run: Run,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTag(pub ValueTag);

impl Value {
  /// Builds a scalar IPP Value from bytes.
  pub fn scalar(
    tag: ValueTag,
    bytes: impl Into<Bytes>,
  ) -> Result<Self, InvalidTag> {
    match tag {
      ValueTag::BEG_COLLECTION
      | ValueTag::END_COLLECTION
      | ValueTag::MEMBER_ATTR_NAME
      | ValueTag::Standard(0x00..=0x0F | 0x7F) => Err(InvalidTag(tag)),
      _ => Ok(Self::trusted(tag, bytes.into())),
    }
  }

  /// Helper for building a nameless value.
  fn trusted(tag: ValueTag, bytes: Bytes) -> Self {
    Self {
      run: vec![tok(tag, Bytes::new(), bytes)],
    }
  }

  /// Builds an integer value.
  pub fn integer(v: i32) -> Self {
    Self::trusted(ValueTag::INTEGER, Bytes::copy_from_slice(&v.to_be_bytes()))
  }

  /// Builds a keyword value.
  pub fn keyword(v: impl Into<Bytes>) -> Self {
    Self::trusted(ValueTag::KEYWORD, v.into())
  }

  /// Builds a collection value from an iterator of owned attributes.
  pub fn collection(members: impl IntoIterator<Item = Attr>) -> Self {
    let mut run =
      vec![tok(ValueTag::BEG_COLLECTION, Bytes::new(), Bytes::new())];

    for member in members {
      member.emit_as_member(&mut run);
    }

    run.push(tok(ValueTag::END_COLLECTION, Bytes::new(), Bytes::new()));

    Self { run }
  }

  /// Emits the value into the given run with the given name.
  fn emit(self, name: &Bytes, out: &mut Run) {
    let mut tokens = self.run.into_iter();

    match tokens.next() {
      Some(Token::Value { tag, value, .. }) => {
        out.push(tok(tag, name.clone(), value));
      }
      _ => unreachable!("a value starts with a value token"),
    }

    out.extend(tokens);
  }
}

/// An owned IPP Attribute.
#[derive(Clone, Debug)]
pub struct Attr {
  name:   Bytes,
  values: Vec<Value>,
}

impl Attr {
  /// Builds an IPP Attribute.
  pub fn new(name: impl Into<Bytes>, first: Value) -> Self {
    Self {
      name:   name.into(),
      values: vec![first],
    }
  }

  /// Adds a value to the attribute.
  #[must_use]
  pub fn with(mut self, value: Value) -> Self {
    self.values.push(value);
    self
  }

  /// Emits the attribute into the given run as a top-level attribute.
  fn emit_as_top(self, out: &mut Run) -> Result<(), EditError> {
    if self.name.is_empty() {
      return Err(EditError::EmptyName);
    }

    // only emit the name for the first value
    let mut name = self.name;
    for value in self.values {
      value.emit(&name, out);
      name = Bytes::new();
    }

    Ok(())
  }

  /// Emits the attribute into the given run as a member attribute.
  fn emit_as_member(self, out: &mut Run) {
    out.push(tok(ValueTag::MEMBER_ATTR_NAME, Bytes::new(), self.name));
    for value in self.values {
      value.emit(&Bytes::new(), out);
    }
  }
}

/// An owned IPP Attribute Group.
#[derive(Clone, Debug)]
pub struct Group {
  tag:   DelimiterTag,
  attrs: Vec<Attr>,
}

impl Group {
  /// Builds an IPP Attribute Group.
  pub fn new(tag: DelimiterTag) -> Self {
    Self {
      tag,
      attrs: Vec::new(),
    }
  }

  /// Adds an attribute to the group.
  #[must_use]
  pub fn with(mut self, attr: Attr) -> Self {
    self.attrs.push(attr);
    self
  }

  /// Emits the group into the given run.
  fn emit(self, out: &mut Run) -> Result<(), EditError> {
    if self.tag == DelimiterTag::END_OF_ATTRIBUTES_TAG || self.tag.0 > 0x0F {
      return Err(EditError::InvalidGroupTag(self.tag));
    }

    out.push(Token::Delimiter { tag: self.tag });

    self.attrs.into_iter().try_for_each(|a| a.emit_as_top(out))
  }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
  Encode(EncodeError),
  EmptyName,
  InvalidGroupTag(DelimiterTag),
  MissingHeader,
}

impl From<EncodeError> for EditError {
  fn from(e: EncodeError) -> Self { Self::Encode(e) }
}

/// What happens to one value of the base message.
enum Fate {
  Keep,
  Drop,
  Replace(Value),
}

/// All changes to one attribute.
struct AttrEdit<'m> {
  /// The attribute in the original message being edited.
  attr:    AttrRef<'m>,
  /// The set name of the attribute.
  name:    Option<Bytes>,
  /// The fate of every original value, whether kept, dropped, or replaced.
  fates:   Vec<Fate>,
  /// Values pushed to this attribute (pushed values go at the end).
  pushed:  Vec<Value>,
  /// Whether the attribute was removed.
  removed: bool,
}

/// The impact of an edit, ranked smallest to biggest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
  Value,
  Attr,
  Group,
}

/// The data payload of something being inserted.
enum Payload {
  Top(Attr),
  Member(Attr),
  Group(Group),
}

struct Insert {
  pos:     u32,
  payload: Payload,
}

/// Replaces base tokens `start..end` with `run`. If the range is empty, the
/// splice is an insertion before base token `start`.
struct Splice {
  start: u32,
  end:   u32,
  /// For an insert, what it adds. For a replacement, what it takes out; a
  /// replacement of part of a value or attribute (such as the token carrying
  /// a name) counts as a value, which owns nothing at its end.
  rank:  Rank,
  run:   Run,
}

impl Splice {
  fn replace(start: u32, end: u32, rank: Rank, run: Run) -> Self {
    debug_assert!(start < end, "a replacement covers at least one token");
    Self {
      start,
      end,
      rank,
      run,
    }
  }

  fn insert(pos: u32, rank: Rank, run: Run) -> Self {
    Self {
      start: pos,
      end: pos,
      rank,
      run,
    }
  }

  fn is_insert(&self) -> bool { self.start == self.end }
}

#[must_use = "edits do nothing until committed"]
pub struct Edit<'m> {
  msg:            &'m Message,
  header:         Option<Header>,
  attr_edits:     BTreeMap<u32, AttrEdit<'m>>,
  group_removals: Vec<GroupRef<'m>>,
  inserts:        Vec<Insert>,
}

impl Message {
  /// Start a batch of edits against this message.
  pub fn edit(&self) -> Edit<'_> {
    Edit {
      msg:            self,
      header:         None,
      attr_edits:     BTreeMap::new(),
      group_removals: Vec::new(),
      inserts:        Vec::new(),
    }
  }
}

impl<'m> Edit<'m> {
  fn check(&self, msg: &Inner) {
    assert!(
      std::ptr::eq(msg, &*self.msg.0),
      "view belongs to a different message"
    );
  }

  /// Open an edit against a given attribute.
  fn slot(&mut self, attr: AttrRef<'m>) -> &mut AttrEdit<'m> {
    // verify the origin
    self.check(attr.msg);

    // fetch mut or insert an AttrEdit for this attribute
    self
      .attr_edits
      .entry(attr.row.tokens.start)
      .or_insert_with(|| AttrEdit {
        attr,
        name: None,
        fates: attr.row.values.iter().map(|_| Fate::Keep).collect(),
        pushed: Vec::new(),
        removed: false,
      })
  }

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

  /// Commits all the edits into a new full message.
  pub fn commit(self) -> Result<Message, EditError> {
    let msg = &*self.msg.0;

    // prepare to collect splices
    let mut splices = Vec::new();

    // for each group to remove, create an empty replacement splice
    for group in self.group_removals {
      let t = group.row.tokens;
      splices.push(Splice::replace(t.start, t.end, Rank::Group, Vec::new()));
    }

    // collect splices for insertions
    for insert in self.inserts {
      let mut run = Vec::new();
      // collect the rank and output the new tokens
      let rank = match insert.payload {
        Payload::Top(attr) => {
          attr.emit_as_top(&mut run)?;
          Rank::Attr
        }
        Payload::Member(attr) => {
          attr.emit_as_member(&mut run);
          Rank::Attr
        }
        Payload::Group(group) => {
          group.emit(&mut run)?;
          Rank::Group
        }
      };
      splices.push(Splice::insert(insert.pos, rank, run));
    }

    // produce splices from attribute edits
    for edit in self.attr_edits.into_values() {
      edit.lower(&mut splices)?;
    }

    // set splice order, which controls which splices overwrite each other.
    // ordering rules:
    // - at one position: inserts before the base token there, smallest first
    //   and otherwise in call order
    // - then replacements, widest first, so an outer replacement is applied and
    //   the ones nested in it are skipped.
    splices.sort_by_key(|s| (s.start, !s.is_insert(), Reverse(s.end), s.rank));

    let base = msg.buffer.end();
    let mut fresh = Vec::new();
    let mut tokens = Vec::with_capacity(msg.tokens.len());
    let mut pos = 0;

    // the header always comes first
    if let Some(h) = self.header {
      let header = Token::<&[u8]>::Header {
        version:             h.version,
        operation_or_status: h.operation_or_status,
        request_id:          h.request_id,
      };
      tokens.push(encode_token_spanned(&header, base, &mut fresh)?);
      pos = u32::from(!msg.tokens.is_empty());
    }

    // where the last replacement ended, and what it took out
    let mut removed: Option<(u32, Rank)> = None;

    for splice in splices {
      let within = splice.start < pos;
      let owned = splice.is_insert()
        && removed
          .is_some_and(|(end, rank)| splice.start == end && splice.rank < rank);
      if within || owned {
        continue;
      }

      tokens
        .extend_from_slice(&msg.tokens[pos as usize..splice.start as usize]);
      for token in &splice.run {
        tokens.push(encode_token_spanned(token, base, &mut fresh)?);
      }
      if !splice.is_insert() {
        removed = Some((splice.end, splice.rank));
      }
      pos = splice.end;
    }
    tokens.extend_from_slice(&msg.tokens[pos as usize..]);

    // anything before a header would be read back as the header
    if !matches!(tokens.first(), None | Some(DecodedToken::Header { .. })) {
      return Err(EditError::MissingHeader);
    }

    Ok(Message::build(msg.buffer.append(fresh.into()), tokens))
  }
}

/// Whether the attribute and its first value start on the same token, which
/// means the first value would carry the name. If not, it's a member token.
fn headed_attribute(attr: AttrRef<'_>) -> bool {
  attr.row.values.first().is_some_and(|v| {
    attr.row.tokens.start == attr.msg.values[v.0 as usize].tokens.start
  })
}

impl AttrEdit<'_> {
  /// Lowers `self` into one or more splices in `out`.
  fn lower(self, out: &mut Vec<Splice>) -> Result<(), EditError> {
    let msg = self.attr.msg;
    let row = self.attr.row;

    // when to remove the attribute altogether
    let none_pushed = self.pushed.is_empty();
    let all_dropped = self.fates.iter().all(|f| matches!(f, Fate::Drop));
    let removed = self.removed;
    let had_values = !self.fates.is_empty();
    if none_pushed && all_dropped && (removed || had_values) {
      out.push(Splice::replace(
        row.tokens.start,
        row.tokens.end,
        Rank::Attr,
        Vec::new(),
      ));
      return Ok(());
    }

    // bail on a present but empty name
    if self.name.as_ref().is_some_and(Bytes::is_empty) {
      return Err(EditError::EmptyName);
    }

    // whether to set the name
    let renamed = self.name.is_some();
    let name = self
      .name
      .unwrap_or_else(|| Bytes::copy_from_slice(self.attr.name()));

    // if the name is a member attribute (name is not on the first value),
    // splice the memberAttrName token with the correct name
    let headed = headed_attribute(self.attr);
    if renamed && !headed {
      let start = row.tokens.start;
      out.push(Splice::replace(start, start + 1, Rank::Value, vec![tok(
        ValueTag::MEMBER_ATTR_NAME,
        Bytes::new(),
        name.clone(),
      )]));
    }

    // prepare to set the name on the value if not a member attribute
    let mut pending = headed;
    // closure to put the name on the first value, and indicate if it was set
    let mut label = |out_name: &mut Bytes| {
      *out_name = if pending { name.clone() } else { Bytes::new() };
      std::mem::replace(&mut pending, false)
    };

    // work through the existing attribute values
    for (index, fate) in row.values.iter().zip(self.fates) {
      // the token range of the current value
      let t = msg.values[index.0 as usize].tokens;

      match fate {
        Fate::Keep => {
          let mut wanted = Bytes::new();
          // move on if the name was not set
          if !label(&mut wanted) {
            continue;
          }
          // splice the token if necessary
          let token = msg.value_token(t.start);
          if msg.buffer.slice(token.name) != &*wanted {
            let value = msg.buffer.slice(token.value);
            out.push(Splice::replace(t.start, t.start + 1, Rank::Value, vec![
              tok(token.tag, wanted, Bytes::copy_from_slice(value)),
            ]));
          }
        }
        Fate::Drop => {
          out.push(Splice::replace(t.start, t.end, Rank::Value, Vec::new()));
        }
        Fate::Replace(value) => {
          let mut wanted = Bytes::new();
          label(&mut wanted);
          let mut run = Vec::new();
          value.emit(&wanted, &mut run);
          out.push(Splice::replace(t.start, t.end, Rank::Value, run));
        }
      }
    }

    // emit any pushed values and splice them in if necessary
    let mut run = Vec::new();
    for value in self.pushed {
      let mut wanted = Bytes::new();
      label(&mut wanted);
      value.emit(&wanted, &mut run);
    }
    if !run.is_empty() {
      out.push(Splice::insert(row.tokens.end, Rank::Value, run));
    }

    Ok(())
  }
}
