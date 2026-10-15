//! Cuts a byte stream into lines. Bytes arrive in any chunks; a line may span several.

/// What ends a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// `\n`. A `\r` before it is dropped too, so `\r\n` devices work as well.
    Lf,
    /// `\r\n` only; a lone `\n` stays part of the line.
    CrLf,
    /// `\r`. A `\n` after it is dropped with the spaces around the next line.
    Cr,
}

impl Ending {
    pub const ALL: &'static [(&'static str, Ending)] =
        &[("\n", Ending::Lf), ("\r\n", Ending::CrLf), ("\r", Ending::Cr)];

    pub fn parse(s: &str) -> Option<Ending> {
        Self::ALL.iter().find(|(t, _)| *t == s).map(|(_, e)| *e)
    }
}

/// One line that came out of the stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Frame<'a> {
    /// A line without its ending and without spaces around it. Never empty: blank lines are
    /// skipped.
    Line(&'a [u8]),
    /// A line longer than the limit. Its bytes are dropped up to its ending.
    TooLong,
}

pub struct Framer {
    ending: Ending,
    max_len: usize,
    buf: Vec<u8>,
    too_long: bool,
}

impl Framer {
    pub fn new(ending: Ending, max_len: usize) -> Framer {
        Framer { ending, max_len, buf: Vec::with_capacity(max_len.min(1024)), too_long: false }
    }

    pub fn push(&mut self, bytes: &[u8], mut out: impl FnMut(Frame<'_>)) {
        for &b in bytes {
            let ends = match self.ending {
                Ending::Lf => b == b'\n',
                Ending::Cr => b == b'\r',
                Ending::CrLf => b == b'\n' && self.buf.last() == Some(&b'\r'),
            };
            if ends {
                if self.too_long {
                    out(Frame::TooLong);
                } else {
                    let line = self.buf.trim_ascii();
                    if !line.is_empty() {
                        out(Frame::Line(line));
                    }
                }
                self.buf.clear();
                self.too_long = false;
                continue;
            }
            if self.too_long {
                // Remember only the last byte, so a `\r\n` ending is still found.
                self.buf.clear();
                self.buf.push(b);
            } else if self.buf.len() >= self.max_len && !(b == b'\r' && self.ending != Ending::Cr) {
                // (A `\r` just before the ending does not count towards the limit.)
                self.too_long = true;
                self.buf.clear();
                self.buf.push(b);
            } else {
                self.buf.push(b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(ending: Ending, max: usize, chunks: &[&[u8]]) -> Vec<String> {
        let mut f = Framer::new(ending, max);
        let mut out = Vec::new();
        for c in chunks {
            f.push(c, |fr| {
                out.push(match fr {
                    Frame::Line(l) => String::from_utf8_lossy(l).into_owned(),
                    Frame::TooLong => "<too long>".into(),
                })
            });
        }
        out
    }

    #[test]
    fn lines_may_span_chunks() {
        assert_eq!(frames(Ending::Lf, 64, &[b"D:1", b"23\nD:4", b"\n", b"D:5"]), ["D:123", "D:4"]);
    }

    #[test]
    fn each_ending() {
        assert_eq!(frames(Ending::Lf, 64, &[b"a\r\nb\n"]), ["a", "b"], "\\r before \\n is dropped");
        assert_eq!(frames(Ending::CrLf, 64, &[b"a\r", b"\nb\nc\r\n"]), ["a", "b\nc"]);
        assert_eq!(frames(Ending::Cr, 64, &[b"a\rb\r\nc\r"]), ["a", "b", "c"]);
        assert_eq!(Ending::parse("\r\n"), Some(Ending::CrLf));
        assert_eq!(Ending::parse("\t"), None);
    }

    #[test]
    fn blank_lines_and_surrounding_spaces_are_dropped() {
        assert_eq!(frames(Ending::Lf, 64, &[b"\n  \n  D:1 \t\n"]), ["D:1"]);
    }

    #[test]
    fn a_line_over_the_limit_is_reported_once_and_the_next_one_is_fine() {
        assert_eq!(frames(Ending::Lf, 4, &[b"abcd\nabcdef", b"ghij\nok\n"]), ["abcd", "<too long>", "ok"]);
        assert_eq!(frames(Ending::Lf, 4, &[b"abcd\r\n"]), ["abcd"]);
        assert_eq!(
            frames(Ending::CrLf, 4, &[b"abcd\r\nabcdefgh\r", b"\nok\r\n"]),
            ["abcd", "<too long>", "ok"]
        );
    }
}
