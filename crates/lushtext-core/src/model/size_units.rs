// SPDX-License-Identifier: GPL-3.0-or-later

//! The one formatter for sizes that name a tier in user-visible text
//! (`power-of-two-thresholds`, design D3): the open refusal, the local-history
//! limit, the Replace All skips. Tiers are powers of two in IEC units, so they
//! print whole ("64 MiB"); any other size prints with one decimal ("476.8
//! MiB"), as `glib::format_size` with IEC units does. GTK-free, so the
//! services that write those messages share it with the UI.

/// Bytes in one KiB.
const KIB: u64 = 1024;

/// `bytes` in the largest IEC unit (B, KiB, MiB, GiB) it reaches: a whole
/// number when it is an exact multiple of that unit, otherwise one decimal,
/// rounded down so a size just above a tier never prints as the tier.
#[must_use]
pub fn iec_size(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KiB", "MiB", "GiB"];
    if bytes < KIB {
        return format!("{bytes} B");
    }
    let mut unit = KIB;
    let mut name = UNITS[0];
    for (index, candidate) in UNITS.iter().enumerate().skip(1) {
        let next = KIB.pow(u32::try_from(index).unwrap_or(0) + 1);
        if bytes < next {
            break;
        }
        unit = next;
        name = candidate;
    }
    let whole = bytes / unit;
    let tenths = (bytes % unit).saturating_mul(10) / unit;
    if bytes.is_multiple_of(unit) {
        format!("{whole} {name}")
    } else {
        format!("{whole}.{tenths} {name}")
    }
}

#[cfg(test)]
mod tests {
    use super::iec_size;

    #[test]
    fn tiers_print_whole_and_other_sizes_with_one_decimal() {
        assert_eq!(iec_size(0), "0 B");
        assert_eq!(iec_size(1023), "1023 B");
        assert_eq!(iec_size(1024), "1 KiB");
        assert_eq!(iec_size(8 * 1024 * 1024), "8 MiB");
        assert_eq!(iec_size(64 * 1024 * 1024), "64 MiB");
        assert_eq!(iec_size(512 * 1024 * 1024), "512 MiB");
        assert_eq!(iec_size(500_000_000), "476.8 MiB");
        assert_eq!(iec_size(8 * 1024 * 1024 + 1), "8.0 MiB");
        assert_eq!(iec_size(3 * 1024 * 1024 * 1024), "3 GiB");
        assert_eq!(iec_size(u64::MAX), "17179869183.9 GiB");
    }
}
