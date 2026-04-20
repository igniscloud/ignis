# Math Proof Lab

`math-proof-lab` is a fullstack Ignis example for a strict multi-agent theorem proof website.

The example is intentionally stricter than a generic "explain this theorem" app. It models the product around two constraints:

- a proof must preserve mathematical status: machine-checked, literature-checked, named black box, sketch, or not derivable from the target audience's prerequisites
- an explanation must match the user's level without pretending that advanced mathematics is already known

## Services

- `web`: static Vue UI for entering a theorem question and target audience
- `api`: HTTP service that returns the agent matrix, strictness contract, workflow, and a Fermat sample boundary
- `orchestrator-agent`: dynamically chooses which specialist agents are needed and merges the proof plan
- `literature-agent`: retrieves papers, textbooks, curriculum data, knowledge graph edges, and formal-library hits
- `formal-verifier-agent`: interacts with Lean/Coq/Isabelle and emits a strict proof dependency tree
- `curriculum-agent`: maps proof dependencies to middle-school, high-school, undergraduate, graduate, or unrestricted audiences
- `pedagogy-agent`: translates the proof into an audience-level explanation with explicit status labels
- `rigor-critic-agent`: rejects loose natural-language proof drafts and sends unsafe translations back for revision

## Why these agents

The workflow is not fixed. The orchestrator should call only the agents needed for the request:

- simple algebra or geometry question: often `formal-verifier-agent` plus `pedagogy-agent`
- audience-sensitive explanation: add `curriculum-agent`
- theorem depending on external literature: add `literature-agent`
- public-facing nontrivial proof: add `rigor-critic-agent`

For a request like:

```text
用高中生能看懂的方式说明费马大定理为什么成立；不能把高等数学黑箱伪装成初等证明。
```

the correct response is not a "pure high-school proof." Such a proof is not available. A strict system must say:

1. high-school algebra and number theory can explain the statement, primitive triples, reduction to prime exponents, and the style of proof by contradiction
2. the actual proof requires new objects such as elliptic curves and modular forms
3. Ribet's theorem and Wiles' modularity theorem are named black boxes for this audience
4. the final proof is therefore `proved_with_named_black_boxes`, not `proved_from_high_school_prerequisites`

That boundary is the core product behavior.

## Local validation

```bash
cargo check --manifest-path services/api/Cargo.toml --target wasm32-wasip2
ignis service check --service api
ignis service check --service web
```

Agent services require a real OpenCode config before build/publish:

```bash
for service in orchestrator-agent literature-agent formal-verifier-agent curriculum-agent pedagogy-agent rigor-critic-agent; do
  cp ~/.config/opencode/opencode.json "services/$service/opencode.json"
  chmod 600 "services/$service/opencode.json"
done
```

Then validate an agent:

```bash
ignis service check --service orchestrator-agent
ignis service build --service orchestrator-agent
```

## API

The example API is deliberately deterministic so it can be inspected before wiring a full TaskPlan executor.

```bash
curl "http://127.0.0.1:3001/proof-plan?audience=unrestricted"
```

Production versions can extend the `api` service to call `orchestrator-agent.svc/v1/tasks`, store run state in SQLite, and poll or receive callbacks from child agents.
