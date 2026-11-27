// SPDX-License-Identifier: GPL-3.0-or-later

//! Zero-cost domain unit wrappers with saturating arithmetic and compile-time size assertions.

use core::fmt;
use core::time::Duration;

use crate::budgets::size_units::iec_size;

/// Byte size or byte budget (`#[repr(transparent)]` over `u64`).
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Bytes(u64);

const _: () = assert!(core::mem::size_of::<Bytes>() == core::mem::size_of::<u64>());

impl Bytes {
    /// Zero bytes.
    pub const ZERO: Self = Self(0);
    /// Maximum representable byte count.
    pub const MAX: Self = Self(u64::MAX);

    /// Construct from a raw byte count.
    #[must_use]
    pub const fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// Construct from whole kibibytes (`1024` bytes each), saturating on overflow.
    #[must_use]
    pub const fn kib(kib: u64) -> Self {
        Self(kib.saturating_mul(1_024))
    }

    /// Construct from whole mebibytes (`1024^2` bytes each), saturating on overflow.
    #[must_use]
    pub const fn mib(mib: u64) -> Self {
        Self(mib.saturating_mul(1_024 * 1_024))
    }

    /// Construct from whole gibibytes (`1024^3` bytes each), saturating on overflow.
    #[must_use]
    pub const fn gib(gib: u64) -> Self {
        Self(gib.saturating_mul(1_024 * 1_024 * 1_024))
    }

    /// Convert a `usize` slice or allocation length into `Bytes`, saturating at `u64::MAX`.
    #[must_use]
    pub fn from_usize_saturating(len: usize) -> Self {
        Self(u64::try_from(len).unwrap_or(u64::MAX))
    }

    /// Return the underlying raw byte count.
    #[must_use]
    pub const fn as_bytes(self) -> u64 {
        self.0
    }

    /// Alias for [`Self::as_bytes`].
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Return `true` when zero bytes are held.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Convert to `usize` if representable on the host target.
    #[must_use]
    pub fn try_into_usize(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }

    /// Add two byte counts, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Subtract `rhs`, saturating at `0`.
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// Multiply by a scalar factor, saturating at `u64::MAX`.
    #[must_use]
    pub const fn saturating_mul(self, factor: u64) -> Self {
        Self(self.0.saturating_mul(factor))
    }

    /// Checked addition returning `None` on overflow.
    #[must_use]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(total) => Some(Self(total)),
            None => None,
        }
    }

    /// Checked subtraction returning `None` on underflow.
    #[must_use]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(diff) => Some(Self(diff)),
            None => None,
        }
    }

    /// Format using LushText's IEC tier formatter (`crate::budgets::size_units::iec_size`).
    #[must_use]
    pub fn format_iec(self) -> String {
        iec_size(self.0)
    }
}

impl From<u64> for Bytes {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} B", self.0)
    }
}

/// Whole scale-independent logical pixels (`#[repr(transparent)]` over `i32`).
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Sp(i32);

const _: () = assert!(core::mem::size_of::<Sp>() == core::mem::size_of::<i32>());

impl Sp {
    /// Zero logical pixels.
    pub const ZERO: Self = Self(0);

    /// Wrap an integer logical pixel count.
    #[must_use]
    pub const fn new(px: i32) -> Self {
        Self(px)
    }

    /// Return the underlying whole-pixel `i32`.
    #[must_use]
    pub const fn as_sp(self) -> i32 {
        self.0
    }

    /// Alias for [`Self::as_sp`].
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// Return `true` when strictly greater than zero pixels.
    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    /// Return `true` when equal to zero pixels.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Add two pixel spans with saturating `i32` arithmetic.
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Subtract two pixel spans with saturating `i32` arithmetic.
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// Scale a pixel span by an integer factor with saturating arithmetic.
    #[must_use]
    pub const fn saturating_mul(self, factor: i32) -> Self {
        Self(self.0.saturating_mul(factor))
    }

    /// Clamp within `[min, max]` without panicking if `min > max` (returns `min`).
    #[must_use]
    pub const fn clamp_between(self, min: Self, max: Self) -> Self {
        if min.0 > max.0 {
            return min;
        }
        if self.0 < min.0 {
            min
        } else if self.0 > max.0 {
            max
        } else {
            self
        }
    }
}

impl From<i32> for Sp {
    fn from(value: i32) -> Self {
        Self(value)
    }
}

impl fmt::Display for Sp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}sp", self.0)
    }
}

/// Duration in microseconds (`#[repr(transparent)]` over `u64`).
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Micros(u64);

const _: () = assert!(core::mem::size_of::<Micros>() == core::mem::size_of::<u64>());

impl Micros {
    /// Zero microseconds.
    pub const ZERO: Self = Self(0);

    /// Construct from microseconds.
    #[must_use]
    pub const fn from_us(us: u64) -> Self {
        Self(us)
    }

    /// Construct from milliseconds, saturating on overflow.
    #[must_use]
    pub const fn from_ms(ms: u64) -> Self {
        Self(ms.saturating_mul(1_000))
    }

    /// Return raw microseconds.
    #[must_use]
    pub const fn as_us(self) -> u64 {
        self.0
    }

    /// Alias for [`Self::as_us`].
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Return `true` when zero microseconds.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Convert to [`Duration`].
    #[must_use]
    pub const fn as_duration(self) -> Duration {
        Duration::from_micros(self.0)
    }

    /// Add microseconds with saturation.
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Subtract microseconds with saturation at zero.
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// Multiply microseconds by a scalar with saturation.
    #[must_use]
    pub const fn saturating_mul(self, factor: u64) -> Self {
        Self(self.0.saturating_mul(factor))
    }

    /// Checked addition returning `None` on overflow.
    #[must_use]
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(total) => Some(Self(total)),
            None => None,
        }
    }

    /// Checked subtraction returning `None` on underflow.
    #[must_use]
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(diff) => Some(Self(diff)),
            None => None,
        }
    }
}

impl From<u64> for Micros {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for Micros {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}us", self.0)
    }
}

/// Unicode scalar character count (`#[repr(transparent)]` over `u64`).
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct CharCount(u64);

const _: () = assert!(core::mem::size_of::<CharCount>() == core::mem::size_of::<u64>());

impl CharCount {
    /// Zero characters.
    pub const ZERO: Self = Self(0);

    /// Construct from a Unicode scalar count.
    #[must_use]
    pub const fn new(chars: u64) -> Self {
        Self(chars)
    }

    /// Convert from `usize`, saturating at `u64::MAX`.
    #[must_use]
    pub fn from_usize_saturating(chars: usize) -> Self {
        Self(u64::try_from(chars).unwrap_or(u64::MAX))
    }

    /// Return raw Unicode scalar count.
    #[must_use]
    pub const fn as_chars(self) -> u64 {
        self.0
    }

    /// Alias for [`Self::as_chars`].
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Return `true` when zero characters.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Add character counts with saturation.
    #[must_use]
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// Subtract character counts with saturation at zero.
    #[must_use]
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }

    /// Multiply by a scalar factor with saturation.
    #[must_use]
    pub const fn saturating_mul(self, factor: u64) -> Self {
        Self(self.0.saturating_mul(factor))
    }
}

impl From<u64> for CharCount {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for CharCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} chars", self.0)
    }
}

/// Zero-based line index (`#[repr(transparent)]` over `u32`).
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct LineIndex(u32);

const _: () = assert!(core::mem::size_of::<LineIndex>() == core::mem::size_of::<u32>());

impl LineIndex {
    /// First zero-based line index (`0`).
    pub const ZERO: Self = Self(0);

    /// Construct from a zero-based `u32` line index.
    #[must_use]
    pub const fn new(line: u32) -> Self {
        Self(line)
    }

    /// Return the underlying `u32` index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Convert to `usize` losslessly on 32-bit/64-bit targets.
    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }

    /// Advance to the next line index, returning `None` at `u32::MAX`.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }

    /// Advance by one line, saturating at `u32::MAX`.
    #[must_use]
    pub const fn saturating_next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// Add `delta` lines, saturating at `u32::MAX`.
    #[must_use]
    pub const fn saturating_add(self, delta: u32) -> Self {
        Self(self.0.saturating_add(delta))
    }
}

impl From<u32> for LineIndex {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl fmt::Display for LineIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_conversions_and_saturating_math_behave_without_panics() {
        assert_eq!(Bytes::kib(64).as_bytes(), 64 * 1_024);
        assert_eq!(Bytes::mib(2).as_bytes(), 2 * 1_024 * 1_024);
        assert_eq!(Bytes::gib(1).as_bytes(), 1_024 * 1_024 * 1_024);
        assert_eq!(Bytes::MAX.saturating_add(Bytes::from_bytes(1)), Bytes::MAX);
        assert_eq!(
            Bytes::ZERO.saturating_sub(Bytes::from_bytes(5)),
            Bytes::ZERO
        );
        assert_eq!(Bytes::kib(8).format_iec(), "8 KiB");

        assert_eq!(Sp::new(10).saturating_add(Sp::new(20)), Sp::new(30));
        assert_eq!(
            Sp::new(10).clamp_between(Sp::new(20), Sp::new(50)),
            Sp::new(20)
        );
        assert_eq!(
            Sp::new(60).clamp_between(Sp::new(20), Sp::new(50)),
            Sp::new(50)
        );

        assert_eq!(Micros::from_ms(16), Micros::from_us(16_000));
        assert_eq!(
            Micros::from_us(8_000).as_duration(),
            Duration::from_micros(8_000)
        );

        assert_eq!(CharCount::from_usize_saturating(42), CharCount::new(42));
        assert_eq!(LineIndex::new(u32::MAX).next(), None);
        assert_eq!(LineIndex::new(4).saturating_next(), LineIndex::new(5));
    }
}
