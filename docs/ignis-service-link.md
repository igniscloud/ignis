# Ignis Service Link

Status: v1 scope, HTTP only.

`Ignis Service Link`, or `ISL`, defines the internal HTTP service-to-service connection model for Ignis inside a single project. It does not rely on Linux `ip:port`; it relies on service identity, binding identity, and a shared runtime / control-plane / node-agent resolution contract.

The current v1 scope covers:

- same-project internal service access
- `http` bindings
- runtime resolution for `.svc` authorities
- the control-plane binding registry
- local node-agent resolution and cross-node HTTP forwarding
- project-boundary access control

The current v1 scope does not support:

- cross-project service access
- streaming

## 1. Core concepts

### 1.1 Service

`service` is the deployable unit, for example:

- `api`
- `users`
- `jobs`

### 1.2 Binding

`binding` is a protocol entry point on a service. The current v1 model only supports `http` bindings.

Examples:

- `api#http`
- `jobs#http`

A service may declare multiple `http` bindings, but only bindings referenced by an exposure become public.

### 1.3 Exposure

`exposure` only describes how a binding is exposed to the public edge.

It does not decide whether the binding is callable from inside the project.

### 1.4 Listener

`listener` is the public ingress surface. Today Ignis only supports HTTP listeners in the `public/http` shape.

## 2. Identity format

ISL uses service identities instead of `ip:port`.

Canonical formats:

- service identity: `svc://<project>/<service>`
- binding identity: `svc://<project>/<service>#<binding>`

Examples:

- `svc://shop/api`
- `svc://shop/api#http`
- `svc://shop/jobs#http`

Inside the same project, shorthand forms are allowed:

- `api`
- `api#http`
- `jobs#http`

The runtime resolves these shorthands against the current project automatically.

## 3. Security boundary

Default v1 rules:

- access is only allowed to services in the current project
- cross-project calls are denied by default
- a service can still be called internally without a public exposure
- a public exposure does not grant cross-project access

That means:

- internal visibility is controlled by bindings
- public visibility is controlled by exposures

Resolvers must enforce project validation inside a trusted boundary, not just through SDK conventions.

## 4. Transport

`http` bindings use `wasi:http`.

Guests still issue standard HTTP requests, but the target authority uses an internal reserved namespace:

```text
http://api.svc/users/1
```

The runtime recognizes `.svc` during the outbound HTTP hook:

1. no real DNS lookup
2. no public internet routing
3. the caller project fills in the target project automatically
4. the request resolves to `svc://<current-project>/api#http`
5. the runtime forwards to the active local or remote revision

If the authority is not `.svc`, the runtime continues through the normal external HTTP path.

## 5. Service discovery

The control plane maintains a project-level binding routing table with at least:

- `project`
- `service`
- `binding`
- `protocol`
- active revision
- active node
- ingress URL
- public exposures

The node agent caches activation data for its node and falls back to the control-plane binding registry when the local cache misses.

## 6. Runtime behavior

HTTP call flow:

1. the guest issues an HTTP request
2. the runtime decides whether the target is an internal `.svc` authority or an external address
3. for internal calls:
   - resolve the service identity
   - validate `target_project == caller_project`
   - look up the `http` binding and active revision
   - dispatch locally or forward remotely
4. for external calls:
   - continue through the current outbound HTTP path

Recommended stable error codes:

- `cross_project_service_access_denied`
- `service_not_found_in_project`
- `binding_not_found`
- `binding_protocol_mismatch`
- `no_active_revision_for_binding`

## 7. Activation payload

Node activation must carry:

- `service_identity`
- `binding_name`
- `protocol`
- `public_exposures`
- `worker_manifest`
- artifact metadata

This allows the node agent to handle both:

- public ingress
- in-project service discovery

## 8. Compiled plan

`ignis.hcl` is the source manifest. `CompiledProjectPlan` is the normalized deployment and communication plan derived from it.

It should stably produce at least:

- service identities
- binding table
- exposure table
- activation plans

In practice:

- `ignis.hcl` defines user intent
- `CompiledProjectPlan` defines the normalized communication and deployment plan
- igniscloud, runtime, and node-agent consume that plan

## 9. Summary

ISL v1 is the internal HTTP service connection layer for Ignis.

It unifies:

- service identity
- http binding
- service discovery
- project-boundary access control
- the shared runtime / control-plane / node-agent resolution contract
