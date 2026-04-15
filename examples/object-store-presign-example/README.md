# `object-store-presign-example`

一个最小的 Ignis object-store presign 示例。

- `api`：Rust `http` service，通过 `ignis_sdk::object_store` 向 host 请求当前 project 的平台托管 presigned URL
- 上传：`GET /presign-upload?filename=demo.txt&content_type=text/plain&size=12`
- 下载：`GET /presign-download/<file_id>`

Wasm 侧不会拿到平台 COS 的 AK/SK；签名由 node-agent host import 转发到 control-plane 完成。

## 本地构建

```bash
ignis service build --service api
```

## 部署后调用

```bash
curl "https://<project-id>.<base-domain>/presign-upload?filename=demo.txt&content_type=text/plain&size=12"
```

响应里的 `upload_url` 可以直接用于 `PUT` 文件内容；上传完成后，使用返回的 `file_id` 生成下载 URL：

```bash
curl "https://<project-id>.<base-domain>/presign-download/<file_id>"
```
