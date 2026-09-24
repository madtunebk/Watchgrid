//! Grid layouts of the live view.

use crate::ui::I;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridLayout {
    One,
    Two,
    Three,
    Four,
}

impl GridLayout {
    pub const ALL: [Self; 4] = [Self::One, Self::Two, Self::Three, Self::Four];

    /// Tiles per row (and per column).
    pub fn columns(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
        }
    }

    pub fn cells(self) -> usize {
        self.columns() * self.columns()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2×2",
            Self::Three => "3×3",
            Self::Four => "4×4",
        }
    }

    pub fn icon(self) -> I {
        match self {
            Self::One => I::Square,
            Self::Two => I::Grid2,
            Self::Three => I::Grid3,
            Self::Four => I::Grid4,
        }
    }

    /// Smallest layout that shows `n` cameras.
    pub fn fitting(n: usize) -> Self {
        Self::ALL.into_iter().find(|l| l.cells() >= n).unwrap_or(Self::Four)
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::Three => "3",
            Self::Four => "4",
        }
    }

    pub fn from_key(k: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.key() == k)
    }
}
