---
name: ui-stack-next
description: "Building with Next.js (App Router): server and client components, layouts, next/font, next/image, next/link, metadata, loading and error files, build checks. Read when the project uses Next.js."
---

# Next.js (App Router)

React rules apply (`ui-stack-react`); these are Next's own.

## Server first

- Components are server components by default. `"use client"` goes on the
  small interactive leaves (a menu button, a form), never at the top of a
  page to make it all client-side.
- Fetch data in server components, close to where it is used.

## Structure

- `app/layout.tsx`: `<html lang="…">`, the fonts, the header and footer
  that every page shares. Route groups (`app/(marketing)/`, `app/(app)/`)
  for different frames.
- Each route segment that loads data has `loading.tsx` (a skeleton of the
  real layout), `error.tsx` (what failed and a retry), and the app a
  `not-found.tsx`.
- Every link in the navigation points at a route that exists.

## Built-ins, not substitutes

- Fonts with `next/font` (`next/font/google` or `next/font/local`),
  exposed as CSS variables; never a `<link>` or `@import` to Google Fonts.
- Images with `next/image`: `width` and `height`, or `fill` with `sizes`;
  `priority` on the one image that is the largest thing on first view.
- Internal links with `next/link`.
- Metadata with `export const metadata` or `generateMetadata`: a title and
  description per page.

## Styling

The project's method: Tailwind, CSS Modules, or global CSS with tokens in
`app/globals.css`. Tokens once, in one place.

## Check

`npm run build` runs the type check and lint; fix what it reports. `npm run
lint` for lint alone. Look at the result with `npm run build && npm run
start` (or `npm run dev`) through the `preview` tool, which starts and stops
the server.
