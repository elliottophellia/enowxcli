//! Vertical rhythm: headings need air around them, and lists need separation
//! once their items wrap. A terminal has no type scale, so hierarchy has to
//! come from weight, colour and spacing.

use enowx_tui::preview_markdown;

fn render(md: &str) -> Vec<String> {
    preview_markdown(md, 74)
        .into_iter()
        .map(|line| {
            // Strip SGR so assertions are about layout, not colour.
            let mut out = String::new();
            let mut in_escape = false;
            for c in line.chars() {
                if in_escape {
                    if c == 'm' {
                        in_escape = false;
                    }
                    continue;
                }
                if c == '\u{1b}' {
                    in_escape = true;
                    continue;
                }
                out.push(c);
            }
            out.trim_end().to_string()
        })
        .collect()
}

fn index_of(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} not found in {rows:?}"))
}

/// A heading pressed against the paragraph above reads as part of it.
#[test]
fn a_heading_is_separated_from_what_precedes_it() {
    let rows = render("Some prose ends here.\n## A new section\nBody.");
    let heading = index_of(&rows, "A new section");
    assert!(
        rows[heading - 1].trim().is_empty(),
        "expected a blank line above the heading, got {rows:?}"
    );
}

/// And from the body it introduces, so it sits with its section.
#[test]
fn a_heading_is_separated_from_its_body() {
    let rows = render("## A new section\nBody follows immediately.");
    let heading = index_of(&rows, "A new section");
    assert!(
        rows[heading + 1].trim().is_empty(),
        "expected a blank line below the heading, got {rows:?}"
    );
}

/// Spacing the source already provides must not be doubled.
#[test]
fn authored_blank_lines_are_not_doubled() {
    let rows = render("Prose.\n\n## Section\n\nBody.");
    let heading = index_of(&rows, "Section");
    assert!(
        !rows[heading - 2].trim().is_empty() || heading < 2,
        "two blank lines stacked above the heading: {rows:?}"
    );
    assert!(
        !rows[heading + 2].trim().is_empty(),
        "two blank lines stacked below the heading: {rows:?}"
    );
}

/// Levels must be distinguishable. They cannot differ by size in a terminal,
/// so they differ by style — the test asserts the rendered styles are not all
/// identical, which is what made a document read as flat.
#[test]
fn heading_levels_are_not_all_identical() {
    let h1 = preview_markdown("# One", 74).join("");
    let h3 = preview_markdown("### Three", 74).join("");
    let h5 = preview_markdown("##### Five", 74).join("");
    assert_ne!(
        style_of(&h1),
        style_of(&h3),
        "h1 and h3 should not render identically"
    );
    assert_ne!(
        style_of(&h3),
        style_of(&h5),
        "h3 and h5 should not render identically"
    );
}

/// The escape sequences preceding the text, i.e. how it is styled.
fn style_of(rendered: &str) -> String {
    rendered
        .split('m')
        .take_while(|part| part.contains('\u{1b}') || part.is_empty())
        .collect()
}

/// A list of one-liners reads fine packed; spacing it would waste the screen.
#[test]
fn a_short_list_stays_tight() {
    let rows = render("- one\n- two\n- three");
    let first = index_of(&rows, "one");
    assert!(
        !rows[first + 1].trim().is_empty(),
        "short list items should not be spaced apart, got {rows:?}"
    );
}

/// Once items wrap, the rows run together and the separation earns its space.
#[test]
fn a_wrapped_list_item_is_followed_by_a_gap() {
    let long = "- ".to_string()
        + &"word ".repeat(30)
        + "\n- second item";
    let rows = render(&long);
    let second = index_of(&rows, "second item");
    assert!(
        rows[second - 1].trim().is_empty(),
        "a wrapped item should be separated from the next, got {rows:?}"
    );
}

/// The gap belongs BETWEEN items. A list followed by prose gets no extra gap
/// from this rule — whatever spacing the author wrote there stands, so the
/// renderer does not quietly rewrite the document's rhythm.
#[test]
fn no_trailing_gap_after_the_final_item() {
    let long = "- ".to_string() + &"word ".repeat(30) + "\nAfter the list.";
    let rows = render(&long);
    let after = index_of(&rows, "After the list.");
    assert!(
        !rows[after - 1].trim().is_empty(),
        "a gap was inserted after the last item even though no item followed: {rows:?}"
    );
}

/// Numbered lists follow the same rule as bulleted ones.
#[test]
fn ordered_lists_space_the_same_way() {
    let short = render("1. one\n2. two");
    let first = index_of(&short, "one");
    assert!(!short[first + 1].trim().is_empty(), "short: {short:?}");

    let long = "1. ".to_string() + &"word ".repeat(30) + "\n2. second";
    let rows = render(&long);
    let second = index_of(&rows, "second");
    assert!(
        rows[second - 1].trim().is_empty(),
        "wrapped ordered item should be separated, got {rows:?}"
    );
}
