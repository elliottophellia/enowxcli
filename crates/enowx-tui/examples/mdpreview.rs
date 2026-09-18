//! Render a markdown sample to a fixed-size buffer and print it with ANSI
//! colour, so code-block highlighting can be eyeballed without a terminal
//! session. Run: cargo run -p enowx-tui --example mdpreview
use enowx_tui::preview_markdown;

fn main() {
    let sample = r#"Here is the fix. The parser now tracks state across lines.

```rust
// Tokenize one line; `state` carries a block comment across the newline.
pub fn highlight(line: &str, syntax: &Syntax, state: &mut State) -> Vec<(String, Tok)> {
    let mut out: Vec<(String, Tok)> = Vec::new();
    /* a block comment
       that spans lines */
    let total = 0x1f + 3.14;
    if line.starts_with("//") { return vec![(line.to_string(), Tok::Comment)]; }
    out
}
```

And the shell side:

```bash
# rebuild and run the tests
cargo build --release && cargo test --workspace
export ENX_HOME="$HOME/.enx"
```

```json
{"model": "claude-sonnet-4.5", "stream": true, "max_tokens": 4096}
```

A block with a very long line that has to wrap rather than being cut:

```python
def compute(alpha, beta, gamma):  # this signature is deliberately long so it exceeds the block width and must soft-wrap
    return alpha * beta + gamma
```

```brainfuck
+[----->+++<]>+.---.
```
"#;
    for line in preview_markdown(sample, 74) {
        println!("{line}");
    }
}
