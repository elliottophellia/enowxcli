---
name: i18n
description: "Putting every piece of user-facing text through internationalisation, and translating it well: what to translate, what to leave as the audience says it (API key, token, email), keys, plurals, dates, numbers and money, choosing and switching the locale, right-to-left layouts, fonts, sorting, emails in the reader's language, and checking with pseudo-localisation. Read before writing or changing any text a user sees, in any language."
---

# Internationalisation and translation

Every piece of text a user sees goes through the project's i18n layer, and
is translated the way people who use that language at work actually speak.
A machine translation of every word ("Kunci API" for "API key") reads as
wrong to exactly the people the translation was for.

## 1. Always through i18n

- No user-facing string written straight into a component, a template or an
  API response: labels, buttons, headings, help text, errors, empty states,
  emails, notifications, page titles, `alt` and `aria-label` text.
- Use the project's i18n setup when it has one: its library, its file
  layout, its key style. Read an existing locale file before adding keys.
- A new project with an interface starts with it, in the stack's usual
  library: `next-intl` for Next.js, `react-i18next` for React, `vue-i18n`
  for Vue, Paraglide or `svelte-i18n` for Svelte, the framework's own for
  mobile (`strings.xml`, `Localizable.strings`, `flutter_localizations`). A
  static page gets a small dictionary module (`locales/en.json`,
  `locales/id.json`) and one function that looks keys up.
- The default locale is the one the user asked for; English when they did
  not say. Add a second locale only when asked, but keep every string in the
  catalogue so adding one later is a translation, not a rewrite.
- Messages and logs meant for developers (exceptions, debug logs, CLI
  internals) stay in English and outside the catalogue.

## 2. Keys and messages

- Keys name the place and the meaning, in English, dotted by feature:
  `billing.invoice.sendReminder`, `auth.signIn.title`, `common.actions.save`.
  Not the English sentence as the key, not `text1`.
- One message per whole sentence, with placeholders:
  `"{count} invoices are overdue"`. Never build a sentence by joining
  pieces, since word order differs between languages.
- Plurals through the library's plural rules (ICU `{count, plural, one {…}
  other {…}}` or its equivalent), never `count === 1 ? "file" : "files"` in
  code. Indonesian has no plural forms; English and many others do.
- Dates, times, numbers and money through `Intl` (or the platform's
  formatter) with the active locale: `28 Sep 2026` in English,
  `28 Sep 2026` or `28/09/2026` in Indonesian, `Rp 1.500.000`, not a
  hand-written format.
- Text can grow by 30 to 50% in translation: layouts do not depend on a
  word's length, and nothing is truncated without a way to read it whole.

## 3. What to translate, and what to leave

Translate the everyday words of an interface; leave the terms the audience
uses as they are, which in technical products is often English. Decide by
how the people who use the product talk, not by whether a translation
exists.

Leave as they are (in Indonesian, and in most languages for a technical
audience):

- Terms of the trade: API, API key, token, access token, webhook, endpoint,
  URL, email, password in many products, username, dashboard, deploy,
  commit, branch, pull request, repository, server, database, cache, cookie,
  plugin, template, framework, CLI, SDK, OAuth, SSO, 2FA, VPN, DNS, SSH.
- Brand and product names, and names of features that are proper nouns
  (Stripe Checkout, GitHub Actions, "Workspaces" when it is the product's
  own feature name).
- Code, commands, file names, keyboard keys (Ctrl, Enter, Esc), units (MB,
  ms, px).

Translate:

- Actions and everyday words: Save → Simpan, Cancel → Batal, Delete → Hapus,
  Settings → Pengaturan, Search → Cari, Sign in → Masuk, Sign out → Keluar,
  Create → Buat, Next → Lanjut, Back → Kembali, Loading → Memuat.
- Sentences around the kept terms: "Copy your API key" → "Salin API key
  kamu", not "Salin kunci API kamu"; "Webhook failed" → "Webhook gagal",
  not "Kait web gagal"; "Paste your access token" → "Tempel access token
  kamu".
- Pick one register and keep it: for Indonesian, "kamu" for a casual
  product, "Anda" for a formal one, never both.

When unsure whether a term is kept, look at how the leading products in
that language for the same audience say it, and follow the majority. Keep a
short glossary at the top of the locale folder (or in `DESIGN.md`) with the
decisions, so every screen says it the same way.

## 3a. Beyond the strings

- Choosing the locale: the user's saved choice first, then the URL (a
  `/id/` prefix or a subdomain for public pages, which search engines can
  index with `hreflang`, `frontend-seo`), then `Accept-Language`, then the
  default. A language switcher names each language in itself ("Bahasa
  Indonesia", "English"), never with flags.
- Right-to-left languages (Arabic, Hebrew, Persian): `dir="rtl"` on the
  root, CSS logical properties (`margin-inline-start`, `padding-inline`,
  `inset-inline-end`, `text-align: start`) instead of left and right,
  mirrored directional icons (arrows, chevrons) but not logos, media
  controls or charts.
- Fonts that cover the scripts in use (Latin extended, Cyrillic, CJK,
  Arabic, Devanagari), with fallbacks, and line heights that fit them.
- Sorting and searching with `Intl.Collator` (accents and case per locale);
  lists of names never sorted by code points.
- Money: the currency is data, not the locale; format it with the locale,
  convert it only with real rates.
- Names, addresses and phone numbers vary by country: one free-text name
  field unless the form truly needs parts, the address form by country, phone
  numbers with international parsing.
- Server-side text in the recipient's language: emails, notifications, PDFs
  and error messages shown to users use the user's stored locale, not the
  server's.

## 3b. The workflow

- New keys land in the default locale with the change; missing translations
  fall back to it visibly in development (a marker) and silently in
  production.
- Pseudo-localisation in development (accented, 40% longer strings) shows
  text left outside the catalogue and layouts that break when text grows.
- Translation files live in the repository or a translation service
  (Crowdin, Lokalise, Tolgee, Weblate) synced by CI; the glossary goes with
  them.
- Machine translation only as a draft, reviewed by someone who speaks the
  language and knows the product's terms.

## 4. Before you call it done

- Search the changed components for quoted user-facing text left outside
  the catalogue.
- Every key used exists in every locale file; no key left unused.
- Read each screen in each locale: terms kept where the audience keeps them,
  one register, nothing overflowing.
