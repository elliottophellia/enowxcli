# TUI grid redesign

The interface is a grid of aligned boxes: one place computes every edge, text
starts on the same two columns everywhere, and the layout changes at fixed
breakpoints rather than drifting with each resize. This records what was
decided, the numbers behind it, and how it is tested.

## Decisions

Each was chosen from rendered mockups.

| Question | Decision |
| --- | --- |
| Structure | Aligned rounded boxes, no outer window frame, no header box |
| Side column | A SESSION card always on screen, then one tabbed detail card |
| Narrow terminal (< 100 columns) | Side column hidden; its figures move to the status bar |
| Wide terminal | Full width; nothing capped or centred |
| Interface language | English throughout |

## The grid

`Grid::new` in `ui/chrome.rs` is the only code that decides where a box goes.
The old layout let the header, sidebar, composer and footer each pick their
own insets, and no two ended on the same column.

| Region | Rule |
| --- | --- |
| Status bar | The last row, full width, text inset two columns each side |
| Side column | 40 columns at 100–159 wide, 52 at 160+; hidden below 100 wide or 12 tall |
| Gap between columns | One column |
| SESSION card | Dropped below 18 rows; its figures then join the status bar |
| Main column | Chat box, then the command palette (only while `/…` is typed), then the composer |
| Composer | 1–8 rows of text plus its edges, never more than half the column |

Both columns end on the row directly above the status bar.

**Text edges.** Inside every box the border is column 0, then two columns and
one row of padding on every side (`chrome::padded`, `PAD_X = 2`, `PAD_Y = 1`;
less when a box is too small to afford it). Markers (`▌` user, `✓ ✗ ›` tool
state, `↳` handover, `✻` thinking, `❯` prompt, `›` selection) sit on column 3
and text starts on column 5, in the transcript, the composer, the palette and
every overlay. Nothing touches a wall, above, below or at either side. A tool
row's metric is flush right in one column, and the `▸`/`▾` chevron has a fixed
column of its own at the far right, blank when there is nothing to open.

`overlay` takes the rows of content and adds the border and padding itself,
so no popup computes its own height. The transcript leaves the last block's
trailing blank rows out of its scroll range; they doubled the bottom padding.

**Boxes.** `panel_box` draws every box: rounded, `theme.border`, panel fill.
`box_title` sets a title into the top edge as `╭─ TITLE ─`, and `box_hint` sets
keys or a pager into the bottom edge as `╰─ keys ─╯`. Box titles are uppercase
labels (SESSION, COMMANDS, THEME); tabs are title case because they are
navigation. The chat box's title is the project and the session, which replaced
the header box: four rows of chrome that printed one word.

## Contrast

`theme.border` was 1.22–1.31:1 against `panel` in all five themes, so no box
outline could be seen. It is now about 2.4:1, mixed from each theme's own panel
and muted colours, and stays below muted text (4.8:1 and up):

| Theme | Border | vs panel | vs composer fill |
| --- | --- | --- | --- |
| obsidian_ice | 72, 83, 98 | 2.42 | 2.26 |
| neo_acid | 68, 86, 74 | 2.41 | 2.22 |
| chrome_void | 79, 79, 97 | 2.42 | 2.28 |
| oled_stealth | 74, 78, 87 | 2.40 | 2.26 |
| classic | 80, 86, 100 | 2.41 | 2.23 |

Separation inside boxes is still done with colour and weight, not rules.

## Chat content

`transcript::GUTTER` reserves the two marker columns. Blocks without a marker
render `GUTTER` narrower and shift onto the text column: `indented()` for
assistant text, notices, reasoning and system lines; `gutter_from()` for an
opened tool body. Lines are pushed straight into the block's own list, so the
row numbers recorded for click targets stay correct.

- **Reasoning** is `✻ thinking`, then its text dimmed and italic. There is no
  accent-filled band.
- **Code blocks** show their language on the first row, on the same `│` bar as
  the code. They have no `┌`/`└` caps.
- **An edit** shows `+N -M` as its metric. The diff has no header row of its
  own, because the tool row above already names the file and opens it.
- **Todo and tree bodies** start on the text column.
- **A handover** is `↳ from → to · reason`. It is no longer a rule across the
  transcript, and the `handoff` call that caused it is not drawn as well.
- **A delegation** is one row from start to finish: `› delegate fe … working`
  while it runs, `✓` or `✗` when it reports. The brief is one click away. The
  report lands under the row, its `DONE / CHANGED / VERIFIED / NEXT` fields as
  a label column and a value column. The router's `delegate` call is not
  drawn, since the row says the same. A resumed session rebuilds the same row
  from the report message and the session's delegation records, and lists the
  sub-agents again so their branches still open.

### Markdown

`ui/markdown.rs` renders with pulldown-cmark (CommonMark plus GFM tables,
strikethrough, task lists and alerts) into a small block tree, then draws it:

| Construct | On screen |
| --- | --- |
| Spacing | One blank row between blocks, never two, none leading or trailing |
| Newline in a paragraph | Kept, as chat interfaces keep it. A line of 60+ columns continued in lower case is taken for hard-wrapped prose and joined |
| Headings | H1 accent bold underlined, H2 accent bold, H3 accent2 bold, H4+ bold |
| Lists | `•`/`◦` by depth, numbers right-aligned to the widest, `☑`/`☐` tasks; continuation rows under the text; loose lists keep their blank rows |
| Quotes | A `│` bar on every row, text dimmed; alerts (`> [!WARNING]`) name themselves in their colour at full strength |
| Code | Language label row, highlighted lines on the tinted band, long lines soft-wrap with `│↳`, tabs expanded |
| Tables | Rounded borders, alignment honoured, cells wrap; a rule under each row once any row wraps. Too narrow to keep ordinary words whole: stacked `header: value` rows |
| Links, images, HTML | Link label only; image alt text dimmed; comments hidden, `<br>` breaks, formatting tags dropped, `Vec<String>` kept as text |

Containers render their children narrower and prefix every row, so nesting
composes and no row is wider than the panel. Wrapping measures display width
and breaks at a space, between wide characters, then after `/` or `-` in a long
path. Control bytes are dropped before they can reach the terminal.
`tests/markdown_render.rs` covers each row of the table and checks every
construct at every width from 10 to 90.

```sh
cargo run -q -p enowx-tui --example mdpreview -- 72        # the gallery
cargo run -q -p enowx-tui --example mdpreview -- 40 notes.md
```

## Side column

- **SESSION card:** context (percent and a bar that turns red at 85%), tokens,
  cost, tool calls (with failures in red), and a TypeSafe trim row once
  something has been trimmed.
- **Detail card:**
  - Tabs `Agents · Tools · Skills · Log` sit in its top edge. The selected tab
    is bold and in the accent colour, and each name is a click target.
  - Keys: Alt+1–4 or F1–F4 select a tab, and F6/F7 open Log. A new delegation
    selects Agents.
  - The pager sits in the card's bottom edge.
- **Roster:** names flow several to a row. The active agent carries `›` as well
  as the accent colour, because colour alone disappears under NO_COLOR.

## Overlays

`chrome::overlay` draws every popup:
- a centred box with an accent edge, because it has the keyboard;
- its title in the top edge and its keys in the bottom edge;
- content on column 2.

`pickers::selectable` gives every list the `›` marker and a background-only
selection band. The band sets only the background, so the name and description
colours inside the row survive. Lists whose rows carry an on/off dot (skills,
MCP) use the dot as the marker instead. The MCP list now shows each server's
command or URL, which was stored but never drawn.

**Command palette (Ctrl+P).** Rows read as actions, without the `/` the inline
list uses: `New session`, `Compact context`. They sit under Session, Agents &
models, Context, View and App. Typing drops the headings and ranks the
matches: label prefix, then word prefix, then contains, then summary. The list
scrolls with the selection, the search row counts the rows, and rows take
clicks.

**Mouse.** With a window open the wheel moves through it as its arrow keys do,
wherever the pointer is; it used to scroll the transcript behind every window
but two. Over the side column it turns the card's pages; over the inline
command list it moves that selection. Moving a selection is throttled so one
trackpad flick moves one row; the transcript is not. Pickers take their click
targets from the list's scroll offset, so a click on a scrolled list picks the
row under the pointer.

## Testing

`TestApp` gained helpers that cut the screen by the area the app recorded while
drawing, not by searching for titles or border glyphs. Helpers that searched
for a title broke every time the decoration changed.

| Helper | What it gives a test |
| --- | --- |
| `side_column` | The side column's rows |
| `main_column` | The chat column's rows |
| `status_bar` | The status bar row |
| `side_area` | Where the side column was drawn |
| `row_bold` | Which cells of a row are bold |

`tests/layout_chrome.rs` and `tests/chrome_layout.rs` assert the grid:
- walls run unbroken;
- both columns end above the status bar, and nothing runs into it;
- the breakpoints behave as specified;
- the pager sits in the edge;
- the selected tab and the active agent are marked;
- the keys survive a narrow window.

Four of these were broken on purpose and went red before being restored.

Galleries render the real app with `TestBackend`:

```sh
cargo run -q -p enowx-tui --example snapshot -- 120 36          # a session
cargo run -q -p enowx-tui --example snapshot -- 100 60 rich     # every block kind
cargo run -q -p enowx-tui --example snapshot -- 100 14 busy     # short: no SESSION card
cargo run -q -p enowx-tui --example popups -- settings          # any overlay
```

## Bugs found on the way

- **The diff read backwards and word emphasis never showed.** On a tie,
  `lcs_diff` preferred the deletion while walking backwards, so once reversed
  each addition came before the line it replaced. `pair_replacements` expects
  deletion-then-addition, so no line was ever paired. The pairing tests built
  their op lists by hand and never ran the real diff.
- **Multi-line delegation tasks were glued into one run of text** in the
  sidebar. Only the first line is shown now.
- **The welcome screen pointed at `/sessions`,** which does not exist. The
  command is `/resume`.
- **The TypeSafe row in the SESSION card read "1 results"** and ran past the
  card.
- **The SESSION card counted 1,175 tool calls for a run that made 8.** The
  replay that draws a sub-agent's branch also bumped the counters, and it runs
  every half second while a branch is watched.
- **Agent tests wrote their sessions into the user's own store:** 372 of 430.
- **The instruction files and the skill list reached the model twice** on
  every call.

## Open

- The panel and canvas fills differ by only 1.04–1.06:1, so boxes read by their
  outlines rather than by their fill.
- The older galleries (`headerstyles`, `composerstyles`, `sidebarstyles`,
  `sidebarlayouts`) show layouts that have since been retired.
