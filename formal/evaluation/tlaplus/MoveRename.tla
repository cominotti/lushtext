------------------------------ MODULE MoveRename ------------------------------
(* T1, second half: MoveProtocol (the copy fallback) and RenameProtocol from   *)
(* crates/lushtext-core/src/services/filesystem/write_protocol.rs, with the    *)
(* move-safety and rename-sync Kani harnesses. Disposable evaluation model.    *)
EXTENDS Naturals

VARIABLES move, sourcePresent, destinationDurable,
          rename, cross, renamed, sourceSynced, destinationSynced
vars == <<move, sourcePresent, destinationDurable, rename, cross, renamed,
          sourceSynced, destinationSynced>>

\* MoveProtocol::step; "Done"/"Failed" are Finish(true)/Finish(false).
MoveStep(p, ok) ==
  CASE p = "Copy" -> IF ok THEN "RemoveSource" ELSE "Failed"
    [] p = "RemoveSource" -> IF ok THEN "SyncSourceDir" ELSE "Failed"
    [] p = "SyncSourceDir" -> IF ok THEN "Done" ELSE "Failed"
    [] OTHER -> p

\* RenameProtocol::step.
RenameStep(p, x, ok) ==
  CASE p = "Rename" -> IF ok THEN "SyncSourceDir" ELSE "Failed"
    [] p = "SyncSourceDir" -> IF ok /\ x THEN "SyncDestinationDir"
                              ELSE IF ok THEN "Done" ELSE "Failed"
    [] p = "SyncDestinationDir" -> IF ok THEN "Done" ELSE "Failed"
    [] OTHER -> p

Init == /\ move = "Copy" /\ sourcePresent = TRUE /\ destinationDurable = FALSE
        /\ rename = "Rename" /\ cross \in BOOLEAN
        /\ renamed = FALSE /\ sourceSynced = FALSE /\ destinationSynced = FALSE

MoveOnce == \E ok \in BOOLEAN :
  /\ move' = MoveStep(move, ok)
  /\ destinationDurable' = IF move = "Copy" THEN ok ELSE destinationDurable
  /\ sourcePresent' = IF move = "RemoveSource" THEN ~ok ELSE sourcePresent
  /\ UNCHANGED <<rename, cross, renamed, sourceSynced, destinationSynced>>

RenameOnce == \E ok \in BOOLEAN :
  /\ rename' = RenameStep(rename, cross, ok)
  /\ renamed' = IF rename = "Rename" THEN ok ELSE renamed
  /\ sourceSynced' = IF rename = "SyncSourceDir" THEN ok /\ renamed ELSE sourceSynced
  /\ destinationSynced' = IF rename = "SyncDestinationDir" THEN ok /\ renamed ELSE destinationSynced
  /\ UNCHANGED <<move, sourcePresent, destinationDurable, cross>>

Next == MoveOnce \/ RenameOnce
Spec == Init /\ [][Next]_vars

MoveSafety == /\ sourcePresent \/ destinationDurable
              /\ (move = "Failed" /\ ~destinationDurable) => sourcePresent
RenameSynced == rename = "Done" => renamed /\ sourceSynced /\ (cross => destinationSynced)
Safety == MoveSafety /\ RenameSynced
================================================================================
