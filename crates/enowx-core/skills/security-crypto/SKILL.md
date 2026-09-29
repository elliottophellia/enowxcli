---
name: security-crypto
description: "Using cryptography correctly: vetted libraries only, the algorithms to choose for encryption, signatures, hashing, key exchange and passwords, randomness, nonces, key management and rotation, constant-time comparison, TLS and certificate validation, tokens, and the mistakes that break it. Read before writing or reviewing code that encrypts, signs, hashes or generates tokens."
---

# Cryptography that holds

The generated version: AES in ECB mode, or CBC with a key written in the
source and an IV of zeros; SHA-256 of the password; `Math.random()` for a
reset token; `==` on an HMAC; `verify=False` to make a certificate error go
away. Each compiles, passes its tests and protects nothing. This skill gives
the vetted libraries, the algorithm for each job, randomness, nonces, keys,
constant-time comparison, certificate checks, tokens, and the mistakes to
grep for. Password storage in review depth is in `security-auth`; secrets
handling in `security-secrets`.

## 1. Use what exists

Never an algorithm, protocol or "encryption" of your own (XOR, base64,
shifted characters, a clever scheme), and never a primitive wired up by hand
when a higher-level API picks the mode and nonce for you.

| Platform | Use |
|---|---|
| Any language with bindings | libsodium: `sodium-native` or `libsodium-wrappers`, PyNaCl, PHP `sodium_*` (built in since 7.2) |
| Browser, Node, Deno, Bun | WebCrypto (`crypto.subtle`), `node:crypto` |
| Python | `cryptography` (AESGCM, ChaCha20Poly1305, Ed25519, HKDF, Fernet) |
| Go | the standard `crypto/*` packages and `golang.org/x/crypto` |
| Rust | RustCrypto (`aes-gcm`, `chacha20poly1305`, `ed25519-dalek`, `argon2`), `ring` or `aws-lc-rs` |
| Java, Kotlin | Tink; the JCA only with explicit modes |
| .NET | `AesGcm`, `ChaCha20Poly1305`, `RandomNumberGenerator` |
| Files and config | age; SOPS for config kept in git |

Dead or dangerous: PyCrypto (abandoned in 2013), `crypto-js` (development
discontinued in 2023; before 4.2.0 its PBKDF2 defaulted to one iteration of
SHA-1), PHP mcrypt (removed in 7.2), Node's `crypto.createCipher` (an MD5 key
derivation; removed in Node 22), `openssl enc` without `-pbkdf2`.

## 2. The algorithm for each job

| Job | Use | Never |
|---|---|---|
| Encrypt data | AES-256-GCM or ChaCha20-Poly1305; XChaCha20-Poly1305 with random nonces; AES-GCM-SIV if a nonce may repeat | ECB; CBC or CTR without a MAC; RC4, DES, 3DES, Blowfish |
| Encrypt to a public key | libsodium sealed boxes, HPKE (RFC 9180), age | RSA with PKCS#1 v1.5 padding, or without OAEP |
| Sign | Ed25519; ECDSA P-256 where required; RSA-PSS at 3072 bits | DSA; RSA under 2048 bits |
| Agree a key | X25519; the hybrid X25519MLKEM768 in TLS | static RSA key transport; DH under 2048 bits |
| Hash for integrity | SHA-256, SHA-512, SHA-3, BLAKE2b, BLAKE3 | MD5 or SHA-1 where an attacker influences input |
| MAC | HMAC-SHA256; keyed BLAKE2b | `hash(key + message)` |
| Store passwords | argon2id, scrypt, bcrypt at cost 12+, PBKDF2 where FIPS demands it | any fast hash, salted or not |
| Derive keys from a key | HKDF-SHA256 with a label per purpose | a plain hash of the key |
| Derive a key from a password | argon2id, then an AEAD (or age with a passphrase) | the password bytes as the key |
| Tokens | 32 bytes from a CSPRNG, base64url | timestamps, UUIDv1, `Math.random` |

Post-quantum: TLS stacks now negotiate the hybrid X25519MLKEM768 by default
(Chrome, Firefox, Cloudflare, OpenSSL 3.5, Go 1.24), which protects recorded
traffic from later decryption; ML-KEM and ML-DSA are standardised (FIPS 203
and 204, 2024). Application-level post-quantum work is rarely needed yet;
long-lived secrets sent today are the case for it.

## 3. Randomness

- A CSPRNG only (CWE-338): `crypto.getRandomValues`, `crypto.randomBytes`,
  `crypto.randomUUID`, `crypto.randomInt` (JavaScript); `secrets.token_bytes`,
  `secrets.token_urlsafe`, `secrets.choice` (Python); `crypto/rand`, with
  `rand.Text()` since Go 1.24; `OsRng` or `getrandom` (Rust); `SecureRandom`
  (Java, Ruby); `random_bytes`, `random_int` (PHP); `RandomNumberGenerator`
  (.NET).
- Never for anything secret: `Math.random`, Python's `random`, Go's
  `math/rand` (v1 or v2), `java.util.Random`, PHP `rand`, `mt_rand`,
  `uniqid`, `lcg_value`, Ruby's `rand`, C's `rand()`, seeds from the time or
  the process id.
- Ranges through the library (`crypto.randomInt`, `secrets.randbelow`,
  `random_int`), not `%` on raw bytes (modulo bias).
- A UUIDv4 from a CSPRNG (`crypto.randomUUID()`) has 122 random bits: fine as
  an unguessable id. UUIDv1, v6 and v7 carry the time (v7 has 74 random bits
  at most) and are not secrets.

```sh
rg -n 'Math\.random|\brandom\.(random|randint|randrange|choice|getrandbits)\(|"math/rand|\bmt_rand\(|\brand\(|uniqid\(|lcg_value|java\.util\.Random|new Random\(' .
```

## 4. Nonces and IVs

- AES-GCM takes a 96-bit nonce that must never repeat under one key (CWE-323):
  a repeat leaks the XOR of the two plaintexts and lets an attacker forge
  messages. Random nonces are safe up to about 2^32 messages per key (NIST SP
  800-38D); beyond that, a counter stored durably, XChaCha20-Poly1305
  (192-bit nonces), AES-GCM-SIV, or a new key.
- CBC IVs are random and unpredictable (CWE-329), never static, derived from
  the key, or reused. CBC without a MAC falls to padding-oracle attacks
  whenever errors differ; move to an AEAD.
- Tags at the full 16 bytes. In Node, pass `authTagLength: 16` to
  `createDecipheriv` so a truncated tag is refused, call `setAuthTag` before
  `final()`, and use nothing from `update()` until `final()` succeeds.
- Associated data binds a ciphertext to its context (row id, tenant,
  purpose), so it cannot be copied into another row and still decrypt.
- ECDSA needs a fresh secret nonce per signature; a repeated or biased one
  reveals the private key. Libraries use RFC 6979 deterministic nonces, and
  Ed25519 is deterministic by design: never hand-roll the signing step.

```sh
rg -n -i '\b(iv|nonce)\s*[:=]\s*(b?["\x27]|new byte\[|Buffer\.(alloc|from)\(|bytes\(\d|\[\s*0|0x0)' .
```

## 5. Keys

- Generated by a CSPRNG at full size (32 bytes for AES-256, ChaCha20 and
  HMAC-SHA256), never a phrase a person typed or a hash of one (CWE-321 for
  keys in code).
- Kept in a KMS or HSM (AWS KMS, GCP Cloud KMS, Azure Key Vault) or the
  secret manager: never in the repository, the image, or the database whose
  data they protect.
- Envelope encryption for data at rest: a data key per record, file or
  tenant, wrapped by a KMS key. Rotating the KMS key re-wraps data keys, not
  the data.
- Versioned: a key id stored with each ciphertext (`v2:...`); decrypt with
  the old key, encrypt with the new, re-encrypt in the background. Signing
  keys published as a JWKS with a `kid`, the old one kept until its tokens
  expire.
- One key per purpose and per environment; sub-keys derived with HKDF and a
  label, never one key for encryption and MAC both.
- Sizes: RSA 2048 at least (3072 for use beyond 2030, per NIST), P-256 or
  Curve25519 for elliptic curves, 256-bit symmetric keys.

## 6. Constant-time comparison

A MAC, webhook signature, API key or token compared with `==` leaks how many
leading bytes matched (CWE-208).

| Language | Use |
|---|---|
| Node | `crypto.timingSafeEqual(a, b)` (check equal lengths first: it throws) |
| Python | `hmac.compare_digest` |
| Go | `hmac.Equal`, `subtle.ConstantTimeCompare` |
| Rust | the `subtle` crate's `ConstantTimeEq`, or `constant_time_eq` |
| Java | `MessageDigest.isEqual` |
| PHP | `hash_equals` |
| Ruby | `Rack::Utils.secure_compare`, `ActiveSupport::SecurityUtils.secure_compare` |
| .NET | `CryptographicOperations.FixedTimeEquals` |

Better still for stored tokens: look them up by their SHA-256 hash, so no
secret is ever compared.

```sh
rg -n -i '(signature|hmac|digest|token|secret|api_?key)\w*\s*(===?|!==?)\s*\w' .
```

## 7. Tokens and signed data

- 128 random bits at least, 32 bytes by default, base64url without padding;
  hashed at rest when they grant access (SHA-256 suffices for random
  tokens); expiring; single use when they authorise one action; bound to one
  purpose, so a verification token cannot reset a password.
- Signed stateless tokens (itsdangerous, Laravel signed URLs, iron-session,
  JWT, PASETO v4) carry expiry and purpose inside the signed part and cannot
  be revoked early; random tokens in a table can.
- Encryption is not integrity (unauthenticated ciphertext can be altered),
  and a signature is not secrecy (a JWT payload is readable). JWT specifics
  are in `security-auth` section 4.

## 8. TLS and certificate checks

Verification is never switched off (CWE-295). Look for Node
`rejectUnauthorized: false` or `NODE_TLS_REJECT_UNAUTHORIZED=0`; Python
`verify=False`, `_create_unverified_context`, `CERT_NONE` or
`check_hostname = False`; Go `InsecureSkipVerify: true`; Java trust
managers that accept everything and hostname verifiers that return true;
.NET certificate callbacks returning true; PHP `verify_peer => false` or
`CURLOPT_SSL_VERIFYPEER => false`; Ruby `VERIFY_NONE`; Rust
`danger_accept_invalid_certs(true)`; `curl -k`;
`wget --no-check-certificate`; `http.sslVerify false`; `strict-ssl=false`.

- An internal CA goes into the trust store (`NODE_EXTRA_CA_CERTS`,
  `SSL_CERT_FILE`, `REQUESTS_CA_BUNDLE`) instead.
- Pinning only with a backup pin and a rotation plan (mobile apps); browsers
  dropped HPKP.
- Database clients verify too: PostgreSQL `sslmode=verify-full`, MySQL
  `ssl-mode=VERIFY_IDENTITY`.

```sh
rg -n 'rejectUnauthorized:\s*false|NODE_TLS_REJECT_UNAUTHORIZED|verify\s*=\s*False|_create_unverified_context|CERT_NONE|check_hostname\s*=\s*False|InsecureSkipVerify|danger_accept_invalid|VERIFY_NONE|SSL_VERIFYPEER|verify_peer|sslVerify|strict-ssl|--insecure|no-check-certificate' .
```

## 9. Encrypting stored data

- Disk and database encryption at rest defends stolen disks and snapshots,
  not SQL injection or a leaked database password. Field-level encryption in
  the application protects the columns that matter: identity numbers, health
  data, third-party OAuth tokens, TOTP secrets.
- Rails `encrypts` (Active Record Encryption) and Laravel's `encrypted` casts
  do this; elsewhere an AEAD with data keys as in section 5.
- Searching encrypted values: deterministic encryption (AES-SIV) or a blind
  index (an HMAC of the normalised value in its own column). Both reveal
  which rows are equal, so only for fields that need lookup.
- `pgcrypto` sends keys through SQL, where statement logs and
  `pg_stat_statements` can capture them: encrypt in the application.

## 10. Mistakes to grep for

```sh
rg -n -i 'AES/ECB|MODE_ECB|aes-\d+-ecb|getInstance\("AES"\)|createCipher\(|MODE_CBC|aes-\d+-cbc|PKCS1Padding|PKCS1v15|\bDES\b|3DES|\bRC4\b|Blowfish' .
rg -n -i 'md5\(|sha1\(|createHash\(["\x27](md5|sha1)|hashlib\.(md5|sha1)|getInstance\("(MD5|SHA-?1)"\)|DigestUtils\.(md5|sha1)' .
```

- ECB (`Cipher.getInstance("AES")` means ECB in Java); CBC without a MAC;
  RSA encryption without OAEP; MD5 or SHA-1 over input an attacker controls
  (a cache key or ETag is fine, and Python's
  `hashlib.md5(usedforsecurity=False)` says so).
- Static IVs and keys in code; a password used directly as a key; base64 or
  hex called encryption; XOR "obfuscation"; `hash(secret + message)` as a
  MAC (SHA-256 allows length extension); deterministic encryption where
  equal values must not be visible; secrets compressed together with
  attacker-chosen text before encryption (CRIME, BREACH); JWTs signed with a
  short or guessable HMAC secret (`security-auth` section 4).
- Rating: a broken primitive guarding passwords, tokens, payments or
  personal data is high, critical when the key is in a public repository;
  the same algorithm for a cache key is not a finding.

## Check it

- Every encrypt, sign, hash, random and compare call found by the patterns
  above is traced to what it protects, and each protection names its
  library, algorithm, key source and nonce handling.
- Round-trip tests exist, and decrypting with the wrong key, an altered
  ciphertext or altered associated data fails.
- Keys come from the environment or a KMS, and a rotation path exists (a key
  id stored with the ciphertext or the token).

## Avoid

Home-made schemes; ECB; CBC without a MAC; a fixed or reused nonce; a
password as a key; SHA-256 for passwords; `Math.random` or `random` for
tokens; `==` on a MAC; certificate checks turned off; keys in the
repository; one key for every purpose; `crypto-js` and PyCrypto; calling
base64 encryption.
