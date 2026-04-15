# module ignis_sdk::object_store

Platform-managed object-store helpers exposed to guest workers.

These APIs ask the Ignis host for presigned URLs for the current project. The host signs with platform-managed object storage credentials and never exposes those credentials to the guest Wasm module.

## Functions

- [`presign_upload`](presign_upload.md): Requests a presigned upload URL for a new project file.
- [`presign_download`](presign_download.md): Requests a presigned download URL for an existing project file.

## Types

- `Header`
- `PresignUploadRequest`
- `PresignedUrl`
