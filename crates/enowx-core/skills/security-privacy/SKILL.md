---
name: security-privacy
description: "Handling personal data responsibly: knowing what is collected and why, minimising it, keeping it out of logs, analytics and error reports, retention and deletion, consent for tracking, encryption and access controls, third-party processors, data subject requests, and the basics of GDPR and similar laws (not legal advice). Read before building or reviewing features that collect, store or share personal data."
---

# Personal data, handled with care

The default version: a privacy policy copied from a template, an email
address in every log line, whole request bodies in the error tracker,
analytics firing before the cookie banner is answered, a "delete account"
button that sets a flag, and backups kept forever. This skill is how to know
what you hold, collect less, keep it out of the places it leaks, delete it
on time, ask for consent that means something, protect it, answer requests,
and meet the main laws in outline. It is not legal advice: a finding names
the risk and the rule it touches; the owner and their counsel decide.

## 1. Inventory first

- **What**: identifiers (name, email, phone, address, IP address, device,
  advertising and cookie ids, all personal data under GDPR), account and
  profile data, what users write or upload, location, payment data (ideally
  only the provider's token), government ids (NIK, passport, SSN), health,
  biometrics, children's data, staff data.
- **Where**: tables and columns, object storage (uploads, exports), caches,
  search indexes (Elasticsearch and Algolia keep copies), queues, logs,
  analytics and the warehouse, backups, emails sent, support and CRM tools,
  AI providers (prompts carrying user data) and vector stores.
- **Who**: the roles and staff with access, admin panels, support
  impersonation, contractors.
- **Which third parties**: every SDK and script (analytics, ads, session
  replay, chat widgets, error tracking), payment, email, SMS, hosting, AI.
- The result is a table (data, purpose, lawful basis, where kept, retention,
  recipients): the record of processing GDPR Article 30 asks for.

```sh
rg -n -i '\b(email|phone|mobile|birth|dob|address|postcode|passport|national_id|nik|ssn|tax_id|gender|ip_address|latitude|longitude|diagnosis|salary)\b' -g '*.sql' -g '*.prisma' -g '*schema*' -g '*models*' -g '*migrations*' -g '*entity*' .
rg -n -i 'gtag\(|googletagmanager|fbq\(|connect\.facebook\.net|hotjar|fullstory|logrocket|clarity\.ms|posthog|mixpanel|segment|amplitude|intercom|sentry|datadog|openai|anthropic' .
```

## 2. Collect less

- Only what the feature needs: an over-18 check rather than a birth date, a
  city rather than an address until something is shipped; optional fields
  optional in the schema, not only in the form.
- Nothing you can fetch again: the payment provider holds the card, you keep
  its token.
- Pseudonymise where the purpose allows (user ids, not emails, in events).
  A hashed email is pseudonymous, not anonymous: it is matched back by
  dictionary, and GDPR still treats it as personal data.
- IP addresses truncated for analytics (/24 for IPv4, /48 for IPv6).
- Production data in development or staging is a finding: synthetic or
  masked data instead.

## 3. Where personal data leaks

- **Logs**: whole request bodies, `Authorization` and `Cookie` headers,
  query strings, `logger.info(user)` dumping an object, ORM query logging
  with parameters in production. Log ids and mask the rest
  (`backend-observability`, `backend-security` section 7); redact in the
  logger itself (pino `redact`, structlog processors, Serilog destructuring
  policies, Logback masking).
- **URLs**: emails, names, tokens or search terms in paths and query strings
  end up in access logs, proxies, browser history, analytics page views and
  the `Referer` sent to third parties. Put them in the body; keep
  `Referrer-Policy: strict-origin-when-cross-origin` (the browser default),
  or `no-referrer` on sensitive pages.
- **Error trackers**: Sentry's `sendDefaultPii` left off (the default), a
  `beforeSend` scrubber, server-side scrubbing on; session replay masked
  (Sentry Replay masks all text and blocks media by default: keep it).
- **Analytics and ads**: event properties holding emails, names or free text;
  page URLs with personal data; ad pixels on pages about health, money or
  children; "advanced matching" that sends hashed emails from forms.
- **Session replay and chat widgets** record screens and keystrokes unless
  fields are masked.
- **Responses and the client**: APIs returning whole rows (`security-web`
  section 9); personal data in HTML comments, `localStorage` or bundles.
- **Exports and support**: CSV exports mailed around, dumps pasted into chat
  tools, backups copied to laptops, support tools showing full records.
- **AI features**: prompts with personal data sent to a provider. Check its
  retention and training terms, sign its data processing agreement, use
  zero-retention options where offered.

```sh
rg -n '(console\.log|logger\.\w+|log\.\w+|logging\.\w+|print)\(.*(req\.body|request\.(data|body|json)|headers|\buser\b|email|phone|password|token)' .
rg -n -i 'sendDefaultPii:\s*true|send_default_pii\s*=\s*True|maskAllText:\s*false|blockAllMedia:\s*false' .
```

## 4. Retention and deletion

- A retention period for each kind of data, written down and enforced by a
  scheduled job or a TTL (TTL indexes, bucket lifecycle rules, log retention
  settings; CloudWatch Logs keeps everything forever unless told otherwise).
- Deletion reaches everything the inventory lists: caches, search indexes,
  analytics, processors (through their deletion APIs), stored files. Backups
  age out within their window, and a restore re-applies deletions made since
  (keep a deletion log).
- A "delete account" that only sets `deleted_at` forever is a finding; soft
  delete with a purge after a stated period (30 days is common) is fine when
  the policy says so.
- Legal duties override deletion (tax law keeps invoices for years): keep
  only what the law requires, and restrict who can see it.
- Anonymised analytics aggregate or drop identifiers entirely; a table keyed
  by hashed emails is not anonymous.

## 5. Consent for tracking

- In the EU and UK, non-essential cookies and similar storage (analytics,
  ads, session replay, most A/B tools) need consent before they are set or
  read; strictly necessary ones (session, CSRF, load balancing, the consent
  record itself) do not.
- A real choice: "Reject all" as easy as "Accept all" on the first layer, no
  pre-ticked boxes, no colours that hide the refusal, withdrawal as easy as
  consent, and each consent recorded (what, when, which notice version).
- Nothing fires before the choice: scripts gated by the consent tool; Google
  Consent Mode v2 defaults set to `denied` before tags load (required for
  ad features in the EEA since March 2024).
- Global Privacy Control (`Sec-GPC: 1`, `navigator.globalPrivacyControl`)
  honoured as an opt-out where law requires it (California, Colorado and
  others), with a "Do Not Sell or Share" link where CCPA applies.
- Check it in a fresh browser profile: the network requests and cookies
  before the banner is touched, then after "Reject all". From a terminal,
  `curl -sI https://site | grep -i '^set-cookie'` shows what the server
  sets before any choice.

## 6. Protecting it

- Encryption in transit and at rest, and field-level encryption for the
  sensitive columns (`security-crypto` section 9).
- Least-privilege access; MFA for staff and admin tools; production data
  reached through audited paths, not a shared database password; access
  reviews every quarter (`security-auth`, `security-infra`).
- Reads of sensitive records logged (who viewed which record, when), and
  support impersonation recorded.
- Tested backups and a written incident plan.

## 7. Rights requests

- Under GDPR: access (a copy), rectification, erasure, restriction,
  portability (a machine-readable export), objection (absolute for direct
  marketing), and no solely automated decisions with legal or similar
  effects without safeguards.
- Deadlines: one month under GDPR, extendable by two for complex requests;
  45 days under CCPA, extendable by 45.
- Identity verified in proportion: a signed-in session or a confirmation to
  the account's email, not an ID card scan by default.
- Self-service covers most of it: export and delete in account settings.
  The export spans every system in the inventory; the deletion cascades to
  rows, files and processors.

## 8. Processors and transfers

- A data processing agreement (GDPR Article 28) with every processor
  (hosting, email, analytics, error tracking, support, AI APIs), and a
  published list of sub-processors kept current.
- Transfers out of the EEA or UK need a mechanism: an adequacy decision
  (such as the EU-US Data Privacy Framework, for certified companies) or
  standard contractual clauses with a transfer assessment.
- Residency promised in a contract or required by law is checked against
  every processor's region, backups and support tools included.

## 9. Special data

- **Special categories** (GDPR Article 9: health, biometrics, genetics,
  ethnic origin, political opinions, religion, union membership, sex life or
  orientation) and criminal records (Article 10): an explicit legal
  condition, a DPIA, tighter access and encryption.
- **Children**: parental consent below the digital age of consent (13 to 16
  across EU member states; under 13 for COPPA in the US); the UK Children's
  Code for services children are likely to use; no profiling or ads by
  default.
- **Cards**: kept out of your systems with the provider's hosted fields or
  checkout; the CVV never stored; PCI DSS 4.0 requires an inventory and
  tamper detection of the scripts on payment pages.
- **Health in the US**: HIPAA for covered entities and their business
  associates (a BAA with each vendor); tracking pixels on health pages have
  drawn enforcement.
- **Precise location and government ids** (NIK, passport numbers): masked
  in the interface, encrypted, reads logged.

## 10. The laws in outline

Not legal advice; which laws apply depends on where the people are, not only
where the company is.

| Law | Applies to | Notable |
|---|---|---|
| GDPR, UK GDPR | People in the EU, EEA or UK | A lawful basis per purpose; rights answered in a month; breaches reported to the authority within 72 hours; fines up to 4% of global turnover or 20 million euros |
| ePrivacy rules | Cookies and similar storage, EU and UK | Consent before non-essential storage |
| CCPA and CPRA | California residents, businesses above thresholds | Notice at collection, opt-out of sale and sharing, GPC, 45-day answers |
| Other US state laws | Virginia, Colorado, Connecticut, Texas and more | Similar rights; opt-in for sensitive data |
| UU PDP (Law 27 of 2022) | Indonesia, fully in force since October 2024 | GDPR-like rights; breaches reported to the people affected and the authority within 3 x 24 hours |
| LGPD | Brazil | GDPR-like rights and a national authority |

## 11. The policy, breaches and design

- The privacy policy matches the code: every SDK in the manifests and every
  script on the pages appears in it, with the retention and processors the
  code actually has. A mismatch is a finding.
- Breaches: contain, assess, record every one (GDPR Article 33(5)), notify
  within the deadlines above when there is risk to people, and tell them
  directly when the risk is high.
- A DPIA before high-risk processing: special data at scale, systematic
  monitoring, profiling with significant effects, automated decisions.
- Privacy by default (GDPR Article 25): the most private option preselected
  (profiles private, marketing opt-in).

## Check it

- The inventory table exists and every row has a purpose, a retention period
  and a deletion path.
- A test account's email, searched for across logs, analytics events, error
  reports and search indexes after a normal session, appears only where the
  inventory says; after deleting the account, it is gone or scheduled to go.
- A fresh browser sets no non-essential cookie and calls no tracker before
  consent, nor after "Reject all".
- Every third party in the code is in the policy and has an agreement.

## Avoid

Collecting "just in case"; emails, tokens or bodies in logs and URLs;
`sendDefaultPii` or unmasked session replay; trackers before consent; a
reject button hidden a layer down; a delete that only sets a flag; hashing
called anonymisation; production data in staging; retention "forever" by
default; a privacy policy that describes some other product; presenting any
of this as legal advice.
