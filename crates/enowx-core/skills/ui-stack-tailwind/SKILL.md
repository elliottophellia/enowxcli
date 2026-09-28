---
name: ui-stack-tailwind
description: "Building with Tailwind CSS without the Tailwind-default look: theme tokens, no arbitrary values, variants, merging classes, dark mode, states. Read when the project uses Tailwind."
---

# Tailwind CSS

Tailwind makes the default look very cheap: `bg-gradient-to-r
from-purple-500 to-pink-500`, `rounded-2xl shadow-2xl`, `backdrop-blur` on
every card. Use it to apply the product's decisions, not the framework's.

## Theme first

- The palette, fonts, radii and shadows live in the theme, named by role:
  `bg-surface`, `text-muted`, `border-line`, `bg-accent`, not
  `bg-purple-600` scattered through the app.
  - Tailwind v4: `@theme { --color-ink: …; --color-accent: …; --font-display:
    …; --radius-md: …; }` in the CSS.
  - Tailwind v3: `theme.extend` in `tailwind.config`, pointing at CSS
    variables (`colors: { ink: 'var(--ink)' }`) so a dark theme swaps the
    variables, not every class.
- Use the scale. Arbitrary values (`w-[347px]`, `text-[#1f2937]`,
  `mt-[13px]`) are the exception with a reason, not the way to match a
  mock-up.

## Components

- A class group that repeats becomes a component (a React, Vue or Svelte
  component, or a partial), not a copy and not `@apply` everywhere. Keep
  `@apply` for base element styles.
- Variants with `cva` (class-variance-authority) or a small map keyed by
  variant; merge a caller's classes with `cn()` (clsx and tailwind-merge)
  so a caller's `px-6` replaces the default rather than both applying.
- Keep class order readable: layout, box, type, colour, state, responsive.
  The Prettier plugin sorts them when the project uses it.

## Responsive and states

- Mobile-first: unprefixed for the phone, `sm:` `md:` `lg:` as it widens.
  Rearrange (`flex-col md:flex-row`, `order-*`), do not duplicate markup
  behind `hidden md:block`.
- Container: `mx-auto max-w-6xl px-4 sm:px-6`, text `max-w-prose`.
- States on every control: `hover:`, `focus-visible:outline-2
  focus-visible:outline-offset-2`, `disabled:opacity-50
  disabled:cursor-not-allowed`, `aria-[invalid=true]:border-danger`. Never
  `outline-none` without a `focus-visible` style.
- Dark mode by the class or data-attribute strategy, through the variables,
  so most elements need no `dark:` class at all.
- `motion-safe:` on animations.

## The default look to avoid

Gradient text (`bg-clip-text text-transparent`), purple-to-pink gradients,
`rounded-full` on everything, `shadow-2xl` cards, `backdrop-blur` on
several surfaces, `uppercase tracking-widest` eyebrows over every heading.
`ui_check` finds them.
