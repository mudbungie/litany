# Design: cryptographic agent attestation (bl-c3c5)

**Ruling: a small prover-side capability in litany; every verifier-side
component refused from litany.** An agent gets a keypair minted at its fork,
announced in its assembled context, and usable only through an
executor-mediated sign tool — so the only public-key assertions an agent can
ever emit are its own. Everything that *checks* those assertions (log
extraction, key indexing, verification) belongs to a different trust domain
by construction and is never litany code. This is a design record: the
feature is deliberately unscheduled (§9), and the document exists so the
trust analysis is not re-derived when a deployment wants it.

**Section-reference convention.** A bare `§N` names a section of *this*
document; cross-document references name their document — `ARCHITECTURE §N`
for `docs/ARCHITECTURE.md`, `TAXONOMY §N` for `docs/TAXONOMY.md`.

## 0. Motivation and trust analysis

Three problems, in increasing ambition:

1. **Impersonation.** Nothing cryptographic today stops one agent from
   claiming another agent said something. (Structurally the executor already
   prevents it — messages are deposits the executor commits, ARCHITECTURE
   §2.11 — but the guarantee is invisible to anything outside the workspace.)
2. **Enforcement.** "Another agent must sign off on this" (adversarial
   review) has no mechanically checkable form; it is a convention held by
   prompts.
3. **Provenance.** In a compliance environment, an artifact (a commit, a
   closed ball, a release) should be attributable to the agent that produced
   it, and from there to the retained record of *how* it was produced.

The analysis that shapes every ruling below — three limits, each honest:

- **Custody is machine-scoped.** Every agent runs under the same UID on the
  same host as the executor. A private key on this machine is reachable, in
  the limit, by anything on this machine. So a signature attests "this
  deployment's executor, driving this agent" — never more. The design narrows
  *accidental* cross-agent use to zero (§2) but claims nothing against a
  compromised host.
- **A signature attests the key, not the cognition.** A valid signature
  proves which agent's channel emitted the assertion, not that the agent's
  reasoning was sound or unmanipulated. Provenance of the *content* lives in
  the retained transcript; the signature is the binding between artifact and
  transcript, not the proof itself.
- **A single machine is its own sole witness.** Signatures verified only
  against state the same host controls prove nothing to an outsider: whoever
  could forge the signature could forge the record. The design's answer is
  the inference-log witness (§3): the binding rides a channel a third party
  already retains.

Each limit maps to a piece of the design: the executor mediation answers the
first as far as structure can (accident-proof, not adversary-proof), the
transcript answers the second, and the witness answers the third.

## 1. Coined terms

Defined here per the terminology rule (PRINCIPLES, *Terminology is
load-bearing*); used nowhere else until an implementation exists.

- **Agent key** — the Ed25519 keypair minted for an agent at its fork. The
  public half is data; the private half never leaves executor-read files
  outside every worktree (§2).
- **Workspace key** — one Ed25519 keypair per workspace, minted at workspace
  creation, which signs each agent-key binding. The issuer role, without
  certificate machinery: its one product is signatures over
  `(agent id, agent pubkey)`.
- **Announcement** — the derived system-slot sentence carrying an agent's id,
  its public key, and the workspace-key signature over the pair
  (ARCHITECTURE §2.8 pattern: one derived sentence, no instruction).
- **Assertion** — one signed statement: an agent key's signature over
  `(agent id, statement)`, where the statement is free text or an artifact
  hash. A frozen fact, like the epitaph (TAXONOMY §3): it records what was
  asserted at signing time and is never re-derived.
- **Sign tool** — the in-process built-in that produces assertions for the
  calling agent, and only for the calling agent (§2).
- **Inference log** — the record a provider, gateway, or compliance store
  retains of model-call requests and responses ("inference" in the
  TAXONOMY §1 sense; "gateway" in the TAXONOMY §2 sense). litany does not
  write it, read it, or depend on it existing; §3 is about what falls out
  when a deployment has one.
- **Witness** — the role the inference log plays for attestation: an
  append-only record of the announcement (and of every signing act) held by
  a party the workspace does not control.

## 2. Prover side: keys, announcement, sign tool — litany, and small

**Key lifecycle.** The executor mints the agent key at the fork, beside the
dispatch artifacts. Private keys live at the workspace root *outside every
worktree*, sibling to `steps/` (ARCHITECTURE §2.3) — which makes
"key material never enters context" structural rather than disciplinary:
context eligibility is being a committed worktree file (PRINCIPLES, *Context
has one home*), and these files are not in any worktree. Mode 0600. The
workspace key is minted once, at workspace creation, in the same territory.
Key deletion follows ref retention (ARCHITECTURE §9.2): when retention
deletes an agent's ref, its private key goes too — the public key survives
wherever it was witnessed, which is the only place it was ever needed.

**Announcement.** Each model call's system slot carries the announcement,
exactly as it carries the agent's name (ARCHITECTURE §2.8). It is stable for
the agent's whole life, so prompt-cache append-only assembly is undisturbed
(ARCHITECTURE §5.5). Carrying it on *every* model call rather than only the
first is deliberate: assembly is stateless and per-model-call (PRINCIPLES,
*Context assembly is deterministic*), a "first model call only" rule would be
state, and the witness (§3) is strengthened, not weakened, by repetition.

**Sign tool.** An in-process built-in, granted per role like any tool
(ARCHITECTURE §4.3, §3.3 grant gate). Input: a statement. Output: the
assertion — signature plus public key. The load-bearing property is what the
input schema *omits*: there is no key-selection parameter. The executor is
the single process driving this branch's step loop (ARCHITECTURE §2.11), it
knows whose step it is executing, and it resolves that agent's key —
period. An agent cannot request another agent's signature for the same
reason it cannot commit to another agent's branch (PRINCIPLES, *One writer
per branch*): no channel exists. This is the "locked into their own PKI
assertions" property, held by structure, not policy.

In-process is required, not merely chosen. The ARCHITECTURE §3.6 criterion —
"shipping a tool in-process is the decision to place it in the trusted
computing base" — cuts *toward* in-process here: the tool's entire authority
is use of the calling agent's own key, and externalizing it would hand key
material to a subprocess, which is exactly the custody widening the design
exists to prevent. (Same shape as the `apply_patch` ruling: externalizing
would bound nothing and cost something.)

**What gets signed.**

- **The result message, automatically.** As the executor deposits an agent's
  result (PRINCIPLES, *Return is not a verb*), it signs the deposit —
  agent id, epitaph, terminal ref sha, content hash — and pins the assertion
  as `signature:` frontmatter beside `epitaph:` (ARCHITECTURE §2.11: the
  path carries framing, frontmatter carries asserted facts). Signing the
  terminal ref sha attests the whole branch history through git's hash
  chain: sign the tip, get the ancestry.
- **Anything else, explicitly.** The sign tool takes an arbitrary statement,
  so a ball closure, a release tag sha, or a review verdict is signable by
  any role granted the tool. The design is artifact-agnostic on purpose:
  what is worth signing is deployment policy, not harness vocabulary.

**Custody, restated without flinching.** Within the machine, separation is
soft: an in-process `bash` with broad authority could read the key files.
The v1.1 sandbox (ARCHITECTURE §3.6) clamps external tools' fs scopes away
from the key territory by default, which narrows accident further; none of
it stops a hostile operator or a compromised host. The trust boundary is the
machine, and every claim in this document is scoped to that.

## 3. The witness: what a retained inference log makes true

litany's side of §2 is complete without this section. But where model calls
transit a gateway or provider that retains them — the normal corporate
posture — two things become true with zero additional litany mechanism:

1. **The binding is registered with a third party.** The announcement rides
   the system slot, so every retained model call carries
   `(agent id, pubkey, workspace-key signature)` — timestamped and stored by
   a party the workspace cannot retroactively edit. That is the append-only
   external witness the §0 analysis demands, obtained for free.
2. **Every signing act is witnessed, not just the key.** The sign tool's
   `tool_use` and `tool_result` are committed transcript entries
   (ARCHITECTURE §2.3), so they appear in the context of every subsequent
   model call on that branch — which means in the retained log. Forging an
   attribution after the fact requires forging the provider-side record.

There is a third fact the log carries that the design never had to build:
the gateway authenticated whoever made the model call. The log row is
effectively `(credential, time, agent id, pubkey)` — so *deployment-level*
attribution (which machine, whose account) comes from credential management
the organization already does, not from any certificate chain.

**Compaction and retention do not undermine the witness.** Rebase-forward
(ARCHITECTURE §2.6) rewrites a branch, and retention (ARCHITECTURE §9.2)
deletes refs — so a signed sha may eventually name a commit no local ref
reaches. That is fine, and it is why the assertion is defined as a frozen
fact (§1): it attests what was true at signing time, and the durable copy of
the evidence is the witness, not the workspace. Local git state is the
working copy of history; the log is the archival one. One fact, one
authoritative home each era (PRINCIPLES, *Single source of truth*).

## 4. Verifier side: emphatically not litany

The scheme works *because* the checker is not the machine being attested.
Folding any of the following into litany would reintroduce the sole-witness
problem §3 eliminates:

- **Extraction service** — parses retained inference logs for announcements,
  producing a key index: `pubkey → (agent id, workspace, credential, time)`.
  The index is a rebuildable projection of the logs, never authoritative —
  lose it and re-parse (PRINCIPLES, *Single source of truth*). Owned by
  whoever owns the logs; one per compliance domain, serving every litany
  deployment in it.
- **Verification** — barely a service: a stateless check taking
  `(artifact, assertion, index)` and answering which agent, which workspace,
  which credential, when — from which the retained transcript is one lookup
  away. A CLI or library, run by the party that wants convincing.

Division of labor, stated as the brazen division is (ARCHITECTURE §4.4):
litany is the prover and owns key custody and signing; the organization is
the verifier and owns logs, indexing, and judgment. They meet only at the
announcement format — a narrow interface neither side can drift alone,
because the witness holds every version ever emitted.

## 5. The corporate chain, optional and thin

Where an organization wants signatures to chain to its own root, the
workspace key is certified by an organizational intermediate. That is a
deployment act (operator-run, like `bz --login` — ARCHITECTURE §4.4's trust
posture), not a litany verb: litany signs with whatever workspace key exists
and never knows whether it is certified. The severability test passes:
deleting the corporate chain deletes a deployment file, not a line of code.

Trusting the intermediate is structurally the trust already extended to a
build server in SLSA-style provenance — and it comes with the right blast
radius: a compromised host means revoking one intermediate, which marks
exactly that machine's assertions suspect and nobody else's. The certificate
hierarchy does not add trust; it makes the boundary that already existed
(the machine) explicit, auditable, and revocable.

## 6. Enforcement worked example: adversarial review

The gate "a merge requires an assertion over the reviewed sha from an agent
key *different from* the author's, both announced under the same workspace
key" is mechanically checkable — a predicate over data, sittable in a tool
control (TAXONOMY §4) or any deployment's merge policy, with no new litany
surface.

Be precise about what it proves. It cannot prove the review was rigorous
(the dispatcher can always fork a lazy reviewer), and it cannot prevent
sock-puppetry at the dispatch level. What it proves is that a **distinct
agent** — a separate branch, forked context, own step loop (ARCHITECTURE
§2.1, §2.2) — produced the approval: no shared conversational state with the
author, no inherited blind spots. A distinct context examining the work is
the active ingredient of adversarial review, so the check enforces exactly
the part that is enforceable and leaves review *quality* where it lives —
in the reviewer's soul and gates.

## 7. Refusals

- **No certificate machinery in core.** The workspace key signing
  `(id, pubkey)` pairs is the entire issuer. Certification of the workspace
  key (§5) is deployment config; no CA verb, no cert store, no chain walker.
- **No key material in any worktree, ever.** Structural (§2): keys live
  outside worktree territory, so they cannot become context or be committed.
- **No verifier-side code in litany** (§4). No log parsing, no index, no
  verification verb.
- **No resident PKI process.** Keys are files; signing happens inside the
  executor's step; nothing daemonizes (PRINCIPLES, *Regenerability*).
- **No per-commit git signing.** Per-step signing would tax every commit and
  dangle under rebase-forward (§3). The tip-sha assertion covers ancestry
  through the hash chain; artifacts worth signing individually go through
  the sign tool.
- **No descent counter-signing chain** (parent signs child's key, up the
  tree). The id already *is* the descent record (ARCHITECTURE §2.11);
  re-encoding it as a signature chain would be a second representation of
  one fact. The flat workspace-key binding plus the witnessed dispatch
  transcript carries the same information.
- **No claim past the machine boundary.** The design never asserts
  tamper-*proofing*, custody against a hostile host, or attestation of
  reasoning. Tamper-*evidence*, accident-proof identity, and
  transcript-bound attribution are the honest products.

## 8. What this costs

Prover side: keygen at fork (microseconds, Ed25519), one stable system-slot
sentence (tens of tokens, cache-friendly), one in-process tool, one
frontmatter field on deposits. No new config key is required for the base
case; the sign tool is granted per role like every tool. The verifier side
costs litany nothing, which is the point.

## 9. Status

Design record only — deliberately unscheduled, no implementation ball filed.
What would justify scheduling: a named deployment that (a) retains inference
logs and wants the §3 witness for compliance, or (b) wants the §6 review
gate enforced rather than promised. Until one exists, building the prover
side would produce signatures nothing verifies — mechanism without a
consumer, which is the smell PRINCIPLES exists to refuse.
