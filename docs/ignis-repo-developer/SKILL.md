---
name: ignis-repo-developer
description: Use for contributing to the ignis repository itself, including ignis-cli, ignis-sdk, ignis-manifest, ignis-runtime, ignis-platform-host, examples, docs, and skills.
---

# Ignis Repo Developer

在当前任务是“修改 Ignis 仓库本身”时使用这个 skill。

适用范围：

- 修改 `ignis-cli`、`ignis-sdk`、`ignis-manifest`
- 修改 `ignis-runtime`、`ignis-platform-host`、examples
- 修改 `docs/`、`README.md`、`skills/`
- 维护 `mddoc` 生成的 `ignis-sdk` Markdown 文档

## 工作流程

1. 先看 `git status`，不要假设工作区干净。
2. 明确改动落在哪些 crate 或文档面。
3. 对用户可见的行为变更，同步更新 `docs/`。
4. 如果改了 `crates/ignis-sdk/src/lib.rs` 的公开 API 或 doc comment，必须重新生成 `docs/ignis-sdk/`：
   `mddoc --manifest-path Cargo.toml --package ignis-sdk --lib --output-dir docs/ignis-sdk`
5. 如果改了 `ignis-manifest` 的 `ignis.toml` 模型或校验规则，必须同步更新 `docs/ignis-toml.md`。
6. skill 里的文档引用优先用软链接指向 `docs/` 或根 `README.md`，不要维护重复 Markdown。
7. 优先做定向验证，不要默认全仓重跑。

## 定向验证

- `cargo check --manifest-path /home/hy/workplace/ignis/Cargo.toml -p ignis-cli`
- `cargo check --manifest-path /home/hy/workplace/ignis/Cargo.toml -p ignis-manifest`
- `cargo check --manifest-path /home/hy/workplace/ignis/Cargo.toml -p ignis-sdk`
- `cargo doc --manifest-path /home/hy/workplace/ignis/Cargo.toml -p ignis-sdk --no-deps`
- `cargo check --manifest-path /home/hy/workplace/ignis/examples/<example>/Cargo.toml`

## 工作规则

- 不要猜测当前 manifest 字段、CLI 行为或 SDK API；以源码和 `docs/` 为准。
- `docs/ignis-sdk/` 是生成产物，优先通过 `mddoc` 重建，不要手工逐页维护。
- skill 的职责要明确：仓库开发者相关内容放这里，使用者视角内容放 `ignis` 与 `ignis-login`。
- 示例和用户文档要面向外部使用者，不要把仓库内部维护细节写进去。

## 参考资料

- 仓库首页：`references/readme.md`
- API 文档：`references/api.md`
- CLI 文档：`references/cli.md`
- 接入文档：`references/integration.md`
- `ignis.toml` 文档：`references/ignis-toml.md`
- `ignis-sdk` 生成文档入口：`references/ignis-sdk/index.md`
- 文档索引：`references/doc_index.md`
