# Stage 1 AR Blueprint

## Purpose

This document is the authoritative Stage 1 architecture refinement blueprint for `ignis`.

Stage focus:

- reduce structural complexity before adding more product surface
- improve cross-platform behavior in the CLI and build pipeline
- tighten runtime, manifest, and SDK boundaries so future expansion costs less

Design philosophy:

> Prefer lightweight, boring, cross-platform building blocks that reduce coupling, cognitive load, and future rework.

This is a documentation-only pass.
No code, cron, worker, or research-doc scaffolding is created as part of this change.

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

An item may be checked only when a corresponding focused research note exists under `docs/researches/Stage_1_AR/` and that note:

- is only about that item
- is non-empty
- is aligned to the design philosophy
- reflects stable SOTA or mature frontier practice
- translates recommendations back into this repository

## Worker Lanes

The blueprint is partitioned into 5 worker-ownable sections:

1. CLI structure and auth
2. Build, publish, and packaging
3. Manifest and domain modeling
4. Runtime and platform host
5. SDK, docs, and testing

Total checklist items: 25

## Section 1: CLI Structure And Auth

- [ ] Split `ignis-cli` command dispatch into dedicated modules for `project`, `service`, `auth`, `build`, and shared output/context. Research output: `docs/researches/Stage_1_AR/cli-command-boundaries.md`
- [ ] Define a small shared `ProjectContext` and service lookup layer so command handlers stop re-implementing manifest-loading concerns. Research output: `docs/researches/Stage_1_AR/cli-context-boundary.md`
- [ ] Extract browser login and loopback callback handling from `main.rs` into an isolated auth module with a narrow API surface. Research output: `docs/researches/Stage_1_AR/cli-auth-module-boundary.md`
- [ ] Replace hand-rolled localhost callback parsing with a minimal, well-tested HTTP handling strategy that reduces edge-case risk. Research output: `docs/researches/Stage_1_AR/cli-loopback-callback-strategy.md`
- [ ] Define a consistent CLI output contract for JSON success, warnings, drift reporting, and actionable failure messages across commands. Research output: `docs/researches/Stage_1_AR/cli-output-contract.md`

## Section 2: Build, Publish, And Packaging

- [ ] Remove shell-fragile default frontend scaffolding commands and replace them with a cross-platform build strategy. Research output: `docs/researches/Stage_1_AR/frontend-build-cross-platform.md`
- [ ] Replace external `tar` command packaging with an internal archive path that behaves consistently across supported developer environments. Research output: `docs/researches/Stage_1_AR/frontend-packaging-internalization.md`
- [ ] Separate build orchestration from publish/deploy orchestration so artifact production, validation, and upload have clear boundaries. Research output: `docs/researches/Stage_1_AR/build-publish-boundaries.md`
- [ ] Introduce an explicit artifact validation stage for HTTP and frontend services before publish starts. Research output: `docs/researches/Stage_1_AR/artifact-validation-gate.md`
- [ ] Redesign `ignis project sync` around an explicit plan/apply model so manifest drift is inspectable and eventually repairable rather than merely reported. Research output: `docs/researches/Stage_1_AR/project-sync-plan-apply.md`

## Section 3: Manifest And Domain Modeling

- [ ] Split `ignis-manifest` into smaller modules for model, loading, validation, network policy, and signing. Research output: `docs/researches/Stage_1_AR/manifest-module-split.md`
- [ ] Separate pure data structures from repository-aware loading helpers so manifest types stay usable in more contexts. Research output: `docs/researches/Stage_1_AR/manifest-data-vs-loading.md`
- [ ] Rework `ServiceManifest::validate` into composable validators by service kind to reduce branching growth as new capabilities appear. Research output: `docs/researches/Stage_1_AR/service-validation-composition.md`
- [ ] Clarify the relationship between project manifests, derived worker manifests, and igniscloud-specific metadata so future expansion does not leak platform concerns everywhere. Research output: `docs/researches/Stage_1_AR/project-worker-cloud-boundaries.md`
- [ ] Generalize auth-provider modeling so the current Google-only shape can evolve without scattering special cases across validation, docs, and CLI checks. Research output: `docs/researches/Stage_1_AR/auth-provider-extensibility.md`

## Section 4: Runtime And Platform Host

- [ ] Define a more explicit runtime lifecycle boundary between component loading, warm-up, request dispatch, and shutdown concerns. Research output: `docs/researches/Stage_1_AR/runtime-lifecycle-boundary.md`
- [ ] Review per-request store construction and epoch ticker behavior for lower operational overhead without weakening isolation guarantees. Research output: `docs/researches/Stage_1_AR/runtime-store-and-epoch-costs.md`
- [ ] Extract outbound network policy evaluation into a dedicated policy component with clearer testable rules. Research output: `docs/researches/Stage_1_AR/runtime-network-policy-component.md`
- [ ] Refactor `ignis-platform-host` so SQLite connection management and row decoding are shared, testable primitives rather than repeated inline flows. Research output: `docs/researches/Stage_1_AR/sqlite-host-connection-and-decoding.md`
- [ ] Define the next-step host abstraction boundary so additional host capabilities can be added without turning `ignis-platform-host` into a monolith. Research output: `docs/researches/Stage_1_AR/platform-host-extensibility.md`

## Section 5: SDK, Docs, And Testing

- [ ] Split `ignis-sdk` HTTP router internals from public ergonomic helpers so the external API can stay stable while internals evolve. Research output: `docs/researches/Stage_1_AR/sdk-router-api-vs-internals.md`
- [ ] Expand router behavior coverage around wildcard routes, method-not-allowed handling, request ID propagation, and CORS preflight behavior. Research output: `docs/researches/Stage_1_AR/sdk-router-test-coverage.md`
- [ ] Add focused tests for CLI auth callback parsing, sync drift reporting, and packaging validation so the highest-risk workflows stop relying on broad manual confidence. Research output: `docs/researches/Stage_1_AR/cli-high-risk-test-coverage.md`
- [ ] Add focused tests for SQLite host behavior, especially disabled mode, transaction behavior, and typed-vs-untyped query paths. Research output: `docs/researches/Stage_1_AR/platform-host-test-coverage.md`
- [ ] Align public docs with the architectural boundaries above so `README`, `docs/cli.md`, `docs/api.md`, and `docs/integration.md` explain a coherent mental model rather than implementation snapshots. Research output: `docs/researches/Stage_1_AR/docs-mental-model-alignment.md`

## Exit Criteria

Stage 1 is ready to close when:

- all 25 items are checked
- every checked item has a matching focused research note
- no checked item conflicts with the design philosophy
- the resulting repository direction is measurably simpler to extend than the current shape
