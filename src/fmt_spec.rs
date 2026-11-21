//! Compile-time format-specifier parsing for f-strings (`f"{x:.2f}"`).
//!
//! The spec text never reaches the emitted C VERBATIM: it is parsed into
//! numbers here and the C format is rebuilt from them, so a spec cannot
//! inject anything into the generated source (SECURITY.md: the compiler,
//! not the program, owns the bytes it emits). The supported surface is a
//! deliberate Python subset — a flag, zero padding, a width, optionally a
//! precision, and one presentation type:
//!
//! | value | spec      | C form            |
//! |-------|-----------|-------------------|
//! | float | `.2f`     | `%.2f`            |
//! | float | `+10.3e`  | `%+10.3e`         |
//! | float | `.6g`     | `%.6g`            |
//! | int   | `d`       | `%lld`            |
//! | int   | `08x`     | `%08llx`          |
//! | int   | `b`       | handwritten bits  |
//!
//! Everything else (grouping `,`, percent `%`, alignment `>`/`^`, nested
//! fields) is a clean, named error rather than a silent passthrough.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sign {
    None,
    Plus,
    Space,
}

impl Sign {
    fn c_prefix(self) -> &'static str {
        match self {
            Sign::None => "",
            Sign::Plus => "+",
            Sign::Space => " ",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Base {
    Dec,
    HexLower,
    HexUpper,
    Oct,
    Bin,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum FmtSpec {
    Float {
        sign: Sign,
        zero: bool,
        width: usize,
        prec: usize,
        kind: char,
    },
    Int {
        sign: Sign,
        zero: bool,
        width: usize,
        base: Base,
    },
}

impl FmtSpec {
    /// True when this is exactly what `str(value)` already produces, so the
    /// emitter can reuse `ax_str_i`/`ax_str_f` and keep the emitted C of a
    /// program that spells `{x:d}` or `{x:f}` identical to `{x}`.
    pub fn is_plain(&self) -> bool {
        match self {
            FmtSpec::Float { sign, zero, width, prec, kind } => {
                *sign == Sign::None && !*zero && *width == 0 && *prec == 6 && *kind == 'f'
            }
            FmtSpec::Int { sign, zero, width, base } => {
                *sign == Sign::None && !*zero && *width == 0 && *base == Base::Dec
            }
        }
    }

    /// The `printf` conversion for everything except binary/hex-sign cases.
    /// `None` for the spellings a helper must hand-roll (`b`, and a sign on
    /// an unsigned base, which C does not define).
    pub fn c_format(&self) -> Option<String> {
        match self {
            FmtSpec::Float { sign, zero, width, prec, kind } => {
                let mut s = String::from("%");
                s.push_str(sign.c_prefix());
                if *zero {
                    s.push('0');
                }
                if *width > 0 {
                    s.push_str(&width.to_string());
                }
                s.push('.');
                s.push_str(&prec.to_string());
                s.push(*kind);
                Some(s)
            }
            FmtSpec::Int { sign, zero, width, base } => {
                let conv = match base {
                    Base::Dec => "lld",
                    Base::HexLower => "llx",
                    Base::HexUpper => "llX",
                    Base::Oct => "llo",
                    Base::Bin => return None,
                };
                // C's `+`/space flags are defined for SIGNED conversions.
                // Asking for one on an unsigned base is a spec error the
                // caller must have rejected already.
                if *sign != Sign::None && *base != Base::Dec {
                    return None;
                }
                let mut s = String::from("%");
                s.push_str(sign.c_prefix());
                if *zero {
                    s.push('0');
                }
                if *width > 0 {
                    s.push_str(&width.to_string());
                }
                s.push_str(conv);
                Some(s)
            }
        }
    }

    /// Bytes the helper allocates for one formatted value.
    pub fn buf_size(&self) -> usize {
        match self {
            FmtSpec::Float { width, prec, .. } => 64 + 320 + *prec + *width,
            FmtSpec::Int { width, .. } => 64 + *width,
        }
    }
}

const MAX_WIDTH: usize = 1024;
const MAX_PREC: usize = 512;

fn too_big(what: &str, max: usize) -> String {
    format!("format specifier {what} is limited to {max}")
}

struct Cursor {
    chars: Vec<char>,
    i: usize,
}

impl Cursor {
    fn new(text: &str) -> Self {
        Cursor { chars: text.chars().collect(), i: 0 }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }
    /// `Ok(Some(n))` when digits were consumed, `Ok(None)` when there were
    /// none, `Err` when they were consumed but overflow usize (which must be
    /// an error, not a silent width of 0).
    fn digits(&mut self) -> Result<Option<usize>, String> {
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if start == self.i {
            return Ok(None);
        }
        let text: String = self.chars[start..self.i].iter().collect();
        text.parse::<usize>().map(Some).map_err(|_| too_big("width", MAX_WIDTH))
    }
}

fn unsupported(text: &str, supported: &str) -> String {
    format!("unsupported format specifier '{text}' (supported forms: {supported})")
}

/// Parse a spec for a `float` value. An empty spec is the `%f` default.
pub fn parse_float(text: &str) -> Result<FmtSpec, String> {
    let mut c = Cursor::new(text);
    let sign = match c.peek() {
        Some('+') => {
            c.i += 1;
            Sign::Plus
        }
        Some(' ') => {
            c.i += 1;
            Sign::Space
        }
        _ => Sign::None,
    };
    let zero = c.peek() == Some('0');
    if zero {
        c.i += 1;
    }
    let width = c.digits()?.unwrap_or(0);
    if width > MAX_WIDTH {
        return Err(too_big("width", MAX_WIDTH));
    }
    let mut prec = 6usize;
    if c.peek() == Some('.') {
        c.i += 1;
        prec = c.digits()?.ok_or_else(|| unsupported(text, "'.<digits>' after the decimal point"))?;
        if prec > MAX_PREC {
            return Err(too_big("precision", MAX_PREC));
        }
    }
    let kind = match c.peek() {
        Some(k @ ('f' | 'F' | 'e' | 'E' | 'g' | 'G')) => {
            c.i += 1;
            k
        }
        Some(_) => {
            return Err(unsupported(
                text,
                "[sign][0][width][.precision](f|e|E|g|G), e.g. '.2f' or '+10.3e'",
            ))
        }
        None => 'f',
    };
    if c.i != c.chars.len() {
        return Err(unsupported(
            text,
            "[sign][0][width][.precision](f|e|E|g|G), e.g. '.2f' or '+10.3e'",
        ));
    }
    Ok(FmtSpec::Float { sign, zero, width, prec, kind })
}

/// Parse a spec for an `int` value. An empty spec is the `%lld` default.
pub fn parse_int(text: &str) -> Result<FmtSpec, String> {
    let mut c = Cursor::new(text);
    let sign = match c.peek() {
        Some('+') => {
            c.i += 1;
            Sign::Plus
        }
        Some(' ') => {
            c.i += 1;
            Sign::Space
        }
        _ => Sign::None,
    };
    let zero = c.peek() == Some('0');
    if zero {
        c.i += 1;
    }
    let width = c.digits()?.unwrap_or(0);
    if width > MAX_WIDTH {
        return Err(too_big("width", MAX_WIDTH));
    }
    let base = match c.peek() {
        Some('d') => {
            c.i += 1;
            Base::Dec
        }
        Some('x') => {
            c.i += 1;
            Base::HexLower
        }
        Some('X') => {
            c.i += 1;
            Base::HexUpper
        }
        Some('o') => {
            c.i += 1;
            Base::Oct
        }
        Some('b') => {
            c.i += 1;
            Base::Bin
        }
        Some('.') => return Err(String::from("an int takes no precision ('.N') in its format specifier")),
        Some(_) | None => Base::Dec,
    };
    if c.i != c.chars.len() {
        return Err(unsupported(
            text,
            "[sign][0][width](d|x|X|o|b), e.g. '08x' or 'b'",
        ));
    }
    if sign != Sign::None && base != Base::Dec {
        return Err(format!(
            "a sign flag is not supported with base {:?} — the value is unsigned there",
            match base {
                Base::HexLower => 'x',
                Base::HexUpper => 'X',
                Base::Oct => 'o',
                Base::Bin => 'b',
                Base::Dec => 'd',
            }
        ));
    }
    Ok(FmtSpec::Int { sign, zero, width, base })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_forms() {
        assert_eq!(
            parse_float(".2f").unwrap(),
            FmtSpec::Float { sign: Sign::None, zero: false, width: 0, prec: 2, kind: 'f' }
        );
        assert_eq!(parse_float("").unwrap(), FmtSpec::Float {
            sign: Sign::None,
            zero: false,
            width: 0,
            prec: 6,
            kind: 'f'
        });
        assert!(parse_float("").unwrap().is_plain());
        assert_eq!(parse_float("+10.3e").unwrap().c_format().unwrap(), "%+10.3e");
        assert_eq!(parse_float("08.2f").unwrap().c_format().unwrap(), "%08.2f");
        assert!(parse_float(".2q").is_err());
        assert!(parse_float("'").is_err());
        assert!(parse_float("\"; printf(\"").is_err());
    }

    #[test]
    fn int_forms() {
        assert_eq!(parse_int("").unwrap().c_format().unwrap(), "%lld");
        assert_eq!(parse_int("08x").unwrap().c_format().unwrap(), "%08llx");
        assert!(parse_int("b").unwrap().c_format().is_none());
        assert!(parse_int("+x").is_err());
        assert!(parse_int(".2d").is_err());
        assert!(parse_int("%").is_err());
    }
}
