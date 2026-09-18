//! Inline markdown is parsed by hand, so the cases that matter are the ones
//! where a coding agent's prose collides with markdown's delimiters: shell
//! globs, arithmetic, identifiers with underscores, and code spans that
//! themselves contain backticks.

use enowx_tui::preview_markdown;

/// The rendered line with its escape sequences intact, for asserting style.
fn text(md: &str) -> String {
    preview_markdown(md, 200).join("\n")
}

/// Just the visible characters: strips complete SGR sequences.
///
/// Removing the ESC byte on its own is not enough — that leaves the `[1m`
/// behind as ordinary text, which is how an earlier version of this helper
/// silently compared against escape codes rather than content.
fn plain(md: &str) -> String {
    let rendered = text(md);
    let mut out = String::new();
    let mut chars = rendered.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // Consume "[ … m" — the whole sequence, terminator included.
        if chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        }
    }
    out
}

#[test]
fn arithmetic_keeps_its_asterisks() {
    let got = plain("Math: 2 * 3 * 4 = 24");
    assert!(
        got.contains("2 * 3 * 4 = 24"),
        "asterisks used as multiplication must survive, got {got:?}"
    );
}

#[test]
fn a_glob_pattern_is_not_emphasis() {
    let got = plain("Run rm *.log and *.tmp to clean up");
    assert!(
        got.contains("*.log") && got.contains("*.tmp"),
        "glob stars must survive, got {got:?}"
    );
}

#[test]
fn escaped_delimiters_render_literally() {
    let got = plain(r"Escaped: \*not italic\* and \`not code\`");
    assert!(
        got.contains("*not italic*"),
        "an escaped asterisk must show as a literal star, got {got:?}"
    );
    assert!(
        !got.contains('\\'),
        "the backslash itself must not be shown, got {got:?}"
    );
}

#[test]
fn double_backticks_can_contain_a_backtick() {
    let got = plain("Use ``a ` b`` here");
    assert!(
        got.contains("a ` b"),
        "a double-backtick span must keep the inner backtick, got {got:?}"
    );
}

#[test]
fn strikethrough_text_is_kept() {
    let got = plain("This is ~~removed~~ now");
    assert!(
        got.contains("removed") && !got.contains("~~"),
        "strikethrough must render its text without the tildes, got {got:?}"
    );
}

#[test]
fn identifiers_with_underscores_are_untouched() {
    let got = plain("Call some_var_name and other_var_here now");
    assert!(
        got.contains("some_var_name") && got.contains("other_var_here"),
        "underscores inside identifiers must not be treated as emphasis, got {got:?}"
    );
}

#[test]
fn real_emphasis_still_works() {
    let got = plain("This is **bold** and *italic* and `code`.");
    for word in ["bold", "italic", "code"] {
        assert!(got.contains(word), "{word} should still render");
    }
    assert!(
        !got.contains("**") && !got.contains('`'),
        "delimiters themselves must not be shown, got {got:?}"
    );
}

#[test]
fn an_unterminated_delimiter_is_shown_literally() {
    let got = plain("A lone * star and a lone ` tick");
    assert!(
        got.contains('*') && got.contains('`'),
        "unmatched delimiters must not swallow the rest of the line, got {got:?}"
    );
}

#[test]
fn a_link_shows_its_label_not_its_url() {
    let got = plain("See [the docs](https://example.com/page) for more");
    assert!(got.contains("the docs"), "the label must render");
    assert!(
        !got.contains("example.com"),
        "the URL must stay out of the prose, got {got:?}"
    );
}

/// Markdown inside emphasis has to be parsed, not taken as literal text.
/// `**a `b` c**` used to print its backticks, which is what a heading like
/// "**1. `enxapi-findings/` — …**" looked like in practice.
#[test]
fn code_inside_bold_is_still_code() {
    let got = plain("**bold with `code` inside**");
    assert!(
        got.contains("bold with code inside"),
        "the backticks should be consumed, got {got:?}"
    );
    assert!(!got.contains('`'), "no delimiter should survive: {got:?}");
}

#[test]
fn code_inside_italic_and_strikethrough_is_parsed_too() {
    for md in ["*italic with `code`*", "~~struck with `code`~~"] {
        let got = plain(md);
        assert!(!got.contains('`'), "{md:?} kept its backticks: {got:?}");
    }
}

/// Emphasis nests: the inner run keeps its own identity while inheriting the
/// outer weight.
#[test]
fn emphasis_can_nest() {
    let got = plain("**bold with *italic* nested**");
    assert!(got.contains("bold with italic nested"), "got {got:?}");
    assert!(!got.contains('*'), "no asterisks should survive: {got:?}");
}

/// A code span inside bold must stay visually code — dropping its colour for
/// the outer style would make it indistinguishable from the prose around it.
#[test]
fn nested_code_keeps_its_own_colour() {
    let styled = text("**bold with `code` inside**");
    // The code run carries a background; the surrounding bold does not.
    assert!(
        styled.contains("48;2;"),
        "the nested code span should keep its background, got {styled:?}"
    );
}

/// An unterminated delimiter must not swallow the rest of the line, and the
/// recursion must not run away on one.
#[test]
fn an_unterminated_emphasis_is_literal() {
    let got = plain("**unterminated");
    assert!(got.contains("**unterminated"), "got {got:?}");
}
