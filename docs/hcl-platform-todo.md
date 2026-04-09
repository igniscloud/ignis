# Ignis HCL Infrastructure TODO

Status: implementation planning draft aligned with `docs/hcl-platform-architecture.md`.

## Phase 0: Freeze The Current Baseline

- [ ] Confirm the current public baseline that exists in code:
  - `ignis.toml` project/service model in `ignis/crates/ignis-manifest/src/lib.rs`
  - control-plane activation flow in `igniscloud/crates/control-plane/src/application/services.rs`
  - node ingress route resolution in `igniscloud/crates/node-agent/src/store.rs`
- [ ] Treat the current model as the compatibility baseline:
  - one project
  - many services
  - `http` and `frontend`
  - longest-prefix route dispatch on `<project_id>.<base_domain>`
- [ ] Mark current `prefix` and `route_prefix` handling as a transition layer, not the long-term public model.

## Phase 1: Redesign The Public HCL Model In `ignis`

- [ ] Introduce the new public config filename:
  - `ignis.hcl`
- [ ] Decide whether `ignis.toml` remains:
  - compatibility-only input
  - generated internal artifact
  - or deprecated input after migration
- [ ] Add public config model types in `ignis/crates/ignis-manifest`:
  - `ProjectSpec`
  - `ListenerSpec`
  - `ExposeSpec`
  - `ServiceSpec`
  - `BindingSpec`
  - `DependencySpec`
  - `ImportedServiceSpec`
  - `ConfigSpec`
  - `SecretRef`
  - `MountSpec`
- [ ] Add package model types:
  - `PackageSpec`
  - `ExportedServiceSpec`
- [ ] Add a compiled internal model:
  - `ResolvedDependencyGraph`
  - `CompiledProjectPlan`
  - `CompiledServicePlan`
  - `CompiledExposurePlan`
  - `ServiceActivationPlan`
- [ ] Keep `WorkerManifest` as an internal runtime-facing type for node activation until there is a strong reason to replace it.
- [ ] Add validators for:
  - duplicate listeners
  - duplicate exposures
  - invalid service references
  - invalid dependency references
  - invalid imported service references
  - invalid mount targets
  - unsupported service kind and binding combinations

## Phase 2: HCL Parser And Validation

- [ ] Choose the Rust HCL parser library and lock its constraints early.
- [ ] Add parser golden tests for:
  - minimal project
  - project with listener and expose
  - project with imported service
  - package with exported service
- [ ] Add compile-time validation tests for:
  - `listener + expose + service` linkage
  - config object references
  - secret references
  - imported service references
  - frontend semantics
  - current `http` service semantics
- [ ] Add clear validation errors that match the existing CLI style.

## Phase 3: Package And Dependency Resolution

- [ ] Support initial dependency source kinds:
  - local path
  - Git URL
  - GitHub repository shorthand
- [ ] Define the dependency source syntax and normalization rules.
- [ ] Decide package metadata filename:
  - `ignis-package.hcl`
  - or unified HCL with package root
- [ ] Add dependency fetch and local cache management under:
  - `.ignis/deps/`
- [ ] Resolve each dependency to an immutable revision:
  - pinned tag
  - or direct commit hash
- [ ] Load package metadata and exported services from the fetched dependency source.
- [ ] Allow the consuming project to instantiate imported services as local project services.
- [ ] Add cycle detection across packages and imported services.

## Phase 4: Lockfile And Supply-Chain Control

- [ ] Introduce a lockfile:
  - `ignis.lock`
- [ ] Store in the lockfile:
  - source URL
  - resolved commit hash
  - package path
  - exported service name
  - build metadata
  - artifact digest
- [ ] Ensure builds are reproducible from `ignis.hcl + ignis.lock`.
- [ ] Decide what signature verification hooks should exist for:
  - dependency packages
  - built artifacts
  - published Wasm components

## Phase 5: Build Graph And Local Wasm Compilation

- [ ] Build a project-level service build graph that includes:
  - local services
  - imported services
- [ ] Support compiling imported source services locally into Wasm artifacts.
- [ ] Keep the build model deterministic and reasonably constrained.
- [ ] Decide the initial supported build model:
  - Rust to Wasm Preview 2 first
- [ ] Keep arbitrary unbounded build script execution out of the first dependency model if possible.
- [ ] Add caching for imported build outputs when inputs have not changed.

## Phase 6: CLI Migration

- [ ] Update `ignis/crates/ignis-cli` project discovery to find `ignis.hcl`.
- [ ] Decide whether the CLI should support both:
  - `ignis.hcl`
  - `ignis.toml`
  during migration.
- [ ] Update `ignis project create` to generate `ignis.hcl`.
- [ ] Update `ignis service new` to edit HCL instead of TOML.
- [ ] Add dependency-related commands such as:
  - `ignis deps fetch`
  - `ignis deps lock`
  - `ignis deps tree`
  if they are useful enough to justify first-class CLI support.
- [ ] Add a validation command:
  - `ignis project check`
  - or `ignis config check`
- [ ] Add a migration helper:
  - `ignis config migrate`
- [ ] Update examples under `ignis/examples/` to HCL.
- [ ] Update generated skill docs to teach HCL and dependency import instead of TOML-only workflows.

## Phase 7: Preserve The Current Runtime Boundary

- [ ] Keep `ignis-runtime` focused on:
  - Wasmtime loading
  - `wasi:http`
  - resource limits
  - outbound request policy
  - host capability linking
- [ ] Do not push application gateway logic into the runtime crate.
- [ ] Continue deriving runtime activation payloads from the compiled project plan.
- [ ] Keep node activation payloads deterministic and minimal.

## Phase 8: Control-Plane Schema Upgrade In `igniscloud`

- [ ] Extend the control plane to accept or derive the compiled HCL project plan.
- [ ] Decide the persistence boundary:
  - store raw HCL only
  - store raw HCL plus compiled normalized plan
  - store normalized service and exposure records only
- [ ] Add control-plane awareness of:
  - listeners
  - exposures
  - service identities
  - imported service provenance where useful
- [ ] Keep `project`, `service`, `version`, and `deployment` as the main persistence backbone.
- [ ] Stop treating path-prefix route declarations as the long-term public model.
- [ ] Keep compatibility mapping from the current `prefix` model during migration.

## Phase 9: Node-Agent Ingress Upgrade

- [ ] Evolve the node ingress model toward:
  - `listener -> exposed service binding`
- [ ] Keep current longest-prefix dispatch only as compatibility logic during transition.
- [ ] Continue to support:
  - frontend static serving
  - http Wasm runtime dispatch
- [ ] Change activation payloads so the node receives exposure state directly rather than inferring everything from service manifests.
- [ ] Ensure ingress lookup remains:
  - deterministic
  - cheap
  - local-cache friendly

## Phase 10: Config Objects And Read-Only Mounts

- [ ] Add first-class `config` objects to the public HCL schema.
- [ ] Add service-level `mount` declarations with:
  - source reference
  - target path
  - read-only mode
- [ ] Keep secrets separate from config objects.
- [ ] Keep writable volumes out of v1.
- [ ] For node activation, decide how config objects are materialized:
  - injected as files before runtime activation
  - or translated into env vars where appropriate
- [ ] Add validation for file target collisions and illegal mount paths.

## Phase 11: Internal Service Communication

- [ ] Define an internal service identity format:
  - `svc://<project>/<service>`
  - optionally `svc://<project>/<service>#<binding>`
- [ ] Allow short service references inside the same project:
  - `api`
  - `users`
- [ ] Define the first internal communication mode:
  - HTTP over service identity
- [ ] Decide how internal HTTP requests are represented for Wasm guests:
  - reserved authorities over `wasi:http`
  - or platform host imports
- [ ] Keep cross-project addressing explicit and fully qualified.

## Phase 12: Microservice Protocol Expansion

- [ ] Add protocol-aware binding types to the schema:
  - `http`
  - `grpc`
  - `rpc`
- [ ] Keep `grpc` out of the first HCL rollout unless the transport design is already clear.
- [ ] Design `grpc` as a binding type, not as a Linux socket concept.
- [ ] Define how service discovery resolves a binding to active revisions and nodes.
- [ ] Keep gateway behavior in user services instead of turning `grpc` support into a platform-owned routing layer.

## Phase 13: Deployment Model Upgrade

- [ ] Keep the current single active deployment behavior as the initial baseline.
- [ ] Make the design leave room for:
  - multiple active revisions
  - weighted rollout
  - canary traffic
  - blue/green deploys
- [ ] Ensure the public HCL model does not bake in single-node assumptions even if the first implementation still has them.
- [ ] Add explicit service-identity-to-revision planning later instead of hiding all resolution inside deployment state.

## Phase 14: Observability And Policy

- [ ] Make listener, exposure, and service identities observable in logs and metrics.
- [ ] Add ingress tracing fields on the node:
  - listener
  - exposure
  - target service
  - target binding
- [ ] Keep policy hooks available for:
  - auth
  - timeout
  - retry
  - rate limit
  - billing
- [ ] Avoid turning the platform into the application's routing brain.

## Phase 15: Documentation And Examples

- [ ] Replace public `ignis.toml` examples with `ignis.hcl`.
- [ ] Add examples for:
  - single exposed API service
  - API + frontend
  - user-owned gateway service exposed publicly
  - imported auth service from a Git dependency
  - config object and read-only mount
- [ ] Add one microservice-oriented example that stays HTTP-only first.
- [ ] Add one future-looking example showing where `grpc` bindings would fit, without requiring implementation on day one.

## Phase 16: Migration Strategy

- [ ] Decide the transition window for `ignis.toml`.
- [ ] Provide a deterministic migration tool from TOML to HCL.
- [ ] Keep project and service semantics stable during migration.
- [ ] Avoid changing too many planes at once:
  - config language first
  - dependency and lockfile second
  - exposure model third
  - internal communication model fourth
- [ ] Do not couple HCL migration to full microservice or gRPC implementation.

## Recommended First Implementation Slice

If work starts immediately, the safest first slice is:

- [ ] HCL public config
- [ ] `listener + expose + service` split
- [ ] package and dependency import model
- [ ] local compilation of imported source services into Wasm
- [ ] lockfile support
- [ ] config objects and read-only mounts
- [ ] compatibility mapping back to the current `http` and `frontend` runtime flow
- [ ] no gRPC yet
- [ ] no writable volumes yet
- [ ] no platform-managed gateway DSL

This keeps the first milestone focused on the real shift:

- moving Ignis from a flat manifest into an infrastructure DSL
- while keeping application-specific routing inside user services
