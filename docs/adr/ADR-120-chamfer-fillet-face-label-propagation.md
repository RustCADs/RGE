# ADR-120: Chamfer FilletOp face-label propagation

| Status | Accepted 2026-05-18 |
|---|---|
| Deciders | Human arbiter + Codex implementation review |
| Supersedes | The parked "FilletOp output must be unlabeled" posture in `docs/architecture/FILLET_OUTPUT_IDENTITY.md` |
| Related issues | GitHub #26, #27 |

## Context

The chamfer-style `FilletOp` already has graph-level face identity inheritance:
`brep_face_ids_for_node` recurses through `OperatorNode::Fillet` and returns the
upstream face IDs unchanged. Its edge resolver also inherits upstream edges
minus the selected filleted edges.

The remaining gap was tessellation labels. `FilletOp::evaluate` cloned upstream
positions and indices, appended two chamfer-cap triangles per filleted edge, and
then called `Tessellation::new`, stripping `face_labels` even when the upstream
mesh was labeled. That made cad-projection unable to resolve inherited faces
through a Fillet root despite the graph resolver already carrying the required
identity mapping.

Cad-projection is now a real consumer of per-triangle face labels through
`brep_face_id_for_triangle`, `pick_face`, face-selection partitioning, and
highlight index generation. That is enough consumer pressure to unpark the
chamfer output-label question.

## Decision

For chamfer `FilletOp`:

1. Labeled upstream input produces labeled output.
2. Upstream triangles keep their original `TopologyFaceId`s unchanged.
3. The two appended chamfer-cap triangles per filleted edge are labeled
   `TopologyFaceId::DEGENERATE`.
4. Unlabeled upstream input remains unlabeled; `FilletOp` does not fabricate
   labels without an input label stream.
5. `FilletOp::output_is_labeled` mirrors the single input's labeled state.
6. `FilletOp` still does not implement direct `BRepProvider` or
   `BRepEdgeProvider`.
7. Chamfer-cap geometry still has no stable `BRepFaceId` in v0. Cad-projection
   resolves inherited upstream labels to `Some(BRepFaceId)` and resolves
   `DEGENERATE` cap triangles to `None`.

## Consequences

The old cad-projection "Fillet is identity-opaque" tests are inverted:
inherited upstream faces survive through a Fillet root, while chamfer caps remain
nameless. Picker and selection behavior now treat labeled filleted surfaces as
resolvable instead of transparent, except where a normal precondition such as
missing `BRepHandle.brep_owner` still makes the hit unresolvable.

This does not mint stable IDs for chamfer caps and does not change the edge
resolver's filtered inheritance behavior.
