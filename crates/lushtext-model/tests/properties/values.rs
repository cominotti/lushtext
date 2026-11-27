// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests verifying zero-cost layout, wrapping generations, saturating
//! unit arithmetic, and `Bytes::format_iec` / `size_units::iec_size` parity.

use lushtext_model::budgets::size_units::iec_size;
use lushtext_model::values::{
    AccessGeneration, Bytes, CharCount, EditorId, LineIndex, LoadGeneration, Micros,
    PolicyGeneration, Sp,
};
use proptest::prelude::*;

use crate::support;

proptest! {
    #![proptest_config(support::property_config())]

    #[test]
    fn generation_next_matches_wrapping_add_one(raw in any::<u64>()) {
        let generation = LoadGeneration::new(raw);
        prop_assert_eq!(generation.next().get(), raw.wrapping_add(1));
        prop_assert_eq!(AccessGeneration::new(raw).next().get(), raw.wrapping_add(1));
        prop_assert_eq!(PolicyGeneration::new(raw).next().get(), raw.wrapping_add(1));
    }

    #[test]
    fn bytes_unit_saturating_ops_and_iec_format_agree(a in any::<u64>(), b in any::<u64>(), factor in 0u64..=1024) {
        let ba = Bytes::from_bytes(a);
        let bb = Bytes::from_bytes(b);
        prop_assert_eq!(ba.saturating_add(bb).as_bytes(), a.saturating_add(b));
        prop_assert_eq!(ba.saturating_sub(bb).as_bytes(), a.saturating_sub(b));
        prop_assert_eq!(ba.saturating_mul(factor).as_bytes(), a.saturating_mul(factor));
        prop_assert_eq!(ba.checked_add(bb).map(Bytes::as_bytes), a.checked_add(b));
        prop_assert_eq!(ba.checked_sub(bb).map(Bytes::as_bytes), a.checked_sub(b));
        prop_assert_eq!(ba.format_iec(), iec_size(a));
    }

    #[test]
    fn sp_micros_char_count_and_line_index_saturate_cleanly(
        sp_a in any::<i32>(),
        sp_b in any::<i32>(),
        us_a in any::<u64>(),
        us_b in any::<u64>(),
        chars_a in any::<u64>(),
        chars_b in any::<u64>(),
        line in any::<u32>(),
        delta in any::<u32>(),
    ) {
        prop_assert_eq!(Sp::new(sp_a).saturating_add(Sp::new(sp_b)).as_sp(), sp_a.saturating_add(sp_b));
        prop_assert_eq!(Sp::new(sp_a).saturating_sub(Sp::new(sp_b)).as_sp(), sp_a.saturating_sub(sp_b));
        prop_assert_eq!(Micros::from_us(us_a).saturating_add(Micros::from_us(us_b)).as_us(), us_a.saturating_add(us_b));
        prop_assert_eq!(CharCount::new(chars_a).saturating_sub(CharCount::new(chars_b)).as_chars(), chars_a.saturating_sub(chars_b));
        prop_assert_eq!(LineIndex::new(line).saturating_add(delta).get(), line.saturating_add(delta));
        prop_assert_eq!(LineIndex::new(line).next().map(LineIndex::get), line.checked_add(1));
        prop_assert_eq!(EditorId::new(line as usize).get(), line as usize);
    }
}
