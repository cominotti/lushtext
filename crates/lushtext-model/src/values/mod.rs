// SPDX-License-Identifier: GPL-3.0-or-later

//! Zero-cost domain value objects and unit/generation wrappers.
//!
//! Every type in this module is `#[repr(transparent)]`, `Copy`, `const fn`,
//! `#[serde(transparent)]`, Kani-compatible (`#[cfg_attr(kani, derive(kani::Arbitrary))]`),
//! and verified at compile time to match the byte size of its underlying primitive.
//!
//! Boundary discipline ("no newtype soup"): use these wrappers across module/workflow
//! seams (`seams.rs`, `policy.rs`, state structs, and unit-carrying thresholds),
//! while keeping local loop indices (`usize`) and single-function temporaries as
//! primitives.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

/// Define a zero-cost `#[repr(transparent)]` monotonic generation counter.
///
/// The generated type derives `Copy`, `Default`, `Ord`, `Hash`, transparent `serde`
/// serialization, and `kani::Arbitrary` under `cfg(kani)`, plus a compile-time
/// `size_of` assertion proving zero layout overhead.
#[macro_export]
macro_rules! define_generation {
    ($(#[doc = $doc:literal])* $vis:vis $name:ident($inner:ty)) => {
        $(#[doc = $doc])*
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
        $vis struct $name($inner);

        const _: () = assert!(::core::mem::size_of::<$name>() == ::core::mem::size_of::<$inner>());

        impl $name {
            /// Initial zero generation.
            pub const ZERO: Self = Self(0);

            /// Wrap a raw primitive counter as this generation type.
            #[must_use]
            pub const fn new(raw: $inner) -> Self {
                Self(raw)
            }

            /// Return the underlying raw primitive counter.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }

            /// Return `true` when the counter is still at [`Self::ZERO`].
            #[must_use]
            pub const fn is_zero(self) -> bool {
                self.0 == 0
            }

            /// Advance monotonically using `wrapping_add(1)`.
            #[must_use]
            pub const fn next(self) -> Self {
                Self(self.0.wrapping_add(1))
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

/// Define a zero-cost `#[repr(transparent)]` typed domain identifier.
#[macro_export]
macro_rules! define_id {
    ($(#[doc = $doc:literal])* $vis:vis $name:ident($inner:ty)) => {
        $(#[doc = $doc])*
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
        $vis struct $name($inner);

        const _: () = assert!(::core::mem::size_of::<$name>() == ::core::mem::size_of::<$inner>());

        impl $name {
            /// Wrap a raw primitive identifier as this domain ID type.
            #[must_use]
            pub const fn new(raw: $inner) -> Self {
                Self(raw)
            }

            /// Return the underlying raw primitive identifier.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

pub mod generations;
pub mod ids;
pub mod units;

pub use generations::*;
pub use ids::*;
pub use units::{Bytes, CharCount, LineIndex, Micros, Sp};
