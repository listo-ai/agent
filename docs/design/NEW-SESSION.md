# New Session — moved

The per-session orientation + decision tree that used to live here is now in:

### **→ [HOW-TO-ADD-CODE.md](HOW-TO-ADD-CODE.md)**

That doc covers:

- Which **language skills** to load from `/home/user/code/workspace/SKILLS/` before writing code
- The **non-negotiables** (Rules A–H) — everything-is-a-node, graph-is-the-world, modular-libraries-are-load-bearing
- **Where does my code go?** — the decision tree that routes a task to the right repo (`contracts` / `agent` / `agent-sdk` / `agent-client-*` / `ui-kit` / `ui-core` / `block-ui-sdk` / `studio` / `blocks` / `listod` / `agent-cli`)
- **What each library is for** + the MUST / MUST NOT rules that keep the "anyone can build a new UI" promise honest
- **Workflow** — driving everything via [`mani`](../../../repos-cli/EXAMPLE.md) across the multi-repo workspace
- **Task-specific reading** — which design docs to open for a given change
- **Worked examples** — three real tasks walked through the decision tree end-to-end

The big-picture map of the workspace (repos, deployment profiles, build targets, key libraries) lives in [OVERVIEW.md](OVERVIEW.md).

This stub stays so existing cross-references (`NODE-AUTHORING.md`, `NODE-RED-MODEL.md`, `TESTS.md`, `TESTING.md`, session docs) keep resolving. New sessions should start at **HOW-TO-ADD-CODE.md** directly.
