---
name: backend-integrations
description: "Calling other services and being called by them: one client per service, timeouts, retries with backoff, receiving and sending webhooks safely, payments, email, storage and AI APIs, keys and sandboxes. Read before integrating a third-party API, a payment provider, an email service or webhooks."
---

# Other services

One part of the backend; the whole is in the `backend` skill.

## 1. One client per service

- Each service gets one module (`integrations/xendit.ts`,
  `payments/midtrans.py`) that builds requests, signs them, parses replies
  into your own types and maps the provider's errors to yours. The rest of
  the code calls that module, never the provider's URLs directly.
- The provider's official SDK when it is maintained; otherwise the project's
  HTTP client. Not a second HTTP library.
- Keys and endpoints from config (`backend-observability`), with the sandbox
  in development and tests and the live one only in production; a live key
  never in the repository, a test, or a log.

## 2. Every call has limits

- A timeout on every outbound request: a few seconds to connect, 10 to 30
  seconds in total unless the provider documents longer. A call with no
  timeout can hold a request, a worker and a connection forever.
- Retry only what is safe and may succeed: a timeout, a lost connection,
  `429` and `5xx`, with exponential backoff and jitter, honouring
  `Retry-After`, three attempts in a request and more in a job. A `POST` is
  retried only with an idempotency key the provider honours.
- A request does not wait on a slow provider when it need not: queue the
  call (`backend-jobs`) and let the client see the state.
- When a provider is down, the rest of the app keeps working, and the
  feature that needs it says so clearly.

## 3. Receiving webhooks

- Verify the signature on the raw body, before parsing, with the provider's
  method (an HMAC with the shared secret, compared in constant time), and
  refuse a timestamp older than about five minutes. An unsigned or invalid
  call gets `400` or `401` and does nothing.
- Deduplicate by the event id: providers deliver at least once, often twice.
  Store the ids handled, and do nothing for one already seen.
- Answer `2xx` within a few seconds, then do the work in a job. A slow
  handler gets retried and runs twice.
- Do not trust the payload's amounts or state blindly for anything that
  matters: fetch the object from the provider's API, or compare with your own
  records.
- The endpoint takes no session, cookie or CSRF check (the signature is the
  check), and is rate-limited like the rest.

## 4. Sending webhooks

- Sign each delivery (an HMAC of the timestamp and body in a header), include
  an event id and type, retry with backoff for a day or so, and let the
  receiver see and replay failed deliveries.
- Never deliver to an address a user entered without the SSRF checks in
  `backend-security`.

## 5. Payments

- The provider's hosted checkout or SDK; card numbers never touch your
  server.
- Amounts in the smallest unit, in the currency the provider expects, created
  on the server from your own prices, never from the client's total.
- The order is paid when the provider's webhook (verified, section 3) or an
  API check says so, not when the browser returns to a "success" page.
- An idempotency key on every create or capture, so a retry cannot charge
  twice.

## 6. Email and messages

- A transactional provider (Resend, Postmark, SES, Mailgun) through its SDK,
  sent from a job, never inside the request.
- Templates with the text through i18n (the `i18n` skill) in the recipient's
  language, a plain-text part, and the sender's domain set up with SPF, DKIM
  and DMARC.
- In development, a catcher (Mailpit, the provider's sandbox), never real
  inboxes.

## 7. Files and AI APIs

- Files in object storage (S3, R2, GCS, Supabase Storage) with uploads and
  downloads through short-lived signed URLs; the app keeps the key and the
  metadata, not the bytes.
- Model APIs: stream long answers, set a timeout and a token limit, keep the
  key on the server, cap spend per user, and treat the output as untrusted
  input (never run it as code or SQL).

## Avoid

Provider URLs called from all over the code; a call with no timeout;
retrying a payment `POST` with no idempotency key; a webhook handled without
checking its signature, handled twice, or handled slowly inside the request;
"payment successful" from the redirect alone; live keys in tests; an email
sent inside the request.
