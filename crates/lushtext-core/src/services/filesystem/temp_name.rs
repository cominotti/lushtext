// SPDX-License-Identifier: GPL-3.0-or-later

//! The **one owner** of the durable-write temp-name format,
//! `.{file}.{tag}.{pid}.{seq}.tmp`.
//!
//! The durable-write builder ([`format()`]) and the crash-leftover sweep
//! predicate ([`parse`]) both come here, so the two can never disagree about
//! what a LushText temp file looks like: a name the builder emits always
//! parses, and the sweep never recognises a name the builder would not emit.
//! Tags come from the closed [`WriteLabel`] set.

use super::types::WriteLabel;

/// Build the temp name for `file_name` written with `tag` by process `pid`
/// with sequence number `sequence`.
#[must_use]
pub fn format(file_name: &str, tag: &str, pid: u32, sequence: u64) -> String {
    format!(".{file_name}.{tag}.{pid}.{sequence}{TEMP_SUFFIX}")
}

/// The sequence field for this launch's `counter`-th temp name: the launch
/// nonce in the high 32 bits, the low 32 bits of the counter below it.
#[must_use]
pub const fn sequence(launch_nonce: u32, counter: u64) -> u64 {
    ((launch_nonce as u64) << 32) | (counter & u32::MAX as u64)
}

/// Suffix every durable-write temp name ends with.
const TEMP_SUFFIX: &str = ".tmp";

/// The parts of a durable-write temp name, parsed from the right.
///
/// `{file}` may itself contain dots, so the fixed-shape fields are split off
/// the end and whatever remains is the target's file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableTempName<'a> {
    /// The target file name the temp file was written for.
    pub target_file: &'a str,
    /// The write label that created it.
    pub tag: &'a str,
    /// The process id that created it.
    pub pid: u32,
    /// The process-local sequence number.
    pub sequence: u64,
}

impl DurableTempName<'_> {
    /// The launch nonce the sequence field carries (see [`sequence`]).
    #[must_use]
    pub const fn launch_nonce(&self) -> u32 {
        (self.sequence >> 32) as u32
    }
}

/// Parse `name` as `.{file}.{tag}.{pid}.{seq}.tmp`, exactly as built by the
/// durable-write temp-name builder, with a tag from [`WriteLabel::KNOWN`].
///
/// Returns `None` for anything else, including non-UTF-8 names, unknown tags,
/// and numbers the builder would not print (signs, leading zeros, overflow).
#[must_use]
pub fn parse(name: &str) -> Option<DurableTempName<'_>> {
    let rest = name.strip_prefix('.')?.strip_suffix(TEMP_SUFFIX)?;
    let (rest, sequence) = rest.rsplit_once('.')?;
    let (rest, pid) = rest.rsplit_once('.')?;
    let (target_file, tag) = rest.rsplit_once('.')?;
    if target_file.is_empty() || !WriteLabel::is_known(tag) {
        return None;
    }
    Some(DurableTempName {
        target_file,
        tag,
        pid: parse_canonical_decimal(pid)?,
        sequence: parse_canonical_decimal(sequence)?,
    })
}

/// Parse an unsigned decimal exactly as `format!("{}")` prints it.
fn parse_canonical_decimal<T: std::str::FromStr>(digits: &str) -> Option<T> {
    let canonical = !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'));
    if canonical { digits.parse().ok() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_the_builder_emits_for_a_known_label_parses_back() {
        for label in WriteLabel::KNOWN {
            for file in ["manifest.json", "notes.v2.md", "abc.7.draft"] {
                let name = format(file, label.as_str(), 4242, 1 << 40 | 7);
                assert_eq!(
                    parse(&name),
                    Some(DurableTempName {
                        target_file: file,
                        tag: label.as_str(),
                        pid: 4242,
                        sequence: 1 << 40 | 7,
                    }),
                    "{name}"
                );
            }
        }
        assert_eq!(parse(&format("a.txt", "not-a-label", 1, 1)), None);
    }

    #[test]
    fn the_sequence_field_carries_the_launch_nonce() {
        let name = format(
            "a.txt",
            WriteLabel::KNOWN[0].as_str(),
            1,
            sequence(9, u64::MAX),
        );
        let parsed = parse(&name).expect("builder output parses");
        assert_eq!(parsed.launch_nonce(), 9);
        assert_eq!(parsed.sequence & u64::from(u32::MAX), u64::from(u32::MAX));
    }
}
