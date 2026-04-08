pub struct BundledFile {
    pub path: &'static str,
    pub contents: &'static str,
}

const IGNIS_USER_SKILL: &str = include_str!("../../../skills/ignis-user/SKILL.md");

const IGNIS_USER_FILES: &[BundledFile] = &[
    BundledFile {
        path: "SKILL.md",
        contents: IGNIS_USER_SKILL,
    },
    BundledFile {
        path: "references/api.md",
        contents: include_str!("../../../docs/api.md"),
    },
    BundledFile {
        path: "references/cli.md",
        contents: include_str!("../../../docs/cli.md"),
    },
    BundledFile {
        path: "references/doc_index.md",
        contents: include_str!("../../../skills/ignis-user/references/doc_index.md"),
    },
    BundledFile {
        path: "references/hello-service.rs",
        contents: include_str!("../../../skills/ignis-user/references/hello-service.rs"),
    },
    BundledFile {
        path: "references/ignis-toml.md",
        contents: include_str!("../../../docs/ignis-toml.md"),
    },
    BundledFile {
        path: "references/integration.md",
        contents: include_str!("../../../docs/integration.md"),
    },
    BundledFile {
        path: "references/readme.md",
        contents: include_str!("../../../README.md"),
    },
    BundledFile {
        path: "references/sqlite-service.rs",
        contents: include_str!("../../../skills/ignis-user/references/sqlite-service.rs"),
    },
    BundledFile {
        path: "references/ignis-sdk/index.md",
        contents: include_str!("../../../docs/ignis-sdk/index.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/Context.md",
        contents: include_str!("../../../docs/ignis-sdk/http/Context.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/Middleware.md",
        contents: include_str!("../../../docs/ignis-sdk/http/Middleware.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/Next.md",
        contents: include_str!("../../../docs/ignis-sdk/http/Next.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/Router.md",
        contents: include_str!("../../../docs/ignis-sdk/http/Router.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/empty_response.md",
        contents: include_str!("../../../docs/ignis-sdk/http/empty_response.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/index.md",
        contents: include_str!("../../../docs/ignis-sdk/http/index.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/middleware/cors.md",
        contents: include_str!("../../../docs/ignis-sdk/http/middleware/cors.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/middleware/index.md",
        contents: include_str!("../../../docs/ignis-sdk/http/middleware/index.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/middleware/logger.md",
        contents: include_str!("../../../docs/ignis-sdk/http/middleware/logger.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/middleware/request_id.md",
        contents: include_str!("../../../docs/ignis-sdk/http/middleware/request_id.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/http/text_response.md",
        contents: include_str!("../../../docs/ignis-sdk/http/text_response.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/execute.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/execute.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/execute_batch.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/execute_batch.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/index.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/index.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/migrations/Migration.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/migrations/Migration.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/migrations/apply.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/migrations/apply.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/migrations/index.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/migrations/index.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/query.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/query.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/query_typed.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/query_typed.md"),
    },
    BundledFile {
        path: "references/ignis-sdk/sqlite/transaction.md",
        contents: include_str!("../../../docs/ignis-sdk/sqlite/transaction.md"),
    },
];

pub fn ignis_user_files() -> &'static [BundledFile] {
    IGNIS_USER_FILES
}

pub fn raw_ignis_user_markdown() -> &'static str {
    strip_frontmatter(IGNIS_USER_SKILL)
}

fn strip_frontmatter(markdown: &'static str) -> &'static str {
    let Some(rest) = markdown.strip_prefix("---\n") else {
        return markdown;
    };
    let Some((_, body)) = rest.split_once("\n---\n") else {
        return markdown;
    };
    body
}
