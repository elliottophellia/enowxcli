---
name: testing-unit
description: "Unit tests for logic: finding the cases (empty, one, many, boundaries, invalid, Unicode, time zones, large inputs), table-driven and parameterised tests, test doubles (fakes before mocks, mocks only at boundaries), property-based testing for invariants, careful snapshots, and the idioms of each language's test framework. Read before writing unit tests."
---

# Unit tests for logic

Generated unit tests have one happy-path case per function, a mock for every
collaborator, `expect(result).toBeTruthy()` and names like `test_calculate`.
They pass while the logic is wrong at the edges (the empty list, the last
page, the leap day), which is where the bugs are. This skill finds the
cases, writes them as tables, chooses doubles, adds properties, and gives
each language's idioms. Principles are in `testing`.

## 1. What a unit is

- A behaviour, not a class or a file: one public function, or a few objects
  that together do one thing, tested through the public API.
- Pure logic is the target. Time, randomness, I/O and configuration are
  passed in (`code`: side effects at the edges). Code that is hard to unit
  test usually mixes a decision with I/O: split it.
- In process, in milliseconds: no network, database or container; files only
  in a temporary directory (`tmp_path`, `t.TempDir()`).

## 2. Finding the cases

Go through the list for every input; most bugs sit in the first rows.

| Kind | Cases |
|---|---|
| Empty and missing | `""`, whitespace only, `[]`, `{}`, `null`/`None`/`nil`, an optional field absent |
| One and many | one item, two (the first interaction), many (1,000 or more) |
| Boundaries | at, below and above each limit: 0, 1, limit minus 1, limit, limit plus 1; first and last index; a page exactly full; inclusive against exclusive ranges |
| Sign and zero | negative, zero, `-0` (`Object.is(-0, 0)` is false) |
| Size and precision | integer overflow; `Number.MAX_SAFE_INTEGER + 1`; `0.1 + 0.2 !== 0.3`; rounding a half (banker's or half up); division by zero; `NaN` |
| Invalid formats | malformed email, `2026-02-30` (in V8, `new Date("2026-02-30")` quietly becomes 2 March), `1,5` against `1.5`, truncated JSON, wrong types, extra fields |
| Unicode | composed `"\u00e9"` against decomposed `"e\u0301"` (unequal until normalised to NFC); `"\u{1F469}\u200D\u{1F469}\u200D\u{1F467}"` is 1 grapheme, 5 code points, 8 UTF-16 units; truncation that splits a pair; Turkish `"I".toLocaleLowerCase("tr")` is `"ı"`; `"ß".toUpperCase()` is `"SS"`; right-to-left text; non-breaking and zero-width spaces |
| Time zones and DST | a local day of 23 hours (2026-03-29 in Europe/London, 2026-03-08 in America/New_York) or 25 (2026-10-25, 2026-11-01); a local time that does not exist or happens twice; offsets of +05:30 and +05:45; midnight UTC being the previous day in the Americas |
| Calendar | 29 February (2028; 1900 was not a leap year, 2000 was); 31 January plus one month; year end; ISO week 53 (2026-12-31 and 2027-01-01 are both in week 53 of 2026) |
| Locale | `1.234,56` in de-DE and id-ID; `03/04` as March or April; collation of accented letters; plural forms |
| Large inputs | a 1 MB string, 100,000 items (quadratic code shows), deep nesting |
| Duplicates and order | equal keys, equal timestamps, stable sorting, results compared ignoring order when order is not promised |
| State | called twice (idempotent?), called out of order, after an error |
| Errors | every documented error, checked by type or code, not only "it throws" |

## 3. Tables of cases

When cases share one shape, list them as data, each with a name, so a
failure names the case and adding one is a line.

```go
func TestParseAmount(t *testing.T) {
	tests := []struct {
		name    string
		in      string
		want    int64
		wantErr bool
	}{
		{name: "whole units", in: "12", want: 1200},
		{name: "two decimals", in: "12.34", want: 1234},
		{name: "empty", in: "", wantErr: true},
		{name: "three decimals", in: "1.234", wantErr: true},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := ParseAmount(tt.in)
			if (err != nil) != tt.wantErr {
				t.Fatalf("ParseAmount(%q) error = %v, wantErr %v", tt.in, err, tt.wantErr)
			}
			if got != tt.want {
				t.Errorf("ParseAmount(%q) = %d, want %d", tt.in, got, tt.want)
			}
		})
	}
}
```

Run one case: `go test -run 'TestParseAmount/three_decimals'` (spaces become
underscores). No `tt := tt` since Go 1.22.

```python
@pytest.mark.parametrize(
    ("raw", "cents"),
    [
        pytest.param("12", 1200, id="whole units"),
        pytest.param("12.34", 1234, id="two decimals"),
    ],
)
def test_parse_amount(raw, cents):
    assert parse_amount(raw) == cents


@pytest.mark.parametrize("raw", ["", "1.234", "-1", "1,5"])
def test_parse_amount_rejects(raw):
    with pytest.raises(InvalidAmount):
        parse_amount(raw)
```

```ts
it.each([
  { raw: "12", cents: 1200 },
  { raw: "12.34", cents: 1234 },
])("parses $raw as $cents cents", ({ raw, cents }) => {
  expect(parseAmount(raw)).toBe(cents);
});
```

```rust
#[rstest]
#[case::whole_units("12", 1200)]
#[case::two_decimals("12.34", 1234)]
fn parses_amounts(#[case] raw: &str, #[case] cents: i64) {
    assert_eq!(parse_amount(raw).unwrap(), cents);
}
```

Also: JUnit `@ParameterizedTest` with `@CsvSource`, xUnit `[Theory]` with
`[InlineData]`, Pest `->with([...])`. Outside Go, successes and failures go
in separate tables.

## 4. Test doubles

| Double | What it is | Use it for |
|---|---|---|
| Dummy | fills a parameter, never used | a required argument |
| Stub | returns canned answers | inputs from a dependency (a rate, a user) |
| Fake | a small working implementation (in-memory repository, fake clock) | stateful dependencies: the default |
| Spy | records calls | checking an output that is the behaviour (the email sent) |
| Mock | expectations set in advance and verified | rarely, at a boundary where the call is the contract |

- Use the real thing for your own pure code: never mock a value object, a
  mapper or a calculation.
- Fakes of your own interfaces before mocks. A mock encodes the call
  sequence and breaks on refactors; a fake behaves like the real thing.
  Run one shared contract test suite against the fake and the real
  implementation so they cannot drift.
- Replace only what crosses a boundary you own: your `PaymentGateway`,
  `Clock`, `Mailer` interfaces.
- Never mock what you do not own (an SDK, `fetch`, `requests`, the ORM):
  wrap it in a small adapter you own, fake the adapter in unit tests, and
  test the adapter against a faked HTTP edge (`testing-integration`).
- Assert on state before interactions: the order is saved as paid, not
  `save` was called. Verify a call only when the call is the outcome, and
  then check its arguments exactly.
- Keep doubles strict, so a misspelt or unexpected call fails:
  `create_autospec(Class, instance=True, spec_set=True)` in Python, Mockito's
  strict stubs with `MockitoExtension`, `instance_double` in RSpec,
  `mockall` expectations in Rust.

```ts
it("expires a reservation after 15 minutes", () => {
  let now = new Date("2026-09-29T10:00:00Z");
  const reservations = new Reservations({ current: () => now }); // a fake Clock
  const r = reservations.hold("seat-12");

  now = new Date("2026-09-29T10:15:00Z");

  expect(reservations.isActive(r.id)).toBe(false);
});
```

## 5. Property-based tests

For invariants over many inputs: round trips (`parse(format(x)) == x`),
idempotence (`slug(slug(s)) == slug(s)`), invariants (a sort's output is
ordered and a permutation), a fast implementation against a slow obvious
one. Examples still pin the business rules; properties add the inputs
nobody thought of, shrunk to a minimal case with a seed to replay.

```ts
it("sorts any list of numbers without losing items", () => {
  fc.assert(
    fc.property(fc.array(fc.integer()), (xs) => {
      const sorted = sortNumbers(xs);
      expect(sorted).toHaveLength(xs.length);
      for (let i = 1; i < sorted.length; i++) expect(sorted[i - 1]).toBeLessThanOrEqual(sorted[i]);
    }),
  );
});
```

That property catches `xs.sort()` without a comparator (JavaScript sorts
numbers as strings) within a few runs and shrinks it to two numbers.

```python
@given(st.text())
@example("  Hello  World ")
def test_slug_is_idempotent(s):
    assert slugify(slugify(s)) == slugify(s)
```

- Tools: fast-check (`@fast-check/vitest` adds `test.prop`), Hypothesis,
  proptest (`proptest! { #[test] fn f(c in 0i64..=1_000_000) { ... } }`),
  rapid for Go, jqwik for Java, FsCheck for .NET. Most run 100 cases by
  default (proptest 256, jqwik 1,000): enough per commit; more nightly.
- Go's native fuzzing does the same for bytes and strings:
  `go test -fuzz=FuzzParseAmount -fuzztime=30s ./money`. Failing inputs are
  saved under `testdata/fuzz/` and rerun by plain `go test`: commit them. A
  round-trip fuzz target finds an `int64` overflow in `whole*100` in seconds.
- Keep every counterexample found: `@example(...)` in Hypothesis, a fixed
  case in the table, proptest's `proptest-regressions/` files committed.
- Constrain generators to valid domains: `fc.date({ noInvalidDate: true })`
  (the default can produce `Invalid Date`), `st.decimals(places=2,
  allow_nan=False)`.

## 6. Snapshots, carefully

- Only for small, stable, reviewed output (an error message, generated SQL,
  a CLI's help), never whole responses, large trees, timestamps or ids.
- Inline snapshots keep the expected value where the reviewer reads it:
  `toMatchInlineSnapshot()`, `insta::assert_snapshot!(v, @"...")`.
- Redact volatile fields: `toMatchSnapshot({ id: expect.any(String) })`;
  insta `assert_json_snapshot!(order, { ".id" => "[id]" })`.
- An update is a change to review line by line (`vitest -u`, `jest -u`,
  `cargo insta review`, `pytest --snapshot-update` with syrupy), never a way
  to make the build green. In CI missing snapshots fail instead of being
  written (Vitest when `CI` is set, Jest with `--ci`, insta by default).

## 7. Assertions and names

- The most specific matcher: `toEqual` for structures, `containsExactly` in
  AssertJ, `assert_eq!`, pytest's plain `assert` (it shows a diff).
- Messages that say what went wrong: `t.Errorf("Total(%v) = %d, want %d",
  items, got, want)`; `cmp.Diff(want, got)` from go-cmp for structs.
- Floats compared with a tolerance (`toBeCloseTo`, `pytest.approx`,
  `isCloseTo(x, within(1e-9))`); money never as a float.
- Errors by type or code: `toThrow(InvalidAmount)`,
  `pytest.raises(InvalidAmount, match="decimals")`, `errors.Is(err,
  ErrInvalid)`, `assert!(matches!(e, ParseError::TooPrecise))`.
- A name is the behaviour in a sentence: `rejects_more_than_two_decimals`,
  `it("rejects more than two decimals")`, JUnit `@DisplayName`.

## 8. Per-language notes

- **Vitest and Jest**: `restoreMocks: true` in config; `vi.useFakeTimers()`
  with `vi.setSystemTime()`, real timers again in `afterEach`; always
  `await` or return promises (`await expect(p).rejects.toThrow()`); injected
  dependencies over hoisted `vi.mock()` module mocks.
- **pytest**: fixtures in `conftest.py` at the narrowest scope; `tmp_path`,
  `monkeypatch`, `caplog`; the `subtests` fixture is built in from pytest 9;
  markers registered and `--strict-markers`.
- **Go**: `t.Helper()` in helpers; `t.Cleanup`, `t.TempDir()`, `t.Setenv`
  (not with `t.Parallel()`), `t.Context()` (1.24); `testing/synctest` (1.25)
  runs timers and goroutines on a fake clock; `Example` functions with
  `// Output:` are tested docs. With testify: `require` for preconditions,
  `assert` for checks, and the `(t, expected, actual)` order.
- **Rust**: `#[cfg(test)] mod tests` beside the code, `tests/` for the public
  API; tests may return `Result` and use `?`; `#[should_panic(expected =
  "...")]` only when panicking is the contract; `cargo test --doc` runs doc
  examples; `mockall` for traits you own.
- **JUnit 5 or 6 with AssertJ**: `@Nested` classes per situation,
  `@ParameterizedTest`, `@TempDir`, Mockito with `MockitoExtension`, jqwik
  `@Property`. JUnit 6 needs Java 17.
- **xUnit**: `[Fact]`, `[Theory]`; constructor for setup, `IDisposable` for
  teardown; `FakeTimeProvider` for time. FluentAssertions 8 and later need a
  paid licence for commercial use; AwesomeAssertions (a fork) and Shouldly
  are free. Keep what the project uses.
- **RSpec, Minitest, PHPUnit, Pest**: `context "when ..."` blocks and `let`;
  `expect { }.to change { Order.count }.by(1)`; Minitest runs in random
  order and prints its seed; PHPUnit attributes `#[Test]`,
  `#[DataProvider]`; Pest `->throws(InvalidAmount::class)`.

## Check it

- Break the logic on purpose (flip `<`, drop a branch): a new test fails.
  Or run a mutation tool on the file (`testing`, section 5).
- The cases cover the rows of section 2 that apply; say which were left out.
- The file passes alone, in the full suite and in random order.
- A property test's failure output shows a seed you can replay.

## Avoid

One happy-path case per function; mocking the unit's own pure
collaborators; mocking libraries you do not own; assertions on call order
or private state; `toBeTruthy()` where the value is known; expected values
computed by the code under test; floats for money; large snapshots; the
real clock, `Math.random`, environment or network inside a unit test;
`sleep`; property tests without the example cases for the rules.
