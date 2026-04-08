---
stepsCompleted: ['step-01-init', 'step-02-discovery', 'step-02b-vision', 'step-02c-executive-summary', 'step-03-success', 'step-04-journeys', 'step-05-domain-skipped', 'step-06-innovation-skipped', 'step-07-project-type', 'step-08-scoping', 'step-09-functional', 'step-10-nonfunctional', 'step-11-polish', 'step-12-complete']
inputDocuments:
  - '_bmad-output/planning-artifacts/research/technical-ripgrep-crate-ecosystem-research-2026-04-06.md'
  - 'docs/next/workspace-content-search.md'
  - '_bmad-output/project-context.md'
workflowType: 'prd'
documentCounts:
  briefs: 0
  research: 1
  brainstorming: 0
  projectDocs: 23
  projectContext: 1
classification:
  projectType: desktop_app
  domain: general
  complexity: low
  projectContext: brownfield
---

# Product Requirements Document - lushtext

**Author:** Danilo
**Date:** 2026-04-06

## Executive Summary

LushText is a minimalist, fast text editor targeting Libadwaita/GNOME. This PRD defines **workspace-wide content search** — the ability to search file contents across all workspace roots at ripgrep-class speed, with streaming results, match highlighting, and click-to-open-at-line. Triggered via `Ctrl+Shift+F`.

Content search is the #1 feature gap between LushText and power-user workflows. Today, users must alt-tab to a terminal and run `rg` or `grep`, then manually open results. No GNOME-native text editor (GNOME Text Editor, Mousepad, xed) offers workspace-wide content search. Users who need it either adopt Electron editors (VS Code, Zed) or maintain a parallel terminal workflow. This feature eliminates that gap.

The target users are developers searching codebases, writers searching across document collections, and system administrators working across configuration files — anyone who manages multiple text files within workspace directories. The primary value is speed and comprehensiveness: results must appear fast enough to feel instant, and the search must surface files the user forgot existed.

The implementation uses the ripgrep crate ecosystem (`grep-searcher`, `grep-regex`, `grep-matcher`, `ignore`) as an in-process library — not a CLI subprocess. This provides parallel file traversal with `.gitignore` awareness, binary file detection, memory-mapping heuristics, and SIMD-accelerated regex matching, all feeding results directly into GTK widgets via channel-based streaming. Technical feasibility has been validated through prior research.

### What Makes This Special

No GTK text editor offers ripgrep-speed content search today. The closest alternatives are terminal tools (fast but no GUI integration) or Electron editors (GUI but heavy and non-native). LushText occupies the empty intersection: **fast + native + integrated**.

The design language follows GNOME HIG conventions with Sublime Text's speed-first minimalism as secondary inspiration — clean, keyboard-driven, zero clutter. This is deliberately not a VS Code search panel port. Users who chose a GNOME-native editor over VS Code did so for a reason; the search experience must honor that choice.

Results stream to the UI as they arrive (not batch-at-completion), creating the perception that the editor already knows the answer. Combined with `.gitignore` filtering and binary detection, the search is both fast and noise-free — users see only what matters.

## Project Classification

- **Project Type:** Desktop Application (native Rust + GTK4/Libadwaita)
- **Domain:** General Software / Developer Productivity
- **Complexity:** Low (no regulatory or compliance concerns)
- **Project Context:** Brownfield — adding a major feature to an existing editor with established architecture, patterns, and conventions

## Success Criteria

### User Success

- **Instant feedback:** Results begin streaming within 500ms of the user stopping typing (after 300ms debounce + search startup). The user should see results appearing *while they're still refining their query* — the speed of response is the primary delight moment.
- **Comprehensiveness:** Searching a workspace surfaces matches across all roots, including files the user hasn't opened recently. `.gitignore`-aware filtering ensures results are noise-free without manual configuration.
- **Zero-friction workflow:** Type query → see streaming results → click match → land on the exact line in the editor. The round-trip from "I need to find something" to "I'm editing it" should feel like one continuous action, not a multi-step process.
- **Replace All confidence:** Users can perform multi-file replacements with enough guardrails (confirmation, preview) that they trust the operation won't destroy their work.
- **GNOME-native feel:** The search panel looks and behaves like it was designed by the GNOME team — Adwaita widgets, HIG-compliant layout, keyboard-driven interaction. Nothing feels transplanted from another platform.

### Business Success

- **Personal delight:** LushText's creator uses content search daily as a natural part of the editing workflow, with no urge to fall back to terminal `rg`.
- **Flathub install growth:** Content search becomes a reason users discover and install LushText. Measurable as an uptick in Flathub installs after the feature ships.
- **GNOME ecosystem positioning:** This feature differentiates LushText from every other GTK text editor. Long-term, it strengthens the case for LushText as a candidate default GNOME text editor.

### Technical Success

- **Performance baseline:** Searching the linux kernel source (~70k files, ~30M lines) returns first results within 500ms and completes within 5 seconds on NVMe storage. Criterion benchmarks establish and track these baselines.
- **Cancellation latency:** Cancelling a search (new query, Escape, panel close) stops background work within 50ms with no visible UI lag.
- **Resource safety:** Bounded channel (1024 items) provides back-pressure. 10,000 result cap prevents OOM. Thread pool capped to prevent thrashing on slow filesystems.
- **Resilience:** Invalid regex shows user-facing error (no panic). Binary files silently skipped. Empty workspaces show "no results." Partial I/O errors (permission denied on individual files) logged but don't abort the search.
- **Test coverage:** Unit tests for service layer (literal, regex, case, word, cancellation, binary skip, gitignore, result cap, multi-root, glob, empty query, invalid regex). Widget tests for panel lifecycle and focus management. Criterion benchmarks for performance regression detection.

### Measurable Outcomes

| Metric | Target | How to Measure |
|---|---|---|
| Time to first result | < 500ms on 70k-file workspace | Criterion benchmark |
| Full search completion | < 5s on 70k-file workspace (NVMe) | Criterion benchmark |
| Cancellation latency | < 50ms | Manual testing + unit test |
| Result cap | 10,000 matches | Unit test |
| UI frame drops during search | 0 dropped frames at 60fps | Manual testing with GTK inspector |
| Test coverage | All 12+ service unit tests passing | CI |

## Product Scope

### MVP — Minimum Viable Product

- Search panel toggled via `Ctrl+Shift+F` with animated reveal (GtkRevealer, matching sidebar pattern)
- Search input with 300ms debounce, streaming results grouped by file in GtkTreeListModel
- Toggle buttons: regex, case sensitivity, whole word
- File glob filter (e.g., `*.rs`, `*.toml`)
- Click-to-open: activate result → open file at matching line
- F4 / Shift+F4 match navigation across files
- Replace All across files with confirmation/preview guardrails
- `.gitignore` respected by default (toggleable)
- Binary file detection and skip
- 10,000 result cap with truncation indicator
- Search history and saved searches
- Cancellation via new query, Escape, or panel close
- Progress reporting in status bar ("Searching 1,234 / 5,678 files...")
- Focus save/restore on panel open/close (matching command palette pattern)
- GSettings persistence: panel visibility, last search options
- Match highlighting in results (Pango markup)
- GNOME HIG-compliant design, Adwaita widgets throughout

### Growth Features (Post-MVP)

- Context lines (before/after match) in results display
- Multi-line / cross-line regex search
- Search scope refinement (search within results, exclude paths)
- Integration with in-editor search bar (Ctrl+F result → Ctrl+Shift+F escalation)

### Vision (Future)

- Incremental index for near-instant repeated searches on unchanged files
- Semantic search / symbol-aware search (leveraging GtkSourceView language specs)
- Live search results that update as files change on disk

## User Journeys

### Journey 1: Marco the Developer — "Where did I define that error handler?"

**Persona:** Marco, a backend developer working on a Rust web service. He has 4 workspace roots open in LushText: the main application crate, a shared library, a deployment config repo, and a documentation folder.

**Opening Scene:** Marco is debugging a 500 error in production logs. The error message reads `"connection pool exhausted"` but he can't remember which module emits it. He has 200+ source files across the workspace.

**Rising Action:** Marco presses `Ctrl+Shift+F`. The search panel slides up from the bottom. He types `connection pool exhausted`. Before he finishes typing "exhausted," results are already streaming in — two matches in `src/db/pool.rs` and one in `docs/troubleshooting.md`. The results are grouped by file with line numbers and the matching text highlighted.

**Climax:** Marco clicks the match in `pool.rs:47`. The file opens (or switches to the already-open tab) and his cursor lands exactly on line 47. He can see the error path immediately — the pool timeout is hardcoded. He fixes the value, saves, and the search panel is still visible with his results in case he needs to check the other matches.

**Resolution:** Marco checks the docs match to update the troubleshooting guide. The entire workflow — from "where is this?" to "I've fixed it and updated the docs" — took under a minute without leaving LushText.

**Requirements revealed:** Streaming results, file grouping, match highlighting, click-to-open-at-line, panel stays visible after navigation, multi-root workspace search.

### Journey 2: Lucia the Writer — "I wrote about this topic last year somewhere"

**Persona:** Lucia, a technical writer maintaining a documentation workspace. She has two roots: a product documentation folder (~300 markdown files organized by version) and a personal notes folder (~150 mixed files).

**Opening Scene:** Lucia is writing a new guide about authentication flows. She vaguely remembers writing notes about OAuth edge cases months ago but can't remember the filename or folder. Searching by filename in the command palette yields nothing — she doesn't remember what she called the file.

**Rising Action:** Lucia presses `Ctrl+Shift+F` and types `OAuth refresh token`. She adds a file glob `*.md` to skip non-documentation files. Results stream in: 4 matches across 3 files. One is in `notes/2025-auth-deep-dive.md` — a file she'd completely forgotten about.

**Climax:** She clicks the match. The file opens and she's on the exact paragraph where she documented the refresh token edge case. The notes are exactly what she needs for the new guide. She copies the relevant section, then uses F4 to jump to the next match to check the other occurrences.

**Resolution:** Lucia finishes the authentication guide in half the time because she found her own prior work. Without content search, she would have either rewritten the analysis from scratch or spent 20 minutes manually browsing folders.

**Requirements revealed:** File glob filter, search across heterogeneous content (docs + notes), F4 match navigation, discovery of forgotten files, `.gitignore` filtering (her docs repo has a `node_modules/` from a docs build tool).

### Journey 3: Tomás the Sysadmin — "Which config file sets this value?"

**Persona:** Tomás, a systems administrator managing server configurations. He has a workspace with 3 roots: Ansible playbooks, Nginx configs, and application environment files.

**Opening Scene:** A deployment failed because the database connection string has the wrong port. Tomás knows the value `5432` appears in multiple config files, but he needs to find every occurrence to understand which one is the source of truth and which ones are stale copies.

**Rising Action:** Tomás presses `Ctrl+Shift+F` and types `5432`. Results stream in immediately — 14 matches across 8 files. The results are grouped by file, so he can see at a glance that the port appears in `ansible/group_vars/production.yml`, `nginx/upstream.conf`, three `.env` files, and two documentation files.

**Climax:** Tomás scans the grouped results without clicking — the line content preview in each result row shows enough context to identify the source of truth (`ansible/group_vars/production.yml:12: db_port: 5432`) versus the stale copies. He identifies 3 files that need updating.

**Resolution:** Tomás clicks through each stale file, fixes the port, and saves. He then re-runs the search to confirm zero remaining instances of the old value. The re-search is instant because the workspace is small. Without content search, he would have run `grep -r 5432 .` in three separate terminals (one per root).

**Requirements revealed:** Multi-root search, result grouping with enough context to triage without clicking, re-search after edits, literal search (not regex) as the default, small-workspace instant completion.

### Journey 4: Marco the Developer — "Rename this constant across the codebase"

**Persona:** Marco again, now refactoring. He needs to rename the constant `MAX_RETRY_COUNT` to `MAX_RETRIES` across all files.

**Opening Scene:** Marco knows the constant is used in at least 6 files but isn't sure of the full extent. A simple find-and-replace in the current buffer won't cut it.

**Rising Action:** Marco presses `Ctrl+Shift+F`, types `MAX_RETRY_COUNT`, and toggles "Whole Word" on. 9 matches appear across 6 files. He types `MAX_RETRIES` in the replace field. A preview list appears showing every replacement — file, line number, the old line, and what it will look like after replacement. Each replacement has a checkbox (all checked by default).

**Climax:** Marco reviews the preview. One match is in a comment that should say "Maximum retries" instead, so he unchecks it. He clicks "Replace All." The replacements execute across 5 files. The status bar confirms: "Replaced 8 of 9 matches in 5 files." An "Undo All" button appears in the search panel.

**Resolution:** Marco runs the search again — 1 remaining match (the unchecked comment). He fixes it manually. He realizes one replacement broke a test assertion string, so he clicks "Undo All" for that file, fixes the assertion, and re-runs the replacement. The entire refactoring took 2 minutes. Without Replace All, he would have opened each file, done Ctrl+H six times, and risked missing one.

**Requirements revealed:** Replace All with preview list, per-replacement checkboxes, whole-word toggle, post-replace status confirmation, Undo All button, re-search after replace to verify.

### Journey 5: Any User — "Something went wrong"

**Persona:** Any of the three users above, encountering edge cases.

**Scenario A — No results:** Lucia searches for `OAuth refresh token` but accidentally has a typo: `OAth`. Zero results. The panel shows "No results found" with her query visible, so she spots the typo immediately, corrects it, and results appear.

**Scenario B — Too many results:** Tomás searches for `port` across his workspace. 12,000+ matches exist. The panel shows results streaming in, then at 10,000 the count label changes to "10,000+ results (truncated) — narrow your search." He adds a file glob `*.yml` and the result set drops to 47 — manageable.

**Scenario C — Invalid regex:** Marco toggles regex mode and types `fn\s+[` (malformed character class). Instead of a crash, the search input shows an inline error: "Invalid pattern: unclosed character class." No search runs. He fixes the pattern to `fn\s+\w+` and results stream in normally.

**Scenario D — Large workspace, slow filesystem:** Lucia opens a workspace on an NFS-mounted share. The search takes 8 seconds instead of 500ms. The status bar shows progress: "Searching 3,421 / 12,000 files..." She can see the search is working. She types a new query mid-search — the previous search cancels immediately and the new one starts.

**Requirements revealed:** Empty state handling, truncation indicator with guidance, inline regex error display, progress reporting, mid-search cancellation, graceful degradation on slow filesystems.

### Journey 6: Lucia the Writer — "I run this search every week"

**Persona:** Lucia again, using search history and saved searches.

**Opening Scene:** Every Monday, Lucia searches for `TODO` across her documentation workspace to find items she flagged during the previous week.

**Rising Action:** Lucia presses `Ctrl+Shift+F`. The search input shows a dropdown of recent searches. She sees her last 5 queries listed. She selects `TODO` from the history — the search runs immediately with her previous toggle settings (case-sensitive, `*.md` glob).

**Climax:** Lucia decides this is important enough to save permanently. She saves the search as "Weekly TODO review" with its options. The next Monday, she opens the search panel, selects the saved search from a separate section, and it runs with all options pre-configured.

**Resolution:** Lucia's weekly review workflow is now two clicks instead of retyping the query and re-configuring the toggles each time.

**Requirements revealed:** Search history (recent queries with options), saved/named searches, option persistence per search, history dropdown in search input.

### Journey Requirements Summary

| Capability | Revealed By |
|---|---|
| Streaming results with file grouping | Journeys 1, 2, 3 |
| Click-to-open at line | Journeys 1, 2, 3 |
| Match highlighting in results | Journeys 1, 2 |
| File glob filter | Journeys 2, 3, 5B |
| F4 / Shift+F4 match navigation | Journey 2 |
| Multi-root workspace search | Journeys 1, 3 |
| Replace All with preview + checkboxes | Journey 4 |
| Undo All for replacements | Journey 4 |
| Whole word toggle | Journey 4 |
| Regex toggle with inline error display | Journey 5C |
| Empty state ("No results found") | Journey 5A |
| Truncation indicator with guidance | Journey 5B |
| Progress reporting on status bar | Journey 5D |
| Mid-search cancellation | Journey 5D |
| Search history (recent queries + options) | Journey 6 |
| Saved/named searches | Journey 6 |
| `.gitignore` filtering | Journeys 1, 2 |
| Panel stays visible after navigation | Journey 1 |
| Re-search after edits | Journeys 3, 4 |

## Desktop Application Specific Requirements

### Project-Type Overview

LushText is a single-platform (Linux) native desktop application distributed via Flatpak. The content search feature adds no new platform requirements — it uses the same GTK4/Libadwaita widget toolkit, the same GSettings persistence, and the same Flatpak sandbox constraints as the rest of the application.

### Technical Architecture Considerations

**Threading model:** Content search introduces a new threading pattern to LushText. The existing `spawn_blocking_then` pattern (fire-and-forget with single result callback) is unsuitable for streaming search results. Instead, a dedicated `std::thread::spawn` manages the `WalkParallel` thread pool internally, communicating via `crossbeam_channel::bounded(1024)` to a `glib::timeout_add_local` polling timer on the GTK main thread. This is the only feature in LushText that uses channel-based streaming.

**Widget placement:** The search panel sits inside a `GtkRevealer` below the content stack (editor + preview paned), within the main horizontal `GtkPaned`'s end child. This positions it below the editor and above the status bar, inside the sidebar-content split. Animation uses `slide-up` transition matching the sidebar's `AdwTimedAnimation` + `EaseOutCubic` pattern.

**Dependency additions:** 5 new direct dependencies (`grep-regex`, `grep-searcher`, `grep-matcher`, `ignore`, `crossbeam-channel`) with ~5-8 marginal transitive crates. All pure Rust — no new system dependencies, no Flatpak manifest changes beyond `cargo-sources.json` regeneration.

### Platform Support

- **Target:** Linux (GNOME desktop environment, GTK 4.20+)
- **Distribution:** Flatpak (primary), system package (secondary)
- **Sandbox:** Content search operates within Flatpak filesystem access — searches only directories the user has granted access to via the file chooser portal or `--filesystem` permissions

### System Integration

- **`.gitignore` awareness:** The `ignore` crate respects `.gitignore`, `.ignore`, and `.rgignore` files in searched directories — matching the behavior users expect from terminal `rg`
- **XDG data directory:** Search history and saved searches persist to `$XDG_DATA_HOME/lushtext/` alongside existing session and workspace data, using the same `json_store` atomic write pattern
- **GSettings:** Panel visibility (`search-panel-visible`), last search options (regex, case, word toggles), and panel height persist via GSettings schema keys
- **CLI integration:** No CLI changes needed — content search is a UI-only feature triggered by `Ctrl+Shift+F`

### Implementation Considerations

- **Flatpak build:** `cargo-sources.json` must be regenerated after adding the 5 new crate dependencies (`make cargo-sources`)
- **cargo-hakari:** `cargo hakari generate` must run after dependency additions to update the workspace-hack crate
- **Memory mapping:** `MmapChoice::auto()` is safe within Flatpak's filesystem access model — mmap'd files are always user-owned and accessible
- **Binary size:** The new crates add ~100-200KB to the stripped release binary (negligible for a GTK application linking system libraries)

## Project Scoping & Phased Development

### MVP Strategy & Philosophy

**MVP Approach:** Complete experience MVP — the search feature ships fully formed in a single release. No incremental public rollout. The user's first encounter with content search should feel finished, not like an early preview.

**Resource Model:** Solo developer, building all MVP capabilities before the first release. Internal implementation phases exist for development organization, not for separate releases.

### MVP Feature Set

All six user journeys (developer search, writer discovery, sysadmin audit, multi-file replace, error handling, search history) are fully supported at launch. The complete MVP feature list is defined in the **Product Scope** section above.

### Internal Implementation Phases

These phases organize development work. All ship together in one release.

**Phase 1 — Service layer:** `model/content_search.rs` + `services/content_search.rs`. Walker + searcher orchestration, channel-based result streaming, cancellation. Unit tests and Criterion benchmarks. No UI.

**Phase 2 — Search panel widget:** `ui/search_panel/` with template, GObject subclass, result item wrapper. Search input, toggle buttons, results ListView with TreeListModel, result count label. `Ctrl+Shift+F` action + GtkRevealer animation. Widget tests. Wired to placeholder data for layout validation.

**Phase 3 — End-to-end integration:** Service wired to UI via channel + timer. Debounce, cancellation, click-to-open, F4 navigation, progress reporting. Replace All with preview, checkboxes, and Undo All. Integration tests.

**Phase 4 — Polish:** Search history and saved searches persistence. File glob filter. GSettings persistence for panel state. Match highlighting with Pango markup. Edge case handling (empty workspace, invalid regex, slow filesystem). Performance tuning on large workspaces.

### Post-MVP Roadmap

Growth and Vision features are defined in the **Product Scope** section above.

### Risk Mitigation Strategy

**Technical Risks:**

| Risk | Severity | Mitigation |
|---|---|---|
| Replace All data loss | High | Preview list with per-item checkboxes shows every change before execution. Undo All button reverts all replacements. Atomic write pattern (temp file + rename) for each file prevents partial writes. |
| Channel backlog causes memory spike | Medium | `crossbeam_channel::bounded(1024)` provides back-pressure. Walker threads block when UI falls behind. |
| WalkParallel thrashes on slow FS | Medium | `WalkBuilder::threads(4)` caps thread count. Progress reporting keeps user informed. |
| SIGBUS from mmap on mutated file | Low | `MmapChoice::auto()` is production-proven in ripgrep. Acceptable risk for a local editor. |
| GtkListView performance at 10k results | Low | GTK4 widget recycling handles millions of items. 10k cap is well within bounds. |
| Invalid user regex crashes | Medium | Wrap `RegexMatcher::new_line_matcher()` in error handling. Show inline error, don't search. |

**Market Risks:**
- Low — content search in a GNOME editor has no direct competition. The risk is execution quality, not market fit.

**Resource Risks:**
- Solo developer — the internal phasing (service → UI → integration → polish) ensures each phase produces testable, working code. If time pressure emerges, the feature can be tested and iterated internally before public release.

## Functional Requirements

### Content Search Execution

- **FR1:** User can search file contents across all workspace roots using a text query
- **FR2:** User can search using regular expressions
- **FR3:** User can toggle case-sensitive matching
- **FR4:** User can toggle whole-word matching
- **FR5:** User can cancel an in-progress search by typing a new query, pressing Escape, or closing the search panel
- **FR6:** System skips binary files during search automatically
- **FR7:** System respects `.gitignore` / `.ignore` / `.rgignore` patterns during search by default
- **FR8:** User can toggle `.gitignore` filtering on or off
- **FR9:** System caps search results at 10,000 matches and indicates truncation to the user

### Search Results Display

- **FR10:** System displays search results grouped by file, with each file as an expandable group
- **FR11:** System shows line number and matching line content for each result row
- **FR12:** System highlights the matching text within each result line
- **FR13:** System shows a total result count (number of matches and number of files)
- **FR14:** System displays a truncation indicator with guidance to narrow the query when the result cap is reached
- **FR15:** System shows an empty state ("No results found") when a search yields zero matches
- **FR16:** System shows an inline error message when the user enters an invalid regex pattern, without executing a search

### Search Results Navigation

- **FR17:** User can open a file at the matching line by activating a search result
- **FR18:** User can navigate to the next match across files via keyboard shortcut (F4)
- **FR19:** User can navigate to the previous match across files via keyboard shortcut (Shift+F4)
- **FR20:** Search panel remains visible after the user navigates to a result

### Search Filtering

- **FR21:** User can filter searched files by glob pattern (e.g., `*.rs`, `*.md`, `*.toml`)

### Multi-File Replace

- **FR22:** User can enter a replacement string for the current search query
- **FR23:** System shows a preview list of all proposed replacements before execution, displaying file path, line number, original line, and resulting line
- **FR24:** User can select or deselect individual replacements via checkboxes (all selected by default)
- **FR25:** User can execute Replace All for the selected replacements
- **FR26:** System confirms replacement results after execution (number of matches replaced, number of files affected)
- **FR27:** User can undo all replacements made in the most recent Replace All operation

### Search Persistence

- **FR28:** System maintains a history of recent search queries with their associated toggle settings and file glob
- **FR29:** User can select a previous search from the history to re-execute it with its saved settings
- **FR30:** User can save a search as a named entry for permanent access
- **FR31:** User can select a saved search to execute it with all its saved options pre-configured

### Search Panel Lifecycle

- **FR32:** User can toggle the search panel open/closed via `Ctrl+Shift+F`
- **FR33:** System animates the search panel reveal and hide transitions
- **FR34:** System saves focus before opening the panel and restores focus after closing it
- **FR35:** System persists search panel visibility and last-used search options across application sessions
- **FR36:** System displays search progress in the status bar during active searches (files searched / total estimate)

## Non-Functional Requirements

### Performance

- **NFR1:** First search results appear within 500ms of query submission on a workspace with 70,000 files (NVMe storage)
- **NFR2:** Full search completes within 5 seconds on a workspace with 70,000 files and 30 million lines (NVMe storage)
- **NFR3:** Search cancellation (new query, Escape, panel close) halts background work within 50ms with no visible UI lag
- **NFR4:** The GTK main thread maintains 60fps (no dropped frames) during active search result streaming
- **NFR5:** Result batching delivers up to 50 results per 50ms polling tick — sufficient for perceived streaming without overloading the GTK main loop
- **NFR6:** Channel back-pressure (bounded at 1024 items) prevents unbounded memory growth during searches that produce results faster than the UI can consume
- **NFR7:** Replace All execution writes files atomically (temp file + rename) — a crash mid-operation leaves each file either fully old or fully new, never partially written
- **NFR8:** Search panel open/close animation completes in 250ms (matching sidebar and preview pane transitions)

### Reliability

- **NFR9:** Per-file I/O errors (permission denied, file locked, encoding failure) are logged and skipped without aborting the overall search
- **NFR10:** Invalid regex input produces a user-facing error message — the application never panics on user-provided search patterns
- **NFR11:** Undo All reliably reverts all replacements from the most recent Replace All operation, even if the user has navigated away from the search panel
- **NFR12:** Search history and saved searches persist across application restarts via atomic JSON writes (same crash-safe pattern as session and workspace persistence)

### Accessibility

- **NFR13:** Search panel uses standard GTK4/Libadwaita widgets throughout, inheriting built-in AT-SPI accessibility (keyboard navigation, screen reader support, focus management)
- **NFR14:** All search panel controls are keyboard-accessible — no mouse-only interactions
- **NFR15:** Search results list supports keyboard navigation (arrow keys for row selection, Enter for activation)

**Deferred accessibility enhancements (post-MVP):**
- Explicit accessible labels/descriptions for individual result rows (file path, line number, match content announced separately)
- High-contrast mode testing and validation
- Screen reader announcement of search progress and result count changes
