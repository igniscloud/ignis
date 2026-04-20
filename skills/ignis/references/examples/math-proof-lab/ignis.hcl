project = {
  name = "math-proof-lab"
  domain = "math-proof-lab.igniscloud.app"
}

listeners = [
  {
    name = "public"
    protocol = "http"
  }
]

exposes = [
  {
    name = "api"
    listener = "public"
    service = "api"
    path = "/api"
  },
  {
    name = "web"
    listener = "public"
    service = "web"
    path = "/"
  }
]

services = [
  {
    name = "api"
    kind = "http"
    path = "services/api"
    http = {
      component = "target/wasm32-wasip2/release/api.wasm"
      base_path = "/"
    }
    sqlite = {
      enabled = true
    }
    resources = {
      memory_limit_bytes = 134217728
    }
  },
  {
    name = "web"
    kind = "frontend"
    path = "services/web"
    frontend = {
      build_command = [
        "bash",
        "-lc",
        "rm -rf dist && mkdir -p dist && cp -R src/. dist/",
      ]
      output_dir = "dist"
      spa_fallback = true
    }
  },
  {
    name = "orchestrator-agent"
    kind = "agent"
    agent_runtime = "codex"
    agent_memory = "session"
    agent_description = "主控 agent：接收数学定理请求，拆成可审计的子任务，调度其他 agent，并汇总成前端渲染数据。"
    path = "services/orchestrator-agent"
    resources = {
      memory_limit_bytes = 536870912
    }
  },
  {
    name = "literature-agent"
    kind = "agent"
    agent_runtime = "codex"
    agent_memory = "none"
    agent_description = "文献与知识图谱搜索 agent：检索权威论文、教材、形式化库条目和课程大纲，输出可追踪证据清单。"
    path = "services/literature-agent"
    resources = {
      memory_limit_bytes = 536870912
    }
  },
  {
    name = "formal-verifier-agent"
    kind = "agent"
    agent_runtime = "codex"
    agent_memory = "none"
    agent_description = "形式化逻辑推理 agent：与 Lean/Coq/Isabelle 等形式化系统交互，输出严格证明依赖树和验证状态。"
    path = "services/formal-verifier-agent"
    resources = {
      memory_limit_bytes = 536870912
    }
  },
  {
    name = "curriculum-agent"
    kind = "agent"
    agent_runtime = "codex"
    agent_memory = "none"
    agent_description = "认知水位映射 agent：按初中、高中、大学、研究生、不受限制等受众建立知识边界，标注知识缺口和补课路径。"
    path = "services/curriculum-agent"
    resources = {
      memory_limit_bytes = 536870912
    }
  },
  {
    name = "pedagogy-agent"
    kind = "agent"
    agent_runtime = "codex"
    agent_memory = "none"
    agent_description = "降维推演与翻译 agent：把严格但晦涩的数学逻辑翻译成目标受众能理解的自然语言，并保留证明状态。"
    path = "services/pedagogy-agent"
    resources = {
      memory_limit_bytes = 536870912
    }
  },
  {
    name = "rigor-critic-agent"
    kind = "agent"
    agent_runtime = "codex"
    agent_memory = "none"
    agent_description = "严谨性审查审稿人 agent：对照严格证明依赖树检查自然语言翻译是否丢失条件、跳步或不严密。"
    path = "services/rigor-critic-agent"
    resources = {
      memory_limit_bytes = 536870912
    }
  }
]
