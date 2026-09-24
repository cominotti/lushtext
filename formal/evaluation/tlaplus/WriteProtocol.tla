---------------------------- MODULE WriteProtocol ----------------------------
(***************************************************************************)
(* SPDX-License-Identifier: GPL-3.0-or-later                               *)
(*                                                                         *)
(* T1 calibration: the durable-write protocol, re-expressed in PlusCal.    *)
(*                                                                         *)
(* DISPOSABLE EVALUATION MODEL, NOT A MAINTAINED PROPERTY. The maintained  *)
(* proof is the Kani harness set in                                        *)
(* crates/lushtext-core/src/services/filesystem/write_protocol/kani_proofs.rs *)
(* over the real WriteProtocol::step (write_protocol.rs). `Step` below is  *)
(* that function arm for arm; the PlusCal process is the harness's model   *)
(* disk and crash point. Report:                                           *)
(* docs/next/formal-verification-quint-vs-tlaplus.md.                      *)
(*                                                                         *)
(* SkipsTempSync = TRUE is the should_panic mutant: the shell answers      *)
(* SyncTemp with Done without syncing.                                     *)
(***************************************************************************)
EXTENDS Naturals

CONSTANT SkipsTempSync

MaxTempNameAttempts == 8
WriteSteps == MaxTempNameAttempts + 7
Outcomes == {"Done", "AlreadyExists", "Failed"}

\* WriteProtocol::step. `class` is the WriteClass payload of Finish.
Step(p, cls, a, o) ==
  LET ok == o = "Done"
      R(n, c, k) == [next |-> n, class |-> c, attempts |-> k]
  IN CASE p = "ProbeMetadata" -> IF ok THEN R("CreateTemp", cls, a) ELSE R("Finish", "BeforeRename", a)
       [] p = "RemoveTemp" -> R("Finish", "BeforeRename", a)
       [] p = "CreateTemp" ->
            IF o = "Done" THEN R("WriteContent", cls, a + 1)
            ELSE IF o = "AlreadyExists" /\ a + 1 < MaxTempNameAttempts THEN R("CreateTemp", cls, a + 1)
            ELSE R("Finish", "BeforeRename", a + 1)
       [] p = "WriteContent" -> IF ok THEN R("ApplyMetadata", cls, a) ELSE R("RemoveTemp", cls, a)
       [] p = "ApplyMetadata" -> IF ok THEN R("SyncTemp", cls, a) ELSE R("RemoveTemp", cls, a)
       [] p = "SyncTemp" -> IF ok THEN R("Rename", cls, a) ELSE R("RemoveTemp", cls, a)
       [] p = "Rename" -> IF ok THEN R("SyncDir", cls, a) ELSE R("RemoveTemp", cls, a)
       [] p = "SyncDir" -> IF ok THEN R("Finish", "Success", a) ELSE R("Finish", "AfterRename", a)
       [] p = "Finish" -> R("Finish", cls, a)

NoTemp == [exists |-> FALSE, data |-> "Empty", durableData |-> "Empty",
           metadataApplied |-> FALSE, durableMetadata |-> FALSE,
           destinationMode |-> FALSE, linked |-> FALSE]

\* Disk::execute on the temp inode.
TempAfter(p, done, t, probed, live) ==
  CASE p = "CreateTemp" ->
         IF done THEN [exists |-> TRUE, data |-> "Empty", durableData |-> "Torn",
                       metadataApplied |-> FALSE, durableMetadata |-> FALSE,
                       destinationMode |-> probed, linked |-> TRUE]
         ELSE t
    [] p = "WriteContent" -> [t EXCEPT !.data = IF done THEN "New" ELSE "Torn"]
    [] p = "ApplyMetadata" -> [t EXCEPT !.metadataApplied = t.metadataApplied \/ done]
    [] p = "SyncTemp" ->
         IF SkipsTempSync \/ ~done THEN t
         ELSE [t EXCEPT !.durableData = t.data, !.durableMetadata = t.metadataApplied]
    [] p = "Rename" -> IF done THEN [t EXCEPT !.linked = FALSE] ELSE t
    [] p = "RemoveTemp" ->
         IF ~done THEN t
         ELSE IF live = "Previous" THEN NoTemp
         ELSE [t EXCEPT !.linked = FALSE]
    [] OTHER -> t

(* --algorithm DurableWrite
variables
  pending = "ProbeMetadata", class = "None", attempts = 0, steps = 0,
  probed = FALSE, temp = NoTemp, liveEntry = "Previous", durableEntry = "Previous",
  removalFailed = FALSE, secondTemp = FALSE,
  crashed = FALSE, crashContent = "Empty", crashMetadataIntact = FALSE,
  lastOutcome = "Done";
begin
Loop:
  while ~crashed do
    either
      await pending # "Finish";
      with outcome \in Outcomes,
           done = (outcome = "Done"),
           seen = IF SkipsTempSync /\ pending = "SyncTemp" THEN "Done" ELSE outcome,
           s = Step(pending, class, attempts, seen) do
        probed := IF pending = "ProbeMetadata" THEN done ELSE probed;
        \* PlusCal statements in one step see earlier assignments, so the
        \* second-temp check reads `temp` before it is replaced.
        secondTemp := secondTemp \/ (pending = "CreateTemp" /\ done /\ temp.exists);
        temp := TempAfter(pending, done, temp, probed, liveEntry);
        liveEntry := IF pending = "Rename" /\ done THEN "Written" ELSE liveEntry;
        durableEntry := IF pending = "SyncDir" /\ done THEN liveEntry ELSE durableEntry;
        removalFailed := removalFailed \/ (pending = "RemoveTemp" /\ ~done);
        pending := s.next || class := s.class || attempts := s.attempts;
        lastOutcome := seen;
        steps := steps + 1;
      end with;
    or
      \* Disk::after_crash: the unsynced rename may or may not survive.
      with keepsRename \in BOOLEAN,
           entry = IF keepsRename THEN liveEntry ELSE durableEntry do
        crashed := TRUE;
        crashContent := IF entry = "Previous" THEN "Old" ELSE temp.durableData;
        crashMetadataIntact := IF entry = "Previous" THEN TRUE
                               ELSE temp.durableMetadata /\ temp.destinationMode;
      end with;
    or
      await pending = "Finish";
      goto Done;
    end either;
  end while;
end algorithm; *)
\* BEGIN TRANSLATION
VARIABLES pending, class, attempts, steps, probed, temp, liveEntry, 
          durableEntry, removalFailed, secondTemp, crashed, crashContent, 
          crashMetadataIntact, lastOutcome, pc

vars == << pending, class, attempts, steps, probed, temp, liveEntry, 
           durableEntry, removalFailed, secondTemp, crashed, crashContent, 
           crashMetadataIntact, lastOutcome, pc >>

Init == (* Global variables *)
        /\ pending = "ProbeMetadata"
        /\ class = "None"
        /\ attempts = 0
        /\ steps = 0
        /\ probed = FALSE
        /\ temp = NoTemp
        /\ liveEntry = "Previous"
        /\ durableEntry = "Previous"
        /\ removalFailed = FALSE
        /\ secondTemp = FALSE
        /\ crashed = FALSE
        /\ crashContent = "Empty"
        /\ crashMetadataIntact = FALSE
        /\ lastOutcome = "Done"
        /\ pc = "Loop"

Loop == /\ pc = "Loop"
        /\ IF ~crashed
              THEN /\ \/ /\ pending # "Finish"
                         /\ \E outcome \in Outcomes:
                              LET done == (outcome = "Done") IN
                                LET seen == IF SkipsTempSync /\ pending = "SyncTemp" THEN "Done" ELSE outcome IN
                                  LET s == Step(pending, class, attempts, seen) IN
                                    /\ probed' = (IF pending = "ProbeMetadata" THEN done ELSE probed)
                                    /\ secondTemp' = (secondTemp \/ (pending = "CreateTemp" /\ done /\ temp.exists))
                                    /\ temp' = TempAfter(pending, done, temp, probed', liveEntry)
                                    /\ liveEntry' = (IF pending = "Rename" /\ done THEN "Written" ELSE liveEntry)
                                    /\ durableEntry' = (IF pending = "SyncDir" /\ done THEN liveEntry' ELSE durableEntry)
                                    /\ removalFailed' = (removalFailed \/ (pending = "RemoveTemp" /\ ~done))
                                    /\ /\ attempts' = s.attempts
                                       /\ class' = s.class
                                       /\ pending' = s.next
                                    /\ lastOutcome' = seen
                                    /\ steps' = steps + 1
                         /\ pc' = "Loop"
                         /\ UNCHANGED <<crashed, crashContent, crashMetadataIntact>>
                      \/ /\ \E keepsRename \in BOOLEAN:
                              LET entry == IF keepsRename THEN liveEntry ELSE durableEntry IN
                                /\ crashed' = TRUE
                                /\ crashContent' = (IF entry = "Previous" THEN "Old" ELSE temp.durableData)
                                /\ crashMetadataIntact' = (IF entry = "Previous" THEN TRUE
                                                           ELSE temp.durableMetadata /\ temp.destinationMode)
                         /\ pc' = "Loop"
                         /\ UNCHANGED <<pending, class, attempts, steps, probed, temp, liveEntry, durableEntry, removalFailed, secondTemp, lastOutcome>>
                      \/ /\ pending = "Finish"
                         /\ pc' = "Done"
                         /\ UNCHANGED <<pending, class, attempts, steps, probed, temp, liveEntry, durableEntry, removalFailed, secondTemp, crashed, crashContent, crashMetadataIntact, lastOutcome>>
              ELSE /\ pc' = "Done"
                   /\ UNCHANGED << pending, class, attempts, steps, probed, 
                                   temp, liveEntry, durableEntry, 
                                   removalFailed, secondTemp, crashed, 
                                   crashContent, crashMetadataIntact, 
                                   lastOutcome >>

(* Allow infinite stuttering to prevent deadlock on termination. *)
Terminating == pc = "Done" /\ UNCHANGED vars

Next == Loop
           \/ Terminating

Spec == Init /\ [][Next]_vars

Termination == <>(pc = "Done")

\* END TRANSLATION

-----------------------------------------------------------------------------
\* Properties, named as in the Kani harnesses.

CrashAtomicity ==
  crashed => /\ crashContent \in {"Old", "New"}
             /\ (crashContent = "New" => crashMetadataIntact)

ModeNonWidening ==
  temp.exists /\ (temp.data = "New" \/ temp.durableData = "New") => temp.destinationMode

OneTempAtATime == ~secondTemp
FinishesWithinBound == steps <= WriteSteps

Visible == IF liveEntry = "Previous" THEN "Old" ELSE temp.data

ClassificationSound ==
  (~crashed /\ pending = "Finish") =>
    CASE class = "BeforeRename" ->
           Visible = "Old" /\ (~temp.exists \/ ~temp.linked \/ removalFailed)
      [] class = "AfterRename" -> Visible = "New"
      [] class = "Success" ->
           /\ Visible = "New" /\ durableEntry = "Written" /\ temp.durableData = "New"
           /\ temp.durableMetadata /\ temp.destinationMode /\ ~temp.linked
      [] OTHER -> FALSE

\* What `run_write` asserts, so what every harness (the mutant's included) checks.
RunWriteAssertions == CrashAtomicity /\ ModeNonWidening /\ OneTempAtATime
                      /\ FinishesWithinBound

Safety == RunWriteAssertions /\ ClassificationSound

\* Witnesses (the harness's kani::cover!): each must be VIOLATED.
NeverSucceeds == ~(~crashed /\ pending = "Finish" /\ class = "Success")
NeverAfterRename == ~(~crashed /\ pending = "Finish" /\ class = "AfterRename")
NeverRemovalFailed == ~(~crashed /\ pending = "Finish" /\ removalFailed)
NeverCrashShowsNew == ~(crashed /\ crashContent = "New")

\* Liveness: the write ends (finishes or crashes). It holds under weak
\* fairness (FairSpec) and fails under Spec, which allows stuttering forever.
Terminates == <>(crashed \/ pc = "Done")
FairSpec == Spec /\ WF_vars(Next)
=============================================================================
