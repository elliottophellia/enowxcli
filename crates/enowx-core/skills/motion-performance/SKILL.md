---
name: motion-performance
description: "What motion costs and how to keep it light: compositor-only properties, loops that stop, frame caps, no scroll handlers, framework rendering, canvas, choosing a library, and measuring jank and weight. Read before any continuous animation, any animation library, or when a page feels heavy."
---

# Light motion

Motion that stutters is worse than none, and motion that keeps a laptop's
fan running on a page nobody is looking at is a cost the visitor pays without
seeing it. A page can move a great deal and stay light: the enowx.ai remake
put motion in every section for about 4 kB gzipped and no library. This is
how.

## 1. What each property costs

A frame goes through three stages: layout (where every box is), paint (the
pixels of each layer) and composite (the layers put on screen by the GPU).
An animation costs every stage it forces on every frame.

| Cost | Properties | Use |
|---|---|---|
| Cheap: composite only | `transform` (translate, scale, rotate), `opacity` | all movement and fading |
| Moderate: paint | `color`, `background-color`, `border-color`, a small `box-shadow`, `clip-path`, strokes of a small SVG (`stroke-dashoffset`), `background-size` on a button | short colour changes, drawings, small reveals |
| Expensive: layout, or paint of a large area | `width`, `height`, `top`, `left`, `margin`, `padding`, `font-size`, `inset`, `gap`, `grid-template-*`; a large `box-shadow`; `filter: blur()`; `backdrop-filter` over something that scrolls | not animated, replaced with transform |

- Move with `translate`, grow with `scale` (and scale the children back when
  they must not stretch), reveal with `clip-path` or a mask only on small
  elements.
- A size change that has to happen (an accordion) animates
  `grid-template-rows` from `0fr` to `1fr`, or `height` with
  `interpolate-size: allow-keywords` where it is supported, on one element at
  a time (`motion-interface`).
- A shadow that grows on hover: fade in a pseudo-element that carries the
  larger shadow; do not animate the shadow itself.
- `transition: all` animates layout properties the moment one of them
  changes, and every property someone adds later. Name the properties.

## 2. will-change and layers

- `will-change: transform` puts an element on its own layer before it moves.
  Browsers already do that while a transform or opacity animation runs, so
  add it only to an element that stutters on its first frame, just before it
  moves, and take it off after.
- Never on many elements and never in a global rule: each layer costs GPU
  memory, and hundreds of them make scrolling slower, not faster.
- A sticky or fixed element with `backdrop-filter` repaints its blur on every
  frame of scrolling: one such element at most, with a small radius (8 to
  10px).

## 3. No scroll handlers

- Reveals use one IntersectionObserver for the whole page, which sets
  `data-shown` once and stops watching (`motion-reveal`). An observer per
  element works, but costs more and loses the stagger.
- Effects tied to the scroll position (a progress bar, a sticky story) use
  CSS scroll-driven animations (`animation-timeline: scroll()` or `view()`)
  behind `@supports`, with a still fallback.
- When a listener cannot be avoided: `{ passive: true }`, read in the
  listener, write in one `requestAnimationFrame`, and never read layout after
  writing in the same frame.
- A `wheel` or `touchmove` listener that calls `preventDefault` to drive a
  custom smooth scroll is scroll-jacking: it fights the platform, breaks
  momentum and costs every frame (`motion-comfort`).
- `pointermove` for an effect that follows the cursor: passive, mouse only,
  eased inside one rAF, and stopped when the pointer leaves the window.

## 4. Loops that stop

Every continuous animation (a canvas field, a demo clock, a CSS loop) stops
when nobody can see it:
- Off screen: an IntersectionObserver on the element pauses it. A CSS loop is
  paused with `animation-play-state: paused` from a class the observer sets.
- In a hidden tab: check `document.visibilityState === "visible"` in the loop,
  or pause on `visibilitychange`.
- Unmounted: every interval, animation frame, observer and listener is
  cleared in the effect's cleanup.
- One clock per scene: one interval or one animation frame loop drives the
  whole scene, never one timer per element.
- High refresh screens: `requestAnimationFrame` fires 120 times a second on
  a 120Hz screen. Cap it, or make movement depend on elapsed time rather than
  on the number of frames, or everything moves twice as fast and costs twice
  as much.
- A text interface that changes in steps needs about ten frames a second,
  not 60: an interval of 90 to 100ms (`motion-demo`).

```ts
let frame = 0;
let drawn = 0;
const loop = (now: number) => {
  frame = requestAnimationFrame(loop); // scheduled first: an error then shows every frame
  if (now - drawn < 12) return;        // at most 60 frames a second
  drawn = now;
  draw(now);
};
// started and stopped by an IntersectionObserver and visibilitychange;
// cancelAnimationFrame(frame) in the cleanup
```

## 5. Frameworks

- Never set state 60 times a second on a component that renders a large
  tree: every tick renders the page again. Put the moving part in its own
  component; better, move it with CSS, or write a CSS variable or a
  transform through a ref.
- A scene is a function of time (`motion-demo`): one state value, `t`,
  updated about ten times a second, and everything derived from it. A phase
  other components follow is passed up only when it changes.
- A flag that must be set before the first paint (the page's `data-motion`,
  a hidden starting state) goes in `useLayoutEffect`, so nothing flashes in
  its final state and then starts again.
- Keep timers away from forms, editors and anything holding what a person
  typed: below them or beside them, never above.
- Attributes set on reveal (`data-shown`, `--rv`) are not props, so React
  leaves them alone on the next render.
- Vue and Svelte follow the same rules; their transition helpers run on
  transform and opacity when written that way (`motion-stacks`).

## 6. Canvas

- Device pixel ratio capped at 1.5: a 3x phone otherwise fills nine times the
  pixels of 1x for a background.
- Counts by area, capped: nodes = `Math.max(36, Math.min(130, area / 11000))`.
- Batch: many lines as a few `Path2D` objects grouped by opacity (four
  buckets), one `stroke()` each; no style change or `save` and `restore` per
  shape.
- No `shadowBlur`, no gradient per shape, no `filter` on the canvas: glow is
  expensive and rarely earns its place.
- Distance checks exit early on each axis before `Math.hypot`.
- A heavy scene (thousands of points, physics) moves to an `OffscreenCanvas`
  in a worker; 3D goes to WebGL, and only when 3D is the product.
- Resized by a ResizeObserver that seeds the scene again; under reduced
  motion each resize draws one still frame.

## 7. Choosing a library

Default: none. CSS keyframes, transitions and a few lines of
IntersectionObserver cover reveals, drawings, state changes and demos.
- **Motion (framer-motion)**: when the work needs layout animation (shared
  elements between routes, lists that reorder), gestures, or springs driven
  by a pointer. `LazyMotion` with the `m` component starts at about 5 kB and
  loads `domAnimation` (about 15 kB more) when needed; plain `motion`
  components pull the full bundle, over 30 kB gzipped. Respect the setting
  with `MotionConfig reducedMotion="user"` or `useReducedMotion`.
- **GSAP**: long, precisely sequenced timelines and scroll stories CSS cannot
  express; only the plugins used; timelines killed on unmount.
- **Lottie or Rive**: when a designer supplies the file (`motion-drawings`);
  loaded lazily, paused off screen.
- **Three.js and WebGL**: only when 3D is the product.
- **View Transitions API**: page and state changes with no library at all
  (`motion-interface`).
- **Avoid**: AOS and libraries like it that fade every element up on scroll
  (a template, and content left hidden when they fail); smooth scrolling
  libraries (Lenis, Locomotive) that replace the native scroll; cursor
  followers, magnetic buttons and text scrambles.
- A library comes in with its reason written in DESIGN.md's `Motion:` line
  and its measured weight in the report.

## 8. A budget

- Motion code on a marketing page: about 5 to 10 kB gzipped, CSS and
  JavaScript together, measured with the build's own output before and
  after.
- In an application motion adds no library unless the work in section 7
  needs one.
- The enowx.ai remake as a reference: CSS from 22.12 to 24.54 kB gzipped
  (2.4 kB more) and JavaScript from 70.47 to 72.20 kB (1.7 kB more, with six
  icons removed), for entrances, reveals, six drawings, a nav marker, copy
  following the demo and a canvas capped at 60 frames.
- Put the before and after in the final report.

## 9. Measuring

- `preview` with `motion: true` is the agent's way of seeing it: the
  timeline of what animates on load and on scroll, loops that never stop and
  whether they run on off screen, animations of layout or paint-heavy
  properties, `transition: all`, scripts changing inline styles many times a
  second, long animation frames while scrolling, layout shift, content still
  hidden after scrolling through, scroll, wheel and pointer-move listeners,
  and the page again under reduced motion. Fix each finding, or say why it
  stays.
- For a person, or when a finding needs a cause: Chrome's Performance panel,
  recording a scroll with the CPU throttled 4x. Look for frames over 50ms,
  layout (purple) and paint (green) inside each frame, and the script that
  runs.
- Long animation frames (`PerformanceObserver` on `long-animation-frame`)
  name the script behind a slow frame; `layout-shift` names what moved.
- A loop's cost: its frames a second at the top of the page and after
  scrolling away. The second number is 0.
- Weight from the build output, as in section 8.

## Avoid

Animating width, height, top, left, margin or padding; `transition: all`;
`will-change` on everything; scroll listeners for reveals; an animation frame
loop that never stops; timers still running after unmount; state set 60
times a second on a page component; canvases at the full device pixel ratio;
`shadowBlur` and blur filters in motion; a library for what CSS does; the
full Motion bundle for one fade; AOS, Lenis, Locomotive and cursor
followers; more than one element with `backdrop-filter`; calling motion
smooth without a measurement.
