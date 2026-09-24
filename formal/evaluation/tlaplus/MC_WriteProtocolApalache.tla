------------------------ MODULE MC_WriteProtocolApalache -----------------------
(* Apalache wrapper for WriteProtocol.tla: Apalache needs a type annotation on *)
(* every VARIABLE and CONSTANT, and the PlusCal translator regenerates the     *)
(* VARIABLES line without them, so the annotated declarations live here and    *)
(* the translated module is instantiated. Disposable evaluation model.         *)
EXTENDS Naturals

CONSTANT
  \* @type: Bool;
  SkipsTempSync

VARIABLES
  \* @type: Str;
  pending,
  \* @type: Str;
  class,
  \* @type: Int;
  attempts,
  \* @type: Int;
  steps,
  \* @type: Bool;
  probed,
  \* @type: { exists: Bool, data: Str, durableData: Str, metadataApplied: Bool, durableMetadata: Bool, destinationMode: Bool, linked: Bool };
  temp,
  \* @type: Str;
  liveEntry,
  \* @type: Str;
  durableEntry,
  \* @type: Bool;
  removalFailed,
  \* @type: Bool;
  secondTemp,
  \* @type: Bool;
  crashed,
  \* @type: Str;
  crashContent,
  \* @type: Bool;
  crashMetadataIntact,
  \* @type: Str;
  lastOutcome,
  \* @type: Str;
  pc

INSTANCE WriteProtocol

CInitProduction == SkipsTempSync = FALSE
CInitMutant == SkipsTempSync = TRUE
================================================================================
