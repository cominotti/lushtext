## ADDED Requirements

### Requirement: Protected installation has measured and deterministic responsiveness evidence
The responsiveness lane SHALL sweep the sliced protected install and clear on synthetic single-paragraph protected documents of 100, 250, and 500 MB, with the accessibility bridge off and with the bridge on and a listener attached. It SHALL record the largest settle dispatch, the per-slice dispatch time across the tail, the slice size, the boundary count, the accessible insertion and deletion payload sizes when the bridge is on, and the run's `btrfs-stall-report` evidence. It SHALL also issue an interaction while an install is in progress (a tab switch and a scroll in another tab) and record it against the input-to-presented-frame budget and B_interact. The sweep's results SHALL be the evidence that sets the protected slice constants. The adoption gate SHALL cite the lane's one-turn protected install case, run with Protected View present, where a case that reaches the lane's wall cap counts as over budget. Pull-request-gated widget tests SHALL assert the deterministic preconditions:
- every hidden-tail slice is within its declared budget;
- the boundary count equals the slice count minus one per paragraph;
- no boundary mark remains after a clear;
- the buffer's full text equals the decoded fixture.

The user's own files SHALL NOT be used or committed as fixtures.

#### Scenario: The sweep records the adoption evidence
- **WHEN** `make responsiveness-smoke` runs the protected-install sweep on the reference lane
- **THEN** the report lists, for each size and bridge setting, the largest install and clear dispatch against B_settle, together with the slice constants used
- **AND** it lists the mid-install interaction's input-to-presented-frame latency against its budget

#### Scenario: A precondition regresses on a pull request
- **WHEN** a change makes a hidden-tail slice exceed its budget, leaves a boundary behind, or alters the installed bytes
- **THEN** a pull-request-gated widget test fails naming the violated precondition
