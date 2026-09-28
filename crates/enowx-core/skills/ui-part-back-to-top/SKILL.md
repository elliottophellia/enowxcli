---
name: ui-part-back-to-top
description: "How to build the back-to-top control every long page has, without the generated look. Read before building a page longer than about three screens (count them on a phone)."
---

# Back to top

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Use it for: getting back to the navigation from far down a long page.
  Every page longer than about three screens has one. Count the screens on
  a phone, where a page runs three to four times as tall as on a laptop, so
  nearly every landing page, article, portfolio and long list has one. A page
  of one or two screens does not.
- Build:
  - A link, not a script: `<a href="#top">`, with `id="top"` on `<body>` or
    on the first element of the page, never on the sticky header, which
    never leaves the screen and so is never scrolled back to. The link works
    without JavaScript, and it moves the keyboard's starting point to the
    top, so the next Tab reaches the skip link and the navigation.
  - A name: an up arrow from the product's icon set with
    `aria-label="Back to top"`, or the words, in the page's language
    ("Kembali ke atas").
  - Fixed at the bottom right, 16 to 24px from the edges plus the safe area,
    44 to 48px square, on the surface colour with a 1px line and the same
    radius as the other buttons; above the content, below menus and dialogs.
  - Hidden at the top, shown after the first screen: a passive scroll
    listener sets `hidden` while `scrollY < innerHeight`, and runs once on
    load. Without JavaScript it simply stays visible.
  - Smooth scrolling only for those who have not asked for less motion.
  - Clear of everything else fixed at the bottom: a bottom tab bar, a chat
    button, a cookie notice, a sticky "Book" button on a phone. Stack above
    them or take the other side; never cover a control or the footer's last
    line.
  - In an app shell whose content area scrolls on its own, the control sits
    in that area and returns it to the top (`main.scrollTo({ top: 0 })`),
    then moves focus to the area's heading.
- A sketch in plain HTML, CSS and JavaScript:

  ```html
  <body id="top">
    …
    <a class="to-top" href="#top" aria-label="Back to top">
      <svg aria-hidden="true" …>…</svg>
    </a>
  </body>
  ```

  ```css
  @media (prefers-reduced-motion: no-preference) {
    html { scroll-behavior: smooth; }
  }
  .to-top {
    position: fixed;
    right: calc(var(--space-4) + env(safe-area-inset-right));
    bottom: calc(var(--space-4) + env(safe-area-inset-bottom));
    z-index: 20;
    display: grid;
    place-items: center;
    inline-size: 48px;
    block-size: 48px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  /* `display: grid` would override the attribute's own display: none. */
  .to-top[hidden] { display: none; }
  ```

  ```js
  const toTop = document.querySelector(".to-top");
  const place = () => { toTop.hidden = scrollY < innerHeight; };
  addEventListener("scroll", place, { passive: true });
  place();
  ```

  In React, Vue or Svelte: the same link as a small component, the listener
  added on mount and removed on unmount.
- Check: the `preview` tool scrolls each page halfway down and reports a
  long page with no visible way back to the top.
- Avoid: a glowing gradient circle with a bouncing arrow; `href="#"`, which
  reads as a link to nowhere; a button that calls `scrollTo` and leaves the
  keyboard's focus at the bottom of the page; one on a page of two screens;
  one that covers the footer's links or a cookie banner's buttons; "SCROLL
  TO TOP" in capitals.
