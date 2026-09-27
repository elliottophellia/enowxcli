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

**Text edges.** Inside every box the border is column 0 and padding column 1.
Markers (`▌` user, `✓ ✗ ›` tool state, `↳` handover, `✻` thinking, `❯` prompt,
`›` selection) sit on column 2 and text starts on column 4, in the transcript,
the composer, the palette and every overlay. A tool row's metric is flush right
in one column, and the `▸`/`▾` chevron has a fixed column of its own at the far
right, blank when there is nothing to open.

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
  transcript.

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

## Open

- The panel and canvas fills differ by only 1.04–1.06:1, so boxes read by their
  outlines rather than by their fill.
- Markdown renders a single newline inside a paragraph as a line break. This
  predates the redesign and was left alone.
- The older galleries (`headerstyles`, `composerstyles`, `sidebarstyles`,
  `sidebarlayouts`) show layouts that have since been retired.
