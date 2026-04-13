# Feature Specification: [FEATURE NAME]

**Feature Branch**: `[###-feature-name]`
**Created**: [DATE]
**Status**: Draft
**Input**: User description: "$ARGUMENTS"

## User Scenarios & Testing *(mandatory)*

<!--
  IMPORTANT: User stories should be PRIORITIZED as user journeys ordered by
  importance. Each user story/journey must be INDEPENDENTLY TESTABLE, meaning
  that implementing just one of them still produces a viable slice of value.

  Assign priorities (P1, P2, P3, etc.) to each story, where P1 is the most
  critical. Each story should name the user-visible contract clearly enough
  that design, implementation, and review can verify the exact behavior.
-->

### User Story 1 - [Brief Title] (Priority: P1)

[Describe this user journey in plain language]

**Why this priority**: [Explain the value and why it has this priority level]

**Independent Test**: [Describe how this can be tested independently and what
value it delivers on its own]

**Acceptance Scenarios**:

1. **Given** [initial state], **When** [action], **Then** [expected outcome]
2. **Given** [initial state], **When** [action], **Then** [expected outcome]

---

### User Story 2 - [Brief Title] (Priority: P2)

[Describe this user journey in plain language]

**Why this priority**: [Explain the value and why it has this priority level]

**Independent Test**: [Describe how this can be tested independently]

**Acceptance Scenarios**:

1. **Given** [initial state], **When** [action], **Then** [expected outcome]

---

### User Story 3 - [Brief Title] (Priority: P3)

[Describe this user journey in plain language]

**Why this priority**: [Explain the value and why it has this priority level]

**Independent Test**: [Describe how this can be tested independently]

**Acceptance Scenarios**:

1. **Given** [initial state], **When** [action], **Then** [expected outcome]

---

[Add more user stories as needed, each with an assigned priority]

### Edge Cases

- What happens when the user cancels, closes the window, or the app crashes
  mid-workflow?
- How does the system handle external file changes, slow I/O, or large inputs
  if they intersect this feature?
- What happens at narrow window widths, with optional panes hidden, or when the
  relevant UI starts from an empty state?

## UX, Safety & Verification Constraints *(mandatory)*

### Interaction Contract

- Describe the exact user-visible behavior this feature introduces or changes.
- Call out focus, sizing, animation, copy, shortcuts, toggle state, and empty
  state expectations whenever they matter.
- If the feature intentionally changes an existing contract, name the old
  behavior and the new behavior explicitly.

### Data Safety & Recovery

- Describe any effect on file contents, drafts, session restore, search and
  replace, undo, or destructive actions.
- State what confirmation, undo, autosave, atomic write, or recovery behavior
  is required.
- If the feature has no user-data risk, say that explicitly.

### Verification & Delivery Impact

- List the automated tests required for this feature (unit, integration,
  widget, benchmark, or other project-specific coverage).
- List any live runtime validation required, including `make run` stderr checks
  when GTK warnings, focus, animation, or layout behavior could regress.
- List documentation, build, or packaging artifacts that must update with the
  feature.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST [specific capability, e.g., "allow users to create accounts"]
- **FR-002**: System MUST [specific capability, e.g., "validate email addresses"]
- **FR-003**: Users MUST be able to [key interaction, e.g., "reset their password"]
- **FR-004**: System MUST [data requirement, e.g., "persist user preferences"]
- **FR-005**: System MUST [behavior, e.g., "log all security events"]

*Example of marking unclear requirements:*

- **FR-006**: System MUST authenticate users via [NEEDS CLARIFICATION: auth method not specified - email/password, SSO, OAuth?]
- **FR-007**: System MUST retain user data for [NEEDS CLARIFICATION: retention period not specified]

### Key Entities *(include if feature involves data)*

- **[Entity 1]**: [What it represents, key attributes without implementation]
- **[Entity 2]**: [What it represents, relationships to other entities]

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: [Measurable metric, e.g., "Users can complete account creation in under 2 minutes"]
- **SC-002**: [Measurable metric, e.g., "System handles 1000 concurrent users without degradation"]
- **SC-003**: [User satisfaction metric, e.g., "90% of users successfully complete primary task on first attempt"]
- **SC-004**: [Business metric, e.g., "Reduce support tickets related to [X] by 50%"]

## Assumptions

- [Assumption about target users, e.g., "Users have stable internet connectivity"]
- [Assumption about scope boundaries, e.g., "Mobile support is out of scope for v1"]
- [Assumption about data/environment, e.g., "Existing authentication system will be reused"]
- [Dependency on existing system/service, e.g., "Requires access to the existing user profile API"]
