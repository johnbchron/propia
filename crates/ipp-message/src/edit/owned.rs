use bytes::Bytes;
use ipp_wire::{DelimiterTag, Token, ValueTag};

use super::{EditError, InvalidTag, Run, tok};

/// An owned IPP Value, used for making edits to [`Message`](crate::Message)s.
#[derive(Clone, Debug)]
pub struct Value {
  pub(crate) run: Run,
}

impl Value {
  /// Builds a scalar IPP Value from bytes.
  ///
  /// # Errors
  ///
  /// This function errors if the given value tag collides with the delimiter
  /// tag range (`0x00..=0x0F`) or if the tag is `ValueTag::Standard(0x7F)`
  /// (should be `ValueTag::Extension`).
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
  pub(crate) fn trusted(tag: ValueTag, bytes: Bytes) -> Self {
    Self {
      run: vec![tok(tag, Bytes::new(), bytes)],
    }
  }

  /// Builds an integer value.
  #[must_use]
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
  pub(crate) fn emit(self, name: &Bytes, out: &mut Run) {
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

/// An owned IPP Attribute, used for making edits to
/// [`Message`](crate::Message)s.
#[derive(Clone, Debug)]
pub struct Attr {
  name: Bytes,
  values: Vec<Value>,
}

impl Attr {
  /// Builds an IPP Attribute.
  pub fn new(name: impl Into<Bytes>, first: Value) -> Self {
    Self {
      name: name.into(),
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
  pub(super) fn emit_as_top(self, out: &mut Run) -> Result<(), EditError> {
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
  pub(super) fn emit_as_member(self, out: &mut Run) {
    out.push(tok(ValueTag::MEMBER_ATTR_NAME, Bytes::new(), self.name));
    for value in self.values {
      value.emit(&Bytes::new(), out);
    }
  }
}

/// An owned IPP Attribute Group, used for making edits to
/// [`Message`](crate::Message)s.
#[derive(Clone, Debug)]
pub struct Group {
  tag: DelimiterTag,
  attrs: Vec<Attr>,
}

impl Group {
  /// Builds an IPP Attribute Group.
  #[must_use]
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
  pub(super) fn emit(self, out: &mut Run) -> Result<(), EditError> {
    if self.tag == DelimiterTag::END_OF_ATTRIBUTES_TAG || self.tag.0 > 0x0F {
      return Err(EditError::InvalidGroupTag(self.tag));
    }

    out.push(Token::Delimiter { tag: self.tag });

    self.attrs.into_iter().try_for_each(|a| a.emit_as_top(out))
  }
}
