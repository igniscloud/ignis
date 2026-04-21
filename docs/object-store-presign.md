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

let public_upload = object_store::presign_public_upload(
    "cover.jpg",
    "image/jpeg",
    123_456,
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
- `public_url`: only present for public uploads when platform public object storage is configured

`presign_download` returns the same URL shape for downloading an existing file.

Use `presign_public_upload` for public assets such as feed images, avatars, or public covers.
The upload still uses a presigned `PUT`, but the object is written to the configured public bucket
and the response includes a stable `public_url`. Store that `public_url` in your application data
instead of calling `presign_download` for every public image render.

Use `presign_upload` plus `presign_download` for private files, drafts, attachments, files that
need authorization checks, or content that should stop resolving when the backend stops issuing
read URLs.

## Platform Flow

1. Wasm service calls `ignis_sdk::object_store::presign_upload`.
2. node-agent host import sends an internal request to the control plane for the current project.
3. control-plane validates the project, file metadata, size, visibility, and storage config.
4. control-plane signs the URL with platform-managed object-storage credentials.
5. The frontend uploads directly to object storage with the returned URL.

The current implementation targets platform-managed storage first. User-owned COS/S3 credentials can be added later as a separate host/control-plane signing mode.

For the broader list of built-in runtime/system APIs, including the reserved `http://__ignis.svc/v1/services` discovery endpoint, read [System API](./system-api.md).

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
- For public uploads, `[object_storage]` must also include `public_bucket` and `public_base_url`.
- node-agent must run a build that includes the `ignis:platform/object-store` host import.
- Browser direct upload requires bucket CORS to allow the deployed project origin to use presigned `PUT` and `GET` URLs.
- Services should enforce their own product limits before calling `presign_upload`; `cos-and-jobs-example` enforces 10 MB per user.

Example platform config:

```toml
[object_storage]
endpoint = "https://<account-id>.r2.cloudflarestorage.com"
region = "auto"
bucket = "appfactory-artifacts"
public_bucket = "appfactory-artifacts-public"
public_base_url = "https://pub-<hash>.r2.dev"
access_key_id = "..."
secret_access_key = "..."
force_path_style = true
```

When `visibility = public`, control-plane signs upload URLs against `public_bucket` and returns:

```json
{
  "file_id": "file-...",
  "upload_url": "https://...",
  "public_url": "https://pub-<hash>.r2.dev/projects/<project>/files/<file-id>/<filename>",
  "method": "PUT",
  "headers": { "content-type": "image/jpeg" }
}
```
