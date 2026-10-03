//! The preview opens pages in a real browser, so these run only where Chrome
//! (or Chromium, Edge, Brave) is installed.

use std::path::PathBuf;

use enowx_core::preview::{self, Target, WidthReport};

const SLOPPY: &str = r##"<!doctype html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sloppy</title>
<style>
body { margin: 0; font: 16px/1.5 system-ui; background: #fff; color: #111; }
.wide { width: 900px; height: 40px; background: #eee; }
.faint { color: #bbb; }
.tiny { display: inline-block; padding: 2px 4px; }
</style></head>
<body>
<h1>First</h1><h1>Second</h1>
<div class="wide">wide</div>
<p class="faint">Barely readable text</p>
<img src="photo.png">
<a href="#pricing">Pricing</a>
<button class="icon-only"><svg width="16" height="16" aria-hidden="true"><rect width="16" height="16"/></svg></button>
<a class="tiny" href="/next">Go</a>
<script>console.error("boom");</script>
</body></html>"##;

const CLEAN: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Clean</title>
<style>
body { margin: 0; font: 16px/1.5 system-ui; background: #fff; color: #1a1a1a; }
main { max-width: 60ch; margin: 0 auto; padding: 24px 16px; }
nav { padding: 0 16px; }
nav a { display: inline-block; min-height: 44px; min-width: 44px; padding: 12px; color: #1a1a1a; }
button { min-height: 44px; min-width: 44px; padding: 0 16px; font: inherit; color: #fff; background: #1d4ed8; border: 0; }
.skip { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
</style></head>
<body>
<a class="skip" href="#main">Skip to content</a>
<nav><a href="#about">About</a></nav>
<main id="main">
<h1>A clean page</h1>
<p id="about">Plain text with a <a href="#about">link in a sentence</a> that is fine.</p>
<img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7" alt="" width="1" height="1">
<button type="button">Save</button>
</main>
</body></html>"##;

fn folder(page: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("enx-preview-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("index.html"), page).unwrap();
    dir
}

fn no_chrome() -> bool {
    if preview::find_chrome().is_some() {
        return false;
    }
    eprintln!("no Chrome on this machine; the preview is not tested here");
    true
}

fn has(items: &[String], part: &str) -> bool {
    items.iter().any(|item| item.contains(part))
}

#[tokio::test]
async fn a_page_is_measured_as_a_person_would_see_it() {
    if no_chrome() {
        return;
    }
    let dir = folder(SLOPPY);
    let reports = preview::preview(
        &dir,
        Target::File(dir.join("index.html")),
        None,
        &dir.join("shots"),
    )
    .await
    .unwrap();
    let text = preview::report("index.html", &reports);
    assert_eq!(
        reports.iter().map(|r| r.width).collect::<Vec<_>>(),
        preview::WIDTHS
    );

    let phone: &WidthReport = &reports[0];
    assert!(phone.overflow_px > 400, "{text}");
    assert!(has(&phone.overflowing, "div.wide"), "{text}");
    assert!(has(&phone.low_contrast, "p.faint"), "{text}");
    assert!(has(&phone.dead_anchors, "#pricing"), "{text}");
    assert!(has(&phone.missing_alt, "photo.png"), "{text}");
    assert!(has(&phone.unnamed_controls, "button.icon-only"), "{text}");
    assert!(has(&phone.small_targets, "a.tiny"), "{text}");
    assert!(has(&phone.console_errors, "boom"), "{text}");
    assert_eq!(phone.h1_count, 2);
    let shot = std::fs::read(phone.screenshot.as_ref().unwrap()).unwrap();
    assert!(shot.starts_with(b"\x89PNG"));

    let wide = &reports[2];
    assert!(wide.overflow_px <= 1, "{text}");
    assert!(
        wide.small_targets.is_empty(),
        "touch targets are a phone's problem"
    );
    assert!(text.contains("links to nowhere"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_clean_page_reports_nothing() {
    if no_chrome() {
        return;
    }
    let dir = folder(CLEAN);
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .unwrap();
    let text = preview::report("index.html", &reports);
    for report in &reports {
        assert_eq!(report.problems(), 0, "{text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_served_page_is_started_and_stopped() {
    if no_chrome()
        || std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
    {
        return;
    }
    let dir = folder(CLEAN);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/");
    let reports = preview::preview(
        &dir,
        Target::Url(url.clone()),
        Some(&format!("python3 -m http.server {port} --bind 127.0.0.1")),
        &dir.join("shots"),
    )
    .await
    .unwrap();
    assert_eq!(reports[0].h1_count, 1);
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", port)).is_err(),
        "the server was left running"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_source_file_is_not_opened_as_a_page() {
    let dir = folder(CLEAN);
    std::fs::write(dir.join("App.tsx"), "export default () => null").unwrap();
    let error = preview::preview(
        &dir,
        Target::File(dir.join("App.tsx")),
        None,
        &dir.join("shots"),
    )
    .await
    .map(|_| ())
    .unwrap_err()
    .to_string();
    if !no_chrome() {
        assert!(error.contains("not an HTML file"), "{error}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_server_that_fails_says_why() {
    if no_chrome() {
        return;
    }
    let dir = folder(CLEAN);
    let error = preview::preview(
        &dir,
        Target::Url("http://127.0.0.1:9/".into()),
        Some("echo 'Error: port 3000 is already in use' >&2; exit 1"),
        &dir.join("shots"),
    )
    .await
    .map(|_| ())
    .unwrap_err()
    .to_string();
    assert!(error.contains("exited"), "{error}");
    assert!(error.contains("port 3000 is already in use"), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Sticky in the CSS, but a wrapper's overflow stops it sticking; five
/// screens tall with no way back up.
const LONG_SLOPPY: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Long and lost</title>
<style>
body { margin: 0; font: 16px/1.5 system-ui; color: #1a1a1a; background: #fff; }
.wrapper { overflow: hidden; }
header { position: sticky; top: 0; background: #fff; border-bottom: 1px solid #ccc; padding: 0 16px; }
header a { display: inline-block; padding: 12px; min-height: 44px; min-width: 44px; color: #1a1a1a; }
section { min-height: 1000px; padding: 16px; }
</style></head>
<body><div class="wrapper">
<header><a href="#one">One</a> <a href="#two">Two</a></header>
<main><h1>A long page</h1>
<section id="one"><p>First part.</p></section>
<section id="two"><p>Second part.</p></section>
<section><p>Third part.</p></section>
<section><p>Fourth part.</p></section>
</main></div></body></html>"##;

/// The same page as it should be: a bar that sticks, and a back-to-top
/// link to `#top` (the top of the document, with no element of that id)
/// that appears after the first screen.
const LONG_CLEAN: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Long and found</title>
<style>
html { scroll-padding-top: 76px; }
body { margin: 0; font: 16px/1.5 system-ui; color: #1a1a1a; background: #fff; }
header { position: sticky; top: 0; z-index: 10; background: #fff; border-bottom: 1px solid #ccc; padding: 0 16px; }
header a { display: inline-block; padding: 12px; min-height: 44px; min-width: 44px; color: #1a1a1a; }
main { padding: 0 16px; }
section { min-height: 1000px; padding: 16px 0; }
blockquote { margin: 0; }
.to-top { position: fixed; right: 16px; bottom: 16px; z-index: 20; display: grid; place-items: center;
  width: 48px; height: 48px; color: #1a1a1a; background: #fff; border: 1px solid #ccc; border-radius: 6px; text-decoration: none; }
.to-top[hidden] { display: none; }
</style></head>
<body>
<header><a href="#one">One</a> <a href="#two">Two</a></header>
<main><h1>A long page</h1>
<section id="one"><h2>First part</h2><p>What the first part says.</p></section>
<section id="two"><p>Second part.</p><ul><li>One point.</li></ul></section>
<section><p>Third part.</p></section>
<section><blockquote>Fourth part.</blockquote></section>
</main>
<a class="to-top" href="#top" aria-label="Back to top">&uarr;</a>
<script>
const toTop = document.querySelector(".to-top");
const place = () => { toTop.hidden = scrollY < innerHeight; };
addEventListener("scroll", place, { passive: true });
place();
</script>
</body></html>"##;

#[tokio::test]
async fn a_long_page_keeps_its_bar_in_view_and_a_way_back_up() {
    if no_chrome() {
        return;
    }
    let dir = folder(LONG_SLOPPY);
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .unwrap();
    let text = preview::report("index.html", &reports);
    for report in &reports {
        assert!(report.screens > 3.0, "{text}");
        assert!(report.header_scrolls_away, "{text}");
        assert!(report.no_back_to_top, "{text}");
    }
    assert!(text.contains("the top bar scrolls away"), "{text}");
    assert!(text.contains("no way back to the top"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);

    let dir = folder(LONG_CLEAN);
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .unwrap();
    let text = preview::report("index.html", &reports);
    for report in &reports {
        assert!(report.screens > 3.0, "{text}");
        assert_eq!(report.problems(), 0, "{text}");
    }
    // Put back at the top for the screenshot, where the control hides.
    let shot = std::fs::read(reports[0].screenshot.as_ref().unwrap()).unwrap();
    assert!(shot.starts_with(b"\x89PNG"));
    let _ = std::fs::remove_dir_all(&dir);
}

fn entries_page(uniform: bool, headline: &str) -> String {
    let entry = |n: usize, big: bool| {
        if big {
            format!("<article class=\"entry\"><h2>Project {n}</h2><p>What it does, in two plain sentences for someone new.</p><pre>run it --now</pre></article>")
        } else {
            format!(
                "<li class=\"row\"><a href=\"#p{n}\">Project {n}</a> <span>one line</span></li>"
            )
        }
    };
    let body: String = if uniform {
        (1..=5).map(|n| entry(n, true)).collect()
    } else {
        format!(
            "{}<ul class=\"rows\">{}</ul>",
            entry(1, true),
            (2..=5).map(|n| entry(n, false)).collect::<String>()
        )
    };
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Entries</title>
<style>body {{ margin: 0; font: 16px/1.5 system-ui; color: #1a1a1a; background: #fff; }} main {{ max-width: 900px; margin: 0 auto; padding: 16px; }}
.entry {{ min-height: 180px; border-bottom: 1px solid #ccc; }} .rows a {{ display: inline-block; min-height: 44px; padding: 10px 0; color: #1a1a1a; }}</style></head>
<body><main><h1>{headline}</h1>{body}<p id="p2"></p><p id="p3"></p><p id="p4"></p><p id="p5"></p></main></body></html>"##
    )
}

#[tokio::test]
async fn one_module_repeated_down_the_page_is_reported() {
    if no_chrome() {
        return;
    }
    let dir = folder(&entries_page(true, "Tools that run on your own machine"));
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .unwrap();
    let text = preview::report("index.html", &reports);
    let wide = reports.last().unwrap();
    assert_eq!(
        wide.repeated_blocks.as_deref(),
        Some("article.entry ×5"),
        "{text}"
    );
    assert!(
        text.contains("the same block repeated down the page"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);

    // A lead entry and the rest as compact rows is what the skills ask for.
    let long = "Local-first developer tools: a self-hosted AI agent, a terminal coding CLI, and the memory and coordination they need";
    let dir = folder(&entries_page(false, long));
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .unwrap();
    let text = preview::report("index.html", &reports);
    let wide = reports.last().unwrap();
    assert_eq!(wide.repeated_blocks, None, "{text}");
    assert_eq!(wide.long_headline, Some(18), "{text}");
    assert!(text.contains("the headline has 18 words"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

fn themed(background: &str, text: &str) -> String {
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Theme</title>
<style>body {{ margin: 0; padding: 16px; font: 16px/1.5 system-ui; background: {background}; color: {text}; }}</style></head>
<body><h1>A page</h1><p>Some text.</p></body></html>"##
    )
}

#[tokio::test]
async fn a_charcoal_dark_theme_is_reported_and_a_real_dark_one_is_not() {
    if no_chrome() {
        return;
    }
    for (background, text, grey) in [
        ("#1e1e1e", "#ecebe8", true),
        ("#e5e5e5", "#16150f", true),
        ("#0c0d0f", "#ecebe8", false),
        ("#fafaf7", "#16150f", false),
    ] {
        let dir = folder(&themed(background, text));
        let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
            .await
            .unwrap();
        let text = preview::report("index.html", &reports);
        assert_eq!(
            reports[0].grey_background.is_some(),
            grey,
            "{background}: {text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

fn dashboard(sticky_side: bool, loud_rows: bool) -> String {
    let side = if sticky_side {
        "position: sticky; top: 0; height: 100dvh;"
    } else {
        "height: 100vh;"
    };
    let action = if loud_rows { "loud" } else { "quiet" };
    let rows: String = (1..=6)
        .map(|n| format!("<li class=\"row\"><span>Item {n}</span> <button class=\"{action}\">Return</button></li>"))
        .collect();
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Desk</title>
<style>body {{ margin: 0; font: 15px/1.5 system-ui; background: #fafaf7; color: #16150f; }}
.shell {{ display: grid; grid-template-columns: 240px 1fr; align-items: start; }}
aside {{ {side} background: #f0efe9; border-right: 1px solid #ddd; }}
aside a {{ display: block; padding: 12px; min-height: 44px; color: #16150f; }}
main {{ padding: 24px; min-height: 3000px; }}
button {{ min-height: 44px; min-width: 44px; padding: 0 12px; font: inherit; border: 1px solid #999; }}
.loud {{ background: #b3401f; color: #fff; border: 0; }} .quiet {{ background: transparent; color: #16150f; }}
.primary {{ background: #b3401f; color: #fff; border: 0; }}
.row {{ padding: 8px 0; }}</style></head>
<body><div class="shell"><aside><nav><a href="#m">Overview</a></nav></aside>
<main id="m"><h1>Today</h1><button class="primary">New loan</button><ul>{rows}</ul></main></div></body></html>"##
    )
}

#[tokio::test]
async fn a_short_sidebar_and_a_primary_on_every_row_are_reported() {
    if no_chrome() {
        return;
    }
    for (sticky, loud, expect_side, expect_loud) in
        [(false, true, true, true), (true, false, false, false)]
    {
        let dir = folder(&dashboard(sticky, loud));
        let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
            .await
            .unwrap();
        let text = preview::report("index.html", &reports);
        let wide = reports.last().unwrap();
        assert_eq!(wide.short_side.is_some(), expect_side, "{text}");
        assert_eq!(wide.repeated_primary.is_some(), expect_loud, "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// An application shell: a sidebar beside a page column that is centred
/// (`floating`) or anchored, and a sidebar that becomes a drawer on narrow
/// screens (`collapses`) or stays.
fn shell(floating: bool, collapses: bool) -> String {
    let page = if floating {
        "max-width: 1280px; margin: 0 auto;"
    } else {
        "max-width: 1280px; margin: 0;"
    };
    let narrow = if collapses {
        "@media (max-width: 1023px) { .shell { grid-template-columns: 1fr; } aside { display: none; } }"
    } else {
        ""
    };
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Shell</title>
<style>body {{ margin: 0; font: 15px/1.5 system-ui; background: #fff; color: #111; }}
.shell {{ display: grid; grid-template-columns: 224px minmax(0, 1fr); min-height: 100vh; }}
aside {{ background: #f4f4f2; border-right: 1px solid #ddd; position: sticky; top: 0; height: 100vh; }}
aside a {{ display: block; padding: 12px; min-height: 44px; color: #111; }}
main {{ padding: 24px; }}
.page {{ {page} }}
table {{ width: 100%; border-collapse: collapse; }} td {{ padding: 12px 8px; border-top: 1px solid #ddd; }}
{narrow}</style></head>
<body><div class="shell"><aside><nav><a href="#m">Products</a></nav></aside>
<main id="m"><div class="page"><h1>Products</h1><table><tr><td>Kopi</td><td>Rp1.500</td></tr><tr><td>Teh</td><td>Rp4.000</td></tr></table></div></main></div></body></html>"##
    )
}

#[tokio::test]
async fn a_floating_page_and_an_open_sidebar_on_a_phone_are_reported() {
    if no_chrome() {
        return;
    }
    for (floating, collapses) in [(true, false), (false, true)] {
        let dir = folder(&shell(floating, collapses));
        let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
            .await
            .unwrap();
        let text = preview::report("index.html", &reports);
        let (phone, tablet, wide) = (&reports[0], &reports[1], &reports[2]);
        assert_eq!(wide.floating_content.is_some(), floating, "{text}");
        assert_eq!(phone.open_sidebar.is_some(), !collapses, "{text}");
        assert_eq!(tablet.open_sidebar.is_some(), !collapses, "{text}");
        assert!(
            wide.open_sidebar.is_none(),
            "a sidebar belongs on a wide screen"
        );
        if floating {
            assert!(text.contains("floats centred beside the sidebar"), "{text}");
            assert!(text.contains("becomes a drawer"), "{text}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

const SIGN_IN: &str = r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Sign in</title></head>
<body><main><form id="f">
<label for="u">Username</label><input id="u" name="username" autocomplete="username">
<label for="p">Password</label><input id="p" name="password" type="password">
<button type="submit">Sign in</button><p role="alert" id="err"></p>
</form></main>
<script>
document.getElementById('f').addEventListener('submit', event => {
  event.preventDefault();
  const user = document.getElementById('u').value, pass = document.getElementById('p').value;
  if (user === 'admin' && pass === 'admin123') {
    localStorage.setItem('token', 'ok');
    location.href = '/app.html';
  } else {
    document.getElementById('err').textContent = 'Wrong username or password';
  }
});
</script></body></html>"##;

/// Shows the products only to a browser that signed in; anyone else gets
/// the sign-in form again, with no heading.
const BEHIND_SIGN_IN: &str = r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>App</title></head>
<body><div id="root"></div><script>
document.getElementById('root').innerHTML = localStorage.getItem('token') === 'ok'
  ? '<main><h1>Products</h1><p>Signed in.</p></main>'
  : '<form><label for="u">Username</label><input id="u"><label for="p">Password</label><input id="p" type="password"></form>';
</script></body></html>"##;

/// Serve the two pages with Python's server, or None where there is none.
fn signed_in_site() -> Option<(PathBuf, u16)> {
    if std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        return None;
    }
    let dir = folder(SIGN_IN);
    std::fs::write(dir.join("login.html"), SIGN_IN).unwrap();
    std::fs::write(dir.join("app.html"), BEHIND_SIGN_IN).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    Some((dir, port))
}

fn admin(port: u16, password: &str) -> preview::Login {
    preview::Login {
        url: format!("http://127.0.0.1:{port}/login.html"),
        fields: [
            ("username".to_owned(), "admin".to_owned()),
            ("password".to_owned(), password.to_owned()),
        ]
        .into(),
        submit: None,
    }
}

#[tokio::test]
async fn a_page_behind_a_sign_in_is_seen_after_signing_in() {
    if no_chrome() {
        return;
    }
    let Some((dir, port)) = signed_in_site() else {
        return;
    };
    let serve = format!("python3 -m http.server {port} --bind 127.0.0.1");
    let url = format!("http://127.0.0.1:{port}/app.html");
    let signed_in = preview::preview_signed_in(
        &dir,
        Target::Url(url.clone()),
        Some(&serve),
        Some(&admin(port, "admin123")),
        &dir.join("shots"),
    )
    .await
    .unwrap();
    assert!(
        signed_in.iter().all(|report| report.h1_count == 1),
        "the products page, at every width: {}",
        preview::report(&url, &signed_in)
    );

    let without = preview::preview(
        &dir,
        Target::Url(url.clone()),
        Some(&serve),
        &dir.join("shots"),
    )
    .await
    .unwrap();
    assert_eq!(without[0].h1_count, 0, "only the sign-in form, without it");

    let refused = preview::preview_signed_in(
        &dir,
        Target::Url(url.clone()),
        Some(&serve),
        Some(&admin(port, "wrong")),
        &dir.join("shots"),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(refused.contains("did not go through"), "{refused}");
    assert!(refused.contains("Wrong username or password"), "{refused}");

    let mut unknown = admin(port, "admin123");
    unknown.fields.insert("email".into(), "a@b.c".into());
    let missing = preview::preview_signed_in(
        &dir,
        Target::Url(url),
        Some(&serve),
        Some(&unknown),
        &dir.join("shots"),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(missing.contains("no field for email"), "{missing}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line drawing on a page: its strokes left black (`fixed`), or drawn in
/// `currentColor` from a colour token for each theme.
fn drawing(strokes: &str, themes: &str) -> String {
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Drawing</title>
<style>{themes}
body {{ margin: 0; font: 16px/1.5 system-ui; background: var(--bg); color: var(--text); }}
main {{ max-width: 640px; margin: 0 auto; padding: 24px 16px; }}
.drawing {{ color: var(--ink); }}</style></head>
<body><main><h1>Survey vessel</h1>
<figure class="drawing"><svg width="400" height="120" viewBox="0 0 400 120" style="max-width: 100%; height: auto" role="img" aria-label="Side elevation">
<g fill="none" stroke="{strokes}" stroke-width="2"><path d="M10 80 L390 80"/><path d="M40 80 L60 40 L340 40 L360 80"/><path d="M200 40 L200 10"/></g>
</svg></figure></main></body></html>"##
    )
}

const DARK_ONLY: &str = ":root { --bg: #0f1419; --text: #e6e8eb; --ink: #c9ced6; }";
const BY_SYSTEM: &str = ":root { --bg: #fafaf7; --text: #16150f; --ink: #1a1d21; }
@media (prefers-color-scheme: dark) { :root { --bg: #0c0d0f; --text: #ecebe8; --ink: #c9ced6; } }";
const BY_CLASS: &str = ":root { --bg: #fafaf7; --text: #16150f; --ink: #1a1d21; }
:root.dark { --bg: #0c0d0f; --text: #ecebe8; --ink: #c9ced6; }";

async fn look(page: &str) -> (Vec<WidthReport>, String) {
    let dir = folder(page);
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .unwrap();
    let text = preview::report("index.html", &reports);
    let _ = std::fs::remove_dir_all(&dir);
    (reports, text)
}

#[tokio::test]
async fn a_drawing_left_black_on_a_dark_page_is_reported() {
    if no_chrome() {
        return;
    }
    let (reports, text) = look(&drawing("#000", DARK_ONLY)).await;
    assert!(
        has(
            &reports[2].low_contrast_graphics,
            "drawn in #000000 on #0f1419"
        ),
        "{text}"
    );
    assert!(text.contains("ui-themes"), "{text}");

    let (reports, text) = look(&drawing("currentColor", DARK_ONLY)).await;
    assert!(
        reports.iter().all(|r| r.low_contrast_graphics.is_empty()),
        "{text}"
    );
    assert_eq!(reports.len(), 3, "one theme, three widths: {text}");
}

#[tokio::test]
async fn the_other_theme_is_looked_at_too() {
    if no_chrome() {
        return;
    }
    // A colour fixed for the light theme is fine there and lost in the dark.
    for themes in [BY_SYSTEM, BY_CLASS] {
        let (reports, text) = look(&drawing("#1a1d21", themes)).await;
        assert_eq!(reports.len(), 4, "a pass in the dark theme: {text}");
        assert!(reports[2].low_contrast_graphics.is_empty(), "{text}");
        let dark = &reports[3];
        assert_eq!(dark.theme.as_deref(), Some("dark"), "{text}");
        assert!(!dark.low_contrast_graphics.is_empty(), "{text}");
        assert!(text.contains("1440px in the dark theme"), "{text}");
        assert!(
            dark.screenshot
                .as_ref()
                .is_some_and(|shot| shot.ends_with("1440-dark.png")),
            "{text}"
        );

        // Drawn from the token, it holds in both.
        let (reports, text) = look(&drawing("currentColor", themes)).await;
        assert_eq!(reports.len(), 4, "{text}");
        assert!(
            reports.iter().all(|r| r.low_contrast_graphics.is_empty()),
            "{text}"
        );
    }
}

/// A drawing in `currentColor`, loaded with `<img>` on a dark page: an image
/// never takes the page's colours, so it draws black.
#[tokio::test]
async fn a_drawing_loaded_as_an_image_keeps_its_own_colours() {
    if no_chrome() {
        return;
    }
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 400 120' fill='none' stroke='currentColor' stroke-width='2'><path d='M10 80 L390 80'/><path d='M40 80 L60 40 L340 40 L360 80'/></svg>";
    let page = format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Ship</title>
<style>body {{ margin: 0; background: #0f1419; color: #e6e8eb; font: 16px system-ui; }} main {{ padding: 24px; }} img {{ max-width: 100%; height: auto; }} .dial {{ width: 44px; height: 15px; }}</style></head>
<body><main><h1>Survey vessel</h1><img src="data:image/svg+xml,{svg}" width="400" height="120" alt="The vessel in profile">
<img class="dial" src="data:image/svg+xml,{svg}" alt=""></main></body></html>"##,
        svg = svg.replace('#', "%23")
    );
    let (reports, text) = look(&page).await;
    let wide = &reports[2];
    assert!(
        wide.low_contrast_graphics
            .iter()
            .any(|item| item.contains("400x120") && item.contains("drawn in #000000")),
        "{text}"
    );
    assert!(
        has(&wide.low_contrast_graphics, "44x15"),
        "a thin dial counts too: {text}"
    );
    assert!(text.contains("keeps its own colours"), "{text}");
}

/// A page whose motion goes wrong in every way the motion pass looks for.
const RESTLESS: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Restless</title>
<style>
html { scroll-behavior: smooth; }
body { margin: 0; font: 18px/1.5 system-ui; background: #fff; color: #111; }
section { min-height: 900px; padding: 40px; }
h1 { animation: rise 700ms ease-out both; }
@keyframes rise { from { opacity: 0; transform: translateY(16px); } }
.bar { height: 12px; background: #333; animation: grow 800ms ease-out both; }
@keyframes grow { from { width: 0; } to { width: 600px; } }
.pulse { display: inline-block; animation: pulse 1.2s ease-in-out infinite; }
@keyframes pulse { 50% { opacity: 0.4; } }
.card { padding: 12px; border: 1px solid #999; transition: all 0.3s; }
.block p { opacity: 0; }
.block.in p { opacity: 1; animation: rise 600ms ease-out backwards; }
.never { opacity: 0; }
</style></head>
<body>
<section><h1>Everything moves</h1><div class="bar"></div><span class="pulse">Live</span><div class="card">A card</div></section>
<section class="block"><p>First block of text</p></section>
<section class="block"><p>Second block of text</p></section>
<section class="never"><h2>Pricing that never shows</h2></section>
<section class="block"><p>Last block of text</p></section>
<script>
const seen = new IntersectionObserver(entries => {
  for (const e of entries) if (e.isIntersecting) e.target.classList.add('in');
});
document.querySelectorAll('.block').forEach(b => seen.observe(b));
function tick() { requestAnimationFrame(tick); }
requestAnimationFrame(tick);
window.addEventListener('wheel', () => {}, { passive: false });
</script>
</body></html>"##;

/// The same content with its motion done as the motion skills say.
const CALM: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Calm</title>
<style>
body { margin: 0; font: 18px/1.5 system-ui; background: #fff; color: #111; }
section { min-height: 900px; padding: 40px; }
@keyframes rise { from { opacity: 0; transform: translateY(12px); } }
@media screen and (prefers-reduced-motion: no-preference) {
  html { scroll-behavior: smooth; }
  h1 { animation: rise 700ms ease-out backwards; }
  [data-motion] [data-reveal]:not([data-shown]) p { opacity: 0; }
  [data-motion] [data-reveal][data-shown] p { animation: rise 600ms ease-out backwards; }
}
</style></head>
<body>
<section><h1>Things arrive once</h1></section>
<section data-reveal><p>First block of text</p></section>
<section data-reveal><p>Second block of text</p></section>
<section data-reveal><p>Last block of text</p></section>
<script>
if (matchMedia('(prefers-reduced-motion: no-preference)').matches) {
  document.documentElement.dataset.motion = 'on';
  const seen = new IntersectionObserver(entries => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      e.target.dataset.shown = '';
      seen.unobserve(e.target);
    }
  }, { rootMargin: '0px 0px -10% 0px' });
  document.querySelectorAll('[data-reveal]').forEach(b => seen.observe(b));
}
</script>
</body></html>"##;

/// The motion report, its text, and whether every frame it names was saved.
async fn watch(page: &str) -> (preview::MotionReport, String, bool) {
    let dir = folder(page);
    let (_, motion) = preview::preview_with(
        &dir,
        Target::File(dir.join("index.html")),
        None,
        None,
        true,
        &dir.join("shots"),
    )
    .await
    .unwrap();
    let motion = motion.expect("the motion was watched");
    let text = preview::motion_report(&motion);
    let saved = motion.frames.len() >= 5
        && motion
            .frames
            .iter()
            .all(|frame| std::fs::read(frame).is_ok_and(|png| png.starts_with(b"\x89PNG")));
    let _ = std::fs::remove_dir_all(&dir);
    (motion, text, saved)
}

#[tokio::test]
async fn motion_that_goes_wrong_is_reported() {
    if no_chrome() {
        return;
    }
    let (m, text, saved) = watch(RESTLESS).await;
    assert!(
        has(&m.on_load, "h1") && has(&m.on_load, "moves 16px"),
        "{text}"
    );
    assert!(
        m.on_scroll >= 3,
        "the blocks animate as they arrive: {text}"
    );
    assert!(
        has(&m.hidden, "section.never"),
        "content that never appears: {text}"
    );
    assert_eq!(
        m.hidden.len(),
        1,
        "the blocks that reveal are not hidden: {text}"
    );
    assert!(has(&m.endless, "span.pulse"), "{text}");
    assert!(m.never_stops == 1 && has(&m.loops, "never stops"), "{text}");
    assert!(has(&m.layout_animated, "div.bar: width"), "{text}");
    assert!(has(&m.transition_all, ".card"), "{text}");
    assert_eq!(m.blocking_listeners, 1, "{text}");
    assert!(
        has(&m.reduced_moving, "h1"),
        "moves with reduced motion: {text}"
    );
    assert!(has(&m.reduced_hidden, "section.never"), "{text}");
    assert!(m.reduced_smooth_scroll, "{text}");
    assert!(m.problems() >= 8, "{text}");
    assert!(saved, "the first seconds are photographed: {text}");
    assert!(text.contains("motion-audit"), "{text}");
}

#[tokio::test]
async fn calm_motion_reports_nothing_wrong() {
    if no_chrome() {
        return;
    }
    let (m, text, _) = watch(CALM).await;
    assert!(has(&m.on_load, "h1"), "{text}");
    assert!(m.on_scroll >= 3 && m.scroll_blocks >= 3, "{text}");
    assert_eq!(m.problems(), 0, "{text}");
    assert!(m.hidden.is_empty() && m.reduced_moving.is_empty(), "{text}");
    assert!(!m.reduced_smooth_scroll, "{text}");
    assert!(
        text.contains("with reduced motion: nothing moves"),
        "{text}"
    );
}

/// Built section by section with no shared container: two sections on
/// different edges and widths, glued together, a label over the text, body
/// text too small, and a section a reveal never showed.
const DRIFTING: &str = r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Drifting</title>
<style>
body { margin: 0; font: 13px/1.5 system-ui; color: #1a1a1a; background: #fff; }
.one { max-width: 520px; margin: 0 auto; padding: 0 16px; }
.two { max-width: 1100px; margin: 0 auto; padding: 0 16px; }
.badge { display: block; margin-bottom: -34px; font-size: 12px; }
p { margin: 0; }
.later { opacity: 0; }
section { padding: 4px 0; }
</style></head>
<body>
<main>
<section class="one"><h1>Drifting page</h1><span class="badge">Label on top</span>
<p>A paragraph that the label above runs into, with enough words to be a real paragraph of text.</p></section>
<section class="two"><h2>Work</h2><p>Another paragraph with enough words in it to count as running text on the page, long enough to fill its wide column from one edge to the other. Another paragraph with enough words in it to count as running text on the page, long enough to fill its wide column from one edge to the other. Another paragraph with enough words in it to count as running text on the page, long enough to fill its wide column from one edge to the other. </p></section>
<section class="two later"><h2>Contact</h2><p>Hidden until a reveal that never runs, with enough words to measure.</p></section>
</main>
</body></html>"##;

#[tokio::test]
async fn a_page_built_without_a_shared_layout_is_reported() {
    if no_chrome() {
        return;
    }
    let dir = folder(DRIFTING);
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .unwrap();
    let text = preview::report("index.html", &reports);
    let wide = &reports[2];
    let has = |needle: &str| wide.layout.iter().any(|line| line.contains(needle));
    assert!(has("different left edges"), "{text}");
    assert!(has("content widths differ"), "{text}");
    assert!(has("run into each other"), "{text}");
    assert!(has("overlap"), "{text}");
    assert!(has("still invisible after scrolling"), "{text}");
    assert!(has("body text is 13px"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A page with no gutter, and a theme toggle with words on it: both are
/// reported, at a phone width and on a desktop.
#[tokio::test]
async fn no_gutter_and_a_worded_theme_toggle_are_reported() {
    if no_chrome() {
        return;
    }
    let dir = folder(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Edge</title>
<style>body { margin: 0; font: 16px/1.5 system-ui; }
header { display: flex; justify-content: space-between; }
button { min-height: 44px; padding: 0 12px; }</style></head>
<body><header><strong>Observatory</strong>
<button type="button" aria-label="Switch to dark theme"><svg width="16" height="16" aria-hidden="true"></svg> Switch to dark theme</button></header>
<main><h1>Market workspace</h1><p>Prices and orders.</p></main>
</body></html>"##,
    );
    let reports = preview::preview(&dir, Target::File(dir.clone()), None, &dir.join("shots"))
        .await
        .expect("preview");
    let text = preview::report("index.html", &reports);
    assert!(text.contains("content touches the window's edge"), "{text}");
    assert!(text.contains("the theme toggle shows text"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
