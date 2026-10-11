## ADDED Requirements

### Requirement: Session restore of protected documents is caret-free and bounded
Restoring a session tab whose document opens in Protected View SHALL restore the scroll position through the navigation mark without moving the insert mark. Session saves of that tab SHALL keep the cursor offset that was persisted, instead of writing the unmoved insert mark. No draft SHALL be created for a protected document that was installed unmodified. Restoring three or more protected documents together with tab switching SHALL keep every dispatch within `B_settle`, except the declared single-paragraph install turn that `main-thread-responsiveness` records as an envelope until `sliced-protected-install` is adopted. Draft recovery into a protected document SHALL install the recovered body through the fold-at-insertion executor, and the recovered document SHALL stay modified and protected.

#### Scenario: Restore of three protected documents
- **WHEN** a session with three protected documents and one ordinary document is restored and the user switches through the tabs
- **THEN** each protected tab is folded at insertion, scrolled to its saved position, and none gains a draft, and the persisted cursor offsets survive the next session save
