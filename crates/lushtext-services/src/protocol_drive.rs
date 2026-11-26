// SPDX-License-Identifier: GPL-3.0-or-later

//! The one loop every proven-core shell runs its core with.
//!
//! The durable write, move and rename protocols
//! (`filesystem/write_protocol.rs`), the Replace All file and undo protocols
//! and the journal shrink (`content_search/replace/protocol.rs`) are
//! I/O-free, Kani-checked cores: each decides the next action from the
//! outcome of the last one. Their shells execute those actions. Before this
//! module each shell hand-copied the same loop (take the action, stop at the
//! terminal one, make one call, feed its outcome back); now each implements
//! [`Protocol`] beside itself and calls [`drive`], so the loop, its
//! termination and its outcome feedback exist once.
//!
//! The trait describes how a shell runs a core, not a decision of a core, so
//! it lives here with the shells and not in either Kani oracle module: the
//! cores and their harnesses stay byte-identical. Each core's harness already
//! proves that it reaches its terminal action within a bound, and [`drive`]
//! is exactly the loop those harnesses run.
//!
//! [`drive`] is also the single site every action and every outcome of every
//! shell passes through, which is where a conformance tap observes a run.

/// A Kani-checked core as its shell drives it.
pub(crate) trait Protocol {
    /// The next operation the core asks for.
    type Action: Copy;
    /// What executing one action produced, fed back unchanged.
    type Outcome;
    /// How the run ended, read off the terminal action.
    type End;

    /// The end `action` denotes, when it is the terminal action.
    fn end(action: Self::Action) -> Option<Self::End>;

    /// Feed the outcome of the last action; get the next one.
    fn advance(&mut self, outcome: Self::Outcome) -> Self::Action;
}

/// Run `protocol` from `first`, executing each action with exactly one
/// `execute` call and feeding its outcome back unchanged.
///
/// The terminal action is never executed: its end is returned. `execute` may
/// itself drive another protocol (a durable move's copy runs a whole durable
/// write), which nests on the same thread.
pub(crate) fn drive<P: Protocol>(
    protocol: &mut P,
    first: P::Action,
    mut execute: impl FnMut(P::Action) -> P::Outcome,
) -> P::End {
    let mut action = first;
    loop {
        if let Some(end) = P::end(action) {
            return end;
        }
        action = protocol.advance(execute(action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A counting protocol: actions `0..len`, then the terminal `len`. Each
    /// outcome is recorded, and the next action is the last one plus one.
    struct Counting {
        len: u32,
        last: u32,
        fed: Vec<i64>,
    }

    impl Counting {
        fn new(len: u32) -> Self {
            Self {
                len,
                last: 0,
                fed: Vec::new(),
            }
        }
    }

    /// The terminal action carries the protocol length in its high bits so
    /// that `end` can recognise it without the protocol's state.
    const TERMINAL: u32 = 1 << 31;

    impl Protocol for Counting {
        type Action = u32;
        type Outcome = i64;
        type End = u32;

        fn end(action: u32) -> Option<u32> {
            (action & TERMINAL != 0).then_some(action & !TERMINAL)
        }

        fn advance(&mut self, outcome: i64) -> u32 {
            self.fed.push(outcome);
            self.last += 1;
            if self.last == self.len {
                TERMINAL | self.len
            } else {
                self.last
            }
        }
    }

    #[test]
    fn drive_stops_at_the_terminal_action_without_executing_it() {
        let mut protocol = Counting::new(3);
        let mut executed = Vec::new();
        let end = drive(&mut protocol, 0, |action| {
            assert_eq!(
                action & TERMINAL,
                0,
                "the terminal action is never executed"
            );
            executed.push(action);
            0
        });
        assert_eq!(end, 3);
        assert_eq!(executed.len(), 3);

        let mut none = Counting::new(0);
        let end = drive(&mut none, TERMINAL, |_| unreachable!("nothing to execute"));
        assert_eq!(end, 0);
        assert!(none.fed.is_empty());
    }

    #[test]
    fn drive_executes_each_other_action_exactly_once_in_order() {
        let mut protocol = Counting::new(5);
        let mut executed = Vec::new();
        drive(&mut protocol, 0, |action| {
            executed.push(action);
            0
        });
        assert_eq!(executed, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn drive_feeds_every_outcome_back_unchanged() {
        let mut protocol = Counting::new(4);
        drive(&mut protocol, 0, |action| -i64::from(action) - 7);
        assert_eq!(protocol.fed, vec![-7, -8, -9, -10]);
    }

    #[test]
    fn drive_supports_a_nested_drive_from_inside_execute() {
        let mut outer = Counting::new(2);
        let mut trace = Vec::new();
        drive(&mut outer, 0, |action| {
            trace.push(format!("outer {action}"));
            let mut inner = Counting::new(2);
            let inner_end = drive(&mut inner, 0, |inner_action| {
                trace.push(format!("inner {inner_action}"));
                1
            });
            assert_eq!(inner.fed, vec![1, 1]);
            i64::from(inner_end)
        });
        assert_eq!(
            trace,
            vec![
                "outer 0", "inner 0", "inner 1", "outer 1", "inner 0", "inner 1"
            ]
        );
        assert_eq!(outer.fed, vec![2, 2]);
    }

    /// The files holding the six shells: three in `durable_write.rs`, and the
    /// Replace All apply, undo and journal-shrink shells.
    const SHELL_SOURCES: [(&str, &str); 4] = [
        ("durable_write.rs", include_str!("durable_write.rs")),
        (
            "content_search/replace/apply.rs",
            include_str!("content_search/replace/apply.rs"),
        ),
        (
            "content_search/replace/undo.rs",
            include_str!("content_search/replace/undo.rs"),
        ),
        ("search_backup.rs", include_str!("search_backup.rs")),
    ];

    /// The byte spans of every `fn advance(` body in `source`.
    fn advance_bodies(source: &str) -> Vec<std::ops::Range<usize>> {
        let mut spans = Vec::new();
        for (start, _) in source.match_indices("fn advance(") {
            let open = start + source[start..].find('{').expect("advance has a body");
            let mut depth = 0usize;
            for (offset, byte) in source.as_bytes()[open..].iter().enumerate() {
                match byte {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            spans.push(open..open + offset);
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        spans
    }

    /// The lines of `source` that step a core outside a
    /// `Protocol::advance` body.
    fn steps_outside_advance(source: &str) -> Vec<&str> {
        let advance = advance_bodies(source);
        [".step(", "shrink_step("]
            .iter()
            .flat_map(|call| source.match_indices(call))
            .filter(|(at, _)| !advance.iter().any(|span| span.contains(at)))
            .map(|(at, _)| {
                let line_start = source[..at].rfind('\n').map_or(0, |line| line + 1);
                let line_end = source[at..]
                    .find('\n')
                    .map_or(source.len(), |line| at + line);
                source[line_start..line_end].trim()
            })
            .filter(|line| !line.starts_with("//") && !line.starts_with("use "))
            .collect()
    }

    /// No shell steps its core in a loop of its own: every core's `step` (or
    /// `shrink_step`) is called only inside that core's `Protocol::advance`,
    /// so [`drive`] is the one loop. A shell that takes the design's
    /// keep-the-loop fallback records it in its module documentation and is
    /// removed from [`SHELL_SOURCES`] with that reason.
    #[test]
    fn no_shell_steps_its_core_outside_protocol_advance() {
        for (file, source) in SHELL_SOURCES {
            assert!(
                source.contains("impl Protocol for"),
                "{file} implements `Protocol` for its core"
            );
            assert_eq!(
                steps_outside_advance(source),
                Vec::<&str>::new(),
                "{file} steps its core outside `Protocol::advance`"
            );
        }
    }

    #[test]
    fn the_source_check_finds_a_hand_written_shell_loop() {
        let shell = "fn run() {\n    loop {\n        action = protocol.step(outcome);\n    }\n}\n\
                     impl Protocol for P {\n    fn advance(&mut self, o: O) -> A {\n        self.step(o)\n    }\n}\n";
        assert_eq!(
            steps_outside_advance(shell),
            vec!["action = protocol.step(outcome);"]
        );
        let shrink = "let next = shrink_step(action, true);\n";
        assert_eq!(
            steps_outside_advance(shrink),
            vec!["let next = shrink_step(action, true);"]
        );
    }
}
