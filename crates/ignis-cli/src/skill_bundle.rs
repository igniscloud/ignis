pub struct BundledFile {
    pub path: &'static str,
    pub contents: &'static str,
}

pub struct BundledSkill {
    pub name: &'static str,
    pub markdown: &'static str,
    pub files: &'static [BundledFile],
}

impl BundledSkill {
    pub fn raw_markdown(&self) -> &'static str {
        strip_frontmatter(self.markdown)
    }
}

const IGNIS_SKILL: &str = include_str!("../../../skills/ignis/SKILL.md");
const IGNIS_LOGIN_SKILL: &str = include_str!("../../../skills/ignis-login/SKILL.md");

const IGNIS_FILES: &[BundledFile] = &[
    BundledFile {
        path: "SKILL.md",
        contents: IGNIS_SKILL,
    },
    BundledFile {
        path: "references/cli.md",
        contents: include_str!("../../../docs/cli.md"),
    },
    BundledFile {
        path: "references/doc_index.md",
        contents: include_str!("../../../skills/ignis/references/doc_index.md"),
    },
    BundledFile {
        path: "references/examples/hello-fullstack/README.md",
        contents: include_str!("../../../examples/hello-fullstack/README.md"),
    },
    BundledFile {
        path: "references/examples/hello-fullstack/ignis.hcl",
        contents: include_str!("../../../examples/hello-fullstack/ignis.hcl"),
    },
    BundledFile {
        path: "references/examples/hello-fullstack/services/api/Cargo.toml",
        contents: include_str!("../../../examples/hello-fullstack/services/api/Cargo.toml"),
    },
    BundledFile {
        path: "references/examples/hello-fullstack/services/api/src/lib.rs",
        contents: include_str!("../../../examples/hello-fullstack/services/api/src/lib.rs"),
    },
    BundledFile {
        path: "references/examples/hello-fullstack/services/api/wit/world.wit",
        contents: include_str!("../../../examples/hello-fullstack/services/api/wit/world.wit"),
    },
    BundledFile {
        path: "references/examples/hello-fullstack/services/web/src/index.html",
        contents: include_str!("../../../examples/hello-fullstack/services/web/src/index.html"),
    },
    BundledFile {
        path: "references/ignis-hcl.md",
        contents: include_str!("../../../docs/ignis-hcl.md"),
    },
    BundledFile {
        path: "references/integration.md",
        contents: include_str!("../../../docs/integration.md"),
    },
    BundledFile {
        path: "references/examples/sqlite-example/README.md",
        contents: include_str!("../../../examples/sqlite-example/README.md"),
    },
    BundledFile {
        path: "references/examples/sqlite-example/ignis.hcl",
        contents: include_str!("../../../examples/sqlite-example/ignis.hcl"),
    },
    BundledFile {
        path: "references/examples/sqlite-example/services/api/Cargo.toml",
        contents: include_str!("../../../examples/sqlite-example/services/api/Cargo.toml"),
    },
    BundledFile {
        path: "references/examples/sqlite-example/services/api/src/lib.rs",
        contents: include_str!("../../../examples/sqlite-example/services/api/src/lib.rs"),
    },
    BundledFile {
        path: "references/examples/sqlite-example/services/api/wit/world.wit",
        contents: include_str!("../../../examples/sqlite-example/services/api/wit/world.wit"),
    },
    BundledFile {
        path: "references/examples/sqlite-example/services/web/src/index.html",
        contents: include_str!("../../../examples/sqlite-example/services/web/src/index.html"),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/README.md",
        contents: include_str!("../../../examples/math-proof-lab/README.md"),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/ignis.hcl",
        contents: include_str!("../../../examples/math-proof-lab/ignis.hcl"),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/api/Cargo.toml",
        contents: include_str!("../../../examples/math-proof-lab/services/api/Cargo.toml"),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/api/src/lib.rs",
        contents: include_str!("../../../examples/math-proof-lab/services/api/src/lib.rs"),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/api/wit/world.wit",
        contents: include_str!("../../../examples/math-proof-lab/services/api/wit/world.wit"),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/web/src/index.html",
        contents: include_str!("../../../examples/math-proof-lab/services/web/src/index.html"),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/orchestrator-agent/AGENTS.md",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/orchestrator-agent/AGENTS.md"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/orchestrator-agent/opencode.json.example",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/orchestrator-agent/opencode.json.example"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/literature-agent/AGENTS.md",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/literature-agent/AGENTS.md"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/literature-agent/opencode.json.example",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/literature-agent/opencode.json.example"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/curriculum-agent/AGENTS.md",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/curriculum-agent/AGENTS.md"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/curriculum-agent/opencode.json.example",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/curriculum-agent/opencode.json.example"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/formal-verifier-agent/AGENTS.md",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/formal-verifier-agent/AGENTS.md"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/formal-verifier-agent/opencode.json.example",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/formal-verifier-agent/opencode.json.example"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/pedagogy-agent/AGENTS.md",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/pedagogy-agent/AGENTS.md"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/pedagogy-agent/opencode.json.example",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/pedagogy-agent/opencode.json.example"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/rigor-critic-agent/AGENTS.md",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/rigor-critic-agent/AGENTS.md"
        ),
    },
    BundledFile {
        path: "references/examples/math-proof-lab/services/rigor-critic-agent/opencode.json.example",
        contents: include_str!(
            "../../../examples/math-proof-lab/services/rigor-critic-agent/opencode.json.example"
        ),
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

const IGNIS_LOGIN_FILES: &[BundledFile] = &[
    BundledFile {
        path: "SKILL.md",
        contents: IGNIS_LOGIN_SKILL,
    },
    BundledFile {
        path: "references/igniscloud-id-public-api.md",
        contents: include_str!(
            "../../../skills/ignis-login/references/igniscloud-id-public-api.md"
        ),
    },
    BundledFile {
        path: "references/examples/ignis-login-example/README.md",
        contents: include_str!("../../../examples/ignis-login-example/README.md"),
    },
    BundledFile {
        path: "references/examples/ignis-login-example/ignis.hcl",
        contents: include_str!("../../../examples/ignis-login-example/ignis.hcl"),
    },
    BundledFile {
        path: "references/examples/ignis-login-example/services/api/Cargo.toml",
        contents: include_str!("../../../examples/ignis-login-example/services/api/Cargo.toml"),
    },
    BundledFile {
        path: "references/examples/ignis-login-example/services/api/src/lib.rs",
        contents: include_str!("../../../examples/ignis-login-example/services/api/src/lib.rs"),
    },
    BundledFile {
        path: "references/examples/ignis-login-example/services/api/wit/world.wit",
        contents: include_str!("../../../examples/ignis-login-example/services/api/wit/world.wit"),
    },
    BundledFile {
        path: "references/examples/ignis-login-example/services/web/src/index.html",
        contents: include_str!("../../../examples/ignis-login-example/services/web/src/index.html"),
    },
];

const BUNDLED_SKILLS: &[BundledSkill] = &[
    BundledSkill {
        name: "ignis",
        markdown: IGNIS_SKILL,
        files: IGNIS_FILES,
    },
    BundledSkill {
        name: "ignis-login",
        markdown: IGNIS_LOGIN_SKILL,
        files: IGNIS_LOGIN_FILES,
    },
];

pub fn bundled_skills() -> &'static [BundledSkill] {
    BUNDLED_SKILLS
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
