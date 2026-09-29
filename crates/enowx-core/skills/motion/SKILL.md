---
name: motion
description: "Motion for interfaces: what should move and why, how much for this product (the motion dial), the motion tokens, the reveal system, comfort and reduced motion, what it costs, and how to check it. Read before adding any animation, transition or scroll effect beyond a hover state, and before judging one."
---

# Motion

Generated motion is recognisable from across the room: every block fades up
by 40px as it enters, cards bounce, a blob floats behind the hero forever,
numbers count up to figures nobody measured, and nothing can be clicked until
the last stagger lands. None of it says anything about the product. Motion
that belongs to a product acts out what the page says, stays out of the way
of reading, and costs almost nothing. This skill says what to move, how much,
and how to check it; the `motion-*` skills hold the detail.

## 1. What motion is for

Motion earns its place by doing one of these jobs. Name the job before
writing a keyframe.

- **Feedback**: the interface heard you. A button presses, a toggle slides,
  a saved row settles, a field that failed says so.
- **Orientation and continuity**: where something came from and where it
  went. A drawer slides from the edge it lives on, a menu grows from its
  button, a removed row leaves from its place and the list closes the gap,
  the nav marker slides to the section on screen.
- **Order and focus**: what to read first. The headline, then its line,
  then the action; the one thing that changed, not everything around it.
- **Cause and effect**: this led to that. A request is sent, then answered;
  work is handed to an agent, then comes back as a report.
- **Showing the product**: a short scripted demo of the real interface
  doing its job, when a still picture cannot show it (`motion-demo`).
- **Character**: a small signature the product is known by, used once.

Everything else is decoration. A page carries at most one ambient element
(a slow field behind the hero, a drifting texture); it stops when scrolled
away or when the tab is hidden, and it never sits at full strength behind
text a person is reading on a phone.

## 2. Read the page before moving it

Motion comes from the content, not from a library's presets. Before the
first keyframe, go through the page part by part and write down the sentence
each part says and the movement that would act it out. A part whose sentence
has no movement stays still.

From a launch page for an agent that builds software:

| The part says | The motion acts it out |
|---|---|
| "An agent that does the work, then shows its work." | The second line arrives a beat after the first, because of "then". |
| Four stages: agree, route, build, report | A line runs across each stage in order and marks it done; its steps appear after it. |
| The orchestrator hands work to specialists | A short light runs along the grid from the orchestrator's column to the others; each group's chips light up as it arrives. |
| A feature card | A small drawing acts the feature out: work sent down one of three routes, an answer picked from options, a report filled in row by row. |
| Waitlist steps 01, 02, 03 | The numbers roll up into place, one after another, as steps do. |
| The product window in the hero | A scripted session of the real interface, and the copy beside it lights up the phrase that names the step on screen. |

The same move on every block (fade up, stagger, fade up) is a template: it
says only that the page has an animation library. Two sections holding the
same kind of content may share a move; sections saying different things
should not.

## 3. How much: the motion dial

Pick one level from the product and the brief before designing any
movement, and hold it on every screen:

1. **Feedback only**: hover, press, focus and state changes. Tools people
   work in all day: dashboards, admin, editors, forms, settings.
2. **Entrances and transitions**: level 1, plus content entering as it
   scrolls into view and transitions between states and views. Product and
   marketing sites, docs home pages, portfolios, stores.
3. **Choreography**: level 2, plus a sequence per section, animated
   drawings, product demos and scroll storytelling. Launch pages, a
   product's front page, campaigns, a studio's own site.

Write it in DESIGN.md's `Motion:` line, with the decisions that follow from
it, so the next change keeps to them:

```markdown
Motion: dial 3. Blocks rise 12px and fade in once, as they scroll into view;
each section acts out what it says (motion.css); the hero plays a 16s
scripted session, paused off screen, with a pause control. Tokens --m-* in
motion.css. Reduced motion: final states, no smooth scroll, the session
rests on its report frame.
```

"Full motion" means dial 3, not permission to move everything: dial 3 still
means one focal movement at a time, small distances, and a page that can be
read while it moves.

## 4. The system

One set of tokens, one reveal mechanism, a sequence per block written in
CSS, and the final state as the base style.

### Tokens

Defined once, in the project's tokens file or a `motion.css`, and nothing
else used:

```css
:root {
  --m-ease-out: cubic-bezier(0.22, 0.61, 0.36, 1);     /* entrances, most state changes */
  --m-ease-in: cubic-bezier(0.55, 0.055, 0.675, 0.19); /* exits */
  --m-ease-in-out: cubic-bezier(0.65, 0, 0.35, 1);     /* travel from A to B, indicators, lines */
  --m-instant: 100ms;  /* press */
  --m-fast: 160ms;     /* hover, colour, tooltip */
  --m-base: 240ms;     /* menus, toggles, tabs */
  --m-panel: 320ms;    /* dialogs, drawers */
  --m-enter: 700ms;    /* content entrances */
  --m-draw: 1000ms;    /* rules and lines drawn across a width */
  --m-shift-sm: 6px;
  --m-shift: 12px;
  --m-shift-lg: 18px;
}
```

`motion-timing` says which one fits which movement and how to sequence them.

### The base style is the final state

Every element's normal CSS is its final, visible state. Motion is a layer on
top that starts somewhere else and returns to the base. Anything that stops
the motion (no JavaScript, print, reduced motion, a screenshot tool, a slow
script) then leaves the page complete.

### One reveal mechanism

- `data-reveal` on each block that enters: a section head, a grid, a card.
- One IntersectionObserver for the whole page sets `data-shown` on a block
  the first time it comes into view, then stops watching it. It also sets
  `--rv`, the block's order among siblings that arrived together, so a row
  staggers in reading order whatever the grid is at this width.
- `data-motion` on the page root, set before the first paint (a layout
  effect, or an inline script) and only when motion can run: reduced motion
  is not asked for and IntersectionObserver exists.
- Hidden starting states live only under `[data-motion]` inside
  `@media screen and (prefers-reduced-motion: no-preference)`. Without
  JavaScript, in print and with reduced motion, nothing is ever hidden.

`motion-reveal` has the observer (about thirty lines) and the CSS.

### A sequence per block, in CSS

Each block's sequence is keyframes plus delays computed from custom
properties: `--rv` from the observer, and indices written in the markup
(`--k` for an item in a list, `--n` for a stage). Keyframes with only a
`from` animate to the base style, so the final state is written once:

```css
@keyframes m-rise { from { opacity: 0; transform: translateY(var(--m-y, 12px)); } }

@media screen and (prefers-reduced-motion: no-preference) {
  [data-motion] .stages:not([data-shown]) li { opacity: 0; }
  [data-motion] .stages[data-shown] li {
    animation: m-rise var(--m-enter) var(--m-ease-out) backwards;
    animation-delay: calc(var(--n) * 420ms + var(--k) * 80ms);
  }
}
```

## 5. Comfort and cost, in brief

- **Comfort** (`motion-comfort`): distances of 6 to 18px, entrances of 0.5
  to 1s easing out, one focal movement at a time; no bounce, blinking,
  parallax on text or scroll-jacking. Reduced motion shows final states,
  rests loops on a still frame and turns smooth scrolling off. Content
  never depends on motion, and a loop beside content can be paused.
- **Cost** (`motion-performance`): move only `transform` and `opacity` (a
  stroke drawn in a small SVG too), one IntersectionObserver and no scroll
  handlers, every loop stopped off screen and in a hidden tab, canvas capped
  at 60 frames a second, no library for what CSS and thirty lines of script
  do. A page of motion fits in a few kilobytes.

## 6. Which skill for which job

- `motion-timing`: durations, distances, easing, springs, stagger,
  sequencing, loops.
- `motion-reveal`: blocks entering as they scroll into view, scroll-driven
  CSS, parallax and pinned sections, the nav marker, smooth anchor links.
- `motion-interface`: hover, press, focus, menus, tooltips, dialogs,
  drawers, toasts, tabs, accordions, lists, view transitions, loading,
  forms, charts and numbers.
- `motion-drawings`: SVG drawings and icons that act out an idea, drawn
  strokes, timed parts, replay on hover, Lottie and Rive.
- `motion-demo`: scripted product demos, held frames, copy that follows a
  demo, canvas fields, video loops.
- `motion-performance`: what is cheap, loops that stop, frame caps, canvas,
  libraries and when they earn their weight, measuring.
- `motion-comfort`: reduced motion, vestibular triggers, flashing, pausing,
  reading, interaction during motion.
- `motion-audit`: judging existing motion and the order to fix it in.
- `motion-stacks`: the same system in plain CSS and JavaScript, Tailwind,
  React with the Motion library, Next.js, Vue, Svelte and native mobile.

## 7. A worked case

The pre-launch page of an agent product, dark, with its content scrolling
inside one container rather than the window. Dial 3, because it is a launch
page and the user asked for full motion that stays calm and light.

- **Hero, on arrival**: the eyebrow fades up 8px; the headline's first line
  rises 18px over 900ms at 100ms; the second line, "then shows its work",
  follows at 440ms; the lede, the form and the buttons follow at 640, 760
  and 880ms. The clock icon in "Launching soon" winds once. The product
  window rises 16px at 280ms, and the session inside it starts 900ms after
  the window is on screen, so it does not play unseen. The canvas field
  behind fades in over 1.8s.
- **Hero, after arrival**: the copy follows the session. The phrase of the
  lede that names the step on screen ("asks what it needs to know", "hands
  each part to a specialist") brightens and gains an underline, and "then
  shows its work" brightens while the report is up. Colour only: nothing
  moves inside text being read.
- **Section titles**: the rule under each draws from the left over 1.1s;
  the title rises 12px, its line 8px.
- **Features**: each card's small drawing acts out its feature, then plays
  again on hover with a mouse. The text rises 6 to 8px.
- **Specialists**: a short light runs along the top edge of the grid from
  the orchestrator's column; the chips of each group arrive lit and settle
  to the rule colour, group by group, 45ms apart.
- **How it works**: per stage, a line fills across the top and fades, the
  square beside the stage's name fills when the line completes, then its
  steps slide in 6px; stages 420ms apart.
- **Waitlist**: the step numbers roll up inside their own line, 160ms apart.
- **Nav**: a pill slides between the links to the section crossing the
  middle of the screen.

The budget held: about 4 kB gzipped for all of it, one observer for the
reveals and one for the nav, no library, no scroll handlers, the canvas
capped at 60 frames a second (a 120Hz screen would otherwise draw twice as
often and move the field twice as fast), every loop paused off screen, and
reduced motion showing the final state with smooth anchor scrolling off.

Checking it caught four mistakes and left one open:

- An underline swept across a phrase as a growing background broke when
  the phrase wrapped onto two lines: the start of each line was underlined
  and the middle was not. It became a fade of the text colour and of a
  `text-decoration` underline, which follows the text across lines.
- The nav pill first appeared by sliding in from the nav's left edge, its
  starting position. It now fades in where it belongs and slides only
  between sections.
- The cards sat in a ruled grid (1px gaps showing the grid's background).
  Hiding a card showed the rule colour as a grey block, so the cards stay
  and their children animate.
- On phones the step numbers rolled up from below into view: their box was
  two grid rows tall. `align-self: start` made the clip one line high.
- Still open when this was written: the session loops beside the page's
  text for more than five seconds, and WCAG 2.2.2 asks for a way to pause it
  (`motion-comfort`). A loop is not finished until it has one.

## 8. Check it

- Run `preview` with `motion: true`. It reads the page's motion as text
  (you do not see its frames; the user can): the timeline of what moves on
  load, with start, duration, element, properties and distance; what starts
  as the page is scrolled through; loops that never stop and whether they
  run while scrolled away; layout or paint-heavy properties animated;
  `transition: all`; elements animated from JavaScript; long frames while
  scrolling; layout shift during reveals; content still hidden after
  scrolling through; scroll and pointer listeners; and the same page with
  reduced motion. `motion-audit` says what each finding means.
- Read the timeline against the plan: the focal element first, the primary
  action usable within about 1s, the load sequence done within about 2.5s,
  distances 6 to 18px, no block that never appears.
- Give scripted demos a held frame (`?t=4200` renders that moment still) so
  each moment can be looked at and checked (`motion-demo`).
- Look once with reduced motion on: every block present, nothing moving,
  loops resting on a still frame.

## Avoid

The same fade-up on every block; distances of 40px or more; bounce and
elastic easing; blinking, pulsing and glowing badges; blobs and orbs
floating forever; count-ups of invented numbers; typewriter headlines and
text scrambles; parallax on text; scroll-jacking and smooth-scroll
libraries; custom cursors and magnetic buttons; tilt cards; intro screens;
content hidden until an animation runs; loops that never stop; a new
animation library for what CSS does; motion that no sentence on the page
asks for.
