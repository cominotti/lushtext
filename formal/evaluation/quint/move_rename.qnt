// -*- mode: Bluespec; -*-
// SPDX-License-Identifier: GPL-3.0-or-later
//
// T1, second half: the copy fallback (`MoveProtocol`) and `RenameProtocol`
// from crates/lushtext-core/src/services/filesystem/write_protocol.rs, with
// the two Kani harnesses `a_move_removes_its_source_only_after_the_copy_is_durable`
// and `a_completed_rename_synced_every_directory_it_mutated`.
//
// DISPOSABLE EVALUATION MODEL, NOT A MAINTAINED PROPERTY.
module move_rename {
  type MoveAction = Copy | RemoveSource | SyncSourceDir | MoveFinish(bool)
  type RenameAction = RenameOp | SyncSrcDir | SyncDestinationDir | RenameFinish(bool)

  /// `MoveProtocol::step`.
  pure def moveStep(pending: MoveAction, succeeded: bool): MoveAction = match pending {
    | Copy => if (succeeded) RemoveSource else MoveFinish(false)
    | RemoveSource => if (succeeded) SyncSourceDir else MoveFinish(false)
    | SyncSourceDir => if (succeeded) MoveFinish(true) else MoveFinish(false)
    | MoveFinish(d) => MoveFinish(d)
  }

  /// `RenameProtocol::step`.
  pure def renameStep(pending: RenameAction, crossDirectory: bool, succeeded: bool): RenameAction = match pending {
    | RenameOp => if (succeeded) SyncSrcDir else RenameFinish(false)
    | SyncSrcDir => if (succeeded and crossDirectory) SyncDestinationDir
                    else if (succeeded) RenameFinish(true) else RenameFinish(false)
    | SyncDestinationDir => if (succeeded) RenameFinish(true) else RenameFinish(false)
    | RenameFinish(d) => RenameFinish(d)
  }

  var move: MoveAction
  var sourcePresent: bool
  var destinationDurable: bool
  var rename: RenameAction
  var crossDirectory: bool
  var renamed: bool
  var sourceSynced: bool
  var destinationSynced: bool

  action init = {
    nondet c = oneOf(Set(true, false))
    all {
      move' = Copy, sourcePresent' = true, destinationDurable' = false,
      rename' = RenameOp, crossDirectory' = c,
      renamed' = false, sourceSynced' = false, destinationSynced' = false,
    }
  }

  action moveOnce = all {
    nondet ok = oneOf(Set(true, false))
    all {
      move' = moveStep(move, ok),
      // The copy is a whole atomic write: success is `WriteClass::Success`.
      destinationDurable' = if (move == Copy) ok else destinationDurable,
      sourcePresent' = if (move == RemoveSource) not(ok) else sourcePresent,
    },
    rename' = rename, crossDirectory' = crossDirectory, renamed' = renamed,
    sourceSynced' = sourceSynced, destinationSynced' = destinationSynced,
  }

  action renameOnce = all {
    nondet ok = oneOf(Set(true, false))
    all {
      rename' = renameStep(rename, crossDirectory, ok),
      renamed' = if (rename == RenameOp) ok else renamed,
      sourceSynced' = if (rename == SyncSrcDir) (ok and renamed) else sourceSynced,
      destinationSynced' = if (rename == SyncDestinationDir) (ok and renamed) else destinationSynced,
    },
    move' = move, sourcePresent' = sourcePresent, destinationDurable' = destinationDurable,
    crossDirectory' = crossDirectory,
  }

  action step = any { moveOnce, renameOnce }

  /// Move safety: the bytes are always in at least one durable place, and a
  /// failed move without a durable copy kept its source.
  val moveSafety = (sourcePresent or destinationDurable) and match move {
    | MoveFinish(d) => (not(d) and not(destinationDurable)) implies sourcePresent
    | _ => true
  }

  /// A completed rename synced every directory it mutated.
  val renameSynced = match rename {
    | RenameFinish(d) => d implies (renamed and sourceSynced and (crossDirectory implies destinationSynced))
    | _ => true
  }

  val safety = moveSafety and renameSynced
}
