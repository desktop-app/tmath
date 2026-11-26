//! The enums of `utils/enums.h` the layout code shares.

/// `tex::AtomType`. The values match the C++, which indexes tables by them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AtomType {
    None = -1,
    Ordinary = 0,
    BigOperator = 1,
    BinaryOperator = 2,
    Relation = 3,
    Opening = 4,
    Closing = 5,
    Punctuation = 6,
    Inner = 7,
    Accent = 10,
    InterText = 11,
    MultiColumn = 12,
    HLine = 13,
    MultiRow = 14,
}

impl AtomType {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            -1 => Self::None,
            0 => Self::Ordinary,
            1 => Self::BigOperator,
            2 => Self::BinaryOperator,
            3 => Self::Relation,
            4 => Self::Opening,
            5 => Self::Closing,
            6 => Self::Punctuation,
            7 => Self::Inner,
            10 => Self::Accent,
            11 => Self::InterText,
            12 => Self::MultiColumn,
            13 => Self::HLine,
            14 => Self::MultiRow,
            _ => return None,
        })
    }
}

/// `tex::SpaceType`: a glue type, negated for the negative skips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceType {
    NegThickMuSkip = -3,
    NegMedMuSkip = -2,
    NegThinMuSkip = -1,
    None = 0,
    ThinMuSkip = 1,
    MedMuSkip = 2,
    ThickMuSkip = 3,
}

impl SpaceType {
    /// `SpaceType::quad` shares its value with the thick skip.
    pub const QUAD: Self = Self::ThickMuSkip;

    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            -3 => Self::NegThickMuSkip,
            -2 => Self::NegMedMuSkip,
            -1 => Self::NegThinMuSkip,
            0 => Self::None,
            1 => Self::ThinMuSkip,
            2 => Self::MedMuSkip,
            3 => Self::ThickMuSkip,
            _ => return None,
        })
    }
}

/// `tex::UnitType`. `None` converts to nothing, as any value outside the
/// named units does in the C++.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    None = -1,
    Em = 0,
    Ex = 1,
    Pixel = 2,
    Point = 3,
    Pica = 4,
    Mu = 5,
    Cm = 6,
    Mm = 7,
    In = 8,
    Sp = 9,
    Pt = 10,
    Dd = 11,
    Cc = 12,
    X8 = 13,
}

impl Unit {
    /// Values without a conversion become `None`, which converts the same.
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Em,
            1 => Self::Ex,
            2 => Self::Pixel,
            3 => Self::Point,
            4 => Self::Pica,
            5 => Self::Mu,
            6 => Self::Cm,
            7 => Self::Mm,
            8 => Self::In,
            9 => Self::Sp,
            10 => Self::Pt,
            11 => Self::Dd,
            12 => Self::Cc,
            13 => Self::X8,
            _ => Self::None,
        }
    }
}
