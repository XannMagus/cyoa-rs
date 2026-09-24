//! Incremental preview extraction, ported from calibre's StreamingStringField.
//! This scanner is deliberately not a JSON validator. Only final wire decoding
//! may authorize a commit. Input chunks must already be valid UTF-8 strings.

pub struct StreamingStringField {
    field: String,
    phase: Phase,
}
enum Phase {
    SeekingRoot,
    Object(ObjectScan),
    Done,
}
struct ObjectScan {
    depth: usize,
    after_colon: bool,
    key: String,
    string: Option<StringScan>,
}
struct StringScan {
    purpose: StringPurpose,
    escape: Escape,
    high_surrogate: Option<u16>,
}
enum StringPurpose {
    Key(String),
    Captured,
    Ignored,
}
enum Escape {
    None,
    Backslash,
    Unicode(String),
}

impl StreamingStringField {
    pub fn new(field: &str) -> Self {
        Self {
            field: field.into(),
            phase: Phase::SeekingRoot,
        }
    }
    pub fn feed(&mut self, text: &str) -> String {
        let mut out = String::new();
        for ch in text.chars() {
            let phase = std::mem::replace(&mut self.phase, Phase::Done);
            self.phase = match phase {
                Phase::SeekingRoot if ch == '{' => Phase::Object(ObjectScan {
                    depth: 1,
                    after_colon: false,
                    key: String::new(),
                    string: None,
                }),
                Phase::SeekingRoot => Phase::SeekingRoot,
                Phase::Done => break,
                Phase::Object(mut object) => {
                    if object.feed(ch, &self.field, &mut out) {
                        Phase::Done
                    } else {
                        Phase::Object(object)
                    }
                }
            };
        }
        out
    }
}
impl ObjectScan {
    fn feed(&mut self, ch: char, field: &str, out: &mut String) -> bool {
        if let Some(mut string) = self.string.take() {
            if string.feed(ch, out) {
                match string.purpose {
                    StringPurpose::Key(key) => self.key = key,
                    StringPurpose::Captured => return true,
                    StringPurpose::Ignored => (),
                }
            } else {
                self.string = Some(string)
            }
            return false;
        }
        match ch {
            '"' => {
                let purpose = if self.depth != 1 {
                    StringPurpose::Ignored
                } else if !self.after_colon {
                    StringPurpose::Key(String::new())
                } else if self.key == field {
                    StringPurpose::Captured
                } else {
                    StringPurpose::Ignored
                };
                self.string = Some(StringScan {
                    purpose,
                    escape: Escape::None,
                    high_surrogate: None,
                });
            }
            '{' | '[' => self.depth += 1,
            '}' | ']' => {
                self.depth -= 1;
                if self.depth == 0 {
                    return true;
                }
            }
            ':' if self.depth == 1 => self.after_colon = true,
            ',' if self.depth == 1 => self.after_colon = false,
            _ => (),
        }
        false
    }
}
impl StringScan {
    fn feed(&mut self, ch: char, out: &mut String) -> bool {
        match std::mem::replace(&mut self.escape, Escape::None) {
            Escape::Unicode(mut digits) => {
                digits.push(ch);
                if digits.chars().count() == 4 {
                    match u16::from_str_radix(&digits, 16) {
                        Ok(code) => self.code_point(code, out),
                        Err(_) => self.emit('�', out),
                    }
                } else {
                    self.escape = Escape::Unicode(digits)
                }
            }
            Escape::Backslash => {
                if ch == 'u' {
                    self.escape = Escape::Unicode(String::new())
                } else {
                    self.emit(
                        match ch {
                            'b' => '\u{8}',
                            'f' => '\u{c}',
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        },
                        out,
                    )
                }
            }
            Escape::None => match ch {
                '\\' => self.escape = Escape::Backslash,
                '"' => {
                    self.flush_surrogate(out);
                    return true;
                }
                _ => self.emit(ch, out),
            },
        }
        false
    }
    fn code_point(&mut self, code: u16, out: &mut String) {
        match code {
            0xD800..=0xDBFF => {
                self.flush_surrogate(out);
                self.high_surrogate = Some(code)
            }
            0xDC00..=0xDFFF => {
                let value = self.high_surrogate.take().map(|high| {
                    0x10000 + ((u32::from(high) - 0xD800) << 10) + (u32::from(code) - 0xDC00)
                });
                self.emit(value.and_then(char::from_u32).unwrap_or('�'), out);
            }
            _ => self.emit(char::from_u32(u32::from(code)).unwrap_or('�'), out),
        }
    }
    fn flush_surrogate(&mut self, out: &mut String) {
        if self.high_surrogate.take().is_some() {
            self.push('�', out)
        }
    }
    fn emit(&mut self, ch: char, out: &mut String) {
        self.flush_surrogate(out);
        self.push(ch, out)
    }
    fn push(&mut self, ch: char, out: &mut String) {
        match &mut self.purpose {
            StringPurpose::Key(key) => key.push(ch),
            StringPurpose::Captured => out.push(ch),
            StringPurpose::Ignored => (),
        }
    }
}
