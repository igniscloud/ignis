# Stage 1 AR Blueprint

## Purpose

This document is the single authoritative Stage 1 architecture refinement execution blueprint for `ignis`.

Stage focus:

- reduce structural complexity before adding more product surface
- improve cross-platform behavior in the CLI and build pipeline
- tighten runtime, manifest, and SDK boundaries so future expansion costs less

Design philosophy:

> Prefer lightweight, boring, cross-platform building blocks that reduce coupling, cognitive load, and future rework.

This blueprint is execution-facing rather than research-only.
Checklist state lives here, daily todos must be generated from this file only, and supporting notes under `docs/researches/Stage_1_AR/` are evidence rather than a second requirement source.

## Execution Boundary

- This file is the only Stage 1 requirement source used by the execution cron.
- `docs/todos_YYYYMMDD.md` is a generated daily snapshot derived only from the checklist items in this file.
- Stage 1 runs in strict section order: later sections must remain unchecked while earlier sections still contain unchecked items.
- Each execution tick may work only inside the first open section and should stay within one coherent batch of at most 6 items.
- If the same unchecked item stays blocked for 5 consecutive ticks, it must be split into child checklist items under the same parent.
- If a parent item has child items, the parent stays `[ ]` until every child is `[x]`; when all children are `[x]`, the parent auto-closes to `[x]`.
- Supporting notes under `docs/researches/Stage_1_AR/` are optional evidence and never sufficient by themselves to close an item.

## Source Scope

This blueprint was derived from the current repository shape, primarily:

- `README.md`
- `docs/cli.md`
- `docs/api.md`
- `docs/integration.md`
- `crates/ignis-cli/src/main.rs`
- `crates/ignis-manifest/src/lib.rs`
- `crates/ignis-runtime/src/lib.rs`
- `crates/ignis-platform-host/src/lib.rs`
- `crates/ignis-sdk/src/lib.rs`

## Completion Gate

An item may be checked only when all of the following are true:

- the relevant code path has actually been refactored or implemented in the repository
- the change is validated with the smallest honest repo command for the affected surface
- the new boundary or behavior is visible in code rather than only described in docs
- any required public-doc drift caused by the refactor is reconciled in the same batch
- the authoritative checklist in this file and the current daily todo snapshot both reflect the result

Validation floor:

- workspace-wide structural passes should finish with `cargo check --workspace`
- narrower checks should be added when an item introduces or changes tests
- docs-only edits, placeholder handlers, or mock success paths do not count as completion

## Execution Order

The blueprint is partitioned into 5 ordered execution sections:

1. CLI structure and auth
2. Build, publish, and packaging
3. Manifest and domain modeling
4. Runtime and platform host
5. SDK, docs, and testing

Total checklist items: 25

## Section 1: CLI Structure And Auth

Primary targets: `crates/ignis-cli/src/**`
Suggested validation: `cargo check -p ignis-cli`

- [x] [CLI-01] Split `ignis-cli` command dispatch into dedicated modules for `project`, `service`, `auth`, `build`, and shared output/context.
- [x] [CLI-02] Define a small shared `ProjectContext` and service lookup layer so command handlers stop re-implementing manifest-loading concerns.
- [x] [CLI-03] Extract browser login and loopback callback handling from `main.rs` into an isolated auth module with a narrow API surface.
- [x] [CLI-04] Replace hand-rolled localhost callback parsing with a minimal, well-tested HTTP handling strategy that reduces edge-case risk.
- [x] [CLI-05] Define a consistent CLI output contract for JSON success, warnings, drift reporting, and actionable failure messages across commands.

## Section 2: Build, Publish, And Packaging

Primary targets: `crates/ignis-cli/src/**`, service build/publish orchestration paths, example service packaging flows
Suggested validation: `cargo check -p ignis-cli`

- [x] [BLD-01] Remove shell-fragile default frontend scaffolding commands and replace them with a cross-platform build strategy.
- [x] [BLD-02] Replace external `tar` command packaging with an internal archive path that behaves consistently across supported developer environments.
- [x] [BLD-03] Separate build orchestration from publish/deploy orchestration so artifact production, validation, and upload have clear boundaries.
- [x] [BLD-04] Introduce an explicit artifact validation stage for HTTP and frontend services before publish starts.
- [ ] [BLD-05] Redesign `ignis project sync` around an explicit plan/apply model so manifest drift is inspectable and eventually repairable rather than merely reported.

## Section 3: Manifest And Domain Modeling

Primary targets: `crates/ignis-manifest/src/**`
Suggested validation: `cargo check -p ignis-manifest`

- [ ] [MAN-01] Split `ignis-manifest` into smaller modules for model, loading, validation, network policy, and signing.
- [ ] [MAN-02] Separate pure data structures from repository-aware loading helpers so manifest types stay usable in more contexts.
- [ ] [MAN-03] Rework `ServiceManifest::validate` into composable validators by service kind to reduce branching growth as new capabilities appear.
- [ ] [MAN-04] Clarify the relationship between project manifests, derived worker manifests, and igniscloud-specific metadata so future expansion does not leak platform concerns everywhere.
- [ ] [MAN-05] Generalize auth-provider modeling so the current Google-only shape can evolve without scattering special cases across validation, docs, and CLI checks.

## Section 4: Runtime And Platform Host

Primary targets: `crates/ignis-runtime/src/**`, `crates/ignis-platform-host/src/**`
Suggested validation: `cargo check -p ignis-runtime -p ignis-platform-host`

- [ ] [RUN-01] Define a more explicit runtime lifecycle boundary between component loading, warm-up, request dispatch, and shutdown concerns.
- [ ] [RUN-02] Review per-request store construction and epoch ticker behavior for lower operational overhead without weakening isolation guarantees.
- [ ] [RUN-03] Extract outbound network policy evaluation into a dedicated policy component with clearer testable rules.
- [ ] [RUN-04] Refactor `ignis-platform-host` so SQLite connection management and row decoding are shared, testable primitives rather than repeated inline flows.
- [ ] [RUN-05] Define the next-step host abstraction boundary so additional host capabilities can be added without turning `ignis-platform-host` into a monolith.

## Section 5: SDK, Docs, And Testing

Primary targets: `crates/ignis-sdk/src/**`, high-risk validation coverage in crate tests, public docs under `README.md` and `docs/*.md`
Suggested validation: `cargo check -p ignis-sdk`, targeted `cargo test` for affected crates

- [ ] [SDK-01] Split `ignis-sdk` HTTP router internals from public ergonomic helpers so the external API can stay stable while internals evolve.
- [ ] [SDK-02] Expand router behavior coverage around wildcard routes, method-not-allowed handling, request ID propagation, and CORS preflight behavior.
- [ ] [SDK-03] Add focused tests for CLI auth callback parsing, sync drift reporting, and packaging validation so the highest-risk workflows stop relying on broad manual confidence.
- [ ] [SDK-04] Add focused tests for SQLite host behavior, especially disabled mode, transaction behavior, and typed-vs-untyped query paths.
- [ ] [SDK-05] Align public docs with the architectural boundaries above so `README`, `docs/cli.md`, `docs/api.md`, and `docs/integration.md` explain a coherent mental model rather than implementation snapshots.

## Exit Criteria

Stage 1 is ready to close when:

- all 25 items are checked
- no generated daily todo reports unfinished Stage 1 work
- `cargo check --workspace` passes on the closing tree
- no checked item conflicts with the design philosophy
- the resulting repository direction is measurably simpler to extend than the current shape
