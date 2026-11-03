#![feature(iterator_try_collect)]

use ipp_wire::{DecodeIterator, DelimiterTag, Token, ValueTag};

#[test]
fn golden_decode_1() {
  const BYTES: &[u8] =
    include_bytes!("../../../corpora/ipp-messages-initial/good/1.ipp");
  const X1: &[u8] = &21590u32.to_be_bytes();
  const Y1: &[u8] = &27940u32.to_be_bytes();
  const X2: &[u8] = &21000u32.to_be_bytes();
  const Y2: &[u8] = &29700u32.to_be_bytes();

  let tokens = DecodeIterator::new(BYTES)
    .unwrap()
    .map(|r| r.map(|t| t.forget_spans()))
    .try_collect::<Vec<_>>()
    .expect("failed to decode tokens");

  let expected_tokens: Vec<Token<&[u8]>> = vec![
    Token::Header {
      version:             (0x01, 0x01),
      operation_or_status: 0x0002,
      request_id:          0x0001,
    },
    // operation attributes group
    Token::Delimiter {
      tag: DelimiterTag::OPERATION_ATTRIBUTES_TAG,
    },
    Token::Value {
      tag:   ValueTag::Standard(0x47),
      name:  b"attributes-charset",
      value: b"utf-8",
    },
    Token::Value {
      tag:   ValueTag::Standard(0x48),
      name:  b"attributes-natural-language",
      value: b"en",
    },
    Token::Value {
      tag:   ValueTag::Standard(0x45),
      name:  b"printer-uri",
      value: b"ipp://localhost/printers/foo",
    },
    // job attributes group
    Token::Delimiter {
      tag: DelimiterTag::JOB_ATTRIBUTES_TAG,
    },
    // start the media-col collection
    Token::Value {
      tag:   ValueTag::BEG_COLLECTION,
      name:  b"media-col",
      value: b"",
    },
    // media-col.media-size
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"media-size",
    },
    // media-col.media-size is a collection
    Token::Value {
      tag:   ValueTag::BEG_COLLECTION,
      name:  b"",
      value: b"",
    },
    // media-col.media-size.x-dimension
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"x-dimension",
    },
    // media-col.media-size.x-dimension is an integer
    Token::Value {
      tag:   ValueTag::INTEGER,
      name:  b"",
      value: X1,
    },
    // media-col.media-size.y-dimension
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"y-dimension",
    },
    // media-col.media-size.y-dimension is an integer
    Token::Value {
      tag:   ValueTag::INTEGER,
      name:  b"",
      value: Y1,
    },
    // end of media-col.media-size
    Token::Value {
      tag:   ValueTag::END_COLLECTION,
      name:  b"",
      value: b"",
    },
    // media-col.media-color
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"media-color",
    },
    // media-col.media-color is a keyword
    Token::Value {
      tag:   ValueTag::KEYWORD,
      name:  b"",
      value: b"blue",
    },
    // media-col.media-type
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"media-type",
    },
    // media-col.media-type is a keyword
    Token::Value {
      tag:   ValueTag::KEYWORD,
      name:  b"",
      value: b"plain",
    },
    // end of media-col
    Token::Value {
      tag:   ValueTag::END_COLLECTION,
      name:  b"",
      value: b"",
    },
    // media-col[1]
    Token::Value {
      tag:   ValueTag::BEG_COLLECTION,
      name:  b"",
      value: b"",
    },
    // media-col[1].media-size
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"media-size",
    },
    // media-col[1].media-size is a collection
    Token::Value {
      tag:   ValueTag::BEG_COLLECTION,
      name:  b"",
      value: b"",
    },
    // media-col[1].media-size.x-dimension
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"x-dimension",
    },
    // media-col[1].media-size.x-dimension is an integer
    Token::Value {
      tag:   ValueTag::INTEGER,
      name:  b"",
      value: X2,
    },
    // media-col[1].media-size.y-dimension
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"y-dimension",
    },
    // media-col[1].media-size.y-dimension is an integer
    Token::Value {
      tag:   ValueTag::INTEGER,
      name:  b"",
      value: Y2,
    },
    // end of media-col[1].media-size
    Token::Value {
      tag:   ValueTag::END_COLLECTION,
      name:  b"",
      value: b"",
    },
    // media-col[1].media-color
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"media-color",
    },
    // media-col[1].media-color is a keyword
    Token::Value {
      tag:   ValueTag::KEYWORD,
      name:  b"",
      value: b"plaid",
    },
    // media-col[1].media-type
    Token::Value {
      tag:   ValueTag::MEMBER_ATTR_NAME,
      name:  b"",
      value: b"media-type",
    },
    // media-col[1].media-type is a keyword
    Token::Value {
      tag:   ValueTag::KEYWORD,
      name:  b"",
      value: b"glossy",
    },
    // end of media-col[1]
    Token::Value {
      tag:   ValueTag::END_COLLECTION,
      name:  b"",
      value: b"",
    },
    // end of all attribute groups
    Token::Delimiter {
      tag: DelimiterTag::END_OF_ATTRIBUTES_TAG,
    },
    // rest of the message
    Token::Data { data: b"" },
  ];

  assert_eq!(tokens, expected_tokens);
}

#[test]
fn golden_decode_2() {
  const BYTES: &[u8] =
    include_bytes!("../../../corpora/ipp-messages-initial/good/2.ipp");

  let tokens = DecodeIterator::new(BYTES)
    .unwrap()
    .map(|r| r.map(|t| t.forget_spans()))
    .try_collect::<Vec<_>>()
    .expect("failed to decode tokens");

  let expected_tokens: Vec<Token<&[u8]>> = vec![
    Token::Header {
      version:             (0x01, 0x01),
      operation_or_status: 0x0002,
      request_id:          0x0001,
    },
    Token::Delimiter {
      tag: DelimiterTag::OPERATION_ATTRIBUTES_TAG,
    },
    Token::Value {
      tag:   ValueTag::Standard(0x21),
      name:  b"notify-lease-duration-supported",
      value: b"\x00\x00\x00\x01",
    },
    Token::Value {
      tag:   ValueTag::Standard(0x33),
      name:  b"",
      value: b"\x00\x00\x00\x10\x00\x00\x00\x20",
    },
    Token::Delimiter {
      tag: DelimiterTag::END_OF_ATTRIBUTES_TAG,
    },
    Token::Data { data: b"" },
  ];

  assert_eq!(tokens, expected_tokens);
}
