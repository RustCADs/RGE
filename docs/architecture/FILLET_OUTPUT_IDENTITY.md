# FILLET_OUTPUT_IDENTITY

| Status | **PARTIALLY RESOLVED** for chamfer `FilletOp`; cap-face stable B-Rep identity remains deferred. |
|---|---|
| Current decision | ADR-120 unparked tessellation face-label propagation for chamfer `FilletOp`. |
| Still deferred | Direct `BRepProvider` / `BRepEdgeProvider` impls on `FilletOp` and stable `BRepFaceId`s for chamfer-cap geometry. |
| Related ADRs | ADR-119 (`RoundFilletOp`), ADR-120 (chamfer `FilletOp` labels). |

## Current behavior

Chamfer `FilletOp` is no longer output-label opaque for labeled input.

`FilletOp::evaluate`:

- clones upstream positions and indices;
- appends two chamfer-cap triangles per filleted edge;
- preserves upstream `face_labels` when the upstream tessellation is labeled;
- labels appended chamfer-cap triangles as `TopologyFaceId::DEGENERATE`;
- keeps unlabeled upstream input unlabeled.

The graph-level resolvers compose identity through `OperatorNode::Fillet`:

- `brep_face_ids_for_node` inherits upstream face IDs unchanged;
- `brep_edge_ids_for_node` inherits upstream edge IDs minus the selected
  filleted edges.

The stable B-Rep identity of chamfer-cap geometry is still intentionally absent:
`TopologyFaceId::DEGENERATE` cap triangles resolve to `None` in cad-projection.

## What changed

The old parked note said `FilletOp` must keep returning unlabeled output until
output-side identity was designed. Cad-projection is now that real consumer:
face lookup, picking, selection partitioning, and highlight index generation all
depend on resolving inherited face labels through operator roots.

ADR-120 answers the bounded chamfer-label question without minting new stable
cap-face IDs. It does not make `FilletOp` a direct `BRepProvider`, and it does
not assign stable identity to chamfer caps.

## Remaining open question

Do chamfer caps need stable B-Rep face IDs?

That remains deferred until a consumer needs to select, persist, materialize, or
otherwise address chamfer-cap faces across rebuilds. If that pressure appears,
the next design step is a focused ADR or dispatch for a cap-face ID scheme,
including kind bytes, canonical cap ordering, owner discipline, and interactions
with topology lineage.

Until then callers should treat:

- inherited non-degenerate labels as stable upstream faces;
- `TopologyFaceId::DEGENERATE` chamfer caps as transient geometry with no stable
  `BRepFaceId`.

## Companion docs

- `docs/adr/ADR-120-chamfer-fillet-face-label-propagation.md`
- `docs/adr/ADR-119-real-round-fillet-substrate.md`
- `docs/architecture/SEMANTIC_ARCHITECTURE_LAWS.md` Section 6
- `crates/cad-core/src/operators/fillet/mod.rs`
- `crates/cad-core/src/topology/resolve.rs`
- `crates/cad-core/src/topology/edge_resolve.rs`
