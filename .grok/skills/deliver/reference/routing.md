# Conditional selection

Owner: @ahutanu. This is orchestration of existing skills, not a replacement for their standards.

| Unresolved question / task | Select | Reuse / omit when |
|---|---|---|
| What behavior is required? | `/specify` | Existing accepted requirements answer it. A consequential ambiguity pauses first. |
| A new load-bearing system boundary? | `/define-architecture` at T2, then missing applicable `/design-slice` | Existing architecture fits the task; selection never labels a new load-bearing boundary T0. |
| Contracts, risks or test boundaries not designed? | `/design-slice` | Grounded applicable design is already valid. Security/identity/data/contracts/money/concurrency raise risk and require a design before implementation. |
| New/changed user interface behavior or visual system? | `/ui-design` before code | Reuse accepted UI design; an interface existing elsewhere does not trigger redesign. |
| Feature / small T0 | `/implement` | Prompt is sufficient for genuinely low-risk work; still TDD and outcome proof. |
| Broken behavior | `/investigate` → repair-review → approved repair | Human reviews diagnosis and named phases. Prior human preauthorization must actually cover this repair; "autopilot", a worker ACK or inferred intent does not. |
| Upgrade, migration or large refactor | `/migrate` | Pin old behavior before changing it; prove equivalence or explicitly approved intentional differences. Do not add a second implementation lifecycle. |
| Documentation-only | `/document` over the requested surface | No code/design/architecture sweep or graph bootstrap by default. Read back the rendered/user-facing result. |
| Approved independent tracks | `/prepare-for-coordination` → `/execute-with-coordination` replacing implementation | Only explicit coordination, actual disjoint paths, approved budgets/contracts and available qualified host mechanics. Reuse a current accepted plan with recorded evidence; do not silently omit preparation. Any required coordination-layer setup remains permission-gated, never automatic adoption. |

Every route ends in real-path verification and applicable review. Genuinely T0 code (tested local refactor, not a routine new feature/fix) and T0 docs can use the §0.2 self-check without unconditional reviewer ceremony; T1/T2 applicable review and every actual hard veto still need independent clearance. Each selected skill retains its own preimplementation/reviewed-exit obligations; a null helper gate does not waive them. `/document --changed` is optional when changed user-facing contracts actually need documentation; it is not an unconditional extra stage. Research is just-in-time for a specific unknown, not a prerequisite skill parade. Raise the safety floor, not the requirements: report a newly discovered risk and pause if authorization/scope must change.

For coordinated migration, the helper's `migration-characterization` is an obligation within the existing `/migrate` skill, not a new skill: perform grounding, blast-radius/surface enumeration and characterization green on the old stack before dispatch. Obtain applicable Test Architect clearance; carry every migration increment, intentional-difference, equivalence and rollback requirement into preparation, track contracts and join proof. Defect investigation and repair-review likewise remain before coordination. Preparation's older commit/setup boilerplate does not authorize a commit or configuration change here.

## Helper facts

The agent grounds the facts; the deterministic helper cannot infer task meaning from prose:

```json
{"kind":"feature","tier":"T0","questions":[],"risks":[],"ui":false,"coordination":false,"design_ready":false}
```

Required keys are the first six. `kind`: feature | defect | migration | docs. `tier`: T0 | T1 | T2 (cost of error, not compute/context tier). `questions`: subset of requirements | architecture | design, naming only unresolved work. `risks`: subset of security | identity | data | contracts | money | concurrency. `ui`: a UI-design step is unresolved. `coordination`: human explicitly selected independent-track execution, **not** permission to spawn. Optional `design_ready` (default false): current grounded design already answers the task's load-bearing questions. Register its source as an input. Unknown fields/types refuse.

`delivery.py route --facts <file>` selects without writes. `start` applies the higher of the compiled contract and grounded facts' risk tiers; any listed risk, migration, load-bearing architecture question or coordination imposes **T2**, per `knowledge/agent-rules-of-the-road.md` §0.2. The architecture selection means a new load-bearing boundary. Coordination introduces inherent concurrency even when the product code's `risks` list is empty; disjoint paths do not remove dispatch/join/state concurrency. Missing T1/T2 design is selected, valid design reused. Genuinely T0 code/docs with no unresolved design do not acquire one. Explain selected and omitted stages in one short statement. The resulting list is a task-specific plan, not an instruction to run every pack skill.
