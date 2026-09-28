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
section { min-height: 1000px; padding: 16px; }
.to-top { position: fixed; right: 16px; bottom: 16px; z-index: 20; display: grid; place-items: center;
  width: 48px; height: 48px; color: #1a1a1a; background: #fff; border: 1px solid #ccc; border-radius: 6px; text-decoration: none; }
.to-top[hidden] { display: none; }
</style></head>
<body>
<header><a href="#one">One</a> <a href="#two">Two</a></header>
<main><h1>A long page</h1>
<section id="one"><p>First part.</p></section>
<section id="two"><p>Second part.</p></section>
<section><p>Third part.</p></section>
<section><p>Fourth part.</p></section>
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
