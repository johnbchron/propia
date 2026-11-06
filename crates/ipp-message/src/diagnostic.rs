use std::fmt;

#[derive(Debug)]
pub struct Diagnostic {
  pub token: u32,
  pub kind:  Kind,
}

impl fmt::Display for Diagnostic {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}: token = {}", self.kind, self.token)
  }
}

#[derive(Debug)]
pub enum Kind {
  AttributeBeforeGroup,
  MemberNameOutsideCollection,
  NamedMemberAttrName,
  NamedValueInCollection,
  StrayEndCollection,
  UnclosedCollection,
  UnexpectedToken,
  ValueWithoutAttribute,
}

impl fmt::Display for Kind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let msg = match self {
      Kind::AttributeBeforeGroup => {
        "an attribute was found before a group was opened"
      }
      Kind::MemberNameOutsideCollection => {
        "a member attribute was found outside of a collection"
      }
      Kind::NamedMemberAttrName => {
        "a member attribute contained a populated name field"
      }
      Kind::NamedValueInCollection => {
        "a named attribute was found inside of a collection"
      }
      Kind::StrayEndCollection => {
        "a stray end-collection attribute was found outside of a collection"
      }
      Kind::UnclosedCollection => {
        "a collection was not closed before the end of its group"
      }
      Kind::UnexpectedToken => "a stray data token was found",
      Kind::ValueWithoutAttribute => {
        "an unnamed value was found without a preceeding array attribute"
      }
    };

    f.write_str(msg)
  }
}
