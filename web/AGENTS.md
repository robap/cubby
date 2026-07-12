# Zero — Agent & Developer Reference

`zero` is a zero-dependency frontend framework distributed as a single CLI binary. This file is a condensed in-tree reference for agents working inside a scaffolded project. The full user guide lives at <https://robap.github.io/zero/> — every section below links to its long-form chapter.

The framework exposes three import paths:

- `"zero"` — runtime: `App`, reactivity, templates, routing.
- `"zero/test"` — test runner and DOM helpers.
- `"zero/components"` — shipped component library.
- `"zero/http"` — HTTP client.

Identifiers prefixed with `_` or `__` are internal — never import them.

---

## Quick start

```bash
zero init [--yes]                   # scaffold a project
zero update [--yes]                 # refresh framework-owned files in .zero/
zero dev                            # dev server (file watch + full-page reload)
zero test [pattern] [--coverage]    # run *.test.{ts,js}; --coverage to coverage/coverage.json
zero mutate [pattern] [--threads N] [--operators ID,…] [--max-mutants N] [--quiet]
                                    # mutation testing across src/
zero build [--sourcemap|--no-sourcemap]   # production build
zero lint [--quiet]                 # SCSS + JS/TS idiom checks
```

Full CLI reference: <https://robap.github.io/zero/config-and-cli.html> —
every flag the CLI accepts is documented there.

### When to run what

- `zero lint` — after any `.ts` / `.js` / `.scss` edit. Sub-second; catches the
  L- and R- rules below before they reach tests.
- `zero test` — after any logic change. Add `--coverage` to write
  `coverage/coverage.json` and print per-file line / function coverage to the
  terminal.
- `zero mutate` — before declaring a task done on correctness-critical code.
  `--threads N` parallelizes (defaults to `min(cores, 8)`);
  `--operators arith,cmp` narrows the run; `--max-mutants N` caps it.
- Every subcommand has its own `--help`.

Generated project layout:

```
.
├── AGENTS.md                # this file
├── .gitignore
├── tsconfig.json            # editor-only; the CLI ignores it
├── zero.toml                # project / dev / build config
├── .zero/                   # framework-owned, refreshed by `zero update` — do not edit
├── index.html               # script tags injected at serve time
├── src/
│   ├── app.ts               # builds and starts the App
│   └── routes/
│       ├── home.ts
│       └── home.test.ts
└── styles/
    └── app.scss             # @use '../.zero/styles/zero';
```

### JavaScript projects

`.js` works everywhere — the scaffold ships `.ts` only because the examples
are in TypeScript. The JSDoc conventions below apply to `.js` files.

### Type-checking

The CLI does not bundle a type-checker. For TypeScript projects, install
TypeScript **5.0 or later** and run it against the scaffold's
`tsconfig.json`. Two options:

```bash
# Per-project (recommended — version pinned alongside the app)
npm i -D typescript
npx tsc --noEmit

# Global (one install, used across every project on the machine)
npm i -g typescript
tsc --noEmit
```

The shipped `tsconfig.json` relies on `allowImportingTsExtensions` and
`moduleResolution: bundler`, both of which require TS 5.0+. A `tsc`
older than 5.0 will fail with errors about `.ts` extensions in imports
— upgrade rather than edit `tsconfig.json`, which is correct as-shipped.
Verify with `tsc --version`.

---

## Imports

```ts
import {
  App, signal, computed, effect,
  html, each, ref, inject,
  navigate, back, forward, route,
} from "zero";

import {
  describe, it, expect,
  beforeAll, afterAll, beforeEach, afterEach,
  render, find, findAll, text, fire, cleanup, spy,
} from "zero/test";

import { Button, Input, Dialog, createForm, required /* … */ } from "zero/components";
import { createHttp, HttpError } from "zero/http";
```

Use explicit extensions on relative imports (`./routes/home.ts`).

---

## Components

Plain functions returning `html\`...\``. They run **once** when committed;
reactive updates happen at the granular `${...}` binding sites.

```ts
import { html, signal } from "zero";

export default function Counter() {
  const count = signal(0);
  return html`
    <button @click=${() => count.update(n => n + 1)}>Clicked ${count} times</button>
  `;
}
```

Props are plain objects; children are just a `children` prop. Templates,
attribute/event binding, modifiers, `each`, `ref` are documented in full at:

- <https://robap.github.io/zero/components.html>
- <https://robap.github.io/zero/templates.html>

---

## Reactivity

Three primitives: `signal` (cell), `computed` (lazy derivation), `effect`
(side effect with cleanup). Dependencies are auto-tracked at each run; no
deps arrays.

```ts
import { signal, computed, effect } from "zero";

const price = signal(10);
const total = computed(() => price.val * 1.2);
effect(() => console.log("total:", total.val));
```

Full chapter, including the React mental-model bridge: <https://robap.github.io/zero/reactivity.html>.

---

## App configuration

```ts
import { App, signal } from "zero";
import Home from "./routes/home.ts";

new App()
  .state("count", signal(0))
  .route("/", Home)
  .run("#app");
```

Builder methods (`state`, `use`, `route`, `layout`, `loading`, `error`)
all return the same instance and **all throw if called after `run()`**.

Routing reference: <https://robap.github.io/zero/routing.html>.

---

## Routes

```ts
app.route("/users/:id", UserPage);
app.route("/blog/:slug", () => import("./routes/post.ts"));   // lazy
app.route("*", NotFound);
```

A route module exports `default` (component), optional `load(ctx)`, and
optional `meta`. `load` runs before render; the framework awaits it. Use
it to hydrate a store; the component reads via `inject`.

Full chapter (params, guards, nested routes, route-scoped fetch, lifecycle):
<https://robap.github.io/zero/routing.html>.

---

## Styles

SCSS is the canonical authoring layer. Tokens are CSS custom properties on
`:root`; theme variants override only the thirteen public `--color-*`
tokens. Layout primitives, utilities, and theme switching are documented
at <https://robap.github.io/zero/theming.html>.

### Reach for these first

| Need                          | Reach for                                  |
|-------------------------------|--------------------------------------------|
| Horizontal layout             | `class="cluster"` + `gap-*` utility        |
| Vertical layout               | `class="stack"` + `gap-*` utility          |
| Spacing                       | `pad-*` utility or `var(--space-{step})`   |
| Color                         | `var(--color-{semantic})` token            |
| Radius                        | `var(--radius-{step})` token               |
| Border                        | `class="border"` (or `border-{t,r,b,l}`)   |
| Typography                    | `text-{display,h1,h2,h3,h4,body,small,…}` utility |
| Theme override                | `<html data-theme="light\|dark\|brand">`   |

### When to reach for which primitive

| Primitive | Reach for it when…                                                                                                                                                                |
|-----------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `cluster` | Default for any horizontal layout. Wraps for free at narrow widths — use it whenever you'd reach for `display: flex` on a row of items (toolbars, button groups, chip lists).     |
| `stack`   | Vertical layout where items should *not* spread horizontally — a form body, a card's contents, a sidebar's list of links.                                                         |
| `split`   | Two end-anchored groups separated by stretched whitespace. Page header with brand on the left, nav on the right. Anywhere `justify-content: space-between` would do.              |
| `flank`   | A fixed-size element next to a flexible one. A form row with a label and an input that fills the rest; media objects (avatar + body), icons next to flowing text.                 |
| `grid`    | A repeating auto-fit column layout — card grids, tile lists, dashboard widgets. Override `--grid-min` to tune the breakpoint. Not for two-column page layouts (use `flank`).       |
| `frame`   | Fixed-aspect-ratio media boxes — video embeds, image thumbnails, hero art. Override `--frame-ratio`.                                                                              |

---

## Common mistakes

`zero lint` enforces these. Full chapter (rationale + cross-links to the
teaching pages): <https://robap.github.io/zero/linting.html>.

### SCSS rules

| Rule | Don't write                                              | Use instead                                                                |
|------|----------------------------------------------------------|----------------------------------------------------------------------------|
| L01  | `font-weight: 600;`                                      | `font-weight: var(--weight-semi);`                                         |
| L02  | `font-size: 0.875rem;`                                   | `font-size: var(--font-size-sm);` — or a `text-*` utility.                 |
| L03  | `line-height: 1.4;`                                      | `line-height: var(--leading-snug);`                                        |
| L04  | `letter-spacing: 0.04em;`                                | `letter-spacing: var(--tracking-wide);`                                    |
| L05  | `background: #228be6;` / `color: red;`                   | Semantic color token — `var(--color-primary)`, `var(--color-danger)`, etc. |
| L06  | `border-radius: 999px;` / `border-radius: 50%;`          | `border-radius: var(--radius-3xl);`                                        |
| L07  | `border: 1px solid var(--color-border);`                 | `class="border"` (utility) or `border-width: var(--border-thin);`          |
| L08  | `padding: 16px;`                                         | `padding: var(--space-md);` or `class="pad-md"`.                           |
| L09  | `margin-top: 24px;`                                      | `margin-top: var(--space-lg);` (prefer `gap` on the parent primitive).     |
| L10  | `gap: 8px;`                                              | `gap: var(--space-sm);` or `class="gap-sm"`.                               |
| L11  | `.toolbar { display: flex; flex-wrap: wrap; gap: … }`    | `class="cluster gap-sm"`.                                                  |
| L12  | `align-items: center; justify-content: center;`          | Utility classes — `class="… align-center justify-center"`.                 |
| L13  | `var(--radius-pill)` / `var(--pad-sm)` (utility name)    | The lint names the missing custom property. Fix the typo, run `zero update`, or declare the token in `styles/app.scss`. |

### JS/TS rules

| Rule | Don't write                                                                          | Use                                                                                  |
|------|--------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------|
| R01  | ``html`${count.val}` ``                                                              | ``html`${count}` `` — pass the signal, not its current value.                        |
| R02  | `count.val = 5;`                                                                     | `count.set(5)` / `count.update(n => n + 1)`.                                         |
| R03  | top-level `effect()` outside `src/app.{ts,js,tsx,jsx}`                               | move into a function, a component body, or the app entry. Top-level `signal()` / `computed()` are fine anywhere — that's what a store is. |
| T01  | `el.addEventListener("click", h)` in components/routes                               | ``html`<button @click=${h}>` ``.                                                     |
| T02  | `@click.captuer=${h}` (typo)                                                         | one of `prevent`, `stop`, `once`, `throttle`, `debounce`, `enter`, `escape`, `space`, `tab`, `up`, `down`, `left`, `right`. |
| T03  | `each(items, render)`                                                                | `each(items, render, item => item.id)`.                                              |
| T04  | `document.querySelector(".x")` in components/routes                                  | `ref()` — `myRef.el?.querySelector(".x")`.                                           |
| C01  | `class Counter { ... }` in components/routes                                         | `function Counter() { return html\`…\`; }`.                                          |
| C02  | `customElements.define("x-el", X);`                                                  | (deferred to the `'zero/wc'` escape hatch).                                          |
| I01  | `import x from "lodash";` / `"node:fs"` / `"npm:..."`                                | one of `"zero"`, `"zero/components"`, `"zero/http"`, `"zero/test"`, or a relative path. |
| I02  | `import x from "../.zero/components/Button.ts";`                                     | `import { Button } from "zero/components";`.                                          |
| S01  | one 200-line function                                                                | split into named helpers (target ≤ 80 lines).                                        |

Tests (`*.test.{ts,js,tsx,jsx}` / `*.spec.{ts,js,tsx,jsx}`) are exempt
from the T-rules, R03, and S01; everything else still applies.

---

## Component library

Imported from `"zero/components"`. Plain function components; stateful
props accept signals directly. Full reference with prop tables and usage
snippets: <https://robap.github.io/zero/components.html#component-library-reference>.

| Component  | Stateful prop(s)                                  |
|------------|---------------------------------------------------|
| `Avatar`   | —                                                 |
| `Badge`    | —                                                 |
| `Button`   | —                                                 |
| `Card`     | —                                                 |
| `Checkbox` | `checked: Signal<boolean>`                        |
| `Combobox` | `value: Signal<string>`                           |
| `Dialog`   | `open: Signal<boolean>`                           |
| `Drawer`   | `open: Signal<boolean>`                           |
| `Input`    | `value: Signal<string>`                           |
| `Pagination` | `page: Signal<number>`, `totalPages: Signal<number> \| number` |
| `Radio`    | `selected: Signal<string>`                        |
| `Select`   | `value: Signal<string>`                           |
| `Spinner`  | —                                                 |
| `Table`    | `rows: Signal<T[]>`, `loading?: Signal<boolean>`  |
| `Tabs`     | `active: Signal<string>`                          |
| `TextArea` | `value: Signal<string>`                           |
| `Toast`    | `open: Signal<boolean>`                           |
| `Toggle`   | `checked: Signal<boolean>`                        |

### Shared control props

Every form control (`Input`, `Select`, `TextArea`, `Checkbox`, `Radio`,
`Toggle`, `Combobox`) also accepts:

- `error?: Signal<string | null>` — while non-null the control renders
  the message below itself, sets `aria-invalid`, and links it via
  `aria-describedby`. Bind `form.fields.<name>.error` from
  [`createForm`](#forms) directly to this prop.
- `autofocus?: boolean` — focuses the underlying element each time it
  commits to the DOM: first render and every dialog/drawer re-open.
- `attrs?: NativeAttrs` — additional native attributes for the
  underlying element (`name`, `autocomplete`, `min`/`max`, `data-*`, …).
  Additive-only: attributes the component renders itself win and the
  colliding key is skipped. `true` sets an empty attribute, `false`
  skips the key, numbers are stringified. Values are plain, not
  reactive.
- `debounceMs?: number` — delays the signal write on the trailing edge
  (default `0`, synchronous). On `Combobox` the same prop instead sets
  the gap before `loadOptions` runs after the last keystroke.

`Input`, `Select`, and `Combobox` accept
`onChange?: (value: string) => void`, invoked with the new value after
each user edit — never on programmatic `value.set(...)` — so reach for
it instead of bridging the `value` signal with an `effect`. `Combobox`
additionally accepts `allowCustom: true` to commit free text that
matches no loaded option.

Every component partial is wrapped in `@layer components`, so any rule in
unlayered `styles/app.scss` overrides framework component rules without
`!important`.

---

## Forms

`createForm` (from `"zero/components"`) declares fields, initial
values, and validators in one call and returns typed reactive state to
bind straight onto the controls — replacing hand-rolled value+error
signal pairs, `validate()` routines, and per-form `HttpError`
unwrapping. Full chapter:
<https://robap.github.io/zero/components.html#forms>.

```ts
import { html } from "zero";
import { Input, createForm, required, maxLength } from "zero/components";

const form = createForm({
  fields: {
    code: { initial: "", validate: [required(), maxLength(10)] },
    name: { initial: "", validate: required("Name is required.") },
  },
  // Optional cross-field validator; fills only fields without a
  // per-field error.
  validate: (values) =>
    values.code.trim() === values.name.trim()
      ? { name: "Name must differ from code." }
      : {},
});

// Validates, gates, and maps HttpError 400/409 `{ errors }` bodies
// onto matching fields automatically; anything else sets `form.error`.
const onSubmit = form.submit(async (values) => {
  await saveLocation({ code: values.code.trim(), name: values.name.trim() });
  // Success handling is yours: close the dialog, toast, navigate.
});

const body = html`
  <form class="stack gap-md" @submit=${onSubmit}>
    ${Input({ value: form.fields.code.value, label: "Code", error: form.fields.code.error })}
    ${Input({ value: form.fields.name.value, label: "Name", error: form.fields.name.error })}
    <button class="button button-primary button-md" type="submit"
      disabled=${() => !form.isValid.val}>Save</button>
  </form>
`;
```

Returned shape: `form.fields.<name>.{value, error, touched}` (signals
— bind `value`/`error` straight to control props), `form.isValid`
(live computed; reading it never populates error signals),
`form.error` (form-level message), `form.values()`, `form.reset()`,
`form.setErrors(errors)`, and `form.submit(action)`.

Validators are plain functions `(value, values) => string | null`; an
array runs in declaration order and the first non-null message wins.
Field errors appear only via `submit()` / `setErrors()` — but once a
field shows an error, editing it re-validates just that field live.
Field values are strings; convert numeric inputs at submit time.

### Built-in rules

Rule factories from `"zero/components"` return plain validators
(`Rule = (value: string) => string | null`) and drop straight into
`validate:` — alone, in arrays, or mixed with hand-written functions.

| Rule | Default message | Empty value |
|------|-----------------|-------------|
| `required(message?)` | This field is required. | fails |
| `minLength(n, opts?)` | Must be at least {n} character(s). | passes |
| `maxLength(n, opts?)` | Must be {n} character(s) or fewer. | passes |
| `intRange(min, max, opts?)` | Must be a whole number between {min} and {max}. | passes |
| `pattern(re, opts?)` | Invalid format. | passes |
| `email(opts?)` | Enter a valid email address. | passes |

Empty values pass for every rule except `required()`, so optional
fields with constraints compose (`validate: maxLength(200)` accepts an
empty notes field but caps a filled one). `opts` is
`string | RuleOptions` — a plain string is shorthand for the custom
message; `allowEmpty: false` opts a single rule into rejecting
empties. There is no async validation — server-side uniqueness belongs
on the 409 path, which `submit()` maps onto fields for you.

---

## The .zero/ directory

`.zero/` is the framework's regenerable file boundary. Owned by the CLI;
**do not edit anything inside it.** `zero update` is the only command that
writes here. Files currently shipped:

| Path                                  | What it is                                                             |
|---------------------------------------|------------------------------------------------------------------------|
| `.zero/zero.d.ts`                     | Type declarations for `"zero"`.                                        |
| `.zero/zero-test.d.ts`                | Type declarations for `"zero/test"`.                                   |
| `.zero/zero-http.d.ts`                | Type declarations for `"zero/http"`.                                   |
| `.zero/components.d.ts`               | Type declarations for `"zero/components"`.                             |
| `.zero/components/index.ts`           | Re-exports every shipped component.                                    |
| `.zero/components/<Name>.ts`          | One source file per component (18 total).                              |
| `.zero/components/<Name>.test.ts`     | One test file per component (18 total).                                |
| `.zero/components/form.ts`            | `createForm` form-state primitive (+ `form.test.ts`).                  |
| `.zero/components/rules.ts`           | Built-in validation rule factories (+ `rules.test.ts`).                |
| `.zero/components/_internal.ts`       | Shared control internals, e.g. `NativeAttrs` (+ `_internal.test.ts`).  |
| `.zero/styles/_palette.scss`          | 55 framework-internal palette tokens.                                  |
| `.zero/styles/_tokens.scss`           | Non-color design tokens.                                               |
| `.zero/styles/_themes.scss`           | Theme aggregator + selector strategy.                                  |
| `.zero/styles/themes/_light.scss`     | Light theme `@mixin tokens`.                                           |
| `.zero/styles/themes/_dark.scss`      | Dark theme `@mixin tokens`.                                            |
| `.zero/styles/_base.scss`             | Minimal reset and token-bound `body` rule.                             |
| `.zero/styles/_layout.scss`           | Six layout primitives.                                                 |
| `.zero/styles/_utilities.scss`        | Gap, padding, and border utility classes.                              |
| `.zero/styles/_alignment.scss`        | Alignment, justify, self, text-align, flex-direction utilities.        |
| `.zero/styles/_components.scss`       | Aggregate of per-component partials.                                   |
| `.zero/styles/components/_<name>.scss`| One SCSS partial per component (18 total).                             |
| `.zero/styles/zero.scss`              | Aggregate `@use`'d by `styles/app.scss`.                               |
| `.zero/fonts/*.woff2`                 | Geist + Geist Mono, locally served.                                    |

Update with `zero update` (interactive plan) or `zero update --yes` (CI).

`AGENTS.md` itself sits at the project root (so Claude Code, Cursor, and
other tools that read a root-level `AGENTS.md` find it) but is
framework-owned just like the files under `.zero/`. `zero update`
refreshes it. Do not put project-specific agent guidance here — it will
be overwritten on the next update. Keep your own notes in a separate
file you maintain.

---

## Navigation

Same-origin `<a href>` clicks are intercepted automatically — no `<Link>`
component exists. Programmatic API:

```ts
import { navigate, back, forward, route } from "zero";

navigate("/dashboard");
navigate("/dashboard", { replace: true });
back();
forward();
const r = route();   // reactive { path, params, query }
```

Clicks are **not** intercepted when the click is modified (Cmd/Ctrl/Shift/Alt/middle
button), the anchor has `target="_blank"` / `download` / `data-external`,
or the href is hash-only / cross-origin.

Active-link styling and full routing semantics: <https://robap.github.io/zero/routing.html>.

---

## App-level state

`inject(key)` reads a value registered with `app.state(key, value)`.
Throws if no app is running or `key` is unregistered.

```ts
import { inject } from "zero";
const count = inject<Signal<number>>("count");
```

For typed `inject`, augment `interface StateTypes` in `src/state.ts`:

```ts
declare module "zero" {
  interface StateTypes {
    count: Signal<number>;
    auth: Signal<AuthState>;
  }
}
```

In tests, seed via `render(tr, { state: { … } })`.

---

## Testing

```ts
import { describe, it, expect, beforeEach, afterEach,
         render, find, fire, text, cleanup, spy } from "zero/test";

afterEach(cleanup);

it("increments", () => {
  const el = render(Counter());
  fire(find(el, "button")!, "click");
  expect(text(el, "p")).toBe("Count: 1");
});
```

`render(tr, { state? })` plays the role of `app.state` for the test's
duration. `cleanup()` disposes scopes and unregisters state — wire it
into `afterEach`. Selectors are simple (`tag`, `#id`, `.class`,
`[attr=value]`); combinators and pseudo-classes are not supported.

Spies, the matcher list, and the in-memory DOM / Web Platform surface
(Fetch, URL, encoding, binary, structuredClone, queueMicrotask) are
documented at <https://robap.github.io/zero/testing.html>.

---

## JSDoc conventions

Every JavaScript file in a zero project is fully JSDoc-annotated:

- Every exported function, class, and class method has `@param`,
  `@returns`, and `@template` where applicable.
- Module-level variables have `@type`.
- `@internal` marks exports that are not part of the public API.
- `@private` marks private class methods.

Canonical shape (from the scaffold's `src/routes/home.ts`):

```js
import { html, inject } from "zero";

/**
 * @typedef {import("zero").TemplateResult} TemplateResult
 */

/**
 * @returns {TemplateResult}
 */
function Counter() {
  return html`<p>Count: ${() => inject("count").val}</p>`;
}

/**
 * @returns {TemplateResult}
 */
export default function Home() {
  return html`
    <h1>Hello from zero</h1>
    <button @click=${() => inject("count").update(n => n + 1)}>Increment</button>
    ${Counter()}
  `;
}
```

Import the `TemplateResult` type via `@typedef`, not at runtime.

---

## Common pitfalls

- **Components run once per mount.** Module-level `signal()` shares state
  across every mount. Put per-instance state inside the function body.
- **Reactive reads need a reactive context.** `${count.val}` in a template
  takes a snapshot; use `${count}` or `${() => count.val}`.
- **`each` needs a key function.** Without one (T03), reorders and churn
  rebuild the whole list. Duplicate keys throw.
- **`inject(key)` throws on unknown keys.** Register every key with
  `app.state` (in code) or `render(tr, { state })` (in tests).
- **`app.run` must be called exactly once.** Builder methods throw afterward.
- **`navigate` / `back` / `forward` / `route()` / `inject()` require a running app.**
- **Same-origin `<a>` clicks are intercepted by default.** Opt out with
  `target="_blank"`, `download`, or `data-external`.
- **`.throttle` and `.debounce` default to 100 ms.** Override per call with a `:<ms>` suffix, e.g. `@input.debounce:250=${onSearch}`. Suffix is only valid on these two modifiers.

---

## Best practices

Real apps benefit from a small, predictable layout. The scaffold's
default is layer-first — `state.ts`, `stores/`, `components/`,
`routes/`, `lib/` — but nothing enforces it: routing is explicit
imports and lint does not care where files live, so a feature-first
layout (`features/<domain>/{store,routes}.ts`) is equally valid. Keep
route components, their `load()`, and their `meta` co-located in one
file per route.

- **Use `zero/components` for every interactive primitive.** Drop to raw
  `<button>` / `<input>` only when the shipped component cannot express
  the behavior, or when you're building a new presentational component
  the library does not ship. Plain containers (`<main>`, `<section>`,
  `<form>`, `<ul>`, `<li>`, `<label>`, `<a>`, `<svg>`, …) are not
  "components" under this rule — use them freely.
- **When building your own presentational component, wrap shipped primitives**
  rather than re-implementing them.
- **Reach for `inject` via the `Keys` registry, not bare strings.** Augment
  `interface StateTypes` from `"zero"` so reads infer their value type.
- **Mutate store signals only via the store's exported mutators.** Components
  never call `signal.set()` on a store signal.
- **Keep an entity's lifecycle in one module.** The store module owns the
  entity's types, signal, mutators, *and* load/save functions. Don't split
  the model into `lib/` and bridge it with a re-export shim — `lib/` is for
  helpers with no entity (formatters, validators, guards, the HTTP client).
- **Co-locate `load` / `meta` / `default` in the route file.** Import them
  at the registration site: `app.route("/issues/:id", IssuePage, { load, meta, guard: requireAuth });`. `load()` is side-effect-only — its return value is awaited but not piped into the component. Use it to hydrate a store; have the component read via `inject`.
- **Use `zero/http` for HTTP, not raw `fetch`.** Construct the client once
  in `src/lib/api.ts`; register middleware in `src/app.ts` before
  `app.run()`. Inside `load()`, thread the route-scoped fetch via
  `init.fetch` so navigation aborts cancel in-flight requests.

For longer rationale and worked examples, see the [Best Practices](https://robap.github.io/zero/best-practices.html) chapter of the user guide.
