---
name: ui-part-choices
description: "Choices: radios, checkboxes, switches, selects, comboboxes, segmented controls and choice cards; which to use when, labels and hit areas, groups and legends, defaults, states, the keyboard and ARIA patterns, libraries and phones. Read before asking a user to pick anything."
---

# Choices: select, radio, checkbox, switch

The generated version: a dropdown with two options, a switch inside a form
that only applies on Save, custom checkboxes the keyboard cannot reach, a
16px box that is the only clickable part, "Select an option" as the only
label, and a consent box ticked by default. Each kind of choice has a
control that fits it; this is the table to pick from, and how to build
each one so it is clicked, tapped and heard correctly. One part of an
interface: the principles (direction, spacing, type, colour, icons,
states, accessibility) are in the `ui` skill, the measures in `ui-layout`,
the form around them in `ui-part-forms`.

## 1. Which control

| The choice | Control |
|---|---|
| One of 2 to 6, worth seeing all at once | radio group |
| One of 7 to about 15 known options | native `select` |
| One of many (people, products, countries), found by typing | combobox with search |
| Any number of a few | checkboxes |
| Any number of many | multi-select combobox with chips, or checkboxes in a searchable list |
| On or off, applied immediately | switch |
| On or off inside a form saved later; an agreement | one checkbox |
| A view mode or a quick filter, 2 to 5 short options | segmented control |
| A visual pick: a plan, a theme, a layout | choice cards, radios underneath |

- In short: radios for one choice from up to five or six, visible at once;
  a native `select` for a longer list; a searchable combobox for long
  lists; checkboxes for any number of choices; a switch only for a setting
  that applies immediately.
- A dropdown with two options is a pair of radios or a segmented control. A
  yes or no question that must be answered is two radios, not a checkbox,
  so "not answered" differs from "no".
- A number the user knows (a quantity, a year) is typed, not picked from a
  list of a hundred.

## 2. Anatomy and hit areas

- Each has a label that toggles it when clicked: the input inside its
  `label`, or `for` and `id`. The whole row is the hit area, 44px tall on
  touch (padding on the label), 32 to 36px in dense desktop settings.
- Box and circle 16 to 20px, 8 to 12px from the label's text; the label at
  body size; a description under it, 13 to 14px in the muted colour, tied
  with `aria-describedby`.
- A group has a legend: `fieldset` and `legend` holding the question ("How
  should we deliver it?"), options stacked 8 to 12px apart; side by side
  only for two or three short ones.
- The borders of unchecked boxes and circles reach 3:1 against the
  background: they are how the control is seen.
- Native controls styled first: `accent-color: var(--accent)` colours a
  checked box, circle and range, and `color-scheme` matches them to the
  theme. Build custom ones only for what that cannot do, on the real input
  with `appearance: none`, or with a headless library.

## 3. States

- Checked, unchecked, and indeterminate for a "Select all" when some are
  chosen: `input.indeterminate = true` in script, `aria-checked="mixed"` on
  a custom one.
- Hover: the border one step stronger. Focus-visible: a 2px ring round the
  box, or round the whole card for choice cards.
- Disabled options say why beside them ("Express: not available for this
  address"), or are left out when the user never needs to know.
- Error: on the group, under the legend ("Choose a delivery option"), with
  the message tied to the group and `aria-invalid` on its inputs.
- Options loading (a combobox searching): a "Searching…" row in the list,
  not an empty list.

## 4. Behaviour and keyboard

- The keys for each pattern are in `frontend-accessibility`; prefer a
  headless library (React Aria, Radix, Headless UI, Ark UI, Bits UI, Melt)
  to a hand-built widget.
- Radios (APG radio group): Tab enters the group at the checked one, the
  arrow keys move and select, Tab leaves. Native radios sharing a `name`
  do all of it.
- Checkboxes: Space toggles; each is its own Tab stop.
- Switch: a `button role="switch"` with `aria-checked`, or `<input
  type="checkbox" role="switch">`; Space toggles (Enter too on a button).
  The label names the setting ("Email me when an invoice is paid") and
  does not change with the state. It applies at once, shows that it saved,
  and turns back with a message when saving fails. Never inside a form
  that applies on submit.
- Native `select`: phones open their own picker, which is the right one.
  Style the closed field; where supported (Chromium, since 2025),
  `appearance: base-select` lets the open list be styled, and other
  browsers keep their native list.
- Combobox (APG combobox with a listbox): Down opens and moves, Up moves,
  Enter picks, Escape closes then clears, typing filters; the input keeps
  focus and `aria-activedescendant` points at the highlighted option.
  Server search after 1 or 2 characters, debounced 150 to 300ms, "No
  results for [query]" when empty, the typed text kept. Libraries: React
  Aria `ComboBox`, Headless UI `Combobox`, Ark UI, Bits UI, Melt,
  Downshift's `useCombobox`; shadcn's combobox is its Popover with Command
  (cmdk).
- Multi-select: the chosen items as removable chips in the field
  (`ui-part-badges`), Backspace removing the last; or checkboxes in a
  popover, with a summary on the trigger ("3 selected").
- Segmented control: a radio group in markup and keys (arrows move and
  select); tabs only when it switches panels of content (`ui-part-tabs`).
  The selected segment gets a filled surface and weight.
- Choice cards: the card is the `label` of a visually hidden radio; the
  checked card gets a 2px accent border and a check icon
  (`:has(:checked)`), the focused one a ring (`:has(:focus-visible)`).

## 5. Defaults and words

- A default when most people choose it and it is safe (standard delivery,
  monthly billing); none when the answer deserves thought; never a
  pre-ticked box for consent or marketing, which is not consent under the
  GDPR.
- Options short, parallel and mutually exclusive, in a meaningful order:
  natural (S, M, L; dates), by frequency, or alphabetical for long lists
  with the likely one first (the user's own country). "Other" with a text
  field, or "None", when the list cannot be complete.
- A select's empty first option says what to do ("Choose a country"); a
  required choice without a default is checked on submit.

## 6. Phones, themes, motion

- Options stack; rows 44px tall; a segmented control fits the width or
  becomes a select; a long list opens as a full-height sheet with the
  search at the top (`ui-part-drawers`).
- Themes: `color-scheme` per theme so native controls follow; unchecked
  borders at 3:1 in both (`ui-themes`).
- Motion: a switch's thumb slides in 160 to 240ms, a check appears at once
  or within 100ms, a segmented indicator slides; none of it under reduced
  motion (`motion-interface`).

## 7. A sketch

```html
<fieldset class="choice-group">
  <legend>How should we deliver it?</legend>
  <label class="choice">
    <input type="radio" name="delivery" value="standard" checked aria-describedby="standard-note">
    <span>Standard delivery</span>
    <small id="standard-note">[Delivery time and price]</small>
  </label>
  <label class="choice">
    <input type="radio" name="delivery" value="express" disabled aria-describedby="express-note">
    <span>Express delivery</span>
    <small id="express-note">Not available for this address</small>
  </label>
</fieldset>
```

```css
.choice-group { display: grid; gap: 8px; margin: 0; padding: 0; border: 0; }
.choice-group legend { margin-block-end: 8px; font-weight: 600; }
.choice {
  display: grid; grid-template-columns: 20px 1fr; column-gap: 12px;
  align-items: center; min-block-size: 44px; cursor: pointer;
}
.choice input { inline-size: 20px; block-size: 20px; margin: 0; accent-color: var(--accent); }
.choice input:focus-visible { outline: 2px solid var(--focus); outline-offset: 2px; }
.choice small { grid-column: 2; font-size: 14px; color: var(--text-muted); }
.choice:has(input:disabled) { cursor: not-allowed; color: var(--text-muted); }
```

## Check it

- Keyboard only: Tab reaches each group once; arrows move through radios
  and segments; Space toggles checkboxes and switches; a combobox opens,
  filters, picks and closes with Escape.
- Click the label text: it toggles. Measure the radio and checkbox rows at
  360px (44px); `preview` lists selects, switches and segment buttons under
  44px, and controls without a name.
- A screen reader reads the legend with each option, the description, and
  the checked, mixed or pressed state.
- Toggle a switch with the network off: it turns back and says why.
- Search the code for `div`s with click handlers or `role="checkbox"`
  where a native input would do.

## Avoid

A dropdown with two options; a switch inside a form that only applies on
submit; a pre-ticked consent box; only the tiny box clickable; checkboxes
built from `div`s; a radio group with no legend; a hand-rolled combobox
without its keyboard pattern; options disabled with no reason; "Select an
option" as the only label; a segmented control used as tabs, or tabs used
as a filter.
