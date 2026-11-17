//! Verifies structural invariants of an `ipp-message::Message`

#![feature(range_into_bounds)]
#![no_main]

use std::{
  ops::{Index, IntoBounds},
  range::Range,
};

use ipp_message::{
  Assembled, Assembler, AttrRef, GroupRef, Limits, Progress, ValueRef,
};
use ipp_wire::{DelimiterTag, Token};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
  let Assembled { message, .. } =
    match Assembler::new(Limits::default()).push(data) {
      Progress::More(assembler) => assembler.finish(),
      Progress::Done(assembled) => assembled,
    };

  // test all the debug printing
  let _ = format!("{message:?}");
  let _ = format!("{message:#?}");
  let _ = format!("{message:+#?}");

  // abort if the message is empty or just the header
  if message.tokens().is_empty() {
    return;
  }

  // assert that every diagnostic is tagged on an actual token
  for diagnostic in message.diagnostics() {
    assert!(
      (diagnostic.token as usize) < message.tokens().len(),
      "diagnostic points to a phantom token"
    );
  }

  // assert group ranges are disjoint and ordered, and tokens are not orphaned
  // the header is always the first token
  let mut last = 1;
  for group in message.groups() {
    assert_eq!(
      last,
      group.tokens().start,
      "group overlaps with the previous group"
    );

    // recursively assert that items are self contained
    assert_compliant_group(group);

    // asserts that the next group lines up with no gap
    last = group.tokens().end;
  }

  // abort if the message was truncated after the group
  if message.tokens().len() == last as usize {
    return;
  }

  // there should only be an end-of-attributes after the last group
  assert_eq!(
    message.tokens().index(last as usize).forget_spans(),
    Token::<()>::Delimiter {
      tag: DelimiterTag::END_OF_ATTRIBUTES_TAG,
    },
    "token after the last group was not end-of-attributes"
  );
  // the end-of-attributes should be the last token in the message
  assert_eq!(
    last as usize,
    message.tokens().len() - 1,
    "the end-of-attributes was not the last token"
  );
});

fn assert_compliant_group(group: GroupRef<'_>) {
  // make sure none of these crash
  let _tag = group.tag();
  let _attrs = group.attrs().collect::<Vec<_>>();
  let _debug = format!("{group:?}");

  let group_range = group.tokens();
  for attr in group.attrs() {
    assert_eq!(
      group_range.intersect(attr.tokens()),
      attr.tokens().into_bounds(),
      "attribute is not fully contained by its group"
    );

    assert_compliant_attr(attr);
  }
}

fn assert_compliant_attr(attr: AttrRef<'_>) {
  // make sure none of these crash
  let _name = attr.name();
  let _values = attr.values().collect::<Vec<_>>();

  let attr_range = attr.tokens();
  for value in attr.values() {
    assert_eq!(
      attr_range.intersect(value.tokens()),
      value.tokens().into_bounds(),
      "value is not fully contained by its attribute"
    );

    assert_compliant_value(value);
  }
}

fn assert_compliant_value(value: ValueRef<'_>) {
  // make sure none of these crash
  let _tag = value.tag();
  let _bytes = value.bytes();
  let _members = value.members().collect::<Vec<_>>();

  let value_range = value.tokens();
  if value.is_collection() {
    let collection_range = Range {
      start: value_range.start + 1,
      end:   value_range.end,
    };

    for member in value.members() {
      assert_eq!(
        collection_range.intersect(member.tokens()),
        member.tokens().into_bounds(),
        "member attribute is not fully contained by its collection"
      );

      assert_compliant_attr(member);
    }
  }
}
