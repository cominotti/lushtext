## ADDED Requirements

### Requirement: Local history treats protected documents as view-only
A document installed in Protected View SHALL NOT trigger baseline or periodic local-history capture, because its buffer cannot diverge from disk. A document that entered Protected View through an edit SHALL keep its existing capture behaviour. Previewing a snapshot whose profile reaches `protected_view` SHALL install the preview non-editable, caret-free and folded at insertion. Restoring a snapshot into an editor, and Undo Restore, SHALL keep the existing restore safety and SHALL install through the fold-at-insertion executor. The restored editor SHALL enter Protected View if the snapshot's profile requires it, and an editor that was already protected SHALL stay protected even when the restored content is small, because Protected View is monotone until the file is reloaded from disk.

#### Scenario: Restore of a giant-paragraph snapshot
- **WHEN** the user restores a snapshot whose longest paragraph exceeds `L_protect` into an editable document
- **THEN** the restore installs with the fold applied at insertion, the document enters Protected View, and Undo Restore remains available
