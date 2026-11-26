#![cfg(feature = "std")]
#![cfg(feature = "cbor")]
#![cfg(feature = "json")]
#![cfg(feature = "additional-controls")]
#![cfg(not(target_arch = "wasm32"))]

//! The string controls denote the strings they match as a whole, through
//! both validators: `.regexp` (RFC 8610, section 3.8.3, whose XSD regular
//! expressions are implicitly anchored) and `.abnf` (RFC 9165, section 3).
//! A string merely holding a match is refused.

use cddl::validator::{validate_cbor_from_slice, validate_json_from_str};

fn cbor_text(s: &str) -> Vec<u8> {
  let mut out = Vec::new();
  ciborium::into_writer(&ciborium::value::Value::Text(s.to_string()), &mut out).unwrap();
  out
}

fn json_text(s: &str) -> String {
  serde_json::Value::String(s.to_string()).to_string()
}

/// `text` accepted by `cddl` in CBOR and in JSON alike, or refused by both.
fn accepted(cddl: &str, text: &str) -> bool {
  let cbor = validate_cbor_from_slice(cddl, &cbor_text(text), None).is_ok();
  let json = validate_json_from_str(cddl, &json_text(text), None).is_ok();
  assert_eq!(cbor, json, "{cddl:?} on {text:?}: CBOR {cbor}, JSON {json}");
  cbor
}

#[test]
fn a_regexp_accepts_only_whole_matches() {
  let cddl = r#"word = tstr .regexp "[a-z]+""#;
  assert!(accepted(cddl, "hello"));
  assert!(!accepted(cddl, "hello1"), "a partial match was accepted");
}

#[test]
fn an_ascii_regexp_refuses_non_ascii_text() {
  let cddl = r#"ascii = tstr .regexp "[\u0000-\u007F]*""#;
  assert!(accepted(cddl, "abc"));
  assert!(accepted(cddl, ""));
  assert!(!accepted(cddl, "ab\u{E9}"), "a non-ASCII text was accepted");
}

#[test]
fn an_abnf_accepts_only_whole_matches() {
  let cddl = "ascii = tstr .abnf (\"ascii\" .det rules)\nrules = '\n  ascii = *%x00-7F\n'";
  assert!(accepted(cddl, "abc"));
  assert!(!accepted(cddl, "ab\u{E9}"), "a prefix match was accepted");
}
