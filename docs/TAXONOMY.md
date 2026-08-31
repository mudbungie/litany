# A working taxonomy for the LLM application stack

A reference document for someone building an AI harness. It maps the terms used across vendor docs, papers, OSS projects, specifications, and influential essays; flags where usage has converged, where it has diverged, and where the field is still arguing. Every definition is backed by direct quotes from primary sources with URLs and source-type tags.

**Scope and conventions:** each term has (1) a consensus definition, (2) the distinct ways it is used in practice, (3) direct quotes labeled `[vendor doc] / [paper] / [OSS doc] / [spec] / [essay] / [blog]` with URLs, and (4) notes on disputes. Where the field has not converged, competing usages are shown side by side.

**Section-reference convention.** This document numbers its own sections (1–12), so a bare `§N` below always names a section of *this* document; every reference to another document's section names that document — `ARCH §N` abbreviates `docs/ARCHITECTURE.md` §N, and anything else goes by path (brazen `specs/architecture.md` §8). This is the cross-document rule in `docs/ARCHITECTURE.md` §2.1, and it matters most at §4: *Tools, function calling, MCP* here, *Providers, Auth, and Models* there. Where two sections of one document are cited together, each reference carries the name (`ARCH §2.5, ARCH §3.3`); a range is written once (`ARCH §§2.6–2.7`).

**Mental model — vocabulary is layered, not unified:** an Application Programming Interface (API)-layer "tool call" is not a Model Context Protocol (MCP) "tool"; a framework "agent" is not an API "assistant message"; an Large Language Model (LLM) "prompt" is not an MCP "prompt." When two terms collide, the layer that owns each meaning is noted. Reference points: Anthropic Claude 4.x, OpenAI GPT-5 and o-series, Google Gemini, MCP specification 2025-11-25.

---

## 1. Execution model: harness, scaffold, agent, loop, turn, step, call

### Harness
**Variants:** agent harness, execution harness, evaluation harness.
**Consensus:** The stateful program that wraps an LLM and drives execution — managing the loop, tool dispatch, prompt construction, memory, and termination. In evaluations (evals), the full scaffold that turns a bare model into a runnable agent against a task.

- "A harness is defined as a stateful program wrapping an LLM that determines prompt construction, retrieval, memory, and context management." — Hugo Cisneros notes on Meta-Harness [blog] — https://hugocisneros.com/notes/leemetaharnessendtoend2026/
- "The harness (also called a scaffold) is everything wrapped around the model to make it useful as an agent. The execution loop that decides when to call the model again. The tools it can use… The logic that decides what context gets loaded…" — Adam Baitch, "The Model vs. the Harness" [blog] — https://medium.com/@adambaitch/the-model-vs-the-harness-which-actually-matters-more-59dd3116bb31
- "This is the pattern baked into many LLM APIs as tools or function calls — the LLM is given the ability to request actions to be executed by its harness…" — Simon Willison [essay] — https://simonw.substack.com/p/i-think-agent-may-finally-have-a
- "The Claude Agent Software Development Kit (SDK) is a powerful, general-purpose agent harness... It has context management capabilities such as compaction, which enables an agent to work on a task without exhausting the context window." — Anthropic, "Effective harnesses for long-running agents" [vendor blog] — https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents

**Notes:** "Harness" also has an older meaning from EleutherAI's `lm-evaluation-harness`, referring to benchmark runners generally; usage has drifted toward the agent-scaffold meaning post-2024. Anthropic's official docs rarely use "harness"; Anthropic staff use it informally and have started to canonicalize it in engineering posts (Claude Agent SDK).

### Scaffold / scaffolding
**Variants:** agent scaffold, agentic scaffold.
**Consensus:** Software surrounding an LLM that augments its capabilities — tools, prompts, loops, memory, multi-model orchestration — without changing weights. In practice "scaffold" and "harness" are near-synonyms; safety/evals communities (Model Evaluation and Threat Research (METR), AI Security Institute (AISI), Apollo, BlueDot) favor "scaffold"; coding-agent and developer-infrastructure communities favor "harness."

- "'Scaffolding' is quite a broad term for ways of augmenting an AI model's capabilities after it has been trained… It typically doesn't include fine-tuning or other methods that directly alter the model's internals." — BlueDot Impact [blog] — https://blog.bluedot.org/p/what-is-ai-scaffolding
- "Agent scaffolds: Scaffolds which try to make LLMs into goal-directed agents… Usually, the LLM is put into a loop with a history of its observations and actions, until its task is done." — aisafety.info [essay] — https://aisafety.info/questions/NM25/What-is-scaffolding
- "This is where we choose an appropriate scaffold (ReAct, Triframe, Claude Code, Codex, etc.) and make any tweaks needed to address spurious failures." — METR, "Task-Completion Time Horizons" [paper/blog] — https://metr.org/time-horizons/
- "Other solvers might do prompt engineering, multi-turn dialog, critique, or provide an agent scaffold." — Inspect AI docs [OSS doc] — https://inspect.aisi.org.uk/

**Harness vs scaffold dispute:** Most practitioners treat them as synonyms (Baitch: "also called a scaffold"). Attempts to distinguish them — scaffold = prompting/structural augmentation, harness = the executable program — are not honored consistently. SWE-agent uses its own term **"agent–computer interface (ACI)"** for this layer. Use both if you want to be understood by both camps; if writing for METR/AISI, prefer "scaffold."

### Agent
**Variants:** LLM agent, AI agent, agentic system, language agent.
**Consensus:** An LLM placed in a loop where it can call tools (and observe results) to pursue a goal. Anthropic explicitly distinguishes *workflows* (predefined code paths) from *agents* (LLM dynamically directs its control flow); both are "agentic systems."

- "Workflows are systems where LLMs and tools are orchestrated through predefined code paths. Agents, on the other hand, are systems where LLMs dynamically direct their own processes and tool usage, maintaining control over how they accomplish tasks." — Schluntz & Zhang, Anthropic, "Building effective agents" [vendor engineering blog] — https://www.anthropic.com/research/building-effective-agents
- "An LLM agent runs tools in a loop to achieve a goal." — Simon Willison [essay] — https://simonw.substack.com/p/i-think-agent-may-finally-have-a
- "Agents: LLMs configured with instructions, tools, guardrails, and handoffs" — OpenAI Agents SDK README [OSS doc] — https://github.com/openai/openai-agents-python
- "In a LLM-powered autonomous agent system, LLM functions as the agent's brain, complemented by several key components: Planning… Memory… Tool use." — Lilian Weng, "LLM Powered Autonomous Agents" [essay] — https://lilianweng.github.io/posts/2023-06-23-agent/

**Notes:** Famously contested. Willison has collected over 200 definitions. Non-technical usage ("customer-support agent") and technical usage diverge. Anthropic flags that even its own customers use it inconsistently. Vendor primitives also diverge: in OpenAI's SDK, "agent" is a **configuration object** (the `Runner` runs the loop, not the agent); in CrewAI it is a role-playing persona; in LangGraph it is a graph of nodes; in AutoGen it is an actor; in Pydantic AI it is a typed object with a `result_type`.

**litany's stance:** litany pins **agent** to the running-instance sense above — one living instantiation (a goal, a growing context, a step loop, a termination), Willison's "LLM runs tools in a loop toward a goal" reading — and makes it the structural primitive (`docs/ARCHITECTURE.md` §2.1). The *configuration* an agent forks from (souls, tool enablements, grants, workflow) is a **config**, never an "agent"; the OpenAI-SDK "agent = config object" sense is explicitly not litany's.

### Agent name vs agent id
**Variants:** agent name, agent id, handle, address, session id.
**Consensus (none — the field conflates the two).** Most frameworks give an agent exactly one string and let it serve as both label and key. In the OpenAI Agents SDK `Agent(name=…)` names a *configuration object*, not a running instance; in CrewAI the persona's `role` is the label; LangGraph identifies nodes by graph key; provider APIs identify a run by an opaque server-side id nobody speaks aloud. Where a framework does have both, the two are typically allowed to overlap, and resolution then guesses.

**litany's stance (authoritative definition in `docs/ARCHITECTURE.md` §2.1; mechanism in ARCH §2.3, addressing in ARCH §2.11).** litany splits the two facts and lets neither stand in for the other:

- an **id** is the *identifier* — the agent's full hyphenated descent, which is simultaneously its branch name, its worktree directory and its `steps/` / `inbox/` namespace keys. Every agent has one, it is minted at the fork, it is a path component, and it carries **no display semantics**: nothing derives a label from an id.
- a **name** is the *display* discriminator — one unbroken word, settled at the dispatch that creates the agent (supplied by the dispatcher, or **minted** from the embedded wordlist on omission — ARCH §2.3, yog bl-aca4 — as **two words joined in PascalCase**, `PeachHollow`, which is one unbroken token and carries no separator, bl-79a2; unnamed is a readable pre-mint state, not a creatable one) and immutable thereafter, unique among the workspace's **living** agents and lawfully **recyclable** once retention (ARCH §9.2) deletes the ref that wore it. It is never a path component. "Display" bounds what it *addresses*, not who hears it: the agent is told its own name in the system slot, as one derived sentence and no instruction (ARCH §2.8), because an agent others speak to by name is owed the name they will use.

Recyclability is the whole reason they are two facts: ids must never collide, names must be re-usable, and one string cannot be both. The two spaces are kept **disjoint by construction** (an id always begins with a compact `YYYYMMDDTHHMMSSZ` timestamp; a name that does is refused at creation), so addressing by "id or name" is a *reading*, never a tiebreak — and a name worn by two living agents is refused with its candidates rather than guessed. Neither term is a *handle* (nothing is polled) or a *session* (a banned term, §3 below).

### Role
**Variants:** agent role, message role, role prompting, role-based access control (RBAC).
**Consensus (none — the senses are unrelated):** the field uses "role" for at least three different things, each owned by a different layer:

1. **Message role** — the `role` field on a wire message (`user` / `assistant` / `system` / `developer` / `tool`): the API-layer sense (§3 above; e.g. "each input message must be an object with a `role` and `content`").
2. **Persona role** — a framework-level character attached to an agent configuration. CrewAI is the canonical case (its Agent "has `role`, `goal`, `backstory`, tools" — §11 below); prompt-engineering guides use "role prompting" for the same move made inside a system prompt ("you are a senior reviewer…").
3. **Access-control role** — the RBAC sense imported from ops/security vocabulary, appearing in agent platforms around tool and data permissions.

The senses do not interact; when two could collide in one sentence, qualify ("message role", "the worker role").

**litany's stance (authoritative definition — role = config-selection key).** litany pins **role** as a term of art, defined here; `docs/ARCHITECTURE.md` §4.3 is its mechanism. A role is *the name under which a config commit bundles what a dispatch needs*: a **model** (the `(provider row, model id)` pointer in `providers.yaml` `roles:`, ARCH §4.3), a **toolset** (the role's `tools:` list, ARCH §3.3), a **soul** (`souls/<role>.md` — the name is the path, no override, ARCH §2.2), a **context-assembly policy** (the role's `manifest.yaml` entry — pinning, order, budget, overflow — ARCH §5.2), and, v1.1, a **capability grant** (the `grant:` ceiling, ARCH §3.6, and *Capability grant* in §4 below). It is a *pure selection key*: `litany dispatch <role>` resolves the name against the governing config commit and the five bindings follow. The name carries **no behavior** and is not an identity — "worker" is not a kind of agent, it is the key an agent was dispatched under; the agent remains the one structural primitive (above). The role set is **open**: a role is valid iff the governing config commit lists `roles.<name>` and carries `souls/<name>.md` — nothing else mints one, the harness never enumerates role names, and any valid role is dispatchable by any dispatcher, unconstrained by the dispatcher's own role (ARCH §4.3).

What a role does **not** solve — stated so future code cannot overload the term: it is not a **behavior hook** (execution semantics never branch on a role name; the one bounded exception is the workflow interpreter's closed vocabulary, which keys three names — worker, compactor, verifier — and is code by the ARCH §6 severability line, ARCH §4.3 / ARCH §6); not a **permission or identity system** (authorization is out of scope per ARCH §1.1; the v1.1 grant bounds a role's *tools*, never agents as principals); not a **persona** in the CrewAI sense (the soul is one of the four bindings, not the concept itself); not the **wire message role** (sense 1); and not a **workflow position** (ARCH §6 — the workflow has no position). Ordinary-English "role" (the part a component plays — "the tool executor is a role inside the executor," ARCH §2.1 / ARCH §3.2) remains available in prose where no config sense could be read; the term of art is the config key.

### Agentic loop / agent loop
**Variants:** tool-use loop, ReAct loop.
**Consensus:** The outer iteration where the model is called, emits tool calls, tools execute, results are appended, and the model is called again until a stop condition (final output, max turns/steps, budget exhausted).

- "When you call Runner.run(), the SDK runs a loop until it gets a final output: The LLM is called… If the response has a final output, the loop ends… Tool calls are processed (if any) and tool response messages are appended. Then the loop continues from step 1. You can use the max_turns parameter to limit the number of loop executions." — OpenAI Agents SDK docs [vendor doc] — https://openai.github.io/openai-agents-python/
- "A critical new skill to develop is designing agentic loops… carefully selecting tools to run in a loop to achieve a specified goal." — Simon Willison [essay] — https://simonw.substack.com/p/designing-agentic-loops
- "The loop continues until: A finish reasoning other than tool-calls is returned, or a tool that is invoked does not have an execute function…" — Vercel AI SDK, "Agents: Loop Control" [vendor doc] — https://ai-sdk.dev/docs/agents/loop-control

### ReAct
**Consensus:** A prompting/agent paradigm (Yao et al., International Conference on Learning Representations (ICLR) 2023) interleaving *reasoning traces* ("thoughts") with *actions* (tool calls) and *observations*. Now widely used as a generic label for any think→act→observe loop, even without the explicit textual scaffolding.

- "We explore the use of LLMs to generate both reasoning traces and task-specific actions in an interleaved manner… reasoning traces help the model induce, track, and update action plans as well as handle exceptions, while actions allow it to interface with external sources…" — Yao et al., "ReAct" [paper] — https://arxiv.org/abs/2210.03629
- "Basic ReAct agent. Agent that runs a tool use loop until the model submits an answer using the submit() tool." — Inspect AI reference [OSS doc] — https://inspect.aisi.org.uk/reference/inspect_ai.solver.html

**Note:** Modern "ReAct agents" usually skip explicit `Thought:/Action:/Observation:` text and rely on native tool-calling APIs; the conceptual loop persists.

### Orchestrator / orchestration
**Consensus:** The component (code or LLM) that coordinates work across multiple LLM calls, tools, or sub-agents. Anthropic's "orchestrator-workers" is a specific workflow pattern where a central LLM decomposes and delegates. In OpenAI's SDK, the `Runner` orchestrates turn-by-turn.

- "Orchestrator-workers: … a central LLM dynamically breaks down tasks, delegates them to worker LLMs, and synthesizes their results." — Anthropic [vendor blog] — https://www.anthropic.com/research/building-effective-agents
- "Agent plus Runner lets the SDK manage turns, tools, guardrails, handoffs, and sessions for you. If you want to own that loop yourself, use the Responses API directly instead." — OpenAI Agents SDK [vendor doc] — https://openai.github.io/openai-agents-python/agents/
- "Multi-agent with supervisor: how to orchestrate individual agents by using an LLM as a 'supervisor' to distribute work." — LangGraph README [OSS doc] — https://pypi.org/project/langgraph/0.0.23/

### Turn — the most overloaded term in the stack
**Consensus (ambiguous):** One round of user↔assistant exchange in a conversation. Providers disagree on whether a turn is (a) one role-alternation in the message list, (b) one API request/response, or (c) an entire assistant response including all intermediate tool round-trips.

- **Anthropic:** "Anthropic trains Claude models to operate on alternating user and assistant conversational turns. When creating a new message, you specify the prior conversational turns with the messages parameter." — Anthropic Messages API via AWS Bedrock [vendor doc] — https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters-anthropic-claude-messages.html
- **Anthropic (with tools):** "Even though there are multiple API messages, the tool use loop is conceptually part of one continuous assistant response… the entire assistant turn (including tool use loops) must operate in a single thinking mode." — Anthropic, "Building with extended thinking" [vendor doc] — https://docs.anthropic.com/en/docs/build-with-claude/extended-thinking
- **OpenAI Agents SDK:** "You can use the max_turns parameter to limit the number of loop executions." — OpenAI Agents SDK [vendor doc] — https://openai.github.io/openai-agents-python/
- **LangGraph:** "Checkpoints represent the state of a thread in a (potentially) multi-turn interaction between a user (or users or other systems)." — LangGraph.js concepts [OSS doc] — https://dev.to/zand/langgraphjs-concept-guide-50g0

**Disambiguation you can depend on:**

| System | "Turn" means |
|---|---|
| Anthropic Messages API | One role-alternation in the `messages` array (user or assistant). An assistant turn with `tool_use` + `tool_result` exchanges is conceptually one continuous assistant turn. |
| OpenAI Agents SDK | One iteration of the `Runner` loop (one LLM call + tool execution), bounded by `max_turns`. Many turns can happen inside one user message. |
| OpenAI Chat/Responses raw | Informal; the unit is a request. |
| LangGraph | "Multi-turn" means across user messages; within a run it counts "super-steps"/nodes, not turns. |
| Inspect AI | "Multi-turn dialog" governed by `message_limit`. |

**When writing:** always specify "conversational turn" (messages-array sense) vs "agent turn / loop iteration" (OpenAI-SDK sense).

### Step
**Consensus:** One discrete unit of progress inside the agent loop — typically one model invocation plus its tool executions. Often synonymous with "turn" in the agent-loop sense; preferred in framework/engineering code (Vercel AI SDK, LangGraph, Inspect AI) over API/product contexts.

- "The AI SDK provides built-in loop control through two parameters: stopWhen for defining stopping conditions and prepareStep for modifying settings… By default, agents stop after 20 steps using stepCountIs(20)." — Vercel AI SDK [vendor doc] — https://ai-sdk.dev/docs/agents/loop-control
- "Inspired by Google's Pregel system, the program proceeds in discrete 'super-steps.' A super-step can be considered a single iteration over the graph nodes. Nodes that run in parallel are part of the same super-step, while nodes that run sequentially belong to separate super-steps." — LangGraph docs [OSS doc] — https://docs.langchain.com/oss/python/langgraph/graph-api
- "Compose a solver from multiple other solvers. Solvers are executed in turn, and a solver step event is added to the transcript for each." — Inspect AI solver reference [OSS doc] — https://inspect.aisi.org.uk/reference/inspect_ai.solver.html

**Step ≈ OpenAI-SDK-turn ≈ LangGraph super-step ≈ one ReAct cycle.** In LangGraph a step can contain multiple parallel nodes; in Vercel AI SDK and OpenAI's SDK it is strictly one model-call iteration.

**litany's stance:** litany adopts **step** as its structural unit — one model call and the tool calls it emits (`docs/ARCHITECTURE.md` §2.1) — and **bans "turn"** outright for the incompatible vendor meanings catalogued above; a step lands as one linear commit on the agent's branch.

### Call
**Consensus:** Four nested meanings, routinely conflated:
- **API call** — one Hypertext Transfer Protocol (HTTP) request to a provider endpoint (e.g., `POST /v1/messages`).
- **Model call / LLM call** — one invocation of a model to produce output; may correspond to one API call, or many (streaming, retries, router fan-out).
- **Tool call** — the model's structured request to invoke a named tool (Anthropic's preferred term; also OpenAI's current term).
- **Function call** — OpenAI's original (June 2023) name for tool call; legacy `function_call` field superseded by `tool_calls`.

- "For many applications, however, optimizing single LLM calls with retrieval and in-context examples is usually enough." — Anthropic [vendor blog] — https://www.anthropic.com/research/building-effective-agents
- "OpenAI originally launched this capability as 'function calling' in June 2023 and later expanded the terminology to 'tool calling' to encompass broader use cases." — aiagentslist.com [blog] — https://aiagentslist.com/blog/what-is-tool-calling
- "A function call or tool call refers to a special kind of response we can get from the model if it examines a prompt, and then determines that in order to follow the instructions in the prompt, it needs to call one of the tools we made available to it." — OpenAI Function calling guide [vendor doc] — https://developers.openai.com/api/docs/guides/function-calling

**Practical note:** A single user message can trigger one conversational turn, many agent turns/steps, many model/API calls, and many tool calls — these are not interchangeable counts. When logging, always separate these axes.

### Generation / Completion / Inference
- **Generation** — the act of the model producing output tokens; in code, `generate()` is the canonical entry point. In Reinforcement Learning (RL) contexts, "a generation" = one sampled trajectory/rollout.
- **Completion** — historically OpenAI's pre-chat `/v1/completions` endpoint. "Chat completion" is the `/v1/chat/completions` endpoint with messages. Today often used loosely for "one assistant response." Anthropic's deprecated Text Completions API used the term; the Messages API output is a `Message`. OpenAI's Responses API (2025) uses `response`. **Terminology is drifting away from "completion" toward "message"/"response."**
- **Inference** — running a trained model in forward-pass mode (as opposed to training). Refers to both the computational act and the service layer.

- "The most elemental solver, generate(), just calls the model with a prompt and collects the output." — Inspect AI [OSS doc] — https://inspect.aisi.org.uk/
- "Chat Completions is the standard API to use with OpenAI's latest models… To have a more interactive and dynamic conversation with our models, you can use messages in chat format instead of the legacy prompt-style used with completions." — OpenAI Help Center [vendor doc] — https://help.openai.com/en/articles/7042661-moving-from-completions-to-chat-completions-in-the-openai-api
- "**Test-time compute** / **inference-time compute**" has become a distinct term of art post-o1 (2024+) for reasoning models that spend variable compute per query. — Lilian Weng [essay] — https://lilianweng.github.io/

### Cross-cutting cheat sheet

| Concept | Anthropic | OpenAI Agents SDK | LangGraph | Inspect AI | Vercel AI SDK |
|---|---|---|---|---|---|
| One user↔assistant exchange | "turn" (message pair) | one user input to `Runner.run()` | "multi-turn interaction" | "multi-turn dialog" | one `generate()` call sequence |
| One model invocation in a loop | "API call" within a turn | **turn** (bounded by `max_turns`) | "super-step"/node execution | "step" (transcript event) | **step** (bounded by `stopWhen`/`stepCountIs`) |
| Model's request to invoke a tool | **tool_use** block | **tool_call** | tool node invocation | tool call | tool call |
| Full agent program | agents / workflows | Agent + Runner | graph (nodes + edges) | solver / agent | `Agent` |

---

## 2. Providers, models, endpoints, SDKs

### Provider (the second-most overloaded term)
**Variants:** model provider, model creator, inference provider, API provider, LLM platform, AI gateway, router.

**Consensus:** A **model provider** trains and owns the weights (OpenAI, Anthropic, Google, Meta, DeepSeek, xAI, Mistral, Cohere). An **inference provider** serves a model (possibly one it didn't train) over an API — Together, Fireworks, Groq, DeepInfra, AWS Bedrock, Google Vertex, Azure OpenAI, Cerebras, HuggingFace Inference Providers. An **AI gateway / router** (OpenRouter, LiteLLM, Vercel AI Gateway, Portkey, Helicone) sits in front of many inference providers and exposes a unified API.

- **OpenRouter uses "provider" to mean inference endpoint operator:** "OpenRouter routes requests to the best available providers for your model. By default, requests are load balanced across the top providers to maximize uptime." — OpenRouter Provider Routing [vendor doc] — https://openrouter.ai/docs/guides/routing/provider-selection
- **Vercel AI Gateway:** "Models are AI algorithms... Providers are the companies or services that host these models, such as xAI, OpenAI, or Anthropic. In some cases, multiple providers, including the model creator, host the same model." — Vercel [vendor doc] — https://vercel.com/docs/ai-gateway/models-and-providers
- **LiteLLM** identifies backends by `provider/model` (e.g., `together_ai/mistralai/Mistral-7B-Instruct-v0.1`) and bills itself as "AI Gateway (Proxy) to call 100+ LLM APIs in OpenAI (or native) format." — https://github.com/BerriAI/litellm

**For your harness:** plan for the three-role split explicitly. Model identity (`creator/family`), inference provider (`groq`, `fireworks`, `bedrock`), and gateway (if any) are three independent axes.

### Provider adapter (litany-specific coinage)
This repo uses **provider** strictly in the *inference provider* sense (an `(endpoint, auth)` pair — config). It also introduces **provider adapter**, a distinct term of art, for the *executable binary* that owns provider wire protocols. Since v0.6 the adapter is [brazen](https://github.com/mudbungie/brazen)'s `bz` — one binary for every provider; a provider is a named row in brazen's config, and litany references rows by name (see `docs/ARCHITECTURE.md` §4.1 and ARCH §4.4). The two terms are still not interchangeable: brazen's config contains providers; `$PATH` contains the adapter. The harness talks to a provider only through the adapter, giving the provider boundary the same externalization shape as the tool boundary (ARCH §3.3). The coinage is litany-specific — the field has no settled term for "the binary that owns the provider wire protocols," and the adapter word is borrowed from the GoF pattern sense, not from any vendor's vocabulary. (Historical, v0.1–v0.5: one `litany-provider-<name>` binary per provider.)

### Model: foundation, base, instruct, reasoning
A **foundation model**, in the Stanford Center for Research on Foundation Models (CRFM) sense, is a model "trained on broad data...that can be adapted to a wide range of downstream tasks." The term covers **base models** (raw next-token predictors) and their tuned descendants.

- "Models are AI algorithms that process your input data to generate responses…" — Vercel AI Gateway [vendor doc] — https://vercel.com/docs/ai-gateway/models-and-providers
- "In this paper, we use the notation DeepSeek-V3-Base as the base model, DeepSeek-V3 as the instructed model." — DeepSeek-R1 paper [paper] — https://arxiv.org/abs/2501.12948
- "The o1 series of models are trained with reinforcement learning to perform complex reasoning. o1 models think before they answer, producing a long internal chain of thought before responding to the user." — OpenAI o1 model page [vendor doc] — https://developers.openai.com/api/docs/models/o1
- "When Claude 3.7 Sonnet is using its extended thinking capability, it could be described as benefiting from 'serial test-time compute'." — Anthropic [vendor blog] — https://www.anthropic.com/news/visible-extended-thinking

### Post-training vocabulary
The training stages between a base model and a deployable assistant. All techniques after pretraining are collectively **post-training**.

- **Supervised Fine-Tuning (SFT)** / **instruction tuning** — fits the base model to follow instructions via curated demonstrations.
- **Reinforcement Learning from Human Feedback (RLHF)** — popularized by OpenAI's InstructGPT (2022); a reward model trained on human preference comparisons drives Proximal Policy Optimization (PPO) updates to the policy.
- **Reinforcement Learning from AI Feedback (RLAIF)** — replaces human preference labels with model-generated ones; cheaper and faster.
- **Direct Preference Optimization (DPO)** — Rafailov et al. (2023); skips the explicit reward model and optimizes the policy directly on preference pairs. Now the dominant alternative to PPO-RLHF in open models.
- **Constitutional AI (CAI)** — Anthropic's self-supervision regime where the model critiques and revises its own outputs against a written constitution; used in conjunction with RLAIF.
- **Group Relative Policy Optimization (GRPO)** — DeepSeek's RL algorithm (R1 paper, 2025); replaces the value network used by PPO with group-normalized rewards.

Sources: InstructGPT (Ouyang et al., 2022) https://arxiv.org/abs/2203.02155; DPO (Rafailov et al., 2023) https://arxiv.org/abs/2305.18290; Constitutional AI (Bai et al., 2022) https://arxiv.org/abs/2212.08073; DeepSeek-R1 (2025) https://arxiv.org/abs/2501.12948.

### Reasoning model — vendor-specific flavors

| Vendor | Shape |
|---|---|
| OpenAI (o1/o3/GPT-5 reasoning) | Distinct trained checkpoints. Reasoning tokens billed but hidden; controlled by `reasoning.effort` (`minimal`/`low`/`medium`/`high`). |
| Anthropic | A **mode** of the same model ("Extended thinking mode isn't an option that switches to a different model with a separate strategy. Instead, it's allowing the very same model to give itself more time." — https://www.anthropic.com/news/visible-extended-thinking). Enabled via `thinking: {type: "enabled", budget_tokens: N}`. Thinking blocks returned in response. |
| Google Gemini | Mode flag; controlled by `thinkingConfig.thinkingBudget`. |
| DeepSeek R1 | Distinct checkpoint. Uses explicit `<think>...</think>` tags in output. |

- Simon Willison flags the category: "I don't really like the term 'reasoning' because I don't think it has a robust definition in the context of LLMs, but OpenAI have committed to using it." [essay] — https://simonwillison.net/2024/Sep/12/openai-o1/
- "Hidden tokens that aren't returned as part of the message response content but are used by the model to help generate a final answer." — OpenAI on reasoning tokens [vendor doc].

**Important:** OpenAI explicitly advises *against* adding "think step by step" to o-series prompts — the trained behavior conflicts with the prompt instruction.

### Inference-time techniques (not architectures)
- **Chain-of-Thought (CoT)** prompting — Wei et al. (2022): "generating a chain of thought — a series of intermediate reasoning steps — significantly improves the ability of large language models to perform complex reasoning." — https://arxiv.org/abs/2201.11903
- **Self-consistency** — Wang et al. (2022): sample multiple CoTs, vote on the answer. — https://arxiv.org/abs/2203.11171
- **Tree of Thoughts (ToT)** — Yao et al. (2023): branch-and-search over reasoning steps. — https://arxiv.org/abs/2305.10601
- **Reflection** — agent critiques and revises its own output; basis for procedural-memory updates.

### Endpoint, API, SDK
- **Endpoint** — a specific HTTP URL. On routers, `(provider, model)` combination.
- **API** — the wire protocol. OpenAI has three major chat surfaces: **Chat Completions** (`/v1/chat/completions`, original), **Responses** (`/v1/responses`, current recommended unified surface for text/images/audio/tools/reasoning state), **Assistants** (deprecated — shutting down 2026-08-26). Anthropic's main surface is the **Messages API** (`/v1/messages`, stateless). Google: `generateContent`.
- **SDK** — a language client library. Unified SDKs (LiteLLM, Vercel AI SDK) abstract providers behind one interface.

- "After achieving feature parity in the Responses API, we've deprecated the Assistants API. It will shut down on August 26, 2026." — OpenAI Assistants docs [vendor doc] — https://platform.openai.com/docs/assistants/deep-dive
- "The Messages API is stateless, which means that you always send the full conversational history to the API." — Anthropic Messages API [vendor doc] — https://docs.anthropic.com/en/api/prompt-validation

### Sampling controls
Cross-vendor knobs governing how tokens are drawn from the model distribution.

- **`temperature`** — 0.0 = greedy-ish, 1.0+ = creative. Anthropic: "even with temperature of 0.0, the results will not be fully deterministic."
- **`top_p`** — nucleus sampling; restrict to the smallest set of tokens whose cumulative probability exceeds `p`.
- **`top_k`** — restrict to the top-k most probable tokens (Anthropic, Google; not OpenAI Chat Completions).
- **`stop` / `stop_sequences`** — terminate generation on a matching string.
- **`max_tokens`** (Anthropic) / **`max_completion_tokens`** / **`max_output_tokens`** (OpenAI Responses) — cap generated output length.
- **`seed`** — best-effort determinism; not guaranteed.

---

## 3. Context, prompts, messages, threads

### Context window and context
- "LLMs, like humans, lose focus or experience confusion at a certain point… as the number of tokens in the context window increases, the model's ability to accurately recall information from that context decreases." — Anthropic coined "context rot" in [vendor blog] — https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Karpathy's analogy, widely quoted: "LLMs are like a new kind of operating system. The LLM is like the Central Processing Unit (CPU) and its context window is like the Random Access Memory (RAM), serving as the model's working memory." — via LangChain [blog] — https://blog.langchain.com/context-engineering-for-agents/
- "Context refers to the set of tokens included when sampling from a large-language model (LLM)." — Anthropic [vendor blog] — same URL

Common context-window sizes in 2026 range from 128K to 2M tokens depending on vendor and model tier.

### Prompt, system prompt, and OpenAI's "developer message" rename
**Prompt** — three meanings, all in active use:
1. The full input sent to a model on any given call (formal sense).
2. The system prompt specifically (casual sense).
3. An MCP `prompts/` primitive — a user-triggered parameterized template that *produces* a message list (protocol sense).

OpenAI's Responses API also overloads "prompt" to mean a versioned behavioral profile (replacing Assistants).

**System prompt vs developer message** — a key vendor difference:

| | Anthropic | OpenAI (post-o1, Dec 2024+) |
|---|---|---|
| Term | "system prompt" | "developer message" (renamed from "system") |
| Placement | Top-level `system` field, outside `messages` | Inside `input`/`messages` as `role: "developer"` (Responses also accepts `instructions`) |
| Hierarchy | 2 levels: system > user/assistant | 5 levels: platform > developer > user > assistant > tool |
| Override semantics | System generally authoritative | Platform (OpenAI) cannot be overridden by developer; developer > user by default |

- "A system prompt lets you provide context and instructions to Anthropic Claude, such as specifying a particular goal or role." — AWS Bedrock Claude docs [vendor doc] — https://docs.aws.amazon.com/bedrock/latest/userguide/model-parameters-anthropic-claude-messages.html
- "**Platform**: Rules that cannot be overridden by developers or users… Developers can choose to send any sequence of developer, user, and assistant messages as an input to the assistant… OpenAI may insert system messages into the input to steer the assistant's behavior." — OpenAI Model Spec [vendor doc] — https://model-spec.openai.com/2025-02-12.html
- "developer is where instructions are provided to the model. This role was previously called system but for all models since the o1 release, it is now called developer." — Aurelio reference [blog] — https://www.aurelio.ai/reference/openai-developer-role
- Backwards compatible: "for GPT-4o, if you happen to use developer messages, they will auto-convert to system messages." — OpenAI community [vendor forum] — https://community.openai.com/t/system-vs-developer-role-in-4o-model/1119179

**Common misconception:** secondary sources claim system and developer messages are "functionally the same." They are not — they sit at different levels in the chain of command and have different override semantics. They behave similarly in trivial cases. Anthropic does **not** have an equivalent of OpenAI's "platform" role; OpenAI's platform layer is implicit policy embedded in the Model Spec.

### User message, assistant message, tool message
- "Each input message must be an object with a `role` and `content`… you can include multiple `user` and `assistant` messages (they must alternate, if so). The first message must always use the user role." — Anthropic prompt-engineering tutorial [OSS doc] — https://github.com/anthropics/prompt-eng-interactive-tutorial
- "**Assistant**: the entity that the end user or developer interacts with… 'model' and 'assistant' will be approximately synonymous." — OpenAI Model Spec [vendor doc] — https://cdn.openai.com/spec/model-spec-2024-05-08.html
- Anthropic returns assistant messages with typed content blocks: `text`, `thinking`, `redacted_thinking`, `tool_use`. — Claude extended thinking [vendor doc] — https://docs.claude.com/en/docs/build-with-claude/extended-thinking
- Tool-result messages: OpenAI uses `role: "tool"` with `tool_call_id`; Anthropic embeds `tool_result` blocks inside a user-role message.

### Conversation, thread, session, chat history — the sprawl

| Term | Scope | Owner | Persistence |
|---|---|---|---|
| chat history / message history | raw list of messages | client | whatever client stores |
| conversation | ordered message/item list; OpenAI server object in Responses | OpenAI (Responses) or client | server-side if `conversation` object |
| thread | server-side message container (OpenAI Assistants) | OpenAI Assistants (deprecating Aug 2026) | server-side, OpenAI-managed |
| session | UI lifetime or connection lifetime; Anthropic Managed Agents append-only log | app or agent platform | duration of interaction; can outlive a context window |

- "A thread was a collection of messages stored server-side. Threads could only store messages. **Conversations store items, which can include messages, tool calls, tool outputs, and other data.**" — OpenAI Assistants migration [vendor doc] — https://platform.openai.com/docs/assistants/quickstart
- "Threads serve as conversation containers that store Messages exchanged between a user and an Assistant…" — Azure/OpenAI [vendor doc] — https://learn.microsoft.com/en-us/azure/ai-services/openai/assistants-reference-threads
- Anthropic Managed Agents: "a session (the append-only log of everything that happened)… the session provides this same benefit, serving as a context object that lives outside Claude's context window." — Anthropic [vendor blog] — https://www.anthropic.com/engineering/managed-agents

LangGraph uses `thread_id` as the key for its checkpointer, which snapshots state at every super-step — distinct from OpenAI's deprecated `Thread` object despite the name overlap.

**litany's usage (litany-specific stance):** litany's structural primitive is the **agent** (`docs/ARCHITECTURE.md` §2.1) — one living instantiation with a goal, a growing context, a step loop, and a termination (the running-instance sense of §1, not the OpenAI-SDK config-object sense). A **root agent** is forked by a user message; a **child agent** by a dispatch tool call; parent/child is recorded provenance, and litany **deletes "subagent" as a category** — a child agent is just an agent. What is deleted is the *category*, not the *word*. An agent is an agent: every agent, however deep, holds the same powers and answers to the same controls — it is addressable by `message` (parent, child, and sibling alike, ARCH §2.11), it may dispatch agents of its own, and the only sanctioned circumscription is an explicit prohibition such as the tree's `max_depth` (ARCH §6). Given that, **"subagent" remains good usage** wherever it names the *relation* — an agent created by another agent, whose lifetime its creator expects to circumscribe. Rewriting it to "child agent" buys nothing and is not required by this ladder; what is forbidden is any prose or shipped prompt string implying a subagent holds fewer powers, is unreachable, or answers to a budget or control its parent does not. Structural implementations may still differ (roles differ in soul and toolset); the *capabilities of the primitive* do not. The agent tree and the config lineage it forks from live in one git repository, the **workspace** — litany's isolation boundary, across which no mechanism reaches. The versioned configuration an agent forks from — souls, tool enablements, grants, workflow, manifest — is a **config**, carried on a descriptively-named config branch; "agent" is never used for this configuration sense.

Against the sprawl above, the ladder retires the structural terms litany once used: **conversation** and **conversation repo** (superseded by *agent* and *workspace*) and **invocation** (subsumed by *model call* / *agent*). **exchange** is demoted to a *span* — the UX stretch between a user message and its terminal response, over a root agent's linear history, owning no branch or lifecycle. **session** stays **banned** as an interaction-span term (its transport/connection sense collides with the per-framework overload catalogued above), the POSIX process-session sense (`setsid`) excepted. **thread** was weighed as the instance term and **rejected**: agent-as-instance already names it, and "thread" is spoken for both by the server-side message-container sense above and by litany's own OS-thread sense (`docs/ARCHITECTURE.md` §3.1, "Processes, not threads"). Full definitions and banned-usage rules in `docs/ARCHITECTURE.md` §2.1. (Historical: v0.1–v0.3 used "conversation"/"invocation" as structural terms; the workspace-substrate rewrite has landed in the architecture, and only passing prose in its later sections still carries the pre-ladder vocabulary — see the transition note in `docs/ARCHITECTURE.md` §2.1.)

litany additionally defines **message** as a structural term (`docs/ARCHITECTURE.md` §2.1, ARCH §2.11): content addressed to an *existing* agent by any sender — the user or another agent — deposited into the recipient's **inbox** (a workspace-root queue directory, namespaced by agent id) and delivered by the recipient's **executor** (the single process driving the branch's step loop, ARCH §2.11 executor lock) as a committed worktree file at a step boundary. Distinct from the wire-level *user message* / *assistant message* senses above: in litany prose, wire messages are always written qualified, and unqualified "message" means the ARCH §2.11 primitive.

litany also coins **epitaph** as a term of art (`docs/ARCHITECTURE.md` §2.6): a **field on the result message**, not a kind of message. It is the pinned manner in which an agent ended — one of *final response*, *stopped*, *budget-exhausted*, or *died* (a hard crash: SIGKILL, OOM, panic) — carried in every result message an agent deposits alongside the terminal ref and, iff the agent spoke, the terminal response. Two properties are load-bearing. It is the **union** over all terminal events, never the exception-set: were it to name only the no-terminal-response cases it would be a message *kind*, code would branch on message shape, and the "child that died without returning" special case would return through the vocabulary door; as a total field, code branches on its *value* and the single delivery path is preserved. And it is **pinned** for the same reason the terminal ref is: later revival of the child moves its live classification (ARCH §3.5, `stopped` → `quiescent`) but must not move what the parent was already offered — the epitaph is a frozen fact, not a live query. The result message's return is executor-performed, never a model tool call ("Return is not a verb," `docs/PRINCIPLES.md`); the epitaph is what the executor pins as it deposits. On disk it is `epitaph:` frontmatter on the ordinary deposit file (`docs/ARCHITECTURE.md` §2.11: the path carries framing, frontmatter carries asserted facts, the body is the content).

Three further coinages ride the epitaph and are defined here (mechanism in `docs/ARCHITECTURE.md` §2.6, *A reply answers the last prompter; an obituary reports to the dispatcher*). They name **speech acts, not message kinds** — one file shape, one delivery path, and code branching on the epitaph's value as ever:

- **Reply** — a result message whose epitaph is *final response*. It is an answer, and it is addressed to whoever asked: the **last prompter**. Not a synonym for "result message"; the other three epitaphs also produce result messages, and they are not answers.
- **Obituary** — a result message whose epitaph is *stopped*, *budget-exhausted*, or *died*. It is not an answer to anyone but a structural fact about the agent tree — *this agent is gone* — so it is addressed to the **dispatcher** (the agent's own id minus its last descent segment, ARCH §2.11), an address descent fixes and no rewrite of a conversation can move. The word is chosen for exactly the property "epitaph" already carries: it reports an ending to the party with a standing interest in it.
- **Last prompter** — the sender of the newest **prompt** in an agent's own transcript: the highest-`NNN` delivered `messages/NNN-<sender>.md` entry (ARCH §2.3) that is not a returning child's result message (a return, not a question) and not the agent's own note to itself (ARCH §2.11 *Self-messages*). Derived on read, never stored. The reserved sender `user` is a legal answer and means *no agent inbox is addressed* — the operator reads the reply in the agent's own conversation. Distinct from **dispatcher**: the two coincide on the dispatch step, which is why the pre-bl-a96a "a child returns to its parent" rule read as correct until somebody else spoke to a running child.


### Context engineering (who coined it)
**Consensus:** Curating everything in the context window (system prompt, tools, retrieved docs, memory, history, compaction), not just writing a prompt. **No single coiner.** Popularized mid-2025 through Lütke → Karpathy → Willison → Anthropic.

- **Tobi Lütke**, 18 Jun 2025: "I really like the term 'context engineering' over prompt engineering. It describes the core skill better: the art of providing all the context for the task to be plausibly solvable by the LLM." — via Simon Willison [blog] — https://simonwillison.net/2025/jun/27/context-engineering/
- **Andrej Karpathy**, 25 Jun 2025: "+1 for 'context engineering' over 'prompt engineering'. People associate prompts with short task descriptions you'd give an LLM in your day-to-day use. When in every industrial-strength LLM app, context engineering is the delicate art and science of filling the context window with just the right information for the next step." — @karpathy [essay] — https://x.com/karpathy/status/1937902205765607626
- Karpathy explicitly disclaims coinage: "I'm not trying to coin a new term." — 36Kr [blog] — https://eu.36kr.com/en/p/3366869315372801
- **Anthropic (formal framing), 29 Sep 2025:** "At Anthropic, we view context engineering as the natural progression of prompt engineering… Context engineering refers to the set of strategies for curating and maintaining the optimal set of tokens (information) during LLM inference, including all the other information that may land there outside of the prompts." — Anthropic [vendor engineering blog] — https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Division of labor: "In contrast to the discrete task of writing a prompt, context engineering is iterative and the curation phase happens each time we decide what to pass to the model." — Anthropic, same URL

### Context management
The runtime side of context engineering: writing, selecting, compressing, isolating tokens as an agent runs. LangChain's widely-adopted taxonomy: "**write, select, compress, and isolate.**" — https://blog.langchain.com/context-engineering-for-agents/. Anthropic refers to "compaction" and "context resets" as concrete techniques in the Agent SDK.

### Compaction landing vocabulary (litany-specific coinage)
Litany's compaction (ARCH §§2.6–2.7) lands by **rebase-forward**, and these terms are defined by that mechanism:

- **Compaction point** — the commit the compactor forks off: the dispatching branch's tip at dispatch, or `HEAD~keep_recent` behind it under a configured retained tail. Derived at return time from the compactor's dispatch-commit parent, never stored.
- **Compaction span** — the commit range the pass covers: from the branch's checkpoint origin (exclusive) up to the compaction point (inclusive). The landing squashes it out of the branch's history.
- **Checkpoint origin** — the commit the checkpoint clock measures from and the span's exclusive lower bound: the branch's founding commit — its dispatch commit, or its most recent compaction base once one has landed (`compactor::checkpoint::origin`). Always derived from git, never a stored counter.
- **Compaction product** — what one pass makes, and the only thing its landing carries: the paths the pass nominated for deletion and the `summary/**` it wrote. Read off the compactor's branch **after its own dispatch commit** — everything at or before that commit is the tree it inherited — so the class is derived from git and needs no record of what the tools did. One definition, read twice: the landing applies it (`compactor::land::base`), and `mark_for_deletion` declines a nomination that falls inside it, because a pass's own product is not the branch's history to shed (ARCH §2.7).
- **Compaction base** — the single commit that replaces the span: the tree at the compaction point with the compaction product applied (nominated deletions removed, the new summary added), subject `compaction base [<compactor-id>]`. The checkpoint clock's origin and the prompt-cache rebuild point.
- **Rebase-forward** — the landing itself, and the **one landing move in the system**: mint a base commit, then replay every commit after a boundary commit on top of it and move the branch to the replayed tip. Zero-downtime: the branch never idles and the retained tail survives verbatim. Compaction is one of its two users and names the boundary the compaction point; **retarget** (below) is the other and names it the branch's dispatch commit. Replaces the retired **compaction merge** (the `--no-ff` merge-back landing); the workflow action is `land_compaction`, with `compaction_merge` parsing as the retired spelling.
- **Retained tail** — the most recent commits kept out of the span (`compaction.intermediate.keep_recent`), structurally outside the compactor's view.

Note "compaction", not "compression": the latter is banned in the context-management sense (ARCH §2.1) — in this taxonomy compression belongs to LangChain's write/select/compress/isolate axis and to weight quantization, and litany's mechanism deletes and summarizes rather than re-encodes.

### Retarget vocabulary (litany-specific coinage)
An agent's governing config commit is derived from its branch's ancestry, so **fork is the freeze** (ARCH §2.2): a config edit after the fork governs nothing that agent does. These terms name the one exit.

- **Retarget** — moving a *running* agent onto another config commit, by re-forking its branch off that commit and replaying its own history on top (**rebase-forward**, above). Not a merge, not a graft, and not "live config update" (deferred, ARCH §11): the freeze still holds by default, and a retarget is a deliberate, auditable break of it requested per agent and per target. The verb is `litany retarget <workspace> <agent> [--config <name>]`.
- **Retarget mark** — the user's half of the act: the ref `refs/litany/retarget/<agent-id>`, pointing at the target config commit. Writing it moves no branch, which is why the ARCH §2.3 single-writer invariant survives — the agent's **own executor** reads the mark at its next step boundary and performs the landing, then consumes the mark.
- **Re-derived dispatch commit** — the base a retarget lands on: a newly minted dispatch commit parented on the target config commit, with everything config-shaped derived from *that* commit (the ARCH §3.3 descriptor cut, the control-file removal, the pinned soul) rather than replayed from the old one. It keeps the old commit's subject verbatim, because a branch's founding commit is identified by its subject.

---

## 4. Tools, function calling, MCP

### Tool
**Consensus:** An abstract piece of functionality with a name, natural-language description, and JavaScript Object Notation (JSON) Schema input signature. In modern APIs "tool" is the umbrella; "function" is one kind of tool, alongside built-ins (web search, code execution, computer use).

- "A function or tool refers in the abstract to a piece of functionality that we tell the model it has access to." — OpenAI Function calling guide [vendor doc] — https://developers.openai.com/api/docs/guides/function-calling
- "Tools enable models to interact with external systems… Each tool is uniquely identified by a name and includes metadata describing its schema." — MCP Specification, Tools [spec] — https://modelcontextprotocol.io/specification/2025-11-25/server/tools

### Function → tool rename (OpenAI history)
- June 2023: `functions` / `function_call` introduced.
- Nov 2023 (`2023-12-01-preview` / GPT-4 Turbo): `tools` / `tool_choice` introduced; `functions` deprecated; parallel tool calls enabled.
- 2024–2026: "function calling" remains the conversational term; the API surface uses "tools."

- "The replacement for functions is the tools parameter. The replacement for function_call is the tool_choice parameter." — Microsoft Learn [vendor doc] — https://learn.microsoft.com/en-us/azure/ai-services/openai/how-to/function-calling
- "Function calling (also known as tool calling)…" — OpenAI [vendor doc] — https://developers.openai.com/api/docs/guides/function-calling

Anthropic uses `tool_use` / `tool_result` blocks; OpenAI uses `tool_calls` on a message and `role: "tool"` for results. A single model call can emit multiple parallel tool calls. Anthropic also distinguishes **server tools** (run on Anthropic infrastructure, e.g., web_search) vs **client tools** (executed by caller).

### `tool_choice` semantics
Converging across vendors:
- `auto` — model decides (default when tools present).
- `any` (Anthropic) / `required` (OpenAI) — model must call some tool.
- `none` — no tools may be called.
- `{type: "tool", name: "X"}` — force a specific tool.

**Parallel tool calls:** on by default in Claude 4.x and GPT-4o+. Anthropic exposes `disable_parallel_tool_use`; OpenAI exposes `parallel_tool_calls: false`.

### Structured outputs / JSON mode
Constrains generation to a provided JSON Schema, giving guaranteed-parseable output. OpenAI's `response_format: {type: "json_schema", strict: true}` is the modern form; the older `{type: "json_object"}` flag is deprecated for new code. Anthropic supports schema enforcement through tool-use (declare a single tool whose input schema is the desired output shape).

### Action, capability, plugin
- **Action** — informal umbrella for any side-effect-producing tool call; not a formal object in OpenAI/Anthropic APIs. Present in older frameworks (LangChain Agents, ChatGPT "Actions" for GPTs). "Tools let AI models take action (model-controlled)" — MCP Server Spot [blog] — https://www.mcpserverspot.com/learn/architecture/mcp-building-blocks
- **Capability** — overloaded: (1) informal marketing umbrella; (2) MCP protocol-level feature flags negotiated at init (tools, resources, prompts, sampling, roots); (3) Anthropic's framing of what Skills grant. "Servers that support tools MUST declare the tools capability." — MCP Spec [spec] — https://modelcontextprotocol.io/specification/2025-11-25/server/tools
- **Plugin** — in 2026, overwhelmingly refers to Claude Code plugins (an installable bundle of skills, sub-agents, slash commands, hooks, MCP servers), not OpenAI's deprecated 2023 ChatGPT Plugins. "Plugins extend Claude Code with skills, agents, hooks, and MCP servers." — Claude Code docs [vendor doc] — https://code.claude.com/docs/en/discover-plugins

### Capability grant (litany-specific coinage)
litany coins **capability grant** (or **grant**) and **capability manifest** as paired terms of art for its v1.1 tool sandbox (`docs/ARCHITECTURE.md` §3.6), distinct from the three field senses of "capability" above:
- **Capability manifest** — the set of host capabilities a tool artifact *declares it needs*, across five axes (fs scopes, net hosts, exec, clock, env). For a `wasm32-wasip2` component it is not a sidecar file but the component's own **WASI imports**, read from the artifact; a non-imported interface has no host function to reach, so the manifest cannot be under-declared.
- **Capability grant** — the ceiling a **role** *permits*, a flat `grant:` block in the role's `providers.yaml` entry (`docs/ARCHITECTURE.md` §4.3, ARCH §3.6). Default empty.
- **Effective authority** = manifest ∩ grant, computed at load, gated by manifest ⊆ grant (a tool asking beyond its grant fails at load, loudly).

This is neither the MCP "capability" (a protocol feature flag negotiated at init, §4 above) nor the marketing "Skills grant capability" sense. litany's grant is a host-authority envelope enforced by the wasmtime/WASI host; its manifest is derived from the artifact, never declared in config — the config carries only the grant.

### Tool window and grant gate (litany-specific coinage)
Two structural terms for the tool half of a step (`docs/ARCHITECTURE.md` §2.5, ARCH §3.3):
- **Tool window** — the span of a step between a settled model-output entry carrying `tool_use` blocks and the commit of their `tool_result` entries: where the grant gate decides, any configured tool control adjudicates, and the tool executor runs. Contrast the model-call window — the adapter invocation that precedes it.
- **Grant gate** — the *declaring is not permitting* check (`docs/ARCHITECTURE.md` §3.3, bl-5a1f), run inside the tool window before anything executes: a `tool_use` must name a tool in the calling role's grant plus its procedure-injected set; any other name returns an in-band error result and nothing runs. Declared-in-the-request is wider than callable by design — an inherited transcript forces declarations the gate never honours.

### Tool control (litany-specific coinage)
litany coins **tool control** (or **control**) for the adjudicator its `workflow.yaml` `tool_control:` block places in front of tool execution (`docs/ARCHITECTURE.md` §3.3 *Tool control*, ARCH §6), with three companion terms:
- **Tool control** — an operator-configured external binary consulted about every grant-permitted tool invocation *before the executor is entered*. It is a *predicate in the tool path*, distinct from an ARCH §6 workflow **action** (an effect run at a lifecycle or per-step event) and from the **capability grant** above (the grant is structure — what a role may call at all; a control is policy — whether *this* invocation runs now). No control ships; the seam does.
- **Verdict** — the control's answer, a closed three-value set: **pass** (the invocation executes unchanged), **refuse** (it never executes; the reason reaches the model as an in-band `is_error` `tool_result`), **hold** (it parks for out-of-band review; the model sees nothing). A control that cannot answer **fails closed** — the invocation does not run and the step aborts loudly.
- **Hold mark** — `refs/litany/held/<agent-id>`, the value-carrying per-agent mark recording a hold: which `tool_use` id parked, on which tool, and the control's reason. Its assertion is exact — *held before execution, nothing at or past it ran* — which is what makes resuming the window replay-safe. **Release** is not a harness verb: the next drive of the agent re-adjudicates the held invocation freshly, so whatever out-of-band fact lifts the hold is the control's own contract.

The codex analogue is the pre-tool-use hook plus the guardian (an LLM adjudicator failing closed); litany's control is the same leverage point with the adjudicator externalized to a config-named binary, so a guardian-shaped control is a verifier-role dispatch away and never a harness feature.

### Multi-tool envelope / inner invocation (litany-specific coinage)
litany coins two paired terms for its `multi_tool` built-in (`docs/ARCHITECTURE.md` §3.3 *The multi-tool*):
- **Multi-tool envelope** (or **envelope**, where the multi-tool is in scope) — one `multi_tool` tool invocation: a `tool_use` block whose input carries a list of inner invocations plus execution metadata (`on_failure`). One envelope is one wire-level tool invocation, one committed `tool_result` transcript entry, and one tool commit. Distinct from the ARCH §3.3 **result envelope**, which is the model-facing rendering of any one finished tool invocation — an inner invocation's rendering *is* a result envelope, nested inside the multi-tool envelope's aggregate.
- **Inner invocation** — one `{name, input}` entry in an envelope's `invocations` list: the same shape a top-level `tool_use` block carries, run through the same grant gate and executor, with a diagnostic record of its own under a derived id (`<envelope tool_use.id>-<k>`). It is not a wire-level tool invocation: the model minted no id for it and no per-entry `tool_result` exists.

Not to be confused with the vendors' **parallel tool calls** (§4 above, `disable_parallel_tool_use` / `parallel_tool_calls`), where the *model* emits several sibling `tool_use` blocks in one assistant message and each gets its own wire-level result. An envelope is one block; its fan-out is application-layer, serial, and attributed inside a single result.

### Model Context Protocol (MCP)
Introduced by Anthropic in late 2024 and now the lingua franca for tool integration; defines a **client–host–server** architecture over JSON Remote Procedure Call 2.0 (JSON-RPC 2.0). The official spec: **"MCP follows a client-host-server architecture where each host can run multiple client instances."**

- **Host** — the AI application itself (Claude Desktop, Cursor, VS Code, a custom agent). Owns the LLM and the UI.
- **Client** — an isolated connection manager living inside the host, maintaining a 1:1 stateful session with one server.
- **Server** — a standalone process exposing data or actions from some external system (GitHub, a database, a filesystem, a SaaS API).

Mental model: host = AI app, clients = ambassadors, servers = external embassies.

### MCP server primitives
Three primitives, distinguished by **who controls them**:

| Primitive | Controlled by | Purpose | Typical UX |
|---|---|---|---|
| Tool | Model | Executable actions the LLM can invoke | Auto-invoked in chat |
| Resource | Application | Read-only data to include as context | Attached/selected in UI |
| Prompt | User | Reusable parameterized templates | Slash command / template picker |

- "Tools in MCP are designed to be **model-controlled**, meaning that the language model can discover and invoke tools automatically based on its contextual understanding and the user's prompts." — MCP Spec [spec] — https://modelcontextprotocol.io/specification/2025-11-25/server/tools
- "Resources are designed to be **application-controlled**, meaning that the client application can decide how and when they should be used." — MCP docs [spec] — https://modelcontextprotocol.info/docs/concepts/resources/
- "Prompts are designed to be **user-controlled**, meaning they are exposed from servers to clients with the intention of the user being able to explicitly select them for use." — MCP docs [spec] — https://modelcontextprotocol.info/docs/concepts/prompts/

**MCP prompt ≠ LLM prompt.** The MCP `prompts/` primitive is a user-triggered template (think VS Code slash command) that *expands into* a structured message list — not the input string sent to a model.

### MCP client primitives
Client-side primitives standardized in spec 2025-06-18:
- **Roots** — client-declared filesystem scopes the server may operate within.
- **Sampling** — server requests the host's LLM to generate text (inverts the usual direction).
- **Elicitation** — server requests structured input from the user mid-session.

### MCP transports and capability negotiation
- **stdio** — local subprocess with pipes; default for local servers.
- **Streamable HTTP** — introduced March 2025, replacing the earlier HTTP+Server-Sent Events (SSE) combo; handles remote servers with bidirectional streaming.

During the initialization handshake, client and server negotiate **capabilities**; only negotiated features can be used in the session.

### Tool discovery: "advertise before load" / progressive disclosure / lazy loading
A tool or skill is **advertised by metadata before its full content is loaded into context**. Three overlapping implementations and names:

**1. Progressive disclosure (Anthropic Skills, Oct 2025).** The term itself is **not** new — it comes from Jakob Nielsen's UX work from the 1980s–90s ("Progressive disclosure defers advanced or rarely used features to a secondary screen, making applications easier to learn and less error-prone." — https://www.nngroup.com/articles/progressive-disclosure/). Anthropic re-applied it as the label for the staged loading mechanism in Agent Skills:

- "At startup, the agent pre-loads the name and description of every installed skill into its system prompt. This metadata is the first level of progressive disclosure: it provides just enough information for Claude to know when each skill should be used without loading all of it into context." — Anthropic, "Equipping agents for the real world with Agent Skills" [vendor engineering blog] — https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills
- "The actual body of this file is the second level of detail. If Claude thinks the skill is relevant to the current task, it will load the skill by reading its full SKILL.md into context." — same URL
- "Agents with a filesystem and code execution tools don't need to read the entirety of a skill into their context window when working on a particular task. This means that the amount of context that can be bundled into a skill is effectively unbounded." — same URL
- Simon Willison's summary: "At the start of a session Claude's various harnesses can scan all available skill files and read a short explanation for each one from the frontmatter YAML in the Markdown file. This is very token efficient: each skill only takes up a few dozen extra tokens, with the full details only loaded in should the user request a task that the skill can help solve." — [essay] — https://simonwillison.net/2025/Oct/16/claude-skills/

**2. Deferred loading / Tool Search (Anthropic, Nov 2025).** For tools rather than skills:

- "The tool search tool enables Claude to work with hundreds or thousands of tools by dynamically discovering and loading them on-demand. Instead of loading all tool definitions into the context window upfront, Claude searches your tool catalog (including tool names, descriptions, argument names, and argument descriptions) and loads only the tools it needs." — Anthropic, Tool search tool [vendor doc] — https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool
- "You provide all your tool definitions to the API, but mark tools with `defer_loading: true` to make them discoverable on-demand. Deferred tools aren't loaded into Claude's context initially." — Anthropic [vendor engineering blog] — https://www.anthropic.com/engineering/advanced-tool-use
- "This represents an 85% reduction in token usage while maintaining access to your full tool library." — same URL
- Variants: `tool_search_tool_regex_20251119`, `tool_search_tool_bm25_*`. Beta header `advanced-tool-use-2025-11-20`.

**3. OpenAI's analogue (GPT-5.4+).** "If you have an application with many functions or large schemas, you can pair function calling with tool search to defer rarely used tools and load them only when the model needs them." — OpenAI Function calling [vendor doc] — https://developers.openai.com/api/docs/guides/function-calling

**4. MCP tool discovery.** Classic upfront listing via `tools/list` — "To discover available tools, clients send a tools/list request." — MCP Spec [spec] — https://modelcontextprotocol.io/specification/2025-11-25/server/tools. Anthropic's `mcp-client-2025-11-20` beta adds deferred loading for whole MCP toolsets.

**Naming summary:** the "advertise before load" pattern does not have a single universal name. The canonical terms are:
1. **Progressive disclosure** (Anthropic, for Skills)
2. **Deferred loading / lazy loading** (Anthropic Tool Search, OpenAI tool_search)
3. **On-demand / just-in-time tool loading** (community)
4. **Metadata preloading** (descriptive)
5. **Progressive discovery** (Phil Whittaker's proposed alternative — https://dev.to/phil-whittaker/progressive-discovery-a-better-mental-model-for-agent-skills-51bd)

### Skill and SKILL.md
**Consensus:** A filesystem directory containing a `SKILL.md` with YAML frontmatter (`name`, `description`) plus Markdown instructions, optionally with bundled scripts and reference files. Announced 16 Oct 2025; released as an open standard 18 Dec 2025 (agentskills.io). Adopted by VS Code/GitHub, Cursor, Goose, Amp, OpenCode, OpenAI Codex Command Line Interface (CLI).

- "A skill is a directory containing a SKILL.md file that contains organized folders of instructions, scripts, and resources that give agents additional capabilities." — Anthropic Engineering [vendor] — https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills
- "At its simplest, a skill is a directory that contains a SKILL.md file. This file must start with YAML frontmatter that contains some required metadata: name and description." — same URL
- Example frontmatter from Claude docs: `--- name: pdf-processing / description: Extract text and tables from PDF files, fill forms, merge documents. Use when working with PDF files or when the user mentions PDFs, forms, or document extraction. ---` — Anthropic, Agent Skills overview [vendor doc] — https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview
- "Claude loads this metadata at startup and includes it in the system prompt. This lightweight approach means you can install many Skills without context penalty; Claude only knows each Skill exists and when to use it." — same URL

**API limits (Oct 2025):** up to 8 skills per request; 8 megabyte (MB) bundle cap; SKILL.md at bundle root.

**Description line best practice (the "advertise" part):** `[What it does] + [When to use it] + [Key capabilities]`, including trigger phrases — Anthropic, "Complete Guide to Building Skills for Claude" (PDF) [vendor doc] — https://resources.anthropic.com/hubfs/The-Complete-Guide-to-Building-Skill-for-Claude.pdf.

### Claude Code: sub-agents / slash commands / skills / CLAUDE.md / plugins

| Primitive | Trigger | Location | Auto vs user |
|---|---|---|---|
| **CLAUDE.md** | Always loaded | project/user root | always-on memory |
| **Slash commands** | `/name` from user | `.claude/commands/*.md` | user-invoked (legacy) |
| **Skills** | Model decides | `.claude/skills/<name>/SKILL.md` | auto + `/name` |
| **Sub-agents** | Main Claude invokes via Task tool | `.claude/agents/*.md` | delegated, isolated context |
| **Plugins** | Installed via marketplace | any git repo | distribution bundle of above |

- "Subagents are defined in Markdown files with YAML frontmatter… Subagents receive only this system prompt (plus basic environment details like working directory), not the full Claude Code system prompt." — Claude Code docs [vendor doc] — https://code.claude.com/docs/en/sub-agents
- "The .claude/commands/ directory is the legacy format. The recommended format is .claude/skills/<name>/SKILL.md, which supports the same slash-command invocation (/name) plus autonomous invocation by Claude." — Claude API docs [vendor doc] — https://platform.claude.com/docs/en/agent-sdk/slash-commands

---

## 5. Knowledge vs memory

The field has **not converged**. Four competing framings coexist; pick one explicitly and use it consistently.

### Framing A — CoALA (Sumers, Yao, Narasimhan, Griffiths, 2023; cognitive-science grounded)
Memory is the **container**; knowledge is one *type* of content. Semantic memory = knowledge (world facts), episodic = experiences, procedural = skills/code, working = current decision cycle. Now treated as canonical by LangGraph and LangMem documentation.

- "A language agent has a short-term working memory and several (optional) long-term memories (episodic for experience, semantic for knowledge, procedural for code/LLM)." — Cognitive Architectures for Language Agents (CoALA) authors [OSS doc] — https://github.com/ysymyth/awesome-language-agents
- "Working memory maintains active and readily available information as symbolic variables for the current decision cycle… This includes perceptual inputs, active knowledge (generated by reasoning or retrieved from long-term memory), and other core information carried over from the previous decision cycle." — CoALA paper [paper] — https://arxiv.org/pdf/2309.02427

### Framing B — Lewis/RAG (parametric vs non-parametric memory)
Both model weights *and* external retrieved docs are called "memory." "Knowledge" is the content; "memory" is the substrate.

- "Large pre-trained language models have been shown to store factual knowledge in their parameters… Pre-trained models with a differentiable access mechanism to explicit non-parametric memory can overcome this issue." — Lewis et al. 2020 [paper] — https://arxiv.org/abs/2005.11401
- "Retrieval-Augmented Generation (RAG) models where the parametric memory is a pre-trained sequence-to-sequence (seq2seq) model and the non-parametric memory is a dense vector index of Wikipedia, accessed with a pre-trained neural retriever." — same URL

### Framing C — LangChain / practitioner split (most common in production)
**Knowledge** = static, organization-level, documents (source-of-truth outside interactions) → served by RAG. **Memory** = dynamic, user/session/agent-specific state accumulated through interactions → served by memory systems.

- "If the knowledge is available from another store (docs site, codebase, etc.), and if that store is the source of truth (rather than the interactions themselves), then your agent may work fine simply retrieving over that knowledge corpus directly… If the knowledge is regarding personalization (about the user) or conceptual relationships not found in the raw materials, then semantic memory is perfect for you." — LangChain LangMem [blog] — https://blog.langchain.com/langmem-sdk-launch/
- "Agent memory maintains evolving state, user preferences, past decisions, and learned procedures… RAG fetches external documents once; agent memory maintains evolving state." — 47Billion [blog] — https://47billion.com/blog/ai-agent-memory-types-implementation-best-practices/

### Framing D — MemGPT/Letta (operating-system metaphor)
Memory = anything managed across the context window boundary (in-context vs out-of-context). Knowledge vs memory distinction collapses — everything durable is "memory."

- "To enable using context beyond limited context windows, we propose virtual context management, a technique drawing inspiration from hierarchical memory systems in traditional operating systems." — Packer et al., MemGPT [paper] — https://arxiv.org/abs/2310.08560
- "Memory hierarchy — Distinguishing between in-context memory (core) and out-of-context memory (archival)." — Letta docs [OSS doc] — https://docs.letta.com/concepts/letta/

### When does something shift from knowledge to memory?
Three distinctions most practitioners use:

1. **Provenance:** knowledge comes from authored documents (external source-of-truth); memory comes from interactions/experience (write-through).
2. **Scope:** knowledge = world/org-wide; memory = user/session/agent-scoped (Mem0's `user_id`, `run_id`, `agent_id` scoping exemplifies this).
3. **Mutability:** knowledge is relatively static between reindexing; memory evolves continuously (Zep's temporal graph explicitly models this).

### Short-term vs long-term memory
LangGraph's documentation crisply distinguishes the two: short-term memory "tracks the ongoing conversation by maintaining message history within a session," while long-term memory "stores user-specific or application-level data across sessions and is shared across conversational threads." Short-term is typically persisted to a checkpointer keyed by `thread_id`; long-term to a separate `Store` scoped by user or namespace.

### Long-term memory subtypes (the CoALA-derived production triad)
- **Semantic memory** — facts and preferences ("user is vegetarian", "fiscal year ends in March"); typically a document collection or structured profile retrieved by semantic search.
- **Episodic memory** — specific past experiences; full transcripts or summarized episodes; frequently materialized as few-shot examples for in-context learning. LangChain: "facts can be written to semantic memory, whereas experiences can be written to episodic memory."
- **Procedural memory** — how the agent should behave: rules, skills, system prompts, tool-use patterns. "A combination of model weights, agent code, and agent's prompt that collectively determine the agent's functionality." Updated at runtime by reflection rewriting the agent's own system prompt, since weights and code rarely change live.

### Vendor-by-vendor side-by-side

| Source | "Knowledge" | "Memory" | Key primitive |
|---|---|---|---|
| Lewis 2020 (RAG) | world facts (content) | parametric weights + non-parametric index (substrate) | Dense Passage Retrieval (DPR) + Bidirectional and Auto-Regressive Transformer (BART) |
| CoALA 2023 | = semantic memory contents | working + episodic + semantic + procedural | memory modules with read/write |
| LangChain/LangMem | external source-of-truth docs (RAG) | personalization, interaction-derived | semantic / episodic / procedural memory APIs |
| LlamaIndex | documents → VectorStoreIndex | ChatMemoryBuffer / Memory class | short-term First-In-First-Out (FIFO) + long-term MemoryBlocks |
| MemGPT/Letta | not a separate concept | everything: in-context + archival | self-editing file memory |
| Mem0 | implicit (facts layer) | episodic / semantic / procedural + short/long-term | user/session/agent scoping |
| Zep/Graphiti | static RAG docs (dismissed as insufficient) | temporal knowledge graph | bi-temporal edges with validity windows |
| Anthropic Memory tool | not addressed | file directory `/memories` | 6 file ops (view/create/str_replace/insert/delete/rename) |
| Anthropic CLAUDE.md | user-written persistent instructions | auto-memory (Claude-written notes) | Markdown files loaded at session start |

### Anthropic's two distinct memory products
**(a) Memory tool (API, beta, Sep 29 2025):**
- "The memory tool enables Claude to store and retrieve information across conversations through a memory file directory. Claude can create, read, update, and delete files that persist between sessions." — Claude API docs [vendor doc] — https://docs.claude.com/en/docs/agents-and-tools/tool-use/memory-tool
- "This is the key primitive for just-in-time context retrieval: rather than loading all relevant information upfront, agents store what they learn in memory and pull it back on demand." — same URL
- "It doesn't use embeddings, vector databases, or knowledge graphs — just files." — Shlok Khemani analysis [blog] — https://www.shloked.com/writing/claude-memory-tool

**(b) CLAUDE.md / MEMORY.md (Claude Code):**
- "Each Claude Code session begins with a fresh context window. Two mechanisms carry knowledge across sessions: CLAUDE.md files: instructions you write to give Claude persistent context. Auto memory: notes Claude writes itself based on your corrections and preferences." — Claude Code docs [vendor doc] — https://docs.anthropic.com/en/docs/claude-code/memory
- "The first 200 lines of MEMORY.md, or the first 25 kilobytes (KB), whichever comes first, are loaded at the start of every conversation." — same URL

**CLAUDE.md is not RAG** — it injects curated files into context wholesale rather than retrieving chunks. Sources differ on whether to call it "memory."

### Memory type taxonomies in the wild
- **Lilian Weng (2023)**: short-term = in-context learning; long-term = external vector store with retrieval. "Short-term memory: I would consider all the in-context learning… as utilizing short-term memory." [essay] — https://lilianweng.github.io/posts/2023-06-23-agent/
- **Mem0**: "Episodic memory — summaries of past interactions or completed tasks. Semantic memory — relationships between concepts so agents can reason about them later." — Mem0 docs [vendor doc] — https://docs.mem0.ai/core-concepts/memory-types. Procedural via `memory_type="procedural_memory"`.
- **Zep/Graphiti**: temporal knowledge graph memory. "Facts have validity windows. When information changes, old facts are invalidated — not deleted." — Graphiti README [OSS doc] — https://github.com/getzep/graphiti. Bi-temporal model with event time + ingestion time.
- **LlamaIndex**: "A Memory object has both short-term memory (i.e. a FIFO queue of messages) and optionally long-term memory (i.e. extracting information over time)." — LlamaIndex docs [vendor doc] — https://docs.llamaindex.ai/en/stable/module_guides/deploying/agents/memory/. Memory blocks: `StaticMemoryBlock`, `FactExtractionMemoryBlock`, `VectorMemoryBlock`.

**Episodic memory is used inconsistently:** CoALA = agent's own past experience; Mem0/LangMem = summaries of user interactions; Graphiti = raw event records that feed the graph. All trace to Tulving 1972 but operationalize differently. Flag which definition you are using.

---

## 6. RAG and retrieval

### RAG
- "Retrieval-Augmented Generation (RAG)… models which combine pre-trained parametric and non-parametric memory for language generation." — Lewis et al. 2020 [paper] — https://arxiv.org/abs/2005.11401
- **Gao et al. taxonomy (widely cited but a survey framing, not a standard):** "Our survey delineates the evolution of RAG within the LLM landscape, identifying three distinct yet interrelated paradigms: **Naive, Advanced, and Modular RAG**." — Gao et al. [paper] — https://arxiv.org/html/2312.10997v2
  - Naive RAG = index → retrieve → generate, chain-like.
  - Advanced RAG = pre- and post-retrieval optimizations, still chain-like.
  - Modular RAG = "greater flexibility… not limited to sequential retrieval and generation; it includes methods such as iterative and adaptive retrieval." — https://arxiv.org/pdf/2312.10997
- **Agentic RAG:** retrieval wrapped as a tool the agent chooses to call. "Instead of doing the chunk retrieval before using an LLM to answer, you make the chunk retrieval function a tool that the LLM can access." — Towards Data Science (TDS) [blog] — https://towardsdatascience.com/how-to-build-agentic-rag-with-hybrid-search/

### Vector store, embeddings, semantic search, hybrid search, reranker, chunking
- **Embeddings** = dense vector encodings of chunks; **vector store** = indexed database of embeddings (Pinecone, Weaviate, pgvector, Qdrant, Milvus); **index** = general term for any searchable structure (vector, Best Match 25 (BM25), graph).
- **Chunking** = splitting documents into retrievable units (fixed-size, semantic, hierarchical, late chunking).
- **Semantic search** = dense-vector cosine similarity (Approximate Nearest Neighbor (ANN)).
- **Hybrid search** = BM25 + vector, typically via Reciprocal Rank Fusion (RRF): "Hybrid search combining BM25 keyword matching with vector similarity via Reciprocal Rank Fusion (RRF)." — Azure guide [vendor blog] — https://dev.to/suhas_mallesh/azure-ai-search-advanced-rag-with-terraform-hybrid-search-semantic-ranking-and-agentic-retrieval-180k
- **Reranker** = cross-encoder re-scoring top-K candidates. "The reranker returns the final 'best' 5–10 chunks, which your application then feeds to the LLM." — Ubuntu [blog] — https://ubuntu.com/blog/hybrid-search-and-reranking-a-deeper-look-at-rag

### GraphRAG
- "GraphRAG is a structured, hierarchical approach to Retrieval Augmented Generation (RAG), as opposed to naive semantic-search approaches using plain text snippets. The GraphRAG process involves extracting a knowledge graph out of raw text, building a community hierarchy, generating summaries for these communities, and then leveraging these structures when performing RAG-based tasks." — Microsoft GraphRAG docs [vendor doc] — https://microsoft.github.io/graphrag/

### Grounding / citations / attribution / provenance (terminology clash)
- **Information grounding** (RAG sense): output anchored in retrieved source. "Retrieval augmentation thus serves as a 'live memory' for the LLM: it supplies factual grounding from an external knowledge base." — RAG systematic review [paper] — https://arxiv.org/html/2507.18910v1
- **CoALA grounding**: the agent's *external actions* (tool calls / environment interaction), contrasted with internal memory ops. "External actions to interact with external environments (grounding); Internal actions to interact with internal memories (reasoning, retrieval, learning)." — CoALA [paper] — https://github.com/ysymyth/awesome-language-agents
- **Citations** = user-visible source links; **attribution** = mapping output spans to sources; **provenance** = metadata tracing a fact to its origin.
- **Ground truth** = verified correct answers used for evaluation; distinct from "grounding" as a generation technique.

**Dispute:** "Grounding" has two incompatible meanings; always check context.

---

## 7. Evaluation

### Eval vs benchmark
Informal practitioner consensus: **eval = project-specific**, often built from production traces; **benchmark = public, standardized**.

- "Evals provide a framework for evaluating large language models (LLMs) or systems built using LLMs… creating high quality evals is one of the most impactful things you can do." — OpenAI Evals README [OSS doc] — https://github.com/openai/evals
- "If you are completely new to product-specific LLM evals (not foundation model benchmarks), see these posts…" — Hamel Husain, Evals Frequently Asked Questions (FAQ) [blog] — https://hamel.dev/blog/posts/evals-faq/
- "In Eval-Driven Development (EDD), evals guide our development. We start by evaluating a baseline… From then on, every prompt tweak, every system update, every iteration, is evaluated." — Eugene Yan [blog] — https://eugeneyan.com/writing/eval-process/
- "Evaluating large language model (LLM) based chat assistants is challenging due to… the inadequacy of existing benchmarks in measuring human preferences." — Zheng et al., MT-Bench [paper] — https://arxiv.org/abs/2306.05685

The distinction is not uniformly encoded — OpenAI Evals calls its public registry "evals," conflating the two.

**litany's stance (eval = evaluation run).** litany pins **eval** / **evaluation** to one strict operational sense — defined in full at `docs/ARCHITECTURE.md` §9.1 / ARCH §9.3 and only *pinned* here: an **evaluation run** is *(experiment × suite × N)* — one workflow-config variant (an *experiment*) run against the fixed task suite N≥5 times, each task **machine-checked** by a `check` script (exit 0 the sole pass signal, never the agent's own claim), aggregated into **pass@1** (mean-of-means, 95% Wilson interval) and **pass@5**. It measures **harness configuration**, holding tasks and models fixed — the project-specific arm of the eval-vs-benchmark split above, aimed at the scaffold. It is therefore **not**: (1) model-capability benchmarking (the MMLU-style categories below) — the models are the fixed control, not the subject; (2) per-response **LLM-as-judge** grading (next subsection) — a task's checker is a script, never a model scoring a model; (3) online / production monitoring (*Offline vs online eval*, below) — `docs/ARCHITECTURE.md` §8's metrics own that. It is also distinct from an **evaluation harness** (the §1 Harness variant, the scaffold itself), which an evaluation run *exercises*, and from a per-task **run** (ARCH §9.2's archival unit, one agent subtree) — an evaluation run performs N of those per task. The runner that performs one is the `agent-eval` binary (ARCH §9.3).

### Rubric, judge, LLM-as-judge
- **Rubric** = explicit scoring criteria, often with per-score descriptors. "The scoring rubric consists of i) a general description of the scoring criteria and ii) a description of each score that the model can assign." — Cameron Wolfe on Prometheus [blog] — https://cameronrwolfe.substack.com/p/finetuned-judge. Hamel argues binary pass/fail rubrics beat 5-point Likert scales.
- **Judge** = scorer (human, code, or model). Inspect AI calls it a Scorer; Braintrust calls it a Scorer; LangSmith and OpenAI Evals use grader/evaluator.

**LLM-as-judge** — origin and variants:
- "We explore using strong LLMs as judges to evaluate these models on more open-ended questions. We examine the usage and limitations of LLM-as-a-judge, including position, verbosity, and self-enhancement biases." — Zheng et al. (MT-Bench paper; canonical origin of the phrase) [paper] — https://arxiv.org/abs/2306.05685
- "Strong LLM judges like GPT-4 can match both controlled and crowdsourced human preferences well, achieving over 80% agreement, the same level of agreement between humans." — same URL
- "These LLM-evaluators assessed output via direct scoring, pairwise comparison, and reference-based evaluation." — Eugene Yan [blog] — https://eugeneyan.com/writing/llm-evaluators/

Four canonical variants: **pairwise comparison** (best for subjective; randomize order to reduce position bias), **single-answer / direct scoring** (scales to online monitoring), **rubric-based** (Prometheus-style with per-score descriptors), **reference-based** (output compared to gold).

**Known biases (from the MT-Bench paper itself):** position bias, verbosity bias, self-enhancement bias. Always measure judge agreement with humans via Cohen's Kappa or Krippendorff's Alpha before trusting it.

### Golden set / regression set / eval set
- **Golden set** (Subject Matter Expert (SME)-validated ground truth): "A golden dataset is one that contains trusted inputs and ideal outputs. These are typically hand-labeled by humans (often with domain expertise) and serve as a benchmark for model output quality." — Arize [vendor blog] — https://arize.com/resource/golden-dataset/
- **Regression set** (deterministic Continuous Integration (CI) gate for known failures): "Turn production traces into eval datasets with one click. Build regression tests from real failures and edge cases, not synthetic examples." — Braintrust [vendor blog] — https://www.braintrust.dev
- **Eval set** = umbrella term; any dataset used in an evaluation run. In DeepEval, `EvaluationDataset` = list of goldens.

Terms overlap heavily; the strongest distinction is: *golden* implies SME validation, *regression* implies CI gating semantics.

### Eval framework primitives — vendor-by-vendor

The eval space has fragmented into roughly five competing vocabularies. They name the same concepts differently and structure the run loop differently.

**OpenAI Evals (open source, registry-driven).**
- Primitives: `Eval` = task definition (data, completion function, evaluator). Public registry of YAML-defined evals.
- "Evals registry" — public collection of community-contributed benchmarks doubling as project-specific evals.
- Conflates "eval" (project-specific) and "benchmark" (public).

**Inspect AI (UK AISI, OSS, audit-grade).**
- Primitives: **Task** (a `Sample` dataset + `Solver` chain + `Scorer`).
- **Solver** = the agent or pipeline being evaluated; "Other solvers might do prompt engineering, multi-turn dialog, critique, or provide an agent scaffold." — https://inspect.aisi.org.uk/
- **Scorer** = the judge (heuristic, model-graded, or composite).
- **Sample** = one input/target pair.
- **Transcript** = full event log of the solver chain (steps, tool calls, model outputs).
- Strong sandboxing primitives for agentic evals; default for AI safety institute work.
- https://inspect.aisi.org.uk/

**Braintrust (commercial).**
- Primitives: **Experiment** = one run of `Eval(name, data, task, scores, ...)`.
- **Scorer** = a function returning 0-1 with metadata; ships with `LLMClassifier`, `Factuality`, `AnswerRelevancy`, etc., from `autoevals`.
- **Dataset** = versioned eval dataset; bidirectional sync with production via `BraintrustSpanProcessor`.
- **Span** = native trace primitive; supports OpenTelemetry (OTel) interop.
- Strong on regression workflows: "Build regression tests from real failures and edge cases, not synthetic examples."
- https://www.braintrust.dev

**LangSmith (commercial, LangChain).**
- Primitives: **Dataset** = list of `Example` (input/output/metadata triples).
- **Evaluator** = function `(run, example) → {key, score, comment}`. Variants: `LLMEvaluator`, `StringEvaluator`, custom.
- **Experiment** = a dataset × evaluator × system-under-test run.
- **Run / RunTree** = proprietary trace primitive with `run_type` enum (chain/llm/tool/retriever/embedding/prompt/parser); supports OTel ingest/export.
- Tight coupling with LangChain/LangGraph but accepts arbitrary tracing.

**DeepEval (open source, Confident AI).**
- Primitives: `EvaluationDataset` = list of `Golden` (input, expected output, retrieval context).
- `LLMTestCase` = per-call wrapper with input/actual/expected.
- Metrics catalog (~40 built-ins): `GEval`, `AnswerRelevancyMetric`, `FaithfulnessMetric`, `HallucinationMetric`, `BiasMetric`, `ToxicityMetric`, RAG-specific metrics (`ContextualPrecisionMetric`, `ContextualRecallMetric`), agent metrics (`TaskCompletionMetric`, `ToolCorrectnessMetric`).
- Pytest integration is first-class (`assert_test(test_case, [metric])`).

**Other notable players:**
- **Promptfoo** (OSS, YAML-config): test cases + assertions + providers; strong red-teaming module.
- **Langfuse** (OSS, OTel-native): observation trees with `generation`/`span`/`event` subtypes; built-in score ingestion.
- **Arize Phoenix** (OSS): OTel-native; ships with LLM-as-judge templates.
- **Helicone**, **Traceloop / OpenLLMetry**: tracing-first; eval is secondary.
- **RAGAS** (OSS, RAG-focused): `faithfulness`, `answer_relevancy`, `context_precision`, `context_recall` as canonical RAG metrics.

### Trace, span, observability
- **Trace** = complete record of an end-to-end request. "A trace is the complete record of all actions, messages, tool calls, and data retrievals from a single initial user query through to the final response." — Hamel Husain [blog] — https://hamel.dev/blog/posts/evals-faq/
- **Span** = one timed unit of work within a trace. "A span represents a single unit of work or operation within a trace. Spans have a start and end time, a name, and can have attributes (key-value pairs of metadata). Spans can be nested to create a hierarchy." — Langfuse docs [vendor doc] — https://langfuse.com/docs/observability/sdk/overview
- **OpenTelemetry GenAI semconv:** "This span represents a client call to Generative AI model or service… Span name SHOULD be {gen_ai.operation.name} {gen_ai.request.model}." — OTel [spec] — https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-spans/

**Vendor alignment with OTel:**
- **Langfuse**: OTel-native; adds "observation" as a superset (generic span, specialized `generation`, event).
- **LangSmith**: proprietary `RunTree` with a `run_type` enum (chain/llm/tool/retriever/embedding/prompt/parser); supports OTel ingest/export.
- **Braintrust**: native `Span` + `BraintrustSpanProcessor` for OTel interop.
- **Arize / Datadog / Traceloop (OpenLLMetry)**: implement OTel GenAI semconv directly.
- "Tools like Langfuse, Helicone, Traceloop, and LangSmith each use incompatible proprietary tracing formats… OpenTelemetry GenAI Semantic Conventions were created specifically to solve this fragmentation." — dev.to [blog] — https://dev.to/x4nent/opentelemetry-genai-semantic-conventions-the-standard-for-llm-observability-1o2a

OTel GenAI semconv attributes are still **experimental** as of 2026 and have undergone renames (`gen_ai.prompt` → `gen_ai.input.messages`).

### Offline vs online eval, pass@k, annotation
- **Offline eval** = pre-deployment, fixed dataset, CI-oriented. **Online eval** = live production traces, async monitoring. "Offline agent evals function like unit tests or integration tests, emphasizing reproducibility." — Braintrust [vendor] — https://www.braintrust.dev/articles/how-to-eval
- **pass@k** (Chen et al., HumanEval 2021): "To evaluate pass@k, we generate n ≥ k samples per task… count the number of correct samples c ≤ n which pass unit tests, and calculate the [unbiased estimator]." — HumanEval paper [paper] — https://arxiv.org/pdf/2107.03374. Naive pass@k has high variance; use the unbiased estimator.
- **Annotation** = humans assigning ground truth. "To evaluate and monitor AI products, we typically sample outputs and annotate them for quality and defects. With enough high-quality annotations, we can calibrate automated evaluators to align with human judgment." — Eugene Yan [blog] — https://eugeneyan.com/writing/eval-process/

### Common benchmark categories (vocabulary only — not exhaustive)
- **Knowledge / reasoning:** Massive Multitask Language Understanding (MMLU), MMLU-Pro, BIG-Bench, BIG-Bench Hard (BBH), Graduate-Level Google-Proof Q&A (GPQA), Massive Multidiscipline Multimodal Understanding (MMMU).
- **Math:** Mathematics Aptitude Test of Heuristics (MATH), GSM8K, American Invitational Mathematics Examination (AIME).
- **Code:** HumanEval, Mostly Basic Python Problems (MBPP), Software Engineering Bench (SWE-Bench), LiveCodeBench.
- **Agent:** AgentBench, GAIA, WebArena, OSWorld, τ-bench, METR Time Horizons.
- **Long context:** Needle in a Haystack (NIAH), Long-Range Arena, RULER.
- **Chat / human preference:** Chatbot Arena, MT-Bench, AlpacaEval, Arena-Hard.
- **Safety:** HarmBench, AdvBench, JailbreakBench.
- **Hallucination / faithfulness:** TruthfulQA, FActScore, HaluEval.

---

## 8. Serving and inference infrastructure

### Inference, serving, batching
- "vLLM is a high-throughput and memory-efficient inference and serving engine for Large Language Models (LLMs)." — vLLM [OSS doc] — https://vllm.ai/
- **Continuous batching** (aka in-flight batching, iteration-level scheduling): "vLLM implements Continuous Batching… Instead of waiting for a batch to finish, the vLLM scheduler operates at the token level." — DEV [blog] — https://dev.to/maximus_prime_1/deep-dive-into-vllm
- Eliminates head-of-line blocking. Cade Daniel et al. reported 23× throughput gain at p50.

### Token and tokenizer
- "Tokens are the building blocks of text that OpenAI models process. They can be as short as a single character or as long as a full word…" — OpenAI Help Center [vendor doc] — https://help.openai.com/en/articles/4936856
- "tiktoken is a fast Byte-Pair Encoding (BPE) tokeniser for use with OpenAI's models." — OpenAI [OSS doc] — https://github.com/openai/tiktoken
- GPT-4o uses `o200k_base`; GPT-4 and 3.5 use `cl100k_base`. Roughly 4 characters per token for English. Different providers tokenize identical text differently — always measure against the actual provider in use. Anthropic flags that Claude Opus 4.7 "uses a new tokenizer" that "may use up to 35% more tokens for the same fixed text" versus prior models.

### Streaming
Server-Sent Events (SSE) or chunked HTTP delivery of tokens as generated. Improves perceived latency but does not change total time. All major APIs support it.

### Latency metrics — TTFT, ITL, TPOT
- **Time to First Token (TTFT)**: "Time to first token (TTFT) is the time it takes to process the prompt and generate the first token… TTFT generally includes both request queuing time, prefill time, and network latency. The longer the prompt, the larger the TTFT." — NVIDIA [vendor blog] — https://developer.nvidia.com/blog/llm-benchmarking-fundamental-concepts/
- **Inter-Token Latency (ITL) vs Time Per Output Token (TPOT) — are they the same?** Per request, yes. Across requests, subtly different.
  - "Intertoken latency (ITL) is the average time between the generation of consecutive tokens in a sequence. It is also known as time per output token (TPOT)." — NVIDIA [vendor blog] — same URL
  - "For a single request, the mean of all ITLs equals TPOT, which is why the two are sometimes used interchangeably." — BentoML LLM Inference Handbook [vendor doc] — https://bentoml.com/llm/inference-optimization/llm-inference-metrics
  - "The distinction between TPOT and ITL varies across the literature, and some sources treat them as equivalent." — Anyscale [vendor doc] — https://docs.anyscale.com/llm/serving/benchmarking/metrics
  - Formula (NVIDIA GenAI-Perf convention): `TPOT = (E2E_latency − TTFT) / (output_tokens − 1)`
  - vLLM's `benchmark_serving.py` reports both separately — they can diverge when ITL includes inter-request gaps.
- **Dispute:** LLMPerf includes TTFT in its average; NVIDIA GenAI-Perf does not. Always state the convention being used.

### Throughput, KV cache, PagedAttention
- **Throughput** in tokens/sec (total or per-user) or requests/sec. **Goodput** = throughput of requests meeting Service-Level Objectives (SLOs).
- **KV cache** = cached Key/Value attention tensors for processed tokens. "The attention mechanism requires the whole input sequence to compute and create the so-called key-value (KV) cache, from which point the iterative generation loop can begin." — NVIDIA [vendor blog] — same URL
- **PagedAttention** (vLLM): "With PagedAttention, vLLM eliminates external fragmentation… and minimizes internal fragmentation… While previous systems waste 60%–80% of the KV cache memory, vLLM achieves near-optimal memory usage with a mere waste of under 4%." — Runpod [blog] — https://www.runpod.io/blog/introduction-to-vllm-and-pagedattention. Original paper: Kwon et al., Symposium on Operating Systems Principles (SOSP) 2023.

### Prompt caching — three vendor philosophies
| Vendor | Default | Control | Min tokens | Pricing |
|---|---|---|---|---|
| **Anthropic** | Explicit `cache_control` (+ optional auto) | Up to 4 breakpoints; 5-min default or 1h Time-To-Live (TTL) (2×) | 1,024 (Sonnet) / 4,096 (Opus, Haiku 4.5) | 1.25× write, 0.1× read |
| **OpenAI** | Fully automatic | `prompt_cache_key` hint; `prompt_cache_retention="24h"` | 1,024; 128-token increments | No write cost; up to 90% off |
| **Gemini** | Implicit (2.5+) + explicit `CachedContent` resource | TTL configurable; 60-min default | 1,028 (Flash) / 2,048 (Pro) | 75–90% off + storage cost |

- "Prompt caching references the entire prompt — tools, system, and messages (in that order) up to and including the block designated with cache_control." — Anthropic [vendor doc] — https://platform.claude.com/docs/en/build-with-claude/prompt-caching
- "Prompt Caching works automatically on all your API requests (no code changes required) and has no additional fees associated with it." — OpenAI [vendor doc] — https://developers.openai.com/api/docs/guides/prompt-caching
- "Implicit caching is enabled by default for all Gemini 2.5 and newer models. We automatically pass on cost savings if your request hits caches." — Google [vendor doc] — https://ai.google.dev/gemini-api/docs/caching

### Speculative decoding
Small draft model proposes k tokens; target model verifies in one forward pass; rejection sampling preserves distribution. Reduces per-request latency when spare compute exists; hurts max throughput at high batch sizes.

- "Our speculative decoding implementation only supports rejection sampling, which guarantees that the output tokens returned will be exactly the same as what the unmodified target model would generate." — Baseten [blog] — https://www.baseten.co/blog/a-quick-introduction-to-speculative-decoding/

### Rate limit, quota
Cap on requests/tokens per unit time (Requests Per Minute (RPM), Tokens Per Minute (TPM)). Cached reads do not reduce rate-limit consumption on most vendors. Anthropic prompt caching note: "If requests for the same prefix and prompt_cache_key combination exceed a certain rate (approximately 15 requests per minute), some may overflow…" — OpenAI [vendor doc] — https://developers.openai.com/api/docs/guides/prompt-caching

### Batch APIs
Both OpenAI and Anthropic offer asynchronous bulk endpoints trading latency for ~50% discount on input and output tokens. Useful for offline eval runs and large embedding ingestion.

---

## 9. Cost and billing vocabulary

Token-billing is the single biggest source of operational surprises in LLM systems. Vendors share a basic vocabulary but differ on multipliers, granularity, and which categories exist.

### Token categories (the billing axes)

| Category | Definition | Billed at |
|---|---|---|
| **Input tokens** | Tokens in the request (system + messages + tools + images-as-tokens) | Standard input rate |
| **Output tokens** | Tokens emitted by the model in the response | Standard output rate (usually 3–5× input) |
| **Cached input tokens** (cache read) | Input tokens served from a prompt cache hit | ~10% of input (Anthropic), up to 90% off (OpenAI), 75–90% off (Gemini) |
| **Cache write tokens** | Tokens written into the cache on first use | 1.25× input (Anthropic 5-min); 2× input (Anthropic 1-hour); free (OpenAI); separate storage cost (Gemini) |
| **Reasoning tokens** / **thinking tokens** | Internal deliberation tokens not returned to caller | Billed as **output tokens** on all vendors; counted against `max_output_tokens` |
| **Image / audio / video input tokens** | Multimodal inputs converted to token equivalents | Vendor-specific schedule (e.g., OpenAI tiles, Anthropic dimension-based) |
| **Tool definition tokens** | The JSON schemas for declared tools, counted as input | Standard input; cacheable |
| **Tool result tokens** | Function output passed back to the model | Standard input on the next call |

**Reasoning tokens are the most commonly missed billing surprise.** OpenAI: "hidden tokens that aren't returned as part of the message response content but are used by the model to help generate a final answer." They count against the output token bill *and* against `max_completion_tokens` / `max_output_tokens`. Anthropic's `thinking` block tokens are visible in the response but billed identically as output.

### Vendor-specific billing surfaces

**Anthropic** — `usage` object on every Message response:
```
input_tokens, output_tokens,
cache_creation_input_tokens, cache_read_input_tokens,
server_tool_use (web_search_requests, etc.)
```

**OpenAI Chat Completions / Responses** — `usage` object:
```
prompt_tokens, completion_tokens, total_tokens,
prompt_tokens_details: { cached_tokens, audio_tokens },
completion_tokens_details: { reasoning_tokens, audio_tokens, accepted_prediction_tokens, rejected_prediction_tokens }
```

**Google Gemini** — `usageMetadata`:
```
promptTokenCount, candidatesTokenCount, totalTokenCount,
cachedContentTokenCount, thoughtsTokenCount
```

### Pricing modifiers
- **Cache write premium** (Anthropic): paying extra to put tokens into cache; only worth it if hits exceed the breakeven (typically ≥ 2 reads for 5-min, more for 1-hour).
- **Cache read discount**: 0.1× input (Anthropic), variable (OpenAI implicit), 0.25× to 0.1× input (Gemini).
- **Batch discount**: ~50% on input + output, asynchronous SLA (24h typical max).
- **Context tier pricing** (Gemini 2.5 Pro, some others): per-token rate increases beyond a context-length threshold (e.g., 200K tokens).
- **Priority / Provisioned Throughput**: AWS Bedrock and Google Vertex offer reserved-capacity tiers at flat hourly rates instead of per-token.
- **Server tool surcharges** (Anthropic): web_search billed per request beyond input/output token counts; computer use, code execution similar.

### Cost math worth committing to memory
For a typical agent loop with N steps, K tools, T tokens of cumulative growing context:
- **Without prompt caching:** total input billed ≈ Σ context-at-step-i ≈ O(N·T) — quadratic in conversation depth.
- **With prompt caching (well-placed breakpoints):** dominated by *delta* tokens per step plus one full read at warm-start; closer to O(N + T).
- **Reasoning models:** add reasoning_token budget per step; default `reasoning.effort=medium` typically adds 1K–10K hidden output tokens per model call.

### Rate limits as a cost dimension
Rate limits are organized by **tier** (OpenAI, Anthropic both use this term) which scales with monthly spend or paid balance. Cached input does not reduce rate-limit consumption on most vendors. Rate limits are enforced separately per model and often per region/endpoint.

---


## 10. Safety and alignment terms relevant to tooling

### Guardrail — a word meaning three different things
1. **Generic pattern** — any programmatic input/output validation (topic filtering, Personally Identifiable Information (PII) masking, refusal enforcement, format check).
2. **NeMo Guardrails** (NVIDIA product): "an open-source toolkit for easily adding programmable guardrails to LLM-based conversational applications." Five rail types: input, dialog, retrieval, execution, output. Uses Colang Domain-Specific Language (DSL). [OSS doc] — https://github.com/NVIDIA-NeMo/Guardrails
3. **Guardrails AI** (different product): "a Python framework that helps build reliable AI applications by performing two key functions: Guardrails runs Input/Output Guards in your application that detect, quantify and mitigate the presence of specific types of risks." [OSS doc] — https://github.com/guardrails-ai/guardrails. Unit = `Validator` combined into a `Guard`.
4. OpenAI Agents SDK uses it generically: "Guardrails enable you to do checks and validations of user input and agent output." [vendor doc] — https://openai.github.io/openai-agents-python/guardrails/

"Rails" is used synonymously. When writing harness code, say which meaning is intended.

### Prompt injection vs jailbreak — a critical distinction (Willison)
**These are different attacks.** Simon Willison has argued for years that the field should keep them separate.

- **Jailbreak** targets the **model's safety training**: "Jailbreaking is the class of attacks that attempt to subvert safety filters built into the LLMs themselves." — Simon Willison [essay] — https://simonwillison.net/2024/Mar/5/prompt-injection-jailbreaking/. Typical risk: "screenshot attacks" — tricking a model into saying something embarrassing.
- **Prompt injection** targets **applications** that concatenate trusted and untrusted text: "Prompt injection is a class of attacks against applications built on top of Large Language Models (LLMs) that work by concatenating untrusted user input with a trusted prompt constructed by the application's developer." — same URL
- "Crucially: if there's no concatenation of trusted and untrusted strings, it's not prompt injection. That's why I called it prompt injection in the first place: it was analogous to Structured Query Language (SQL) injection…" — same URL
- "The risks from prompt injection are far more serious, because the attack is not against the models themselves, it's against applications that are built on those models." — same URL

**Coinage timeline:** Riley Goodside posted the first public example (Sep 11 2022); Willison named it "prompt injection" on Sep 12 2022; Kai Greshake named **indirect** prompt injection (Feb 2023) — untrusted text embedded in retrieved content (webpages, emails, docs).

**OWASP conflates them:** "While prompt injection and jailbreaking are related concepts in LLM security, they are often used interchangeably." — OWASP LLM01:2025 [spec] — https://genai.owasp.org/llmrisk/llm01-prompt-injection/. Willison acknowledges the semantic drift but maintains that the distinction matters for defense (architecture-level mitigation for injection vs model-level alignment for jailbreak).

### Policy, refusal, moderation, red teaming
- **Policy** = written rules governing allowed behavior (vendor usage policies, organization rules). OpenAI Moderation API flags content "that violates OpenAI's policies." [vendor doc] — https://developers.openai.com/api/docs/guides/moderation
- **Refusal** = model declines per safety training. Less formally defined; Anthropic emphasizes principled refusal via Constitutional AI.
- **Moderation** = separate classifier model applied pre/post filter. OpenAI's endpoint (free, `omni-moderation-latest`) is canonical. Categories: sexual, sexual/minors, harassment(/threatening), hate(/threatening), illicit(/violent), self-harm(/intent/instructions), violence(/graphic). Peers: Meta Prompt Guard, Llama Guard, NemoGuard Content Safety NIM.
- **Red teaming** = adversarial testing for vulnerabilities/harms. "Red teaming is a critical tool for improving the safety and security of AI systems. It involves adversarially testing a technological system to identify potential vulnerabilities." — Anthropic [vendor blog] — https://www.anthropic.com/news/challenges-in-red-teaming-ai-systems. Standard metric = Attack Success Rate (ASR).

**OWASP Top 10 for LLM Applications (2025):** Prompt Injection is LLM01 for the second edition running. New 2025 additions: System Prompt Leakage (LLM07), Vector/Embedding Weaknesses (LLM08), Misinformation (LLM09), Unbounded Consumption (LLM10). — https://owasp.org/www-project-top-10-for-large-language-model-applications/

---

## 11. Orchestration frameworks and their primitives

Frameworks sit above the provider API and organize multi-step, multi-agent, stateful computation. Their primitives are not interchangeable — learning one does not give you another for free.

### LangGraph (LangChain) — graph-based state machines
- **State** — typed `TypedDict` shared across nodes.
- **Nodes** — functions that read and update state.
- **Edges** — fixed or conditional transitions between nodes.
- **Checkpointer** — snapshots state at every super-step, organized into **threads** identified by `thread_id`.
- **Super-step** — one Pregel-style iteration; can contain multiple parallel nodes.
- **Store** — cross-thread long-term memory (separate from short-term thread checkpoints).
- **Send** — fan-out primitive for parallel subtasks to the same node.
- **Command** — return value letting a node both update state and dictate the next node.
- **`interrupt()`** — pauses execution mid-graph for human-in-the-loop approval.

"LangGraph has a built-in persistence layer that saves graph state as checkpoints. When you compile a graph with a checkpointer, a snapshot of the graph state is saved at every step of execution, organized into threads." — LangGraph docs.

### OpenAI Agents SDK — code-first
- **Agent** — configured LLM with instructions, model, tools, optional **handoffs**.
- **Handoff** — delegation to a specialist agent, exposed to the LLM as a tool named `transfer_to_<agent>`.
- **Runner** — executes the agent loop.
- **Sessions** — persistent working context across runs.
- **Guardrails** — parallel checks that can raise tripwires on input or output.
- **Tracing** — built in; spans LLM calls, tool calls, handoffs, and guardrails.

OpenAI positions handoffs vs "agents-as-tools" as the two main multi-agent patterns; the choice is "who should own the final answer."

### CrewAI — role-playing metaphor
- **Crew** — collection of agents.
- **Agent** — has `role`, `goal`, `backstory`, tools.
- **Task** — description, expected output, assigned agent.
- **Process** — `Process.sequential` or `Process.hierarchical` (latter introduces a manager agent requiring `manager_llm` or `manager_agent`).
- **Flows** — separate event-driven primitive for deterministic workflows; complementary to Crews.

### Microsoft AutoGen v0.4+
Separates `autogen-core` (event-driven actor runtime) from `autogen-agentchat` (conversational multi-agent teams).
- **Team** — coordinates agents through patterns: `RoundRobinGroupChat`, `SelectorGroupChat`, `Swarm` (handoff-based).
- **Agent** — actor with subscribed message types in core; conversational participant in agentchat.

### Anthropic Claude Agent SDK
Wraps Claude with a `query` API and MCP integration. Emphasizes the agent loop (compaction, context resets, durable file memory) over multi-agent orchestration.

### DSPy — programs as optimizable
The outlier: treats LLM programs as optimizable artifacts.
- **Signature** — typed input/output specification (`question -> answer: str`).
- **Module** — composable program unit: `Predict`, `ChainOfThought`, `ReAct`, custom.
- **Teleprompter / Optimizer** — automatically tunes prompts and few-shot examples against a metric (`BootstrapFewShot`, `MIPROv2`, `BootstrapFinetune`).

### Pydantic AI
Type-safe `Agent` objects with `result_type` validation enforced via Pydantic models. Strong on structured outputs; thin on multi-agent orchestration.

### Haystack (deepset)
Pipeline of typed `Component`s for document-centric applications. Strong on RAG indexing pipelines.

### Cross-framework generic terms
- **Handoff** — one-way transfer of conversational control from one agent to another.
- **Swarm** — decentralized multi-agent pattern where agents hand off peer-to-peer without a central orchestrator.
- **Human-in-the-loop (HITL)** — workflow can pause for human review; LangGraph's `interrupt()`, OpenAI's approval flow, CrewAI's task-level callbacks.
- **Workflow vs agent** (Anthropic's taxonomy): workflow = predetermined code path; agent = LLM-driven control flow. Both are "agentic systems."

---

## 12. Disputes and ambiguities — the summary card

Build your harness with explicit stances on these, because the field has not.

1. **Turn** is the single most overloaded term. Always specify conversational-turn (messages-array) vs agent-turn (loop iteration).
2. **Harness ≈ scaffold** in practice. Use "scaffold" for METR/AISI audiences, "harness" for developer-infra audiences. SWE-agent's "ACI" is a third term for the same layer.
3. **Function call ≈ tool call** today; "tool call" is newer and broader (includes server tools, built-ins).
4. **Completion** is becoming legacy; newer APIs prefer "message" (Anthropic) or "response" (OpenAI Responses API).
5. **Provider** means training lab *or* inference host *or* gateway — OpenRouter/LiteLLM/Vercel use it mostly for inference hosts. Be explicit.
6. **System prompt vs developer message** is mostly an OpenAI rename (Dec 2024) with a formalized 5-level chain of command (platform > developer > user > assistant > tool). Anthropic has no "platform" layer at the API surface. Secondary sources that call them "functionally the same" are wrong on override semantics.
7. **Reasoning model** is marketing, not architecture. OpenAI/DeepSeek = distinct checkpoints with hidden/tagged thinking; Anthropic = a *mode* of a single model; Google Gemini = a flag on the same model. Willison flags the term as under-defined.
8. **Thread** (OpenAI Assistants) is being replaced by **conversation** (OpenAI Responses) by Aug 2026. **Session** means UI-lifetime, connection-lifetime, or agent-platform log — least standardized. LangGraph's `thread_id` is unrelated to OpenAI threads.
9. **Context engineering** was not coined by any single person. Lütke (18 Jun 2025) → Karpathy → Willison → Anthropic (Sep 2025 formal framing).
10. **Progressive disclosure** is not Anthropic's term — it is Jakob Nielsen's from UX (1980s–90s). Anthropic re-applied it (Oct 2025) to Skills. The pattern "advertise tool/skill by description before loading its body" has no single universal name — the canonical labels are *progressive disclosure* (Skills), *deferred/lazy loading* (Tool Search), and *tool discovery*.
11. **Knowledge vs memory** has four competing framings (CoALA / Lewis-RAG / LangChain-practitioner / MemGPT-OS). Most production systems use the practitioner split: knowledge = org-wide, document-sourced, static → RAG; memory = user-scoped, interaction-sourced, evolving → memory systems.
12. **Episodic memory** is used inconsistently: CoALA = agent's own past experience; Mem0/LangMem = user-interaction summaries; Graphiti = raw event records. Declare your definition.
13. **Grounding** has two incompatible meanings: RAG factual anchoring vs CoALA external action. Always disambiguate.
14. **ITL = TPOT** per request; differ across multiple requests. Literature inconsistent. Baseten/NVIDIA treat them as interchangeable; vLLM's benchmark tool reports both. Specify the convention being used.
15. **Prompt injection ≠ jailbreak** (Willison). They target different layers (application boundary vs model alignment) and require different defenses. OWASP conflates them; the field is drifting toward conflation, but threat models should keep them separate.
16. **Guardrail** is both a generic term and two product names (NeMo Guardrails, Guardrails AI). Say which is meant.
17. **Eval vs benchmark:** practitioner consensus (Hamel, Eugene) = project-specific vs public. Not uniformly encoded — OpenAI Evals calls its public registry "evals."
18. **Trace/span vendor alignment with OpenTelemetry GenAI semconv** is partial and evolving. Langfuse is OTel-native; LangSmith uses a proprietary RunTree with OTel export; Braintrust has native spans + OTel processor. Semconv attributes are still marked experimental in 2026.
19. **Golden set / regression set / eval set** overlap. Clearest: golden = SME-validated ground truth; regression = CI gate for known failures; eval set = umbrella.
20. **CLAUDE.md is not RAG** — it injects curated files into context wholesale rather than retrieving chunks. Sources differ on whether to call it "memory."
21. **Reasoning tokens** (OpenAI) and **thinking tokens** (Anthropic, Google) are the same billing category under different names — both billed as output tokens, both counted against the output token cap.
22. **Agent** is the least-stable framework primitive: looped LLM-plus-tools (OpenAI/Anthropic), role-playing persona (CrewAI), node-in-a-graph (LangGraph), actor (AutoGen), typed Pydantic object (Pydantic AI).

Build explicit conventions on each into your harness documentation. The terminology is still being negotiated; choices made now will compound.
