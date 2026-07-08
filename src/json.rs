//! Мини-JSON: разбор плоских объектов протокола и настроек без serde.
//!
//! Поддерживается ровно то, что шлёт Python-версия: объект верхнего уровня
//! со строковыми и целочисленными значениями (включая \uXXXX-экранирование,
//! которое даёт json.dumps с ensure_ascii=True). Вложенные объекты/массивы
//! в протоколе не встречаются — на них разбор честно возвращает None.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum JVal {
    Str(String),
    Int(i64),
    #[allow(dead_code)] // в протоколе не встречается, но парсер обязан не спотыкаться
    Bool(bool),
    Null,
}

impl JVal {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JVal::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            JVal::Int(n) => Some(*n),
            _ => None,
        }
    }
}

pub fn parse_object(input: &str) -> Option<HashMap<String, JVal>> {
    let mut p = Parser { s: input.as_bytes(), i: 0 };
    p.skip_ws();
    let obj = p.object()?;
    p.skip_ws();
    if p.i != p.s.len() {
        return None; // мусор после объекта
    }
    Some(obj)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Option<()> {
        if self.i < self.s.len() && self.s[self.i] == c {
            self.i += 1;
            Some(())
        } else {
            None
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn object(&mut self) -> Option<HashMap<String, JVal>> {
        self.eat(b'{')?;
        let mut map = HashMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Some(map);
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.eat(b':')?;
            self.skip_ws();
            let val = self.value()?;
            map.insert(key, val);
            self.skip_ws();
            match self.peek()? {
                b',' => self.i += 1,
                b'}' => {
                    self.i += 1;
                    return Some(map);
                }
                _ => return None,
            }
        }
    }

    fn value(&mut self) -> Option<JVal> {
        match self.peek()? {
            b'"' => Some(JVal::Str(self.string()?)),
            b't' => self.literal(b"true").map(|_| JVal::Bool(true)),
            b'f' => self.literal(b"false").map(|_| JVal::Bool(false)),
            b'n' => self.literal(b"null").map(|_| JVal::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn literal(&mut self, lit: &[u8]) -> Option<()> {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            Some(())
        } else {
            None
        }
    }

    fn number(&mut self) -> Option<JVal> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        // дробные/экспоненты в протоколе не используются
        std::str::from_utf8(&self.s[start..self.i])
            .ok()?
            .parse()
            .ok()
            .map(JVal::Int)
    }

    fn string(&mut self) -> Option<String> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'"' => return Some(out),
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let ch = if (0xD800..0xDC00).contains(&hi) {
                                // суррогатная пара
                                self.eat(b'\\')?;
                                self.eat(b'u')?;
                                let lo = self.hex4()?;
                                let code =
                                    0x10000 + ((hi - 0xD800) << 10) + (lo.checked_sub(0xDC00)?);
                                char::from_u32(code)?
                            } else {
                                char::from_u32(hi)?
                            };
                            out.push(ch);
                        }
                        _ => return None,
                    }
                }
                _ => {
                    // UTF-8 байты копируем как есть
                    let len = utf8_len(c);
                    let start = self.i - 1;
                    self.i = start + len;
                    if self.i > self.s.len() {
                        return None;
                    }
                    out.push_str(std::str::from_utf8(&self.s[start..self.i]).ok()?);
                }
            }
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        if self.i + 4 > self.s.len() {
            return None;
        }
        let v = std::str::from_utf8(&self.s[self.i..self.i + 4]).ok()?;
        self.i += 4;
        u32::from_str_radix(v, 16).ok()
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Строка -> JSON-литерал с кавычками ("abc" -> "\"abc\"").
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Сборка плоского JSON-объекта из готовых пар "ключ": <literal>.
pub fn object(pairs: &[(&str, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{}: {}", quote(k), v))
        .collect();
    format!("{{{}}}", body.join(", "))
}
