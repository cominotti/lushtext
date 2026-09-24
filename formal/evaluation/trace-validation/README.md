# Trace validation (E2)

**Status: disposable evaluation model, not a maintained property.** This is
exploration idea E2 of
[`docs/next/formal-verification-quint-vs-tlaplus.md`](../../../docs/next/formal-verification-quint-vs-tlaplus.md).
It only **reads** existing smoke artefacts, and it does not change the
automation contract.

- `WorkflowEvents.tla` and `workflow_events.qnt` specify the producer of the
  content-free `GetWorkflowEvents` log (`docs/automation.md`): per workflow
  id, `started` and `finished` alternate, and `sequence` has no gaps.
- `gen_traces.py` turns recorded `workflow-events.json` artefacts into:
  - a TLC trace-validation model (`WorkflowEventsTraces.tla`, checked by
    `WorkflowEventsTrace.tla`);
  - Quint `run` tests (`workflow_events_traces.qnt`).
- `--mutate` corrupts the first trace, to show that validation rejects it.

```sh
O=build/formal-evaluation/runs/e2
formal/evaluation/trace-validation/gen_traces.py $O $(find build/smoke -name workflow-events.json)
cp formal/evaluation/trace-validation/WorkflowEvents*.tla $O/
printf 'SPECIFICATION Spec\nINVARIANT Accepted\n' > $O/WorkflowEventsTrace.cfg
(cd $O && java -cp ../../tools/tlc/tla2tools.jar tlc2.TLC -config WorkflowEventsTrace.cfg WorkflowEventsTrace.tla)
(cd $O && quint test workflow_events_traces.qnt)
```
