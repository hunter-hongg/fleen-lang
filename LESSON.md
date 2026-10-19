# LESSON.md — Behavior Lessons for the Machine

> What deviates is what gets recorded — not the happy path.  These are
> process/behavior mistakes the coding agent made on this repo, written so the
> next run avoids repeating them.  **This file is the record of "don't do that
> again"; it does not override SPEC.md rules, it documents how to keep them.**

## How to use it (see SPEC §LESSON.md)

- Read this file before planning or editing docs/commits on the repo.
- Once an entry is here it stays; we never delete a lesson to make room.
- An entry is closed only when a change that structurally prevents the mistake
  is in place (a checklist item, a rule in SPEC, a never-again instruction),
  not merely when time passes.

---

## L1 — Do not write spec/plan docs before the rules are settled

**When**: 0.0.4 planning session, first pass.

**What happened**: While discussing a 0.0.4 feature breakdown, the agent
immediately wrote `docs/0.0.4/PLAN.md` and `docs/0.0.4/TICKETS.md` to disk the
moment the user asked for a feature list.  The design decisions (Future
mechanism, concurrency model, Send rules) were not settled yet.  User reaction:
*"谁让你写文档了？规则都还没有定好，写了就拉倒"* (nobody told you to write
documents — the rules were not fixed, writing first is reckless).

**Root cause**: mistaking "user wants a breakdown/enumeration" for "user wants
documents written now."  A feature list answered in the conversation is not a
mandate to persist a PLAN/TICKETS pair.

**Rule going forward**:
- Settle the design decisions **first, in conversation**.  Only write a
  PLAN/TICKETS file after the user asks for it or explicitly approves persisting.
- When unsure whether to write a file, **ask** — a one-line ask costs nothing
  compared to writing a file the user didn't want.
- The tentative 0.0.4 docs were only kept after the user reviewed and said
  "用我们讨论的决策点，更新你原本写的破文档" (update what you wrote with the
  decisions we discussed) — i.e. the user, not the agent, decided they survive.

**Closed when**: writing any spec/plan file is gated on explicit user approval.

---

## L2 — Do not pick "the easy option today" just to kick the real decision down the road

**When**: 0.0.4 planning, the Future-mechanism and Send-model decisions.

**What happened**: The agent resolved several open design forks by choosing the
simple/reusable option and justifying it with *"we can switch to the better one
later, smoothly"*.  When challenged that this was side-stepping the real
question, the agent defended a claim ("coroutine frame can be smoothly
replaced by a state machine later") that is actually **false** — swapping the
Future representation touches lowering, codegen, `.flnc` bytecode, verify, and
scheduling, and bytecode is frozen for compatibility once released.

**Root cause**: treating "complex vs simple" as the axis when the real axis is
"what is the semantic goal / what gets locked by this choice."  Reaching for the
cheap path and dressing it as "forward compatible."

**Rule going forward**:
- Never use "we can change it later smoothly" as a justification.  If a choice
  can't be cleanly replaced, say so; if it can, name the migration cost.
- For a genuine fork, lay out what each option **permanently locks** (VM layer
  vs language semantics) and by what compat rule, before recommending.
- When the user says a decision matters ("都选那个简单选哪个别的以后再说？不
  行" — that's just picking easy and deferring, not acceptable), reopen the
  analysis instead of rationalizing the easy pick.

**Closed when**: every design-decision recommendation in a plan names the
compat/rewrite cost of choosing it, not a hand-waved "replaceable later."

---

## L3 — Verify claims against the authoritative doc before writing them

**When**: writing the 0.0.4 bytecode section.

**What happened**: The draft assigned `Future = 0x0D` and `Channel = 0x0E`
"value tags," and claimed opcode region "0xBC–0xBF reserved."  Both were wrong:
BYTECODE.md's numeric tags (`const-pool 0/1/2`, `FieldTypeTag 0–11`) are disk-
format and already allocated; runtime `Value` variants are a non-serialized VM
enum with **no** numeric tag — and the reserved opcode region is 0xBC–**0xFF**,
not 0xBC–0xBF.  Caught only after the user asked to re-check.

**Root cause**: writing the doc from memory instead of reading the supply doc
(BYTECODE.md) first; inventing details that look plausible instead of checking.

**Rule going forward**:
- Before any doc/commit that states a bytecode value, opcode, tag, or version:
  **read BYTECODE.md (and SPEC) first** and quote the region/number.
- Numbers that aren't confirmed as free are marked "TBD — verify against
  BYTECODE.md", never invented.

**Closed when**: no unverified numeric claim survives in a plan/commit.

---

## L4 — Do not assert facts about the repo (including commits) you haven't checked

**When**: telling the user the `chore: 0.0.3 release prep` commit "used Chinese."

**What happened**: After pushing the 0.0.4 docs, the agent told the user that a
recent commit's message violated the SPEC §10 "English only" rule — without
actually reading that commit's full message.  On checking, **44fe842's message
is fully English**; the Chinese-message commits are a much earlier 0.0.1/0.0.2
batch (pre-date the "translate all Chinese content to English" cleanup).  The
claim was plausible but false.

**Root cause**: pattern-matching ("early repo has Chinese commits" ⇒ "this one
does too") instead of inspecting the specific object.

**Rule going forward**:
- Before asserting anything about a specific commit/revision, run
  `git log -1 --format=%B <ref>` (full message, not just subject).
- Distinguish "checked" claims from "assumed" claims out loud.

**Closed when**: no repo/commit assertion is made without having run the command
that shows it.