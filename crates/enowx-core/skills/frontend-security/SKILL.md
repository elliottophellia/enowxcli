---
name: frontend-security
description: "Frontend security: safe rendering and XSS, sanitising user HTML and markdown, a Content Security Policy, where tokens live, CSRF with cookies, public environment variables, third-party scripts, redirects, postMessage, iframes and clickjacking, and dependency hygiene. Read before rendering user content, handling auth on the client, or adding a script."
---

# Frontend security

The generated frontend renders a comment's HTML with
`dangerouslySetInnerHTML`, puts a user's website into an `href` unchecked,
keeps the access token in `localStorage`, calls a paid API with the
secret key from a `VITE_` variable, follows whatever `?next=` says after
sign-in, and loads six marketing tags from other domains with full access
to the page. Each is one line, and each hands the page to an attacker.
Every rule here is backed by the server (`backend-security`,
`backend-auth`): the browser is the attacker's machine. The whole
frontend is in `frontend`.

## 1. Rendering safely

Frameworks escape text by default: `{value}` in React, `{{ value }}` in
Vue, `{value}` in Svelte, interpolation in Angular. The holes are the
sinks that take HTML, code or URLs as they are:

| Stack | Sinks |
|---|---|
| React | `dangerouslySetInnerHTML`; `href`, `src`, `action`, `formAction` built from user input |
| Vue | `v-html`; `:href` and `:src` from user input; templates compiled from user strings |
| Svelte | `{@html}`; `href` and `src` from user input |
| Angular | `bypassSecurityTrustHtml` and the other `bypassSecurityTrust*` calls (`[innerHTML]` is sanitised) |
| The DOM | `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `document.write`, `srcdoc`, `eval`, `new Function`, `setTimeout` and `setInterval` with a string |

- Data inlined into a server-rendered page (`<script>window.__STATE__ =
  ...</script>`, JSON-LD) is escaped so a `</script>` in a value cannot
  close the tag: `JSON.stringify(data).replace(/</g, "\\u003c")`, or a
  serialiser built for it (devalue, `serialize-javascript`).
- Trusted Types (`require-trusted-types-for 'script'`) makes DOM sinks
  refuse plain strings where supported; phase it in with report-only.

## 2. User HTML and markdown

- Sanitise with DOMPurify (in the browser; `isomorphic-dompurify` on the
  server) against an allow-list, when rendering:

```ts
import DOMPurify from "dompurify";

DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A") {
    node.setAttribute("rel", "noopener noreferrer nofollow ugc");
    node.setAttribute("target", "_blank");
  }
});

export const cleanHtml = (dirty: string) =>
  DOMPurify.sanitize(dirty, {
    ALLOWED_TAGS: ["p", "br", "strong", "em", "a", "ul", "ol", "li", "blockquote", "code", "pre", "h2", "h3"],
    ALLOWED_ATTR: ["href", "title"],
  });
```

- Markdown: `react-markdown` renders no raw HTML by default; if a project
  adds `rehype-raw`, `rehype-sanitize` must follow it. `marked` output is
  unsanitised HTML: run it through DOMPurify. `markdown-it` keeps `html:
  false` unless there is a reason.
- Rich text editors (TipTap, ProseMirror, Lexical) store structured JSON:
  render it through the editor's schema, not as stored HTML.
- The server sanitises too wherever it renders or sends the HTML (emails,
  other clients); stored text is kept as written.

## 3. URLs from users

Profile links, "website" fields, redirects, image URLs: parse, then allow
only the schemes you expect.

```ts
export function safeHref(input: string): string | undefined {
  try {
    const url = new URL(input, window.location.origin);
    return ["http:", "https:", "mailto:"].includes(url.protocol) ? url.href : undefined;
  } catch {
    return undefined; // not a URL at all
  }
}
```

- This refuses `javascript:`, `data:` and `vbscript:` in any case or
  spacing. Images from user URLs (tracking, mixed content) go through your
  image proxy, or an allow-list of hosts in `img-src`.

## 4. A Content Security Policy

- Start with `Content-Security-Policy-Report-Only` and a report endpoint,
  watch the reports for a week or two against real traffic, then enforce.
- A strict policy with a nonce per response:

```text
Content-Security-Policy:
  script-src 'nonce-{random}' 'strict-dynamic' https: 'unsafe-inline';
  object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self';
  report-uri https://example.com/csp-reports; report-to csp
Reporting-Endpoints: csp="https://example.com/csp-reports"
```

- `'strict-dynamic'` lets scripts carrying the nonce load others. Browsers
  that support it ignore `https:`, and those that support nonces ignore
  `'unsafe-inline'`: both are there only for old browsers. Keep
  `report-uri` beside `report-to`, which not every browser sends to.
- The nonce: at least 128 random bits, new for every response. Static
  pages cannot have one: hash their inline scripts (`'sha256-...'`).
- `frame-ancestors` and reporting only work as a header, not in a
  `<meta>` tag. Add `connect-src`, `img-src` and `font-src` lists as the
  page needs; styles are a smaller risk, so tighten scripts first.
- Next.js sets it in middleware (`proxy.ts` from Next 16, `middleware.ts`
  before), and applies the nonce to its own scripts; pages then render
  dynamically:

```ts
// proxy.ts (before Next 16: middleware.ts, exporting a function named middleware)
import { NextResponse, type NextRequest } from "next/server";

export function proxy(request: NextRequest) {
  const nonce = btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(16))));
  const dev = process.env.NODE_ENV === "development" ? " 'unsafe-eval'" : ""; // React debugs with eval in dev
  const csp = `script-src 'nonce-${nonce}' 'strict-dynamic'${dev}; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`;
  const headers = new Headers(request.headers);
  headers.set("x-nonce", nonce);
  headers.set("Content-Security-Policy", csp);
  const response = NextResponse.next({ request: { headers } });
  response.headers.set("Content-Security-Policy", csp);
  return response;
}
```

## 5. Where tokens live

- The session is a cookie set by the server: `HttpOnly`, `Secure`,
  `SameSite=Lax` (or `Strict`), ideally named with the `__Host-` prefix
  (which requires `Secure`, `Path=/` and no `Domain`). Scripts never see
  it; injected code can act while the page is open, but cannot carry the
  session away.
- Never an access or refresh token in `localStorage`, `sessionStorage`,
  IndexedDB or a readable cookie: any script on the page reads them, and a
  compromised dependency or tag is a script on the page.
- A single-page app signing in with OAuth uses a backend-for-frontend: the
  server holds the tokens and gives the browser a session cookie. A token
  that must live in the browser stays in memory (a module variable),
  short-lived, renewed through an `HttpOnly` cookie, PKCE for the flow.
- Signing out ends the server session and clears client caches and user
  data held in memory (`queryClient.clear()`).

## 6. CSRF

- With cookie sessions, every state-changing request needs a defence.
  `SameSite=Lax` stops cookies on cross-site POSTs, but not from a sibling
  subdomain (same site), and not on top-level GETs: so no GET ever changes
  state.
- Add one of: the framework's CSRF token; a custom header on API calls
  (`X-CSRF-Token`), which a cross-site form cannot send; or a server check
  of `Origin` or `Sec-Fetch-Site`. Next.js server actions compare `Origin`
  with the host by themselves. CORS is not a CSRF defence.

## 7. Public environment variables

- Every `VITE_`, `NEXT_PUBLIC_`, `PUBLIC_`, `NUXT_PUBLIC_` and
  `EXPO_PUBLIC_` value is in the bundle, readable by anyone
  (`frontend-architecture`). A call that needs a secret key (an AI API, a
  payment provider's secret key, email sending) goes through your server,
  which holds the key, checks the user and limits the rate.
- Everything shipped is public, including feature flags, internal URLs and
  admin route names: permissions are enforced on the server.
- Source maps: `build.sourcemap: "hidden"` in Vite, uploaded to the error
  tracker and not served, when the code should not be browsable; that hides
  nothing from a determined reader, so no secret is ever in client code.

## 8. Third-party scripts

- Each one runs with the page's full power (the DOM, form fields, readable
  cookies): fewer is safer, and faster (`frontend-performance`). Analytics
  and marketing tags load after consent where the law requires it (the
  EU's GDPR and ePrivacy rules, Indonesia's UU PDP).
- Static files from a CDN at a fixed version carry Subresource Integrity
  (`integrity="sha384-..." crossorigin="anonymous"`, the hash from
  `openssl dgst -sha384 -binary lib.js | openssl base64 -A`), or are
  self-hosted. In 2024 the polyfill.io domain changed hands and served
  malicious code to every site that still loaded it.
- A tag manager lets whoever publishes it inject any script: restrict who
  can, and keep the CSP in force. Card details go into the payment
  provider's hosted fields (Stripe Elements and the like), never your own
  inputs; PCI DSS 4.0 also wants the scripts on payment pages inventoried.

## 9. Redirects, links and messages

- `?next=` and `?returnTo=` are checked before use, or sign-in becomes an
  open redirect to a phishing page:

```ts
export function safeReturnTo(value: string | null, fallback = "/"): string {
  if (!value || !value.startsWith("/") || value.startsWith("//") || value.startsWith("/\\")) return fallback;
  const url = new URL(value, window.location.origin);
  return url.origin === window.location.origin ? url.pathname + url.search + url.hash : fallback;
}
```

- `target="_blank"` links get `noopener` by default in current browsers;
  add `noreferrer` when the URL must not leak. No tokens or personal data
  in URLs: they end up in history, logs and `Referer` headers.
- `postMessage`: the receiver checks `event.origin` against an exact
  origin and validates `event.data` with a schema; the sender names the
  target origin, never `"*"` for anything that matters:

```ts
window.addEventListener("message", (event) => {
  if (event.origin !== "https://checkout.example.com") return;
  const message = CheckoutMessage.safeParse(event.data);
  if (message.success) handleCheckout(message.data);
});
frame.contentWindow?.postMessage({ type: "init", orderId }, "https://checkout.example.com");
```

## 10. Iframes, clickjacking and headers

- Untrusted content in an iframe gets `sandbox`, starting empty (every
  restriction on) and adding only what it needs; never `allow-scripts`
  together with `allow-same-origin` for content from your own origin,
  since it can then remove its own sandbox. User-supplied HTML and SVG are
  served from a separate domain, or as downloads.
- Your pages inside other sites' frames: `frame-ancestors 'none'` (or
  `'self'`, or the named partners), plus `X-Frame-Options: DENY` for old
  browsers.
- The host sends, once for the whole site: `Strict-Transport-Security`,
  `X-Content-Type-Options: nosniff`, `Referrer-Policy:
  strict-origin-when-cross-origin`, a `Permissions-Policy` turning off what
  the site does not use (`camera=(), microphone=(), geolocation=()`), and
  `Cross-Origin-Opener-Policy: same-origin` (or `same-origin-allow-popups`
  when sign-in uses a popup).
- Uploads: client checks of type and size are for quick feedback; the
  server validates the content (`backend-files`). Previews made with
  `URL.createObjectURL` are revoked when done.

## 11. Dependencies

- The lock file is committed and CI installs from it (`pnpm install
  --frozen-lockfile`, `npm ci`); `pnpm audit --prod` or `npm audit
  --omit=dev` and `osv-scanner scan source -r .` run in CI; Renovate or
  Dependabot propose updates, grouped, with a few days' delay for new
  releases.
- A new dependency is a new author with access to your users: check its
  maintainers, age, downloads and install scripts. Worms spread through
  npm in 2025 by stealing tokens from install scripts; pnpm 10 runs no
  dependency's install script unless it is allowed
  (`onlyBuiltDependencies`), and `minimumReleaseAge` holds back versions
  published minutes ago.

## Check it

```sh
grep -rnE "dangerouslySetInnerHTML|v-html|\{@html|innerHTML|insertAdjacentHTML|document\.write|new Function|eval\(" src
grep -rnE "localStorage\.(set|get)Item\([\"'](token|access|refresh|jwt|auth)" src
pnpm build && grep -rlE "sk_live_|sk_test_|PRIVATE KEY" dist/ || echo "no secret in dist"
curl -sI https://staging.example.com | grep -iE "content-security-policy|strict-transport|x-content-type|referrer-policy|x-frame"
```

- Every hit of the first search is sanitised or justified in a comment.
- Type `<img src=x onerror=alert(1)>` and `javascript:alert(1)` into each
  field whose value is shown again: nothing runs, the link is refused.
- `?returnTo=//evil.example` after sign-in lands on the home page.
- The CSP ran report-only first, with no report left unexplained; Google's
  CSP Evaluator and MDN's HTTP Observatory review the headers.

## Avoid

Unsanitised HTML in any sink; user URLs in `href` unchecked; tokens in web
storage; a secret behind a public prefix; state changed by a GET; CSRF
left to CORS; a CSP with `'unsafe-inline'` and no nonce, or one set only
in a `<meta>` tag; scripts from CDNs without integrity; `postMessage` to
`"*"` or from any origin; `returnTo` followed blindly; a sandbox with
`allow-scripts` and `allow-same-origin` on your own origin; user SVG served
from your domain; audits that nobody runs.
