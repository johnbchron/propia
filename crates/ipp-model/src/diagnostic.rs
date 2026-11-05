#[derive(Debug)]
pub struct Diagnostic {
  pub token: u32,
  pub kind:  Kind,
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
