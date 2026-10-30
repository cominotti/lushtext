## ADDED Requirements

### Requirement: The post-open toolkit costs are ledger axioms
The ledger SHALL record seven axioms, provisionally S1–S7, with the next free ids at the time they are applied. A42–A45 belong to `document-scale-policy`, A46–A51 are applied, and `long-line-protected-view` reserves P1–P8.

- **S1.** A GtkSourceView language whose definition embeds others (HTML) parses its definitions when it is first attached to a buffer, and the cost of that parse, recorded per language. Whether the parsed definitions are shared through the language object across buffers. Whether a private language manager and a buffer created, attached and used on a worker thread, and then handed to the GTK thread and used only there, attach without any GDK, style-manager, style-scheme-manager or main-context access on the worker. Whether a language object from that second manager attaches and highlights on the GTK thread exactly as one from the application's own manager.
- **S2.** The cost of `set_highlight_syntax(true)` against buffer size, with a language attached. The cost of attaching a language to an empty buffer followed by sliced inserts, and the highlight idles those inserts schedule.
- **S3.** The context engine's background analysis runs in dispatches of a fixed slice that the application cannot configure, with the slice constant cited from the GtkSourceView source of the verified version. The priority of those dispatches, and whether the engine reschedules through an idle or a timeout, together with the gap between slices.
- **S4.** GtkTextView's validation dispatch after an insert lays out only the inserted, invalid lines, and its cost grows with their visible characters, in either wrap mode. When the next insert waits for the frame that lays out the previous one, each validation dispatch holds one insert's lines. This extends A31 to inserted lines.
- **S5.** Whether the first write of a keyfile-backend settings file that does not exist yet makes the backend's file monitor reload it and emit `changed` on other instances, including GTK's own settings. This extends A41 to a fresh profile.
- **S6.** Whether the style carried by a lowest-priority tag, through `tabs`, `family` and a relative `scale`, lays out like the view-wide properties. Whether re-tagging a range invalidates only that range. Whether a view outside the `.monospace` zoom and font CSS keeps its gutter usable. What a second view of the same buffer (a `GtkSourceMap`) and the print compositor show under that tag, and whether character-width consumers (right margin, readable-column margins) are right from the tag's metrics. And, for the fallback, whether hiding the paragraphs outside the visible window and revealing them in ranges keeps the scroll anchor and insert mark, and what search, AT-SPI and the minimap see meanwhile.
- **S7.** Whether a `pixels-below-lines` tag can carry the end-of-document overscroll: on an empty last paragraph, for the current-line highlight, the gutter, a `GtkSourceMap` and printing. And the cost of changing `bottom-margin` on a dense document, in both wrap modes.

Each entry SHALL be pinned by an isolated `gtk-lush-axioms` probe, with a sample that has an interactive and a `--check` mode. A probe that records costs SHALL record magnitudes, never a verdict. The post-open settle design SHALL cite the ids.

#### Scenario: The post-open axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the S1–S7 probes run under the private headless session, and any CRITICAL or WARNING they provoke fails the lane unless the probe captures it on purpose
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probes and samples in agreement

#### Scenario: The language parse cannot leave the GTK thread
- **WHEN** the S1 probe shows that the worker handoff touches GDK, the style manager or a main context, or that the parsed definitions are not shared through the language object
- **THEN** the language attach stays a GTK dispatch, deferred to a low-priority idle after input-quiet, and the ledger entry records it as a toolkit-floor exception with its measured cost and the drafted upstream report

#### Scenario: The dense overscroll cannot be a tag
- **WHEN** the S7 probe shows that the tag fails on an empty last paragraph or changes what another consumer of the last line's geometry shows
- **THEN** a dense document's overscroll is a margin set before the install, and a later resize-driven change over `B_SETTLE_US` is recorded as a toolkit-floor exception against S7

#### Scenario: Neither restyle mechanism passes
- **WHEN** the S6 probe shows that neither the tag-carried style nor hide-and-reveal meets its conditions
- **THEN** the user-started invalidation on dense documents is recorded as a toolkit-floor exception against S6, with its measured batch cost and a drafted upstream report
