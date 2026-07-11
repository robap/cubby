---
name: refine
description: Turn a rough cubby feature idea into a durable spec at docs/features/<slug>-spec.md. Use at the START of any new feature or roadmap phase, before planning. Produces the "what and why" with client-observable acceptance criteria; stops for review before /plan.
argument-hint: <feature description or roadmap phase>
---

# refine — write the spec

You are turning a rough ask into a **spec**: the *what* and *why*, with
acceptance criteria that a real client can prove. You are NOT designing the
implementation (that is `/plan`) and NOT writing code.

## Inputs to read first

1. `CONCEPT.md` — design principles are the tiebreaker for scope.
2. `ROADMAP.md` — does this map to a numbered phase? Reuse its goal and its
   named acceptance client(s).
3. Any existing `docs/features/*-spec.md` for overlap or precedent.

## Pick the slug

`docs/features/<slug>-spec.md`. If the feature maps to a roadmap phase, prefix
with its number so `docs/features/` sorts into build order
(e.g. `02-listing-delimiters-spec.md`). Otherwise a plain kebab name. The
`<slug>` carries forward unchanged to `<slug>-plan.md`.

## The one rule that matters

**Every acceptance criterion names a concrete observer.** A criterion is only
valid if it is satisfiable by watching something happen, never by reading code:

- a specific client doing a specific op — "boto3 `put_object` of a 100MB file",
  "`aws s3 ls s3://b --recursive`", "rclone `sync`", "aws-sdk-js v3 presigned GET"
- OR a filesystem assertion — "`cat s3data/buckets/b/key` shows the real bytes",
  "`Path('.tmp/…')` is gone after commit"

If you can't phrase a criterion as an observation, the feature isn't understood
yet — say so and ask, don't invent one.

## Spec template

Write `docs/features/<slug>-spec.md`:

```markdown
# <Feature> — spec

**Status:** draft · **Roadmap:** Phase N (or "ad-hoc") · **Slug:** <slug>

## Why
<The user/dev problem. Which north star(s) it serves — cite them.>

## In scope
- <bullet>

## Out of scope
- <bullet — especially anything from CONCEPT's non-goals that's nearby>

## Behavior
<How it should behave from the client's side. S3 semantics, edge cases,
error codes. Quirks that specific SDKs care about.>

## Acceptance criteria
Each names a client + op, or a filesystem assertion. This list becomes the
final checkboxes in the plan.
- [ ] <client> <op> → <observable outcome>

## Open questions
- <anything blocking; ask the user rather than guessing>
```

## Finish

Write the file, then summarize it in chat and **stop for review**. Do not
proceed to planning. Tell the user: review the spec, then run
`/plan <slug>`. If open questions block the acceptance criteria, surface them
now — an unresolved spec makes a worthless plan.
