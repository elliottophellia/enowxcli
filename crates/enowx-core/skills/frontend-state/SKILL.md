---
name: frontend-state
description: "Where each piece of state lives: server data in a query cache, the URL for what should survive a reload or a shared link, forms in a form library, derived values computed not stored, and a little local or global state for the rest. Read before adding state, a context or a store."
---

# Where state lives

The generated app keeps everything in one global store: the API response
copied in and going stale, the filters that vanish on reload, a
`filteredItems` array kept in sync by an effect, `isLoading`, `isError`
and `isSuccess` all true at once after a race, a context that re-renders
the whole page on every keystroke. Most state bugs are state in the wrong
place. This gives each piece its home, and the rules that keep it
there. The whole frontend is in `frontend`.

## 1. Four kinds, four homes

Ask of each piece: does it come from the server? Should a reload or a
shared link keep it? Is it being typed into a form? Can it be computed
from something else?

| Kind | Examples | Home |
|---|---|---|
| Server state | orders, the signed-in user, counts, permissions | the query cache (`frontend-data`) |
| URL state | filters, sort, page, tab, selected record, search query, date range, open detail panel | the path and search params (section 4) |
| Form state | field values, touched, errors, dirty, submitting | the form library, until submit (`frontend-forms`) |
| UI state | a menu open, a row hovered, a panel collapsed, a local toggle | the component that owns it (section 3) |
| Derived | a filtered list, a total, "can submit", a label from a status | nowhere: computed during render (section 2) |

What remains for a global store is small: client-only state that many
distant components write, such as an editor's selection and active tool,
a media player's queue, or a multi-panel layout.

## 2. Derived values are computed, not stored

A copy kept in sync with an effect renders one frame late, drifts when
someone forgets a dependency, and doubles the places a bug can live.

```tsx
// Wrong: a second source of truth, updated after the render that needed it
const [visible, setVisible] = useState<Order[]>([]);
useEffect(() => setVisible(orders.filter((o) => o.status === status)), [orders, status]);

// Right: computed from what is already there
const visible = orders.filter((o) => o.status === status);
```

- Memoise (`useMemo`) only when the profiler shows the computation is
  slow, as a rule of thumb over about 1ms per render. Vue's `computed` and
  Svelte's `$derived` cache for you.
- State that must reset when an identity changes (a form for another
  record) resets with a `key` (`<OrderForm key={order.id} />`), not an
  effect that copies props into state.
- Model states that exclude each other as one value, not booleans that can
  contradict: `status: "idle" | "loading" | "error" | "done"` (section 6).

## 3. Client state: the smallest scope that works

In this order, stopping at the first that fits:

1. **Local**: `useState` in the component that uses it (`ref` in Vue,
   `$state` in Svelte).
2. **Lifted** to the nearest common parent when two siblings need it.
3. **Composition** before drilling: pass the rendered child
   (`<Layout sidebar={<Filters />}>`) instead of threading props through
   layers that do not use them.
4. **Context** for values that change rarely and are read widely: theme,
   locale, the signed-in user from the query cache, feature flags. Every
   consumer re-renders when the value changes, so keep fast-changing
   values out, memoise the value object, and split state from its setters
   when both are shared.
5. **A store** when many distant components write the same client state:
   Zustand (small, selectors), Jotai (atoms that derive from each other),
   Redux Toolkit (a large team or an existing Redux app); Pinia in Vue;
   a `$state` object in a `.svelte.ts` module in Svelte 5; signals in
   Angular.

```ts
type EditorState = {
  selected: string[];
  tool: "select" | "draw" | "text";
  select: (ids: string[]) => void;
  setTool: (tool: EditorState["tool"]) => void;
};

export const useEditor = create<EditorState>()((set) => ({
  selected: [],
  tool: "select",
  select: (ids) => set({ selected: ids }),
  setTool: (tool) => set({ tool }),
}));

// Each component subscribes to what it reads, and re-renders only when that changes
const tool = useEditor((s) => s.tool);
const count = useEditor((s) => s.selected.length);
```

## 4. The URL

For list and detail views the URL is the source of truth: a reload keeps
the view, Back undoes a step, a copied link opens the same screen.

- **What goes in it**: the record's identity in the path
  (`/orders/42`); view options in the search params
  (`?status=paid&sort=-date&page=3&q=kopi`); a tab, a step, an open
  detail panel.
- **What does not**: secrets, tokens, personal data (URLs leak through
  history, logs and the `Referer` header), large blobs (keep URLs well
  under 2,000 characters), and state nobody would share.
- **Push or replace**: push when the user navigates (a page, a tab, a
  record), so Back returns; replace while they type in a search box or
  drag a slider, so Back does not step through every keystroke.
- **Parse and validate**: params are strings from anyone; a bad
  `?page=abc` or unknown `?status=` falls back to the default, never
  crashes.
- **Defaults stay out of the URL**: `/orders`, not
  `/orders?page=1&sort=-date&status=all`.
- **A change of filter resets the page** to the first.
- **Typed text** lives in local state for instant feedback, and is written
  to the URL debounced (about 300ms).

Libraries: nuqs (Next.js, React Router, TanStack Router and plain React,
through its adapters), TanStack Router's `validateSearch`, SvelteKit's
`page.url.searchParams` with `goto(url, { replaceState: true, keepFocus:
true, noScroll: true })`, Vue Router's `route.query` with
`router.replace({ query })`. In Next.js 15 and later, a server page
receives `searchParams` as a promise to `await`.

```tsx
import { parseAsInteger, parseAsString, parseAsStringLiteral, useQueryStates } from "nuqs";

const statuses = ["all", "open", "paid", "overdue"] as const;
const [{ status, page, q }, setView] = useQueryStates({
  status: parseAsStringLiteral(statuses).withDefault("all"),
  page: parseAsInteger.withDefault(1),
  q: parseAsString.withDefault(""),
});

// A new filter starts again at page 1 (null removes the key); defaults drop out of the URL
const showPaid = () => setView({ status: "paid", page: null }, { history: "push" });
const orders = useOrders({ status, page, q }); // the view state is also the query key
```

## 5. Storage that outlives the tab

- **`localStorage`**: preferences only (theme, density, a collapsed
  sidebar, chosen table columns). Never tokens (any injected script reads
  them, `frontend-security`) and never personal data. About 5 MB per
  origin, strings only, synchronous: small values.
- **Versioned keys**, parsed against a schema, with defaults when the value
  is missing, old or corrupt; every access in `try`, since storage can be
  full, disabled or blocked:

```ts
const KEY = "app.prefs.v2";
const Prefs = z.object({ density: z.enum(["comfortable", "compact"]), sidebar: z.boolean() });
type Prefs = z.infer<typeof Prefs>;
const DEFAULTS: Prefs = { density: "comfortable", sidebar: true };

export function loadPrefs(): Prefs {
  try {
    const parsed = Prefs.safeParse(JSON.parse(localStorage.getItem(KEY) ?? "null"));
    return parsed.success ? parsed.data : DEFAULTS;
  } catch {
    return DEFAULTS; // storage blocked, or a value that is not JSON
  }
}
```

- **`sessionStorage`**: a draft of a long form or a step of a wizard, per
  tab, gone when the tab closes; never card numbers or passwords.
- **IndexedDB** (`idb-keyval`, Dexie) for larger data kept offline.
- **Other tabs**: the `storage` event, or a `BroadcastChannel`, keeps them
  in step when that matters (signing out everywhere).
- **Server rendering**: storage does not exist on the server; read it after
  mount, or keep what the first paint needs (the theme) in a cookie, so
  the HTML is right from the start (`ui-themes`).
- Zustand's `persist` middleware takes a `version` and a `migrate`
  function; use them from the first release.

## 6. Flows with rules: a reducer or a state machine

Checkout, a wizard, an upload, a media player, a connection with retries:
when several booleans start guarding each other, name the states.

```ts
type Upload =
  | { status: "idle" }
  | { status: "uploading"; progress: number }
  | { status: "failed"; error: string }
  | { status: "done"; url: string };

type Event =
  | { type: "start" }
  | { type: "progress"; progress: number }
  | { type: "fail"; error: string }
  | { type: "finish"; url: string };

function upload(state: Upload, event: Event): Upload {
  switch (event.type) {
    case "start": return state.status === "uploading" ? state : { status: "uploading", progress: 0 };
    case "progress": return state.status === "uploading" ? { ...state, progress: event.progress } : state;
    case "fail": return { status: "failed", error: event.error };
    case "finish": return { status: "done", url: event.url };
  }
}
```

- A `url` exists only when done and an `error` only when failed: the
  impossible combinations cannot be written.
- XState (v5, `setup(...).createMachine(...)`, `useMachine`) when there
  are many states, timers, parallel regions or guards, or when a diagram
  helps the team agree.

## 7. Re-renders

- Measure first: React DevTools Profiler, with "Highlight updates when
  components render". Fix what is slow, not what re-renders.
- Keep fast-changing values (pointer position, input text, scroll) local,
  or in a ref when nothing renders from them.
- Split contexts by how often they change; read stores through selectors
  (Zustand's `useShallow` when a selector returns an object or array).
- Move state down into the part that uses it, or pass heavy children as
  props, so a parent's update does not re-render them.
- With the React Compiler (1.0 since 2025) enabled, memoisation is
  automatic; do not add `useMemo` and `useCallback` by hand without a
  measurement.

## 8. Server state is never copied

Copying a response into a store makes two sources of truth: the copy goes
stale after a refetch, and every mutation must remember to update both.

- Read server data from its query hook wherever it is needed; the cache
  deduplicates, so three components asking cost one request.
- Shape it with `select` in the query, or compute it during render.
- The signed-in user comes from a query (`useMe()`), invalidated on sign-in
  and role change, not copied into Redux.
- A form editing a record takes it as `defaultValues` (reset with a `key`
  when the record changes); the form owns the edits until submit.

## Check it

- Set filters, sort and a page, then reload: the same view. Press Back:
  one step back, not one keystroke back. Open the copied URL in a private
  window and sign in: the same screen.
- Try `?page=abc&status=bogus`: defaults, no crash, no error in the
  console.
- Change a preference, reload, open a second tab: kept, or synced, as
  designed.
- Profiler: typing in a field re-renders the field, not the page.
- Search for effects that copy state:
  `grep -rnE "useEffect\(\(\) => \{?[[:space:]]*set[A-Z]" src`.
- `preview` a list URL with its params set (and `login` behind a
  sign-in): the page opens in that state.

## Avoid

API responses copied into a store; filters, tabs and pages that reset on
reload; a derived array kept in sync by an effect; booleans that can
contradict; one giant context for everything; a global store for state one
component uses; tokens or personal data in `localStorage` or the URL;
unversioned storage keys read without a `try`; every keystroke pushed to
history; memoisation sprinkled without a measurement.
