//! XSD regular expressions (XML Schema Part 2: Datatypes, Second Edition,
//! Appendix F), the language of the `.regexp` control (RFC 8610, section
//! 3.8.3), translated into the `regex` crate's dialect.
//!
//! The translation follows the Appendix F grammar, productions [1] to [37a],
//! and refuses every pattern outside it: an escape the grammar does not name
//! (`\:`, `\x41`), a construct of another dialect (`(?:`, `(?<name>`,
//! lookaround, backreferences), an unbalanced group or class, a range whose
//! end precedes its start, a quantity whose minimum exceeds its maximum. An
//! XSD regular expression is implicitly anchored at head and tail, so `^`
//! and `$` are ordinary characters; the caller anchors the translation
//! (`super::whole_regex`).
//!
//! Every character is emitted as a `\x{..}` escape, every group as a
//! non-capturing one, so no XSD character can take a meaning of the target
//! dialect. The classes the grammar defines are emitted as `regex` classes:
//!
//! - `.` is `[^\n\r]`; `\s` is `[\x20\t\n\r]`; `\d` is `\p{Nd}`; `\w` is every
//!   character but punctuation, separators and others (`[^\p{P}\p{Z}\p{C}]`);
//!   `\S`, `\D`, `\W` their complements;
//! - `\i` is XML 1.0's `Letter | '_' | ':'` and `\c` its `NameChar`, from that
//!   Recommendation's Appendix B (`data/xsd/xml-1.0-character-classes.txt`);
//!   `\I`, `\C` their complements;
//! - `\p{IsBlock}` is the block of the Appendix F.1.1 table
//!   (`data/xsd/blocks.txt`); `\p{Category}` the Unicode general category, by
//!   the `regex` crate's Unicode version; `\P{..}` their complements;
//! - a subtraction `[G-[C]]` is the set difference of the two classes.
//!
//! Ranges span every Unicode scalar value, not only XML characters: a CDDL
//! text string holds any scalar, and RFC 8610, section 3.8.3.1, writes code
//! points in a pattern with CDDL's `\u` escapes.

use std::fmt::Write;

/// The classes XML 1.0 (Second Edition), Appendix B, defines, one range per
/// line (`Class; Start..End`).
const XML_CLASSES: &str = include_str!("../../data/xsd/xml-1.0-character-classes.txt");

/// The block names of XML Schema Part 2, Appendix F.1.1, one range per line
/// (`Start..End; Name`); a name may own more than one range.
const BLOCKS: &str = include_str!("../../data/xsd/blocks.txt");

/// The Unicode general categories an XSD category escape names
/// (productions [28] to [35]).
const CATEGORIES: &[&str] = &[
  "L", "Lu", "Ll", "Lt", "Lm", "Lo", "M", "Mn", "Mc", "Me", "N", "Nd", "Nl", "No", "P", "Pc", "Pd",
  "Ps", "Pe", "Pi", "Pf", "Po", "Z", "Zs", "Zl", "Zp", "S", "Sm", "Sc", "Sk", "So", "C", "Cc",
  "Cf", "Co", "Cn",
];

/// `pattern`, an XSD regular expression, in the `regex` crate's dialect, or
/// why it is not one: the refusal names the offending construct and its
/// character offset.
pub fn to_regex(pattern: &str) -> Result<String, String> {
  let mut translation = Translation {
    chars: pattern.chars().collect(),
    at: 0,
    out: String::with_capacity(pattern.len() * 4),
  };
  translation.reg_exp()?;
  match translation.peek() {
    None => Ok(translation.out),
    Some(c) => Err(translation.refuse(&format!("unbalanced {:?}", c))),
  }
}

/// A translation in progress: the pattern's characters, the offset reached,
/// and the target pattern written so far.
struct Translation {
  chars: Vec<char>,
  at: usize,
  out: String,
}

/// One item of a character group: a single character, which may begin a
/// range, or a class, which may not.
enum Item {
  Char(char),
  Class(String),
}

impl Translation {
  fn peek(&self) -> Option<char> {
    self.chars.get(self.at).copied()
  }

  fn peek_at(&self, ahead: usize) -> Option<char> {
    self.chars.get(self.at + ahead).copied()
  }

  fn next(&mut self) -> Option<char> {
    let c = self.peek();
    if c.is_some() {
      self.at += 1;
    }
    c
  }

  fn refuse(&self, what: &str) -> String {
    format!(
      "invalid XSD regular expression at character {}: {}",
      self.at, what
    )
  }

  /// [1] regExp ::= branch ( '|' branch )*
  fn reg_exp(&mut self) -> Result<(), String> {
    self.branch()?;
    while self.peek() == Some('|') {
      self.next();
      self.out.push('|');
      self.branch()?;
    }
    Ok(())
  }

  /// [2] branch ::= piece*
  fn branch(&mut self) -> Result<(), String> {
    while let Some(c) = self.peek() {
      if c == '|' || c == ')' {
        return Ok(());
      }
      self.piece()?;
    }
    Ok(())
  }

  /// [3] piece ::= atom quantifier?
  fn piece(&mut self) -> Result<(), String> {
    self.atom()?;
    self.quantifier()
  }

  /// [4] to [8]: `?`, `*`, `+`, `{n}`, `{n,}`, `{n,m}`, at most one per
  /// atom, `n` at most `m`.
  fn quantifier(&mut self) -> Result<(), String> {
    match self.peek() {
      Some(c @ ('?' | '*' | '+')) => {
        self.next();
        self.out.push(c);
      }
      Some('{') => {
        self.next();
        let min = self.quantity()?;
        let max = if self.peek() == Some(',') {
          self.next();
          if self.peek() == Some('}') {
            None
          } else {
            Some(self.quantity()?)
          }
        } else {
          Some(min)
        };
        if self.next() != Some('}') {
          return Err(self.refuse("a quantifier is not closed by '}'"));
        }
        match max {
          Some(max) if max < min => {
            return Err(self.refuse("a quantifier's minimum exceeds its maximum"));
          }
          Some(max) if max == min => write!(self.out, "{{{}}}", min),
          Some(max) => write!(self.out, "{{{},{}}}", min, max),
          None => write!(self.out, "{{{},}}", min),
        }
        .map_err(|e| e.to_string())?;
      }
      _ => return Ok(()),
    }
    match self.peek() {
      Some('?' | '*' | '+' | '{') => Err(self.refuse("a second quantifier on one atom")),
      _ => Ok(()),
    }
  }

  /// [8] QuantExact ::= [0-9]+
  fn quantity(&mut self) -> Result<u32, String> {
    let start = self.at;
    while matches!(self.peek(), Some('0'..='9')) {
      self.next();
    }
    if start == self.at {
      return Err(self.refuse("a quantity is not a decimal number"));
    }
    self.chars[start..self.at]
      .iter()
      .collect::<String>()
      .parse()
      .map_err(|_| self.refuse("a quantity does not fit"))
  }

  /// [9] atom ::= Char | charClass | ( '(' regExp ')' )
  fn atom(&mut self) -> Result<(), String> {
    match self.peek() {
      Some('(') => {
        self.next();
        if self.peek() == Some('?') {
          return Err(self.refuse("'(?' groups are not XSD"));
        }
        self.out.push_str("(?:");
        self.reg_exp()?;
        if self.next() != Some(')') {
          return Err(self.refuse("a group is not closed by ')'"));
        }
        self.out.push(')');
        Ok(())
      }
      Some('[') => {
        let class = self.char_class_expr()?;
        self.out.push_str(&class);
        Ok(())
      }
      Some('.') => {
        self.next();
        self.out.push_str(r"[^\n\r]");
        Ok(())
      }
      Some('\\') => {
        let class = match self.escape()? {
          Item::Char(c) => literal(c),
          Item::Class(class) => class,
        };
        self.out.push_str(&class);
        Ok(())
      }
      // [10] Char ::= [^.\?*+()|#x5B#x5D]
      Some(c @ ('?' | '*' | '+' | ')' | ']' | '{')) if c != '{' || self.is_quantifier_start() => {
        Err(self.refuse(&format!("{:?} has nothing to apply to", c)))
      }
      Some(c @ (')' | ']')) => Err(self.refuse(&format!("unbalanced {:?}", c))),
      Some(c) => {
        self.next();
        self.out.push_str(&literal(c));
        Ok(())
      }
      None => Ok(()),
    }
  }

  /// Whether a `{` here begins a quantity: it does when a digit follows.
  fn is_quantifier_start(&self) -> bool {
    matches!(self.peek_at(1), Some('0'..='9'))
  }

  /// [23] charClassEsc ::= ( SingleCharEsc | MultiCharEsc | catEsc | complEsc )
  fn escape(&mut self) -> Result<Item, String> {
    self.next();
    match self.next() {
      // [24] SingleCharEsc ::= '\' [nrt\|.?*+(){}#x2D#x5B#x5D#x5E]
      Some('n') => Ok(Item::Char('\n')),
      Some('r') => Ok(Item::Char('\r')),
      Some('t') => Ok(Item::Char('\t')),
      Some(
        c @ ('\\' | '|' | '.' | '?' | '*' | '+' | '(' | ')' | '{' | '}' | '-' | '[' | ']' | '^'),
      ) => Ok(Item::Char(c)),
      // [37] MultiCharEsc ::= '\' [sSiIcCdDwW]
      Some('s') => Ok(Item::Class(r"[\x{20}\t\n\r]".into())),
      Some('S') => Ok(Item::Class(r"[^\x{20}\t\n\r]".into())),
      Some('d') => Ok(Item::Class(r"\p{Nd}".into())),
      Some('D') => Ok(Item::Class(r"\P{Nd}".into())),
      Some('w') => Ok(Item::Class(r"[^\p{P}\p{Z}\p{C}]".into())),
      Some('W') => Ok(Item::Class(r"[\p{P}\p{Z}\p{C}]".into())),
      Some('i') => Ok(Item::Class(format!("[{}]", initial_name_chars()))),
      Some('I') => Ok(Item::Class(format!("[^{}]", initial_name_chars()))),
      Some('c') => Ok(Item::Class(format!("[{}]", name_chars()))),
      Some('C') => Ok(Item::Class(format!("[^{}]", name_chars()))),
      // [25] catEsc ::= '\p{' charProp '}', [26] complEsc ::= '\P{' charProp '}'
      Some(p @ ('p' | 'P')) => {
        let property = self.char_prop()?;
        Ok(Item::Class(if p == 'p' {
          format!("[{}]", property)
        } else {
          format!("[^{}]", property)
        }))
      }
      Some(c) => Err(self.refuse(&format!("'\\{}' is not an XSD escape", c))),
      None => Err(self.refuse("a lone '\\' ends the pattern")),
    }
  }

  /// [27] charProp ::= IsCategory | IsBlock, between braces: the class's
  /// members, as the body of a `regex` class.
  fn char_prop(&mut self) -> Result<String, String> {
    if self.next() != Some('{') {
      return Err(self.refuse("a category escape is not followed by '{'"));
    }
    let start = self.at;
    while let Some(c) = self.peek() {
      if c == '}' {
        break;
      }
      self.next();
    }
    let name: String = self.chars[start..self.at].iter().collect();
    if self.next() != Some('}') {
      return Err(self.refuse("a category escape is not closed by '}'"));
    }
    if let Some(block) = name.strip_prefix("Is") {
      let ranges = block_ranges(block);
      if ranges.is_empty() {
        return Err(self.refuse(&format!("'Is{}' is not an XSD block name", block)));
      }
      return Ok(ranges);
    }
    if CATEGORIES.contains(&name.as_str()) {
      return Ok(format!(r"\p{{{}}}", name));
    }
    Err(self.refuse(&format!("'{}' is not an XSD category", name)))
  }

  /// [12] charClassExpr ::= '[' charGroup ']'
  fn char_class_expr(&mut self) -> Result<String, String> {
    self.next();
    let group = self.char_group()?;
    if self.next() != Some(']') {
      return Err(self.refuse("a character class is not closed by ']'"));
    }
    Ok(group)
  }

  /// [13] charGroup ::= posCharGroup | negCharGroup | charClassSub, as one
  /// `regex` class.
  fn char_group(&mut self) -> Result<String, String> {
    let negated = self.peek() == Some('^');
    if negated {
      self.next();
    }
    let body = self.pos_char_group()?;
    let group = if negated {
      format!("[^{}]", body)
    } else {
      format!("[{}]", body)
    };
    // [16] charClassSub ::= ( posCharGroup | negCharGroup ) '-' charClassExpr
    if self.peek() == Some('-') && self.peek_at(1) == Some('[') {
      self.next();
      let subtracted = self.char_class_expr()?;
      return Ok(format!("[{}--{}]", group, subtracted));
    }
    Ok(group)
  }

  /// [14] posCharGroup ::= ( charRange | charClassEsc )+, as the body of a
  /// `regex` class. `-` is a range only at the group's beginning or end
  /// (Appendix F, after [22]); `[`, `]` and `\` are never ranges.
  fn pos_char_group(&mut self) -> Result<String, String> {
    let mut body = String::new();
    let mut first = true;
    loop {
      match self.peek() {
        None => return Err(self.refuse("a character class is not closed by ']'")),
        Some(']') if !first => return Ok(body),
        Some('-') if self.peek_at(1) == Some('[') && !first => return Ok(body),
        Some('-') if first || self.peek_at(1) == Some(']') => {
          self.next();
          body.push_str(&literal('-'));
        }
        Some('-') => return Err(self.refuse("'-' inside a character group is not a range")),
        Some('[') => return Err(self.refuse("'[' is not a character range")),
        Some(']') => return Err(self.refuse("an empty character group")),
        Some(_) => {
          let start = self.group_item()?;
          match start {
            Item::Class(class) => body.push_str(&class),
            Item::Char(s) => {
              // [18] seRange ::= charOrEsc '-' charOrEsc
              if self.peek() == Some('-') && !matches!(self.peek_at(1), Some(']' | '[') | None) {
                self.next();
                let e = match self.group_item()? {
                  Item::Char(e) => e,
                  Item::Class(_) => {
                    return Err(self.refuse("a range ends in a multi-character escape"));
                  }
                };
                if e < s {
                  return Err(self.refuse("a range's end precedes its start"));
                }
                write!(body, "{}-{}", literal(s), literal(e)).map_err(|e| e.to_string())?;
              } else {
                body.push_str(&literal(s));
              }
            }
          }
        }
      }
      first = false;
    }
  }

  /// One character or class escape of a character group: [20] charOrEsc ::=
  /// XmlChar | SingleCharEsc, or a class escape.
  fn group_item(&mut self) -> Result<Item, String> {
    match self.peek() {
      Some('\\') => self.escape(),
      Some(c @ ('[' | ']')) => Err(self.refuse(&format!("{:?} is not a character range", c))),
      Some(c) => {
        self.next();
        Ok(Item::Char(c))
      }
      None => Err(self.refuse("a character class is not closed by ']'")),
    }
  }
}

/// `c` as one literal character of the `regex` dialect, inside a class or
/// outside one.
fn literal(c: char) -> String {
  format!(r"\x{{{:X}}}", u32::from(c))
}

/// The ranges `table` lists for each line `keep` selects, as the body of a
/// `regex` class; `range` reads a line's `Start..End` field.
fn ranges<'t>(table: &'t str, keep: impl Fn(&'t str) -> Option<&'t str>) -> String {
  let mut body = String::new();
  for line in table.lines().filter(|line| !line.starts_with('#')) {
    if let Some(range) = keep(line) {
      if let Some((start, end)) = range.trim().split_once("..") {
        let (start, end) = (
          u32::from_str_radix(start, 16).expect("data/xsd ranges are hexadecimal"),
          u32::from_str_radix(end, 16).expect("data/xsd ranges are hexadecimal"),
        );
        let _ = write!(body, r"\x{{{:X}}}-\x{{{:X}}}", start, end);
      }
    }
  }
  body
}

/// XML 1.0 Appendix B ranges of the `classes` named.
fn xml_classes(classes: &[&str]) -> String {
  ranges(XML_CLASSES, |line| {
    let (class, range) = line.split_once(';')?;
    classes.contains(&class.trim()).then_some(range)
  })
}

/// `\i`: [84] Letter (BaseChar | Ideographic) | '_' | ':'.
fn initial_name_chars() -> String {
  format!(
    "{}{}{}",
    xml_classes(&["BaseChar", "Ideographic"]),
    literal('_'),
    literal(':')
  )
}

/// `\c`: [4] NameChar ::= Letter | Digit | '.' | '-' | '_' | ':' |
/// CombiningChar | Extender.
fn name_chars() -> String {
  format!(
    "{}{}{}{}{}",
    xml_classes(&[
      "BaseChar",
      "Ideographic",
      "Digit",
      "CombiningChar",
      "Extender"
    ]),
    literal('.'),
    literal('-'),
    literal('_'),
    literal(':')
  )
}

/// The ranges of the block named `name` (its name in the Appendix F.1.1
/// table), empty when the table names no such block.
fn block_ranges(name: &str) -> String {
  ranges(BLOCKS, |line| {
    let (range, block) = line.split_once(';')?;
    (block.trim() == name).then_some(range)
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The translation of `pattern`, compiled and anchored as the validator
  /// does.
  fn matcher(pattern: &str) -> regex::Regex {
    super::super::whole_regex(&to_regex(pattern).unwrap()).unwrap()
  }

  /// An XSD regular expression matches a target only as a whole, and `^`
  /// and `$` are ordinary characters (Appendix F, implicitly anchored).
  #[test]
  fn a_pattern_matches_only_the_whole_target_and_anchors_are_characters() {
    let letters = matcher("[a-z]+");
    assert!(letters.is_match("abc"));
    assert!(!letters.is_match("abc1"));
    let anchors = matcher("^a$");
    assert!(anchors.is_match("^a$"));
    assert!(!anchors.is_match("a"));
    assert!(matcher("a{2,3}").is_match("aaa"));
    assert!(!matcher("a{2,3}").is_match("aaaa"));
    assert!(matcher("a{2,}").is_match("aaaaa"));
    assert!(matcher("(ab|c)?d").is_match("abd"));
    assert!(
      matcher("{x}").is_match("{x}"),
      "a brace with no quantity is a character"
    );
  }

  /// The multi-character escapes and the wildcard are Appendix F's classes,
  /// not the target dialect's.
  #[test]
  fn multi_character_escapes_are_appendix_f_classes() {
    assert!(
      !matcher(".").is_match("\r"),
      "'.' matches no carriage return"
    );
    assert!(matcher(".").is_match("\u{85}"));
    assert!(matcher(r"\s").is_match("\t"));
    assert!(
      !matcher(r"\s").is_match("\u{A0}"),
      "\\s is the four XML spaces alone"
    );
    assert!(matcher(r"\d").is_match("\u{0663}"));
    assert!(!matcher(r"\w").is_match("-"));
    assert!(matcher(r"\w").is_match("\u{E9}"));
    assert!(matcher(r"\i\c*").is_match("_a.b-c:1"));
    assert!(!matcher(r"\i").is_match("1"));
    assert!(matcher(r"\I").is_match("1"));
    assert!(
      matcher(r"\c").is_match("\u{0300}"),
      "a combining character is a NameChar"
    );
    assert!(!matcher(r"\c").is_match("\u{0300}\u{0300}"));
  }

  /// Category and block escapes: a category by its Unicode name, a block by
  /// the Appendix F.1.1 table, a name it does not list refused.
  #[test]
  fn category_and_block_escapes_name_their_sets() {
    assert!(matcher(r"\p{Lu}+").is_match("AB"));
    assert!(!matcher(r"\p{Lu}").is_match("a"));
    assert!(matcher(r"\P{Lu}").is_match("a"));
    let ascii = matcher(r"\p{IsBasicLatin}*");
    assert!(ascii.is_match("\u{0}\u{7F}"));
    assert!(!ascii.is_match("\u{80}"));
    assert!(matcher(r"\p{IsSpecials}").is_match("\u{FEFF}"));
    assert!(
      matcher(r"\p{IsSpecials}").is_match("\u{FFFD}"),
      "both Specials ranges"
    );
    assert!(matcher(r"\p{IsLatin-1Supplement}").is_match("\u{E9}"));
    assert!(to_regex(r"\p{IsKlingon}").is_err());
    assert!(to_regex(r"\p{Xx}").is_err());
  }

  /// Character groups: ranges, `-` at either end, negation, subtraction,
  /// escapes inside a group; and every malformed group refused.
  #[test]
  fn character_groups_follow_productions_12_to_22() {
    assert!(matcher("[a-c-]+").is_match("a-c"));
    assert!(matcher("[-a]").is_match("-"));
    assert!(matcher("[^a]").is_match("b"));
    assert!(!matcher("[^a]").is_match("a"));
    let consonants = matcher("[a-z-[aeiou]]+");
    assert!(consonants.is_match("bcd"));
    assert!(!consonants.is_match("bad"));
    assert!(matcher(r"[\d\-]+").is_match("1-2"));
    assert!(matcher("[\u{0}-\u{7F}]*").is_match("a\u{0}\u{7F}"));
    assert!(!matcher("[\u{0}-\u{7F}]*").is_match("\u{80}"));
    for malformed in [
      "[a", "[]", "[a-]b-c]", "[z-a]", "[a-c-e]", "[[a]]", r"[a-\d]",
    ] {
      assert!(to_regex(malformed).is_err(), "{:?} was accepted", malformed);
    }
  }

  /// What other dialects write and XSD does not is refused, not given a
  /// meaning: escapes the grammar does not name, `(?` groups, unbalanced
  /// groups, stray quantifiers, a trailing backslash.
  #[test]
  fn constructs_outside_appendix_f_are_refused() {
    for foreign in [
      r"\:", r"\x41", r"\b", r"\1", "(?:a)", "(?<n>a)", "(?=a)", "(a", "a)", "*a", "a**", "a{3,2}",
      "a\\", "a{1", "]",
    ] {
      assert!(to_regex(foreign).is_err(), "{:?} was accepted", foreign);
    }
  }

  /// The tables read as their sources state: 85 block rows, `Specials`
  /// twice; every XML class named, its ranges ordered start to end.
  #[test]
  fn the_specification_tables_are_read_whole() {
    let rows: Vec<&str> = BLOCKS.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(rows.len(), 85);
    assert_eq!(rows.iter().filter(|r| r.ends_with("; Specials")).count(), 2);
    for class in [
      "BaseChar",
      "Ideographic",
      "CombiningChar",
      "Digit",
      "Extender",
    ] {
      assert!(!xml_classes(&[class]).is_empty(), "{} has no ranges", class);
    }
    for line in XML_CLASSES
      .lines()
      .chain(BLOCKS.lines())
      .filter(|l| !l.starts_with('#'))
    {
      let range = line
        .split(';')
        .find(|field| field.contains(".."))
        .expect("each row holds a range");
      let (start, end) = range.trim().split_once("..").unwrap();
      assert!(
        u32::from_str_radix(start, 16).unwrap() <= u32::from_str_radix(end, 16).unwrap(),
        "{}",
        line
      );
    }
  }
}
