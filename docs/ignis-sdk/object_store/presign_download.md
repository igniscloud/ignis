# function ignis_sdk::object_store::presign_download

Requests a presigned download URL for an existing project file.

```rust
pub fn presign_download(
    file_id: &str,
    expires_in_ms: Option<u64>,
) -> Result<PresignedUrl, String>
```

The `file_id` must be a file previously created for the current project. The returned URL can be handed to the browser for direct download.
