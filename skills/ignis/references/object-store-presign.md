# Object Store Presign

Ignis now supports platform-managed object-store presigned URLs for services running in Wasm.

## What Was Added

- Guest SDK: `ignis_sdk::object_store`
- Host ABI: `ignis:platform/object-store`
- Runtime host import: node-agent links the object-store host functions into Wasm services
- Control-plane signing endpoints for platform-managed object storage
- Example: `examples/cos-and-jobs-example`

The service asks the host for a presigned URL. The host forwards the request to the control plane. The control plane signs the URL using platform object-store credentials. The Wasm module and browser never receive COS/S3 credentials.

## SDK API

```rust
use ignis_sdk::object_store;

let upload = object_store::presign_upload(
    "demo.txt",
    "text/plain",
    12,
    None,
    Some(15 * 60 * 1000),
)?;

let download = object_store::presign_download(&upload.file_id, Some(15 * 60 * 1000))?;
```

`presign_upload` returns:

- `file_id`: platform file id scoped to the current project
- `url`: the presigned upload URL
- `method`: usually `PUT`
- `headers`: headers the client should send with the upload
- `expires_at_ms`: optional expiration timestamp

`presign_download` returns the same URL shape for downloading an existing file.

## Platform Flow

1. Wasm service calls `ignis_sdk::object_store::presign_upload`.
2. node-agent host import sends an internal request to the control plane for the current project.
3. control-plane validates the project, file metadata, size, and storage config.
4. control-plane signs the URL with platform-managed object-storage credentials.
5. The frontend uploads directly to object storage with the returned URL.

The current implementation targets platform-managed storage first. User-owned COS/S3 credentials can be added later as a separate host/control-plane signing mode.

## Examples

`cos-and-jobs-example` is a fullstack example:

- Google login through `ignis_login`
- SQLite-backed upload records
- per-user 10 MB quota
- backend presign endpoint
- browser direct upload to COS/S3
- download URL signing
- a daily cron job that releases quota for expired pending uploads

## Operational Notes

- `control-plane` must have `[object_storage]` configured.
- node-agent must run a build that includes the `ignis:platform/object-store` host import.
- Browser direct upload requires bucket CORS to allow the deployed project origin to use presigned `PUT` and `GET` URLs.
- Services should enforce their own product limits before calling `presign_upload`; `cos-and-jobs-example` enforces 10 MB per user.
