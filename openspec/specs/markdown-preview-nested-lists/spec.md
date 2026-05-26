# markdown-preview-nested-lists Specification

## Purpose
TBD - created by archiving change markdown-preview-followups. Update Purpose after archive.
## Requirements
### Requirement: Markdown preview preserves nested list hierarchy
The system SHALL render nested ordered and unordered Markdown lists with depth-aware indentation and correct marker sequencing so parent-child relationships remain readable in the native preview. Nested list blocks MUST begin on their own rendered row beneath the parent item, even when the parent item already contains prose before the nested list.

#### Scenario: Render nested unordered list items
- **WHEN** the user previews a Markdown document containing unordered list items nested beneath a parent item
- **THEN** the preview keeps child items visually indented beneath their parent item
- **AND** each list level remains distinguishable instead of flattening into one margin
- **AND** the first child marker appears on a rendered row below the parent item's prose rather than continuing on the same row

#### Scenario: Render mixed ordered and unordered nesting
- **WHEN** the user previews a Markdown document that mixes ordered and unordered lists across multiple nesting levels
- **THEN** the preview preserves each item's source order and marker type at every level
- **AND** ordered items keep readable numbering without collapsing nested levels into plain paragraphs
- **AND** child list items keep their own visual row flow instead of inheriting a parent paragraph separator

### Requirement: Markdown preview preserves list item line flow and spacing
The system SHALL render tight and loose Markdown lists with visible row flow matching rendered Markdown semantics. Tight lists MUST show consecutive items without unintended blank rows, and loose lists MUST preserve intentional paragraph spacing without duplicating empty rows around list item boundaries.

#### Scenario: Render tight ordered list without blank rows
- **WHEN** the user previews a Markdown document containing a tight ordered list such as `1. first`, `2. second`, and `3. third`
- **THEN** each item appears on its own rendered row in source order
- **AND** no empty rendered row appears between consecutive items
- **AND** each ordered marker appears on the same rendered row as its item text

#### Scenario: Render tight unordered list without blank rows
- **WHEN** the user previews a Markdown document containing a tight unordered list
- **THEN** each item appears on its own rendered row in source order
- **AND** no empty rendered row appears between consecutive items
- **AND** each bullet marker appears on the same rendered row as its item text

#### Scenario: Preserve loose list paragraph spacing without duplication
- **WHEN** the user previews a Markdown document containing a loose list item with multiple paragraphs separated by a blank line
- **THEN** the preview preserves the intentional paragraph break inside that list item
- **AND** the preview does not add an additional empty rendered row before the next list item

#### Scenario: Preserve task-list marker flow
- **WHEN** the user previews a Markdown document containing checked and unchecked task-list items
- **THEN** the preview shows the checked and unchecked markers in source order
- **AND** each task marker appears on the same rendered row as its item text
- **AND** task-list items do not gain unintended blank rows compared with ordinary tight list items

### Requirement: Markdown preview keeps list markers attached during wrapping
The system SHALL keep list markers visually attached to their item text in the rendered preview. Ordered markers, including multi-digit and offset markers, MUST remain on the same rendered row as the first item text at normal preview widths, and wrapped continuation lines MUST align under the item text rather than under the marker.

#### Scenario: Render offset ordered markers with item text
- **WHEN** the user previews a Markdown document containing an ordered list starting at `57.`
- **THEN** the preview shows `57.` and the first item text on the same rendered row
- **AND** the following item increments visibly as the next ordered marker
- **AND** neither ordered marker appears alone on a rendered row while its item text appears below it

#### Scenario: Wrap long ordered list items with hanging indentation
- **WHEN** the user previews an ordered list item whose text wraps in the native preview at a constrained but usable width
- **THEN** the ordered marker and first item text remain visually attached on the first rendered row
- **AND** wrapped continuation text aligns under the item text rather than under the ordered marker

#### Scenario: Wrap nested unordered list items with hanging indentation
- **WHEN** the user previews a nested unordered list item whose text wraps in the native preview at a constrained but usable width
- **THEN** the child bullet and first child text remain visually attached on the first rendered row
- **AND** wrapped continuation text stays within the child list indentation level rather than returning to the parent marker column
