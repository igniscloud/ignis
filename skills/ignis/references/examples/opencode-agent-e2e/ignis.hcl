project = {
  "name" = "fermats-last-theorem-high-school"
  "domain" = "prj-d36af2b718db1a1a.igniscloud.app"
}
listeners = [
  {
    "name" = "public"
    "protocol" = "http"
  }
]
exposes = [
  {
    "name" = "api"
    "listener" = "public"
    "service" = "api"
    "binding" = "http"
    "path" = "/api"
  },
  {
    "name" = "web"
    "listener" = "public"
    "service" = "web"
    "binding" = "frontend"
    "path" = "/"
  }
]
services = [
  {
    "name" = "api"
    "kind" = "http"
    "path" = "services/api"
    "bindings" = [
      {
        "name" = "http"
        "kind" = "http"
      }
    ]
    "http" = {
      "component" = "target/wasm32-wasip2/release/api.wasm"
      "base_path" = "/"
    }
    "frontend" = null
    "ignis_login" = null
    "sqlite" = {
      "enabled" = true
    }
    "resources" = {
      "memory_limit_bytes" = 134217728
    }
  },
  {
    "name" = "web"
    "kind" = "frontend"
    "path" = "services/web"
    "bindings" = [
      {
        "name" = "frontend"
        "kind" = "frontend"
      }
    ]
    "http" = null
    "frontend" = {
      "build_command" = [
        "bash",
        "-lc",
        "rm -rf dist && mkdir -p dist && cp -R src/. dist/"
      ]
      "output_dir" = "dist"
      "spa_fallback" = true
    }
    "ignis_login" = null
  },
  {
    "name" = "coordinator-agent"
    "kind" = "agent"
    "agent_runtime" = "opencode"
    "agent_memory" = "session"
    "agent_description" = "主 agent：规划费马大定理高中生证明导览的子任务，等待子 agent 结果后合成最终 JSON。"
    "path" = "services/coordinator-agent"
    "resources" = {
      "memory_limit_bytes" = 536870912
    }
  },
  {
    "name" = "elementary-agent"
    "kind" = "agent"
    "agent_runtime" = "opencode"
    "agent_memory" = "none"
    "agent_description" = "解释高中生能理解的初等数论铺垫：反证法、互质化、素数指数约化和 n=4 无穷递降。"
    "path" = "services/elementary-agent"
    "resources" = {
      "memory_limit_bytes" = 536870912
    }
  },
  {
    "name" = "bridge-agent"
    "kind" = "agent"
    "agent_runtime" = "opencode"
    "agent_memory" = "none"
    "agent_description" = "解释反例如何构造 Frey 曲线，以及 Ribet 定理如何把反例转成非 modular 的矛盾方向。"
    "path" = "services/bridge-agent"
    "resources" = {
      "memory_limit_bytes" = 536870912
    }
  },
  {
    "name" = "modularity-agent"
    "kind" = "agent"
    "agent_runtime" = "opencode"
    "agent_memory" = "none"
    "agent_description" = "用高中生可理解的类比解释 modularity、模形式和 Wiles 定理在证明链中的作用。"
    "path" = "services/modularity-agent"
    "resources" = {
      "memory_limit_bytes" = 536870912
    }
  },
  {
    "name" = "teacher-agent"
    "kind" = "agent"
    "agent_runtime" = "opencode"
    "agent_memory" = "none"
    "agent_description" = "把各 specialist 输出改写成高中生能读懂的证明导览，保留黑箱定理标注。"
    "path" = "services/teacher-agent"
    "resources" = {
      "memory_limit_bytes" = 536870912
    }
  },
  {
    "name" = "rigor-agent"
    "kind" = "agent"
    "agent_runtime" = "opencode"
    "agent_memory" = "none"
    "agent_description" = "审查最终导览是否过度宣称、是否把高等数学黑箱误写成初等证明，并给出修正建议。"
    "path" = "services/rigor-agent"
    "resources" = {
      "memory_limit_bytes" = 536870912
    }
  }
]
