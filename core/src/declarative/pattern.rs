//! Line patterns: `D:{meters:f32} mm`, literal text with typed fields in braces.
//!
//! A field reads everything up to the next piece of literal text (or the end of the line), so
//! two fields need some text between them. `{_}` skips text the same way. `{{` and `}}` are
//! literal braces. A pattern matches the whole line.

use std::fmt;

/// What a field's text must look like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldType {
    /// A decimal number, `-1.25` or `3e2`.
    F32,
    /// A whole number, `-12`.
    I32,
    /// A whole number of at least 0, `12`.
    U32,
    /// A whole number of at least 0 in hexadecimal, `1F` or `0x1f`.
    Hex,
}

impl FieldType {
    const ALL: &'static [(&'static str, FieldType)] =
        &[("f32", FieldType::F32), ("i32", FieldType::I32), ("u32", FieldType::U32), ("hex", FieldType::Hex)];

    fn parse(s: &str) -> Option<FieldType> {
        Self::ALL.iter().find(|(n, _)| *n == s).map(|(_, t)| *t)
    }

    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(_, t)| *t == self).map(|(n, _)| *n).expect("every type is listed")
    }

    /// Whole numbers stay whole on the way into a reading (a satellite count), unless scaled.
    pub fn is_whole(self) -> bool {
        self != FieldType::F32
    }

    /// The value of `text` (spaces around it are fine), or why it isn't one.
    pub fn read(self, text: &str) -> Result<f64, String> {
        let t = text.trim();
        let bad = || format!("\"{t}\" is not {}", self.described());
        match self {
            FieldType::F32 => match t.parse::<f32>() {
                Ok(v) if v.is_finite() => Ok(f64::from(v)),
                _ => Err(bad()),
            },
            FieldType::I32 => t.parse::<i32>().map(f64::from).map_err(|_| bad()),
            FieldType::U32 => t.parse::<u32>().map(f64::from).map_err(|_| bad()),
            FieldType::Hex => {
                let digits = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t);
                if digits.starts_with('+') {
                    return Err(bad());
                }
                u32::from_str_radix(digits, 16).map(f64::from).map_err(|_| bad())
            }
        }
    }

    fn described(self) -> &'static str {
        match self {
            FieldType::F32 => "a number",
            FieldType::I32 => "a whole number",
            FieldType::U32 => "a whole number of 0 or more",
            FieldType::Hex => "a hexadecimal number",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Piece {
    Text(String),
    Field { name: String, ty: FieldType },
    Skip,
}

/// A parsed pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pieces: Vec<Piece>,
}

/// Why a pattern can't be used: what, and what to write instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternError {
    pub message: String,
    pub hint: String,
}

impl PatternError {
    fn new(message: impl Into<String>, hint: impl Into<String>) -> PatternError {
        PatternError { message: message.into(), hint: hint.into() }
    }
}

/// A field name: a reading field in kebab-case, with dots for nested fields (`position.x`).
fn is_field_name(s: &str) -> bool {
    s.split('.').all(faber_connector::is_kebab_name)
}

impl Pattern {
    pub fn parse(text: &str) -> Result<Pattern, PatternError> {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut literal = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    literal.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    literal.push('}');
                }
                '}' => {
                    return Err(PatternError::new(
                        "The pattern has a } without a { before it.",
                        "Write }} for a literal }.",
                    ));
                }
                '{' => {
                    let mut inner = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(ch) => inner.push(ch),
                            None => {
                                return Err(PatternError::new(
                                    format!("The field {{{inner} is never closed."),
                                    "End it with }, like {meters:f32}. Write {{ for a literal {.",
                                ));
                            }
                        }
                    }
                    if !literal.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut literal)));
                    }
                    let piece = field(&inner)?;
                    if matches!(pieces.last(), Some(Piece::Field { .. } | Piece::Skip)) {
                        return Err(PatternError::new(
                            format!(
                                "{{{inner}}} follows another field directly, so Faber can't tell where one ends."
                            ),
                            "Put the text that separates them in between, like {a:f32},{b:f32}.",
                        ));
                    }
                    pieces.push(piece);
                }
                ch => literal.push(ch),
            }
        }
        if !literal.is_empty() {
            pieces.push(Piece::Text(literal));
        }
        let pattern = Pattern { pieces };
        let names = pattern.fields();
        if names.is_empty() {
            return Err(PatternError::new(
                "The pattern has no fields, so it can't read a value.",
                "`pattern` needs at least one field like {meters:f32}.",
            ));
        }
        for (i, (name, _)) in names.iter().enumerate() {
            if names[..i].iter().any(|(n, _)| n == name) {
                return Err(PatternError::new(
                    format!("The field \"{name}\" appears twice in the pattern."),
                    "Use {_} to skip the second one.",
                ));
            }
        }
        Ok(pattern)
    }

    /// The typed fields, in order.
    pub fn fields(&self) -> Vec<(&str, FieldType)> {
        self.pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Field { name, ty } => Some((name.as_str(), *ty)),
                _ => None,
            })
            .collect()
    }

    /// The values of the fields in `line`, in order. `Ok(None)`: the line has a different shape.
    /// `Err`: it has this shape, but a field's text isn't a value of its type.
    pub fn read(&self, line: &str) -> Result<Option<Vec<(&str, f64)>>, String> {
        let mut rest = line;
        let mut values = Vec::new();
        let mut pieces = self.pieces.iter().peekable();
        while let Some(piece) = pieces.next() {
            match piece {
                Piece::Text(t) => match rest.strip_prefix(t.as_str()) {
                    Some(r) => rest = r,
                    None => return Ok(None),
                },
                Piece::Field { .. } | Piece::Skip => {
                    let end = match pieces.peek() {
                        Some(Piece::Text(next)) => match rest.find(next.as_str()) {
                            Some(i) => i,
                            None => return Ok(None),
                        },
                        _ => rest.len(),
                    };
                    let (text, after) = rest.split_at(end);
                    if let Piece::Field { name, ty } = piece {
                        let v = ty.read(text).map_err(|why| format!("{name}: {why}"))?;
                        values.push((name.as_str(), v));
                    }
                    rest = after;
                }
            }
        }
        Ok(rest.is_empty().then_some(values))
    }
}

fn field(inner: &str) -> Result<Piece, PatternError> {
    if inner.trim() == "_" {
        return Ok(Piece::Skip);
    }
    let types = || FieldType::ALL.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ");
    let Some((name, ty)) = inner.split_once(':') else {
        return Err(PatternError::new(
            format!("The field {{{inner}}} has no type."),
            format!("Write it as {{{inner}:f32}}. The types are {}. Use {{_}} to skip text.", types()),
        ));
    };
    let (name, ty) = (name.trim(), ty.trim());
    if !is_field_name(name) {
        return Err(PatternError::new(
            format!("\"{name}\" in {{{inner}}} is not a field name."),
            "Name it after the reading field it fills, in kebab-case, like {meters:f32} or {position.x:f32}.",
        ));
    }
    let Some(ty) = FieldType::parse(ty) else {
        return Err(PatternError::new(
            format!("\"{ty}\" in {{{inner}}} is not a field type."),
            format!("Use one of {}, like {{{name}:f32}}.", types()),
        ));
    };
    Ok(Piece::Field { name: name.to_string(), ty })
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for p in &self.pieces {
            match p {
                Piece::Text(t) => write!(f, "{}", t.replace('{', "{{").replace('}', "}}"))?,
                Piece::Field { name, ty } => write!(f, "{{{name}:{}}}", ty.name())?,
                Piece::Skip => write!(f, "{{_}}")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(pattern: &str, line: &str) -> Result<Option<Vec<(String, f64)>>, String> {
        let p = Pattern::parse(pattern).unwrap();
        p.read(line).map(|o| o.map(|v| v.into_iter().map(|(n, x)| (n.to_string(), x)).collect()))
    }

    fn one(pattern: &str, line: &str) -> f64 {
        read(pattern, line).unwrap().unwrap()[0].1
    }

    #[test]
    fn typed_fields_read_their_values() {
        assert_eq!(one("D:{meters:f32}", "D:123"), 123.0);
        assert_eq!(one("D:{meters:f32}", "D: 1.5"), 1.5, "spaces around a number are fine");
        assert_eq!(one("T={t:i32}C", "T=-12C"), -12.0);
        assert_eq!(one("N {n:u32}", "N 40000"), 40000.0);
        assert_eq!(one("R{raw:hex}", "R1F"), 31.0);
        assert_eq!(one("R{raw:hex}", "R0xff"), 255.0);
        assert_eq!(
            read("{a:f32},{_},{b:i32};", "1.5,ignored,7;").unwrap().unwrap(),
            [("a".to_string(), 1.5), ("b".to_string(), 7.0)]
        );
        assert_eq!(one("{{{x:f32}}}", "{2}"), 2.0, "doubled braces are literal");
        assert_eq!(one("{position.x:f32}", "4"), 4.0, "dots name nested fields");
    }

    #[test]
    fn other_lines_do_not_match() {
        assert_eq!(read("D:{meters:f32}", "T:123"), Ok(None));
        assert_eq!(read("D:{meters:f32}mm", "D:123"), Ok(None), "the trailing text is missing");
        assert_eq!(read("D:{meters:f32}", "xD:1"), Ok(None), "patterns match from the start");
        assert_eq!(read("{a:f32},{b:f32}", "1;2"), Ok(None));
    }

    #[test]
    fn a_line_of_the_right_shape_with_a_bad_value_says_why() {
        assert_eq!(read("D:{meters:f32}", "D:abc"), Err("meters: \"abc\" is not a number".into()));
        assert!(read("D:{meters:f32}", "D:inf").is_err(), "infinity is not a measurement");
        assert!(read("D:{meters:f32}", "D:").is_err());
        assert!(read("N{n:u32}", "N-1").unwrap_err().contains("0 or more"));
        assert!(read("N{n:i32}", "N1.5").unwrap_err().contains("whole number"));
        assert!(read("R{raw:hex}", "RZZ").unwrap_err().contains("hexadecimal"));
    }

    #[test]
    fn mistakes_in_patterns_say_what_to_write() {
        let cases = [
            ("D:", "at least one field like {meters:f32}"),
            ("D:{_}", "at least one field"),
            ("D:{meters}", "{meters:f32}"),
            ("D:{meters:float}", "f32, i32, u32, hex"),
            ("D:{Meters:f32}", "kebab-case"),
            ("D:{meters:f32", "never closed"),
            ("D:}", "}}"),
            ("{a:f32}{b:f32}", "in between"),
            ("{a:f32},{a:f32}", "{_}"),
        ];
        for (pattern, words) in cases {
            let e = Pattern::parse(pattern).unwrap_err();
            assert!(format!("{} {}", e.message, e.hint).contains(words), "{pattern}: {e:?}");
        }
    }

    #[test]
    fn patterns_print_as_written() {
        for p in ["D:{meters:f32}", "{{{x:hex}}},{_};{y:u32}"] {
            assert_eq!(Pattern::parse(p).unwrap().to_string(), p);
        }
    }
}
