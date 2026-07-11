---
name: plan
description: Turn a buckit spec into an implementation plan at docs/features/<slug>-plan.md — an ordered checklist the /implement skill checks off. Use AFTER /refine has produced <slug>-spec.md and it's approved. Produces the "how"; stops for review before /implement.
argument-hint: <slug (matches docs/features/<slug>-spec.md)>
---

# plan — write the implementation plan

You are turning an approved spec into a **plan**: the *how*. An ordered
checklist that `/implement` works through and checks off. You are NOT writing
production code — only the plan.

## Inputs to read first

1. `docs/features/<slug>-spec.md` — the source of truth for scope and
   acceptance. If it's missing, stop and tell the user to run `/refine` first.
2. `CONCEPT.md` — the Architecture, Storage model, SQLite schema, and Rust
   stack sections. The plan must fit this architecture, not reinvent it.
3. `ROADMAP.md` — the phase this belongs to and what precedes it.
4. The actual code as it exists now — plan against reality, not the concept doc.

## Checkbox granularity — the load-bearing decision

One checkbox ≈ **one small commit that moves an observable behavior.**

- Too coarse: "implement multipart" — it's 60% done for days, box never checks.
- Too fine: "add a struct field" — noise.
- Right: "UploadPart writes part to `.multipart/{id}/{n}` and records its MD5",
  "Complete assembles parts and computes md5-of-md5s-N ETag".

Order the boxes so the thing is runnable/testable as early as possible and each
box builds on checked ones.

## Always include a documentation step

Every plan ends with a **documentation** checkbox that updates `README.md` as
needed. "As needed" = whenever the feature changes user-facing surface: a new
CLI command or flag, a supported S3 operation, an endpoint, default behavior, or
one of CONCEPT's "known sharp edges" worth pre-empting (e.g. the presigned-URL
Docker host gotcha). Internal-only refactors may legitimately need no README
change — in that case the box still exists but reads "confirm no README change
needed". Never leave docs implicit; make it a box someone has to tick.

If `README.md` doesn't exist yet, the step creates it — leading with
`./buckit serve` per CONCEPT's Distribution section.

## Cite the tiebreaker

Where a design choice isn't obvious, name the north star (from CONCEPT/ROADMAP)
it serves — e.g. "listing from SQLite not readdir → *filesystem-is-API* stays
correct under lexicographic order". This keeps `/implement` from drifting.

## Plan template

Write `docs/features/<slug>-plan.md`:

```markdown
# <Feature> — plan

**Spec:** [<slug>-spec.md](<slug>-spec.md) · **Roadmap:** Phase N

## Approach
<2–5 sentences: the shape of the solution and why it fits CONCEPT's
architecture. Note key design choices + the north star each serves.>

## Files
- `path` — <what changes / new>

## Risks & unknowns
- <anything that could invalidate the plan; sharp edges from CONCEPT>

## Steps
Each box ≈ one small commit moving an observable behavior. Check only when the
outcome is real, not when code is written.
- [ ] <verb phrase> — <observable outcome>
- [ ] ...
- [ ] Docs — update `README.md` for <the user-facing change>, or confirm none needed

## Acceptance
Mirrors the spec's acceptance criteria. `/implement` isn't done until every box
here passes by driving the named client.
- [ ] <client> <op> → <observable outcome>
```

## Finish

Write the file, then summarize the approach and step count in chat and **stop
for review**. Do not start implementing. Tell the user: review the plan, adjust
checkboxes, then run `/implement <slug>`.
