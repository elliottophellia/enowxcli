---
name: ui-part-pagination
description: "How to build pagination and loading more without the generated look. Read before building or reworking one."
---

# Pagination and loading more

One part of an interface. The principles (direction, spacing, type, colour,
icons, states, accessibility) are in the `ui` skill, the measures in
`ui-layout`.

- Build: numbered pages when position matters (search results, records);
  a "Show more" button for feeds; infinite scroll only when nothing needs
  the footer, and never without keeping the scroll position on return.
  - On the server, always, once a list can pass a couple of hundred rows:
    the page, the sort and the filters go into the query (`LIMIT` with a
    cursor or an offset, backed by an index), and the response carries the
    rows and either the total or whether there is more.
  - Cursor pagination (`?after=<id>`) for long or growing lists, where an
    offset gets slow and skips rows as new ones arrive; page numbers with a
    total ("1 to 25 of 1,284") when the reader jumps around.
  - A page size of 25 by default, with 50 and 100 to choose from in tables;
    the page, the size, the sort and the filters in the URL, so back and
    reload return to the same page.
  - While the next page loads, the current one stays on screen, dimmed, and
    the controls show the wait; changing a filter goes back to page one.
- Avoid: fetching every row to paginate in the browser, a page count that
  jumps as you page, pages that lose the filters.
