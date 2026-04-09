# Ignis HCL Infrastructure Architecture Design

Status: design draft based on the current `ignis` and `igniscloud` codebases, updated for an infrastructure-kernel direction.

## 1. Goal

The current Ignis stack already has a useful split between:

- `ignis`: developer-facing manifest, CLI, local runtime, guest SDK, and host ABI
- `igniscloud`: control plane, node agent, ingress, deployment activation, and billing

That split is still correct. The change is in product philosophy.

The target is not:

- a platform-managed gateway product
- a platform-managed application routing system
- a Wasm version of a traditional PaaS with built-in edge behavior

The target is:

- a Wasm-native infrastructure kernel
- service identity instead of Linux `ip:port` as the primary communication abstraction
- user-owned application topology, including user-owned gateway or BFF services
- reusable service packages that can be imported from Git or GitHub and compiled locally into Wasm

This document defines:

1. what the current code already does
2. what the public HCL model should become
3. how dependency import and local Wasm build should work
4. what responsibilities belong to the infrastructure and what should stay in user services

## 2. Current Architecture From Code

### 2.1 Repo split

Current boundaries are already relatively clean.

`ignis` contains:

- `crates/ignis-manifest`: project and worker manifest types, validation, signing
- `crates/ignis-cli`: project/service CLI, local build/dev flow, control-plane client
- `crates/ignis-runtime`: Wasmtime-based `wasi:http` runtime
- `crates/ignis-platform-host`: current SQLite host implementation
- `crates/ignis-sdk`: guest-side Rust HTTP and SQLite helpers

`igniscloud` contains:

- `crates/control-plane`: projects, services, versions, deployments, node selection, auth, billing
- `crates/node-agent`: node activation, runtime cache, ingress, static frontend serving, SQLite backup/restore
- `crates/rpc`: shared activation and billing RPC payloads

This split is already a strong foundation for an infrastructure-first system.

### 2.2 Public config and manifest model today

The current public project config is `ignis.toml`.

Code source:

- `ignis/crates/ignis-manifest/src/lib.rs`

Important current properties:

- `ProjectManifest` contains `project` plus `services`
- each `ServiceManifest` has:
  - `name`
  - `kind`
  - `path`
  - `prefix`
- `ServiceKind` is currently only:
  - `http`
  - `frontend`
- `http` services can define:
  - component path
  - base path
  - env
  - secrets
  - sqlite
  - resource limits
  - outbound network policy
- `frontend` services can define frontend build output

Important implication:

- the current manifest is still shaped like a flat application manifest
- `prefix` is acting as the public ingress declaration
- there is no first-class notion of:
  - listener
  - exposure
  - service identity
  - package dependency
  - imported service
  - config object
  - file mount

### 2.3 Local runtime model today

Code source:

- `ignis/crates/ignis-runtime/src/lib.rs`
- `ignis/crates/ignis-platform-host/src/lib.rs`

The runtime path is:

1. load a `WorkerManifest`
2. load a Wasm component with Wasmtime
3. link WASI p2 and `wasi:http`
4. link platform host imports
5. rewrite request path using `base_path`
6. execute guest handler
7. enforce:
   - CPU limit through epoch interruption
   - memory limit through store limits
   - outbound HTTP policy through `WasiHttpHooks`

Platform host support is currently minimal and explicit:

- SQLite is injected through host imports
- SQLite file path is derived locally on the node or dev machine

This is already aligned with a capability-oriented infrastructure model.

### 2.4 Control-plane deployment model today

Code source:

- `igniscloud/crates/control-plane/src/application/services.rs`
- `igniscloud/crates/control-plane/src/application/ports.rs`
- `igniscloud/crates/control-plane/src/infrastructure/external_api/node_client.rs`

The current deployment path is:

1. a service version is published
2. the control plane stores:
   - service manifest snapshot
   - worker manifest
   - artifact location
   - checksum and optional signature
3. deployment chooses one target node
4. the control plane sends a `NodeActivationRequest`

Important activation payload fields:

- `service_key`
- `project`
- `project_id`
- `service`
- `service_kind`
- `route_prefix`
- `version`
- `manifest`
- `component_base64`

Important implication:

- the platform already deploys by service identity, not by exposed port
- node activation is already close to an infrastructure-kernel model
- the public config is behind the actual internal architecture

### 2.5 Node agent and ingress model today

Code source:

- `igniscloud/crates/node-agent/src/main.rs`
- `igniscloud/crates/node-agent/src/store.rs`

The current node model is:

- activated services are persisted under `services/<service_key>/...`
- one active version is selected per service on a node
- ingress accepts HTTP requests on one listener
- the host format is:
  - `<project_id>.<base_domain>`
- the node looks up all services for that project and resolves the longest matching `route_prefix`
- frontend services are served as static bundles
- http services are dispatched into `WorkerRuntime`

Important implication:

- current routing is already closer to "project ingress + service selection" than to "port per service"
- however, it still assumes the platform is responsible for path-prefix dispatch

For the new direction, this should be treated as a transition mechanism, not as the final public model.

### 2.6 What is missing today

From the code, the main gaps are:

- no first-class `listener`
- no first-class `expose`
- no first-class internal `service identity`
- no first-class package dependency model
- no config objects and read-only mounts
- no protocol bindings beyond `http` and `frontend`
- no internal RPC/gRPC communication model
- deployment is still effectively one active node assignment per service

## 3. Product Direction: Infrastructure Kernel, Not Managed Gateway

The key design decision is:

- Ignis should provide infrastructure primitives
- users should build application behavior on top

That means a gateway is not a special platform feature. It is just another service.

If a user wants:

- API gateway
- BFF
- edge auth gateway
- gRPC gateway
- internal routing gateway

they should implement that as a normal user-owned service, typically an `http` service.

The platform should not own:

- the application's path-routing rules
- the application's gateway semantics
- the application's internal topology logic

The platform should own:

- service deployment
- service discovery
- listeners
- external exposure
- revision and activation
- capabilities
- observability

This keeps the platform small, composable, and reusable.

## 4. Why HCL Is The Right Public Format

The public config is moving beyond package metadata into infrastructure authoring.

The next config must express:

- listeners
- exposures
- services
- bindings
- dependencies
- imported services
- config objects
- mounts
- future per-environment overlays

TOML is weak here because:

- arrays-of-tables become hard to read quickly
- object references are awkward
- reusable imported objects are clumsy
- future dependency and exposure blocks will become noisy

HCL is a better fit because it is designed around:

- block-oriented infrastructure objects
- readable references between objects
- stable diffs
- incremental growth from simple to structured configurations

## 5. Target Design Principles

### 5.1 Replace Linux addresses with service identity

Developers should stop thinking in:

- `127.0.0.1:3001`
- `service-a -> service-b via ip:port`

They should think in:

- listeners
- exposures
- service identities
- bindings

The platform may still use sockets internally, but those must stay implementation details.

### 5.2 Separate build-time source addresses from run-time service identities

This distinction is critical.

Build-time addresses are things like:

- local paths
- Git URLs
- GitHub repos
- future registries

Run-time addresses are things like:

- `svc://my-project/api`
- `svc://my-project/auth#http`

The platform should never confuse these two layers.

A GitHub repository is a source dependency, not a run-time destination.

### 5.3 Keep `service` as the stable runtime unit

`service` should remain the main runtime and deployment unit.

It should own:

- code or static assets
- runtime kind
- capability bindings
- revisions
- deployment state

### 5.4 Keep gateway behavior in user services

The platform should provide:

- a public listener
- a way to expose one or more service bindings
- service discovery for internal calls

The platform should not provide, as a primary public abstraction:

- an application routing DSL that replaces user gateway logic

If a user wants rich HTTP routing, they should write a `gateway` service that forwards to:

- `svc://project/api`
- `svc://project/users`
- `svc://project/orders`

### 5.5 Keep capabilities explicit

The runtime should remain capability-oriented.

Capabilities should be modeled explicitly for:

- env
- secrets
- config objects
- read-only file mounts
- sqlite
- outbound network
- future queue/blob/cache/database bindings

### 5.6 Treat imported services as first-class build inputs

Users should be able to import services from remote sources and instantiate them into their own project.

That means the platform needs first-class support for:

- dependency declaration
- package metadata
- exported services
- imported service instances
- lockfiles
- deterministic local builds

## 6. Proposed Public HCL Model

The public config should move from `ignis.toml` to `ignis.hcl`.

The platform can keep derived internal manifests for runtime activation. Public authoring format and internal execution format do not need to be the same thing.

### 6.1 Recommended project-level objects

The first public HCL model should define these first-class objects:

- `project`
- `listener`
- `expose`
- `service`
- `dependency`
- `config`
- `secret`

And these nested concepts:

- `binding`
- `resources`
- `network`
- `mount`
- `frontend`

### 6.2 Recommended package-level objects

Dependency repositories should be able to expose reusable services as packages.

Recommended first-class package objects:

- `package`
- `export_service`

These package definitions can live in:

- a dedicated `ignis-package.hcl`
- or a future unified HCL format that supports either `project` or `package` roots

The exact filename can be finalized later. The important part is the model.

### 6.3 Service kinds

Recommended v1 kinds:

- `http`
- `frontend`

Reserved next kinds:

- `grpc`
- `rpc`
- `worker`
- `cron`

There should not be a special platform-owned `gateway` kind. A gateway is user code, not a platform primitive.

### 6.4 Public project example

```hcl
project "demo" {
  name = "demo"
}

listener "public" {
  protocol = "http"
  hostname = "demo.local"
}

dependency "auth_pkg" {
  source = "git::https://github.com/acme/auth-services.git"
  ref    = "v0.3.1"
  path   = "packages/basic-auth"
}

config "gateway_yaml" {
  file = "./configs/gateway.yaml"
}

service "auth" {
  from = "auth_pkg.auth_api"

  env = {
    AUTH_ISSUER = "demo"
  }
}

service "gateway" {
  kind      = "http"
  component = "./artifacts/gateway.wasm"

  binding "http" {
    base_path = "/"
  }

  mount "gateway_config" {
    source = "config.gateway_yaml"
    target = "/configs/gateway.yaml"
    mode   = "read_only"
  }
}

service "web" {
  kind = "frontend"

  frontend {
    build_command = ["npm", "run", "build"]
    output_dir    = "dist"
    spa_fallback  = true
  }
}

expose "public_gateway" {
  listener = "public"
  service  = "gateway"
  binding  = "http"
}
```

In this model:

- the platform exposes `gateway`
- the user-owned `gateway` service decides how to route requests internally
- `auth` is imported from a remote package but instantiated as a local service in the current project

### 6.5 Package example

```hcl
package "basic-auth" {
  name = "basic-auth"
}

export_service "auth_api" {
  kind = "http"

  source {
    path = "./services/auth-api"
  }

  build {
    toolchain     = "rust-wasi-p2"
    cargo_package = "auth-api"
  }

  binding "http" {
    base_path = "/"
  }
}
```

In this model:

- the dependency repo exports a reusable service definition
- the consuming project chooses whether and how to instantiate it
- the exported service is a build input, not a deployed remote endpoint

### 6.6 Mapping from the current model

The current `ignis.toml` fields map naturally into the proposed HCL model.

- `project.name` -> `project "<name>"`
- `services[].name` -> `service "<name>"`
- `services[].kind` -> `service.kind`
- `services[].prefix` -> transition-time compatibility only
- `services.http.component` -> `service.component`
- `services.http.base_path` -> `service.binding "http".base_path`
- `services.env` -> `service.env`
- `services.secrets` -> service secret bindings
- `services.sqlite` -> `service.sqlite`
- `services.resources` -> `service.resources`
- `services.network` -> `service.network`
- `services.frontend` -> `service.frontend`

Important note:

- `prefix` should not remain the long-term public authoring primitive
- it can exist temporarily as a compatibility layer while the platform moves toward `listener + expose + user-owned gateway`

## 7. Dependency And Build Model

### 7.1 Dependency sources

The first dependency model should support:

- local path
- Git URL
- GitHub repository shorthand

Future sources can include:

- artifact registries
- package registries

### 7.2 Build-time resolution flow

The build flow should be:

1. parse `ignis.hcl`
2. resolve dependencies into a local cache such as `.ignis/deps/`
3. load package metadata from each dependency source
4. resolve imported services like `auth_pkg.auth_api`
5. instantiate imported services into the current project as local services
6. build local and imported services into Wasm artifacts
7. write a lockfile
8. derive the compiled project plan and activation payloads

### 7.3 Lockfile

The system should use a lockfile such as `ignis.lock`.

It should pin:

- dependency source URL
- resolved commit hash
- package path
- exported service identity
- artifact digest
- toolchain metadata when necessary

This is required for deterministic builds and supply-chain control.

### 7.4 Source address versus runtime identity

After import, the service should be addressed locally by its project identity, not by its source address.

Example:

- build source: `git::https://github.com/acme/auth-services.git`
- imported alias: `service "auth"`
- runtime identity: `svc://demo/auth`

This boundary must stay strict.

## 8. Runtime And Platform Model

### 8.1 Public HCL, internal compiled plan

The public HCL file should compile into a canonical in-memory project plan.

Suggested internal layers:

1. `HclProjectConfig`
   - raw parsed authoring model
2. `ResolvedDependencyGraph`
   - fetched and pinned package inputs
3. `CompiledProjectPlan`
   - validated canonical object graph
4. `ServiceActivationPlan`
   - one deployable runtime payload per service revision
5. `IngressExposurePlan`
   - listener-to-service exposure map

### 8.2 External ingress model

The infrastructure should provide:

- listeners
- exposure of service bindings on listeners

The infrastructure should not require a global route DSL for application logic.

For v1, the minimal public ingress model is:

- `listener`
- `expose`

That is enough to let users expose:

- a frontend
- an API
- a gateway service

### 8.3 User-owned gateway model

If a user needs complex HTTP routing, they should write a service for it.

That service may:

- inspect host/path/header/method
- enforce custom auth
- aggregate multiple backend services
- translate between public and internal APIs

The platform only needs to make sure that service can:

- be exposed publicly
- discover internal services
- call them reliably by service identity

### 8.4 Internal service communication

The first internal service identity format should be something like:

- `svc://<project>/<service>`
- optionally `svc://<project>/<service>#<binding>`

Within one project, short references like `api` are acceptable in config, but the compiled plan should use canonical identities.

### 8.5 Config and mount model

The platform should not expose raw host-path bind mounts.

Instead it should expose:

- `config` objects
- `secret` objects
- read-only `mount` declarations

This fits a distributed Wasm platform much better because:

- host filesystem paths are not stable across nodes
- config objects can be versioned and validated
- mounts remain portable across nodes

Writable volumes should stay out of v1.

### 8.6 Microservice growth path

The runtime already supports `wasi:http`, which gives a practical internal HTTP bridge.

For future microservices:

- keep internal HTTP as the first communication mode
- add first-class `grpc` and `rpc` bindings later
- keep transport resolution inside the platform
- keep gateway behavior in user services

The likely evolution path is:

1. internal HTTP by service identity
2. service binding-aware discovery
3. gRPC binding support
4. richer protocol-aware service communication

## 9. Repo Responsibilities

### 9.1 `ignis`

`ignis` should own:

- HCL schema
- dependency resolution
- package model
- local source fetching
- build graph construction
- lockfile generation
- compiled project plan generation
- runtime-facing activation manifests

### 9.2 `igniscloud control-plane`

`igniscloud` control plane should own:

- projects
- services
- revisions
- deployments
- listeners
- exposures
- node selection
- service identity registration

It should not become the application's routing brain.

### 9.3 `igniscloud node-agent`

The node agent should own:

- runtime cache
- frontend static serving
- service activation
- listener handling
- exposed service dispatch
- internal execution limits and observability

The current `route_prefix` dispatch should be treated as compatibility logic while moving toward a cleaner `listener -> exposed service` model.

## 10. Explicit Non-Goals For The First HCL Version

Do not include in v1:

- platform-managed gateway behavior as the primary model
- a large global application routing DSL
- writable distributed volumes
- multi-region routing
- sidecar-style extension systems
- full gRPC service mesh semantics
- arbitrary package build execution with unbounded scripts

The first HCL version should focus on:

- HCL public config
- infrastructure primitives
- dependency import
- deterministic local Wasm builds
- service identity
- listeners and exposures
- config objects and read-only mounts

## 11. Conclusion

The current code already contains the right seeds:

- service-centered deployment
- runtime capability boundaries
- node-side ingress and activation flow
- platform-controlled revisions and deployments

The main upgrade is philosophical and public-facing:

- move from `ignis.toml` to HCL
- move from flat manifests to an infrastructure DSL
- keep the platform small
- let users implement gateways as ordinary services
- treat Git and GitHub addresses as source dependencies
- treat service identity as the runtime communication address

That is the right direction if Ignis is meant to become Wasm-native infrastructure rather than a managed application framework.
