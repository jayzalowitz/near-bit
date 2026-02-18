use std::fmt;
use std::fmt::Write;

/// An error which can be returned when parsing a NEAR Account ID.
#[derive(Eq, Clone, Debug, PartialEq)]
pub struct ParseAccountError {
    pub(crate) kind: ParseErrorKind,
    pub(crate) char: Option<(usize, char)>,
}

impl ParseAccountError {
    /// Returns the specific cause why parsing the Account ID failed.
    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }
}

impl std::error::Error for ParseAccountError {}
impl fmt::Display for ParseAccountError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut buf = self.kind.to_string();
        if let Some((idx, char)) = self.char {
            write!(buf, " {:?} at index {}", char, idx)?
        }
        buf.fmt(f)
    }
}

/// A list of errors that occur when parsing an invalid Account ID.
#[non_exhaustive]
#[derive(Eq, Clone, Debug, PartialEq)]
pub enum ParseErrorKind {
    TooLong,
    TooShort,
    RedundantSeparator,
    InvalidChar,
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseErrorKind::TooLong => "the Account ID is too long".fmt(f),
            ParseErrorKind::TooShort => "the Account ID is too short".fmt(f),
            ParseErrorKind::RedundantSeparator => "the Account ID has a redundant separator".fmt(f),
            ParseErrorKind::InvalidChar => "the Account ID contains an invalid character".fmt(f),
        }
    }
}
