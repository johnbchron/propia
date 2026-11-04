pub struct Diagnostic {
  pub token: u32,
  pub kind:  Kind,
}

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
