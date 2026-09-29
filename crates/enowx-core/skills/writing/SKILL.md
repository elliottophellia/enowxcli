---
name: writing
description: "Writing words people read: the specific thing instead of claims, words to drop, sentences, interface copy (buttons, labels, errors, empty states, confirmations), page copy, notifications and emails, alt text and link text, numbers and dates, words that translate well, documentation and voice. Read before writing a page's copy, an interface's text or a document."
---

# Writing a person would sign

Generated prose gives itself away: "Unlock the power of", "seamless", "In
today's fast-paced world", three adjectives where one fact belongs, numbers
nobody measured, a cheerful closing line. This is how to write so that every
sentence tells the reader something.

## 1. Say the specific thing

- Name what the product does, for whom, with what result, in the reader's
  words. "Sends a reminder when an invoice is 7 days overdue" says more than
  "Streamline your billing workflow".
- Evidence over claims. With no real number, customer or quote, there is no
  statistic, logo or testimonial: say what the product does instead.
- Never invent facts, names, figures, quotes or features. A placeholder says
  it is one: `[Customer quote]`.
- Facts about the business that you were not given (its name, prices,
  opening hours, policies, what it accepts) are placeholders on the page
  itself, `[Business name]`, `[Price]`, not plausible guesses. A guess reads
  as a promise the business never made.

## 2. Words to drop

- Words that promise and say nothing: unlock, elevate, empower, seamless,
  leverage, robust, cutting-edge, next-generation, revolutionary,
  game-changing, powerful, effortless, supercharge, delve, journey,
  landscape, testament. Replace each with the concrete thing it stands for,
  or delete it.
- Stock moves: "In today's…" openings, "It's not just X, it's Y",
  rhetorical questions, "Let's dive in", "Here's the thing", and closings
  such as "I hope this helps".

## 3. Sentences

- Short sentences and plain words. Active voice with the actor named ("We
  update the price", not "The price is updated") unless the actor is unknown
  or beside the point.
- One idea per sentence. Cut filler: "in order to" is "to"; "it is important
  to note that" is nothing.
- No em dashes: use a comma, a full stop, a colon or brackets. No stacks of
  three adjectives, and no list of three out of habit: a list has as many
  items as the content has.
- Sentence case for headings and buttons. No capitals for emphasis and no
  emoji as decoration.

## 4. Interface copy

- Buttons say what happens: "Create invoice", "Send reminder", "Delete
  project". Not "Get started", "Learn more", "Submit", "Explore" or "Click
  here".
- A headline states the benefit or the fact, and the line under it backs it
  with a detail. No slogan that would fit any product.
- Labels are the nouns the user uses ("Due date"); help text says what to
  enter and why; a placeholder is an example, never the label.
- Errors say what happened, why if it is known, and what to do next, in the
  user's terms: "That card was declined. Try another card, or ask your bank."
  Not "Error 402" or "Something went wrong".
- Empty states say why the view is empty and the one action that fills it:
  "No invoices yet. Create your first one."
- Confirmations name the consequence: "Delete 3 files? This cannot be
  undone."
- One term per concept everywhere: "project", not "project" on one screen,
  "workspace" on the next and "space" on a third.
- Interface text goes through i18n, and a translation keeps the terms its
  audience keeps in English (API key, token, webhook, email): the `i18n`
  skill says what to translate and what to leave.

## 4a. Page copy

- A page reads in layers: the headline says the one thing, the line under it
  backs it with a detail, each section heading says its point (not
  "Features"), and the body gives the evidence. Someone who reads only the
  headings still gets the argument.
- Lead with the reader's problem or outcome, then how the product gets
  there. One idea per section; paragraphs of two to four sentences; lists
  when items are parallel.
- Numbers, names and examples from the real product. Where they are
  missing, a visible placeholder, never a plausible invention.
- The call to action repeats the page's one action in the same words each
  time.

## 4b. Notifications and email

- A subject line or title that says what happened ("Invoice INV-104 was
  paid"), not "Update from Acme".
- The first sentence carries the news; the action is one button that names
  it; the reason the person received the message and how to change that
  are at the end.
- Transactional messages stay transactional: no marketing inside a password
  reset.
- Push notifications are short, specific and rare; never "We miss you".

## 4c. Alt text, link text and labels for assistive technology

- Alt text says what the image shows that matters here, in a sentence
  ("Chart: sign-ups doubled after the March launch"), not "image of". A
  decorative image gets `alt=""`.
- Link text makes sense alone: "Read the pricing details", not "click here"
  or a bare URL; icon buttons have a name that says the action ("Close
  dialog").

## 4d. Numbers, dates and small things

- Numerals for numbers ("3 files"), the reader's locale formats for dates,
  times, currency and thousands (through `Intl`, not hand-written), units
  always, and ranges written "5 to 10" in prose.
- Time relative when recent ("2 hours ago"), absolute when it matters
  ("Due 12 March, 17:00").
- Sentence case, no full stop on headings, buttons or single-line labels; a
  full stop on sentences in help text and errors.
- Plural forms handled by the i18n library, never "1 file(s)".

## 4e. Words that translate

- Whole sentences as single strings, never glued from pieces ("You have" +
  count + "items"): word order differs between languages.
- No idioms, puns or cultural references in interface text; they do not
  survive translation.
- Room for text to grow by 30 to 40% in other languages; no text baked into
  images.
- One term per concept, kept in a glossary when the product has one.

## 5. Documentation

- Answer the reader's first question first: what this is and how to run it,
  on the first screen.
- Show, then explain: a working command or example before the paragraph
  about it.
- Describe what the code does now, read from the code, not guessed from
  names. Mark anything you could not check.
- Structure follows the content: no "Features, Benefits, Conclusion"
  template, no section with nothing to say. A changelog says what changed
  for the user; a commit message says what changed and why.

## 6. Voice

- Match the product's voice where it has one (existing copy, a style guide).
  Otherwise: direct, warm and unhurried, like a colleague who knows the
  subject.
- No hype, no false modesty, no apologising for the product, and an
  exclamation mark only for a genuine moment.

## 6a. Inclusive and plain

- Plain language a reader in a hurry understands: common words, the
  reader's terms, jargon only where the audience uses it, acronyms spelled
  out once.
- People first, no assumptions about gender (they, you), ability, age or
  culture; examples with a range of names.
- No blame in errors ("That code has expired", not "You entered a wrong
  code").

## 7. Before you call it done

Read it aloud. Delete every sentence that would fit another product, every
word that adds nothing, and every claim you cannot back. Check every button,
error and empty state says what happens or what to do.
