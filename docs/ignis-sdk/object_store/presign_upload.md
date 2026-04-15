# function ignis_sdk::object_store::presign_upload

Requests a presigned upload URL for a new project file.

```rust
pub fn presign_upload(
    filename: &str,
    content_type: &str,
    size_bytes: u64,
    sha256: Option<&str>,
    expires_in_ms: Option<u64>,
) -> Result<PresignedUrl, String>
```

The returned `PresignedUrl` contains the file id, URL, HTTP method, required headers, and optional expiration timestamp. Frontends can upload directly to the returned URL without receiving platform COS/S3 credentials.
