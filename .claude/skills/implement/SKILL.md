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

Every **Steps** box goes through the same inner loop before it earns its `[x]`:

1. **Red — test first.** Write a failing test pinning the box's observable
   outcome *before* the implementation. Logic tests go in a `#[cfg(test)]`
   module beside the code; a full request path goes in `tests/`. Name the
   behavior: `delete_removes_row_before_unlink`, `etag_is_md5_of_md5s`.

   **The test must fail on behavior, not on compilation.** A test that won't
   compile because the function/type doesn't exist yet is not a red — it's a
   broken build. Write just enough stub for it to compile and run: the real
   signature (types, params, return) with a body of `todo!()`, or one returning
   a wrong/default value. Then `cargo test` compiles clean and fails on the
   *assertion*. That failing assertion is the red you're looking for — it proves
   the test actually exercises the behavior and would catch a regression.
2. **Green — make it pass.** Simplest code that satisfies the test and honors
   CONCEPT's storage invariants.
3. **Refactor** with the test as a safety net.
4. **Gate — before checking the box, all clean:**
   - `cargo test` — green, including the new test.
   - `cargo clippy --all-targets -- -D warnings` — zero warnings. Fix them;
     don't `#[allow]` without a comment saying why.
   - `cargo fmt --all` — applied.

**Coverage.** Track with `cargo llvm-cov`. The target is *meaningful* coverage,
not a percentage: every error path and edge case the spec's Behavior section
calls out has a test — listing delimiter/continuation edges, multipart
part-boundary and ETag composition, crash-ordering (row-before-unlink,
rename-before-insert). Don't test trivial getters to pad a number, and don't
duplicate in a unit test what an Acceptance box already proves end-to-end. If a
branch is deliberately left untested, say why.

**The two loops nest:** TDD unit tests are the inner loop per Steps box; the
Acceptance boxes are the outer loop — real SDKs proving the whole feature. Green
unit tests never substitute for a passing Acceptance box.

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
