//! Contains types used for making edits to [`Message`]s.
//!
//! See the [`Edit`] docs for more info.

mod mutators;
mod owned;

use std::{cmp::Reverse, collections::BTreeMap, error::Error, fmt};

use bytes::Bytes;
use ipp_wire::{
  DecodedToken, DelimiterTag, EncodeError, Token, ValueTag,
  encode_token_to_vec_spanned,
};

pub use self::owned::*;
use crate::{AttrRef, GroupRef, Header, Message, message::Inner};

/// A flat run of synthesized tokens.
type Run = Vec<Token<Bytes>>;

/// Shorthand for constructing an owned `Token`.
fn tok(tag: ValueTag, name: Bytes, value: Bytes) -> Token<Bytes> {
  Token::Value { tag, name, value }
}

/// An error returned when trying to use an invalid value tag to create an owned
/// [`Value`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTag(pub ValueTag);

impl fmt::Display for InvalidTag {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "invalid value tag used to construct a value: {:?}",
      self.0
    )
  }
}

impl Error for InvalidTag {}

/// An error occurring when trying to commit a round of edits to a [`Message`].
#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
  /// Some failure occurred in encoding the edited tokens.
  Encode(EncodeError),
  /// An attribute name was present but empty.
  EmptyName,
  /// A group delimiter tag exceeded `0x0F`, or equalled `0x03`, the
  /// `end-of-attributes`.
  InvalidGroupTag(DelimiterTag),
  /// The first token of the new message was not the header. A message must
  /// either be completely empty or begin with a header.
  MissingHeader,
}

impl fmt::Display for EditError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      EditError::Encode(_) => {
        write!(f, "failed to encode edited bytes")
      }
      EditError::EmptyName => write!(f, "an edited attribute name is empty"),
      EditError::InvalidGroupTag(tag) => {
        write!(f, "a group delimiter tag exceeded 0x0F: {:x}", tag.0)
      }
      EditError::MissingHeader => {
        write!(f, "the first token of the new message was not a header")
      }
    }
  }
}

impl Error for EditError {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    match self {
      EditError::Encode(encode_error) => Some(encode_error),
      EditError::EmptyName
      | EditError::InvalidGroupTag(_)
      | EditError::MissingHeader => None,
    }
  }
}

impl From<EncodeError> for EditError {
  fn from(e: EncodeError) -> Self {
    Self::Encode(e)
  }
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
  attr: AttrRef<'m>,
  /// The set name of the attribute.
  name: Option<Bytes>,
  /// The fate of every original value, whether kept, dropped, or replaced.
  fates: Vec<Fate>,
  /// Values pushed to this attribute (pushed values go at the end).
  pushed: Vec<Value>,
  /// Whether the attribute was removed.
  removed: bool,
}

/// The impact of an edit, ranked smallest to biggest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
  pos: u32,
  payload: Payload,
}

/// Replaces base tokens `start..end` with `run`. If the range is empty, the
/// splice is an insertion before base token `start`.
struct Splice {
  start: u32,
  end: u32,
  /// For an insert, what it adds. For a replacement, what it takes out; a
  /// replacement of part of a value or attribute (such as the token carrying
  /// a name) counts as a value, which owns nothing at its end.
  rank: Rank,
  run: Run,
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

  fn is_insert(&self) -> bool {
    self.start == self.end
  }
}

/// A builder for a round of edits to a [`Message`].
///
/// `Edit` is used to edit [`Message`]s. It's returned by [`Message::edit`]
/// and stages a set of edits, which are computed when [`commit`] is called.
/// [`commit`] produces a new message, and the original is untouched.
///
/// ```
/// use ipp_message::{
///   Limits, Message,
///   edit::{Attr, Group, Value},
/// };
/// use ipp_wire::DelimiterTag;
///
/// const BYTES: &[u8] =
///   include_bytes!("../../../corpora/ipp-messages-initial/good/1.ipp");
///
/// let message = Message::from_slice(BYTES, Limits::default()).message;
/// let op_attrs = message.groups().next().unwrap();
/// let charset = op_attrs.get(b"attributes-charset").unwrap();
/// let language = op_attrs.get(b"attributes-natural-language").unwrap();
///
/// let mut edit = message.edit();
/// // edits refer to views of the original message...
/// edit.rename(charset, "my-charset");
/// edit.push_value(charset, Value::keyword("us-ascii"));
/// edit.remove(language);
/// edit.push_attr(op_attrs, Attr::new("copies", Value::integer(2)));
/// // ...and new data is built up front from owned types
/// edit.push_group(
///   Group::new(DelimiterTag::JOB_ATTRIBUTES_TAG)
///     .with(Attr::new("job-priority", Value::integer(50))),
/// );
/// let edited = edit.commit().unwrap();
///
/// // the original message is untouched
/// assert!(
///   message
///     .groups()
///     .next()
///     .unwrap()
///     .get(b"attributes-charset")
///     .is_some()
/// );
///
/// // the new message is edited
/// let op_attrs = edited.groups().next().unwrap();
/// assert_eq!(op_attrs.get(b"my-charset").unwrap().values().len(), 2);
/// assert!(op_attrs.get(b"attributes-natural-language").is_none());
/// assert!(op_attrs.get(b"copies").is_some());
/// ```
///
/// ## Edits Refer to the Original Message
///
/// Every anchor you pass to an edit -- [`GroupRef`], [`AttrRef`], [`ValueRef`],
/// and value indices -- refers to the original message the `Edit` was created
/// from. Edits are staged, and they are all applied at once at [`commit`].
/// Edits don't see the effects of other edits made against the same `Edit`,
/// meaning for example even after `edit.remove_value(attr, 0)`, index `1` still
/// means the second value in the attribute.
///
/// Similarly, data you add cannot be used as an anchor for other edits in the
/// same round. Build new data fully with the owned [`Value`], [`Attr`], and
/// [`Group`] types, or commit and start another round.
///
/// ## Combining Edits
///
/// Some rules for how edits combine:
/// - Edits to the same attribute combine: one attribute can be renamed, have a
///   value replaced, and have values pushed all in one round. Where edits
///   conflict, the later call wins. [`remove`] drops all of an attribute's
///   values; any later edit that gives it values ([`push_value`],
///   [`set_values`], [`replace_value`]) brings it back with only those values.
/// - An edit that leaves an attribute with no values removes it entirely
///   (attributes that had no values to begin with are kept).
/// - Removing a group with [`remove_group`] drops every other edit made inside
///   it, including attributes pushed to it. Inserts *next to* removed items are
///   kept: [`insert_before`] and [`insert_after`] still apply when their anchor
///   is removed.
/// - Multiple inserts at the same position are applied in call order with
///   values before attributes before groups.
///
/// ## Committing
///
/// [`commit`] is the only step that can fail; see [`EditError`]. For example,
/// pushing a group to an empty message without calling [`set_header`] fails
/// with [`EditError::MissingHeader`][^1].
///
/// Committing doesn't copy or re-encode the original message's bytes: untouched
/// tokens are reused as-is, and only new tokens are encoded and appended to the
/// message's shared buffer. The message keeps track of which byte spans are
/// relevant to the current version, and these can be read out with
/// [`Message::chunks`], or copied into one chunk with [`Message::to_bytes`].
///
/// ## Panics
///
/// Methods taking a view will panic if the view they are passed refers to a
/// different message than the message the `Edit` was started from. See also the
/// individual methods.
///
/// [^1]: While an empty message will always be read back as missing a header, adding a group to it would mean that, after committing, the first 8 bytes of the group would be read back as the header.
///
/// [`ValueRef`]: crate::ValueRef
/// [`commit`]: Edit::commit
/// [`remove`]: Edit::remove
/// [`push_value`]: Edit::push_value
/// [`set_values`]: Edit::set_values
/// [`remove_value`]: Edit::remove_value
/// [`retain_values`]: Edit::retain_values
/// [`replace_value`]: Edit::replace_value
/// [`remove_group`]: Edit::remove_group
/// [`insert_before`]: Edit::insert_before
/// [`insert_after`]: Edit::insert_after
/// [`set_header`]: Edit::set_header
#[must_use = "edits are inert until committed"]
pub struct Edit<'m> {
  msg: &'m Message,
  header: Option<Header>,
  attr_edits: BTreeMap<u32, AttrEdit<'m>>,
  group_removals: Vec<GroupRef<'m>>,
  inserts: Vec<Insert>,
}

impl Message {
  /// Start a batch of edits against this message.
  pub fn edit(&self) -> Edit<'_> {
    Edit {
      msg: self,
      header: None,
      attr_edits: BTreeMap::new(),
      group_removals: Vec::new(),
      inserts: Vec::new(),
    }
  }
}

impl<'m> Edit<'m> {
  fn check(&self, msg: &Inner) {
    assert!(
      std::ptr::eq(msg, &raw const *self.msg.0),
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

  /// Commits all the edits into a new full message.
  ///
  /// # Errors
  ///
  /// See the docs on [`Edit`] and [`EditError`] for errors.
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
        version: h.version,
        operation_or_status: h.operation_or_status,
        request_id: h.request_id,
      };
      tokens.push(encode_token_to_vec_spanned(&header, base, &mut fresh)?);
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
        tokens.push(encode_token_to_vec_spanned(token, base, &mut fresh)?);
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
      out.push(Splice::replace(
        start,
        start + 1,
        Rank::Value,
        vec![tok(ValueTag::MEMBER_ATTR_NAME, Bytes::new(), name.clone())],
      ));
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
            out.push(Splice::replace(
              t.start,
              t.start + 1,
              Rank::Value,
              vec![tok(token.tag, wanted, Bytes::copy_from_slice(value))],
            ));
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
