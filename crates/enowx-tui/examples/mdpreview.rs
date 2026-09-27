//! Render a markdown sample with ANSI colour, so the renderer can be
//! eyeballed without a terminal session.
//!
//! cargo run -q -p enowx-tui --example mdpreview -- [WIDTH] [FILE]
//!
//! Without a file it renders a gallery of every construct the renderer
//! handles.
use enowx_tui::preview_markdown;

const GALLERY: &str = r#"# Heading one
## Heading two
### Heading three
#### Heading four

A paragraph with **bold**, *italic*, ***both***, ~~struck~~, `inline code`,
a [link](https://example.com) and an image ![diagram of the flow](x.png).
This sentence is on its own line in the source and stays on its own line.
Arithmetic 2 * 3 * 4 and globs *.log survive, as do snake_case_names.

DONE: rebuilt the portfolio page
CHANGED: index.html, style.css
VERIFIED: opened it at 375 and 1280 wide
NEXT: nothing

- A bullet
- A bullet whose text is long enough that it has to wrap onto a second row and keep its indent
  - Nested once
    - Nested twice
- Back at the top

1. First step
2. Second step
   with a continuation line
3. Third step

Numbers keep their column:

8. Eight
9. Nine
10. Ten, whose number is wider

- [x] Write the renderer
- [ ] Test every construct

1. A loose list

2. Items separated by blank lines

> A quotation, set back from the prose around it, long enough to wrap
> onto another row.
>
> > And one nested inside it.

> [!WARNING]
> Alerts name themselves and keep the text at full strength.

> [!TIP]
> `cargo test` before you push.

```rust
fn main() {
	let greeting = "hello"; // a tab indents this line
    println!("{greeting}, a line that is long enough to have to soft-wrap inside the block");
}
```

~~~
a tilde fence with no language
~~~

    an indented code block

| Agent | Role | Tools used | Cost |
| :---- | :--: | ---------: | ---: |
| router | Reads the request and picks who handles it | 2 | $0.0010 |
| fe | Front-end work: pages, components, styling | 8 | $0.0214 |

---

Setext heading
--------------

Line one with a hard break
line two, and <kbd>Ctrl</kbd>+<kbd>P</kbd> keys, and a Vec<String> in prose.

<!-- an html comment is not shown -->

Mixed 中文字符和English words 日本語のテキスト wrap between wide characters.

A path too long for any row: crates/enowx-tui/src/ui/markdown/renderer/tables/column_widths.rs
"#;

fn main() {
    let mut args = std::env::args().skip(1);
    let width: usize = args.next().and_then(|w| w.parse().ok()).unwrap_or(74);
    let sample = match args.next() {
        Some(path) => std::fs::read_to_string(path).expect("read markdown file"),
        None => GALLERY.to_owned(),
    };
    let edge = "·".repeat(width);
    println!("{edge}");
    for line in preview_markdown(&sample, width) {
        println!("{line}\x1b[0m");
    }
    println!("{edge}");
}
