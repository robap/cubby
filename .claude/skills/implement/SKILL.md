---
name: implement
description: Execute a cubby plan at docs/features/<slug>-plan.md — work its checklist top to bottom, checking boxes only when their outcome is observably true, driving the real client for acceptance boxes. Use AFTER /plan has produced an approved <slug>-plan.md.
argument-hint: <slug (matches docs/features/<slug>-plan.md)>
---

# implement — execute the plan

You are building the feature by working through the plan's checklist. The plan
is your contract and your progress record.

## Start

1. Read `docs/features/<slug>-plan.md`. If missing, stop — tell the user to run
   `/plan <slug>` first.
2. Read its `<slug>-spec.md` for the acceptance criteria's intent.
3. Skim `CONCEPT.md` for the architecture you're building into.
4. Work the **Steps** top to bottom. The next unchecked box is the next task.

## The discipline that makes checkboxes mean something

- **Check `- [x]` only when the box's stated outcome is observably true** — the
  behavior happens, not "the code is written". This is cubby's whole DoD:
  "boto3 round-trips it" ≠ "I wrote the handler".
- **One box ≈ one commit's worth of change** — that's the *sizing* rule for a
  box, not an instruction to commit. **Do NOT run `git commit`, `git add`, or
  any git write command — committing is the user's job, always.** Leave the
  working tree ready to commit and let the user do it. Don't init git either;
  that's theirs too.
- **The plan is living.** When reality diverges — a box is wrong, missing, or
  splits in two — *edit the plan*: rework the boxes and add a one-line note in a
  `## Progress notes` section saying what changed and why. Never silently
  improvise around a stale plan; a plan that no longer matches the code is a bug.

## Per-box loop: TDD, lint, coverage

Every **Steps** box goes through the same inner loop — **red → green →
refactor → gate** — before it earns its `[x]`. But cubby is **two codebases with
two toolchains**, and the loop's tools differ by which one the box touches.
**Determine the box's surface first, then use that lane's tools — never import
one lane's habits into the other:**

- **Rust backend** — `src/`, `tests/`, `Cargo.toml`. The cargo lane.
- **The zero web UI** — anything under `web/`. The zero lane. **`web/AGENTS.md`
  is the authority for this project — read it before writing a single
  `.ts`/`.scss` line and follow it exactly.** It defines the test runner, the
  lint rules, the component/testing idioms, and the file layout. It is not
  optional background; skipping it is how the TDD loop goes wrong here.

### 1. Red — test first (both lanes)

Write a failing test pinning the box's observable outcome *before* the
implementation. Name the behavior (`delete_removes_row_before_unlink`,
`filter_hides_non_matching_rows`). **The test must fail on an assertion, not on a
load/compile error** — stub just enough (real signature, `todo!()`/wrong return,
or a component that renders but misses the assertion) so the file loads clean and
the *assertion* is the red. That failing assertion proves the test exercises the
behavior and would catch a regression.

- **Rust:** logic tests in a `#[cfg(test)]` module beside the code; a full
  request path in `tests/`. Stub with the real signature + `todo!()`.
- **zero:** a `*.test.{ts,js}` beside the source, using `zero/test`.
  **Components and routes are testable — render them.** `render(Component())`
  mounts to the in-memory DOM; assert with `find`/`findAll`/`text`, drive with
  `fire`, seed injected state via `render(tr, { state })`, clean up in
  `afterEach(cleanup)`. **TDD the rendered behavior** — a row appears when an
  event arrives, a filter hides non-matching rows, a click expands the detail,
  the empty state shows with no data. Extracting pure logic into `lib/` and
  unit-testing it is good, but it is *in addition to*, **not instead of**,
  rendering-level tests for the component's own logic. Do **not** downgrade a
  screen to "human-observable only" and test merely the helpers you carved out —
  that is the wrong-TDD trap. The human-in-a-browser Acceptance box is the outer
  loop, never a substitute for a red-first component test that `zero/test` can
  render and assert.

### 2. Green — make it pass

Simplest code that satisfies the test and honors CONCEPT's storage invariants.

### 3. Refactor with the test as a safety net.

### 4. Gate — before checking the box, all clean

Run the gate for the box's lane (run zero commands from inside `web/`):

- **Rust:** `cargo test` green (incl. the new test) · `cargo clippy
  --all-targets -- -D warnings` zero warnings (fix them; no bare `#[allow]`
  without a why-comment) · `cargo fmt --all` applied.
- **zero (`web/`):** `zero test` green (incl. the new test) · `zero lint` clean
  (fix the L-/R- rules; don't suppress) · `zero mutate` on correctness-critical
  logic **before declaring the box done** — a surviving mutant means a missing
  assertion, so tighten the test. (See `web/AGENTS.md` → "When to run what".)

**Coverage / meaningful tests.** Track with `cargo llvm-cov` (Rust) or
`zero test --coverage` (web). The target is *meaningful* coverage, not a
percentage: every error path and edge the spec's Behavior section calls out has a
test — Rust: listing delimiter/continuation edges, multipart part-boundary and
ETag composition, crash-ordering (row-before-unlink, rename-before-insert); web:
filter/pause/expand logic, folder-vs-search view switching, preview-kind
selection, upload-key composition. `zero mutate` is the honesty check that a
web test actually pins behavior. Don't pad with trivial getters, and don't
duplicate in a unit test what an Acceptance box already proves end-to-end. If a
branch is deliberately left untested, say why.

**The two loops nest:** TDD tests (cargo *or* zero) are the inner loop per Steps
box; the Acceptance boxes are the outer loop — real SDKs and a human in a browser
proving the whole feature. Green inner tests never substitute for a passing
Acceptance box.

## Acceptance boxes

The `## Acceptance` boxes are not checked by writing code — they're checked by
**driving the actual named client** and watching it pass. Use the `/verify`
skill (or `/run`) to exercise the real flow end-to-end: the AWS CLI, boto3,
rclone, whichever the box names. If you can't run a client, say so and leave the
box unchecked — do not check it on faith.

## Match the surrounding code

Read neighboring code before adding to it; match its idioms, error handling, and
naming. Honor CONCEPT's storage invariants exactly — streaming writes (never
buffer whole objects), temp→fsync→rename→SQLite-insert ordering, row-before-
unlink on delete. These are correctness, not style.

## Finish

When every box (Steps + Acceptance) is checked:
- Set the spec/plan status to done.
- Summarize in chat: what shipped, which acceptance criteria passed and how they
  were verified, and any Progress-notes divergences from the original plan.

If you stop partway, leave the plan's checkboxes accurate — they're the resume
point for the next session.
