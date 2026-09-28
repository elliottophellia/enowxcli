//! Scratch: preview a URL (or a served folder) and print the report.
//! `look URL OUT [SIGN_IN_URL USER PASSWORD]` signs in first.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next().unwrap();
    let out = std::path::PathBuf::from(args.next().unwrap());
    // Optional: a sign-in page, a user name and a password to sign in with.
    let login = match (args.next(), args.next(), args.next()) {
        (Some(url), Some(user), Some(pass)) => Some(enowx_core::preview::Login {
            url,
            fields: [("username".to_owned(), user), ("password".to_owned(), pass)].into(),
            submit: None,
        }),
        _ => None,
    };
    let reports = if first.starts_with("http") {
        enowx_core::preview::preview_signed_in(
            &std::env::temp_dir(),
            enowx_core::preview::Target::Url(first.clone()),
            None,
            login.as_ref(),
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
