//! Scratch: preview a URL (or a served folder) and print the report.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next().unwrap();
    let out = std::path::PathBuf::from(args.next().unwrap());
    let reports = if first.starts_with("http") {
        enowx_core::preview::preview(
            &std::env::temp_dir(),
            enowx_core::preview::Target::Url(first.clone()),
            None,
            &out,
        )
        .await?
    } else {
        let dir = std::path::PathBuf::from(&first);
        let port = std::net::TcpListener::bind("127.0.0.1:0")?
            .local_addr()?
            .port();
        let url = format!("http://127.0.0.1:{port}/");
        enowx_core::preview::preview(
            &dir,
            enowx_core::preview::Target::Url(url),
            Some(&format!("python3 -m http.server {port} --bind 127.0.0.1")),
            &out,
        )
        .await?
    };
    println!("{}", enowx_core::preview::report(&first, &reports));
    Ok(())
}
