//! Errors, as the exceptions the C++ throws.
//!
//! The C++ catches a few exception types at a few places (`ex_parse` in the
//! parser, everything in a partial sub-formula), so the kind is kept, not
//! only the message. Messages are kept because they are part of the output
//! the oracle records.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// `ex_parse`.
    Parse,
    /// `ex_symbol_not_found`.
    SymbolNotFound,
    /// `ex_formula_not_found`.
    FormulaNotFound,
    /// `ex_symbol_mapping_not_found`.
    SymbolMappingNotFound,
    /// `ex_text_style_mapping_not_found`.
    TextStyleMappingNotFound,
    /// `ex_invalid_param`.
    InvalidParam,
    /// `ex_invalid_state`.
    InvalidState,
    /// Any other `ex_tex` subclass that no code catches by type.
    Other,
    /// `std::out_of_range` from the standard library, as an unchecked
    /// `substr` throws it.
    OutOfRange,
    /// A part of MicroTeX this port does not cover yet.
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn parse(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Parse, message)
    }

    pub fn symbol_not_found(name: &str) -> Self {
        Self::new(
            ErrorKind::SymbolNotFound,
            format!("There's no symbol with the name '{name}' defined."),
        )
    }

    pub fn formula_not_found(name: &str) -> Self {
        Self::new(
            ErrorKind::FormulaNotFound,
            format!("There's no predefined formula with the name '{name}"),
        )
    }

    pub fn symbol_mapping_not_found(name: &str) -> Self {
        Self::new(
            ErrorKind::SymbolMappingNotFound,
            format!("No mapping found for the symbol '{name}'!"),
        )
    }

    pub fn text_style_mapping_not_found(name: &str) -> Self {
        Self::new(
            ErrorKind::TextStyleMappingNotFound,
            format!("No mapping found for the text style '{name}'!"),
        )
    }

    pub fn invalid_param(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidParam, message)
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidState, message)
    }

    /// MSVC's message for `std::out_of_range` out of `basic_string`.
    pub fn out_of_range() -> Self {
        Self::new(ErrorKind::OutOfRange, "invalid string position")
    }

    pub fn unsupported(what: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, what)
    }

    pub fn is_parse(&self) -> bool {
        self.kind == ErrorKind::Parse
    }
}

pub type Result<T> = std::result::Result<T, Error>;
