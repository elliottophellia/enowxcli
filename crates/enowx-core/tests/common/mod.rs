//! Shared by the tests that stand in for a model provider.

/// Whether a connection to a stand-in provider is a model call. Tools on the
/// machine probe newly opened local ports (a `HEAD /` to see what serves
/// there), and a stand-in that counted those as calls handed its scripted
/// replies to the probe.
pub async fn is_model_call(socket: &tokio::net::TcpStream) -> bool {
    let mut start = [0u8; 4];
    loop {
        match socket.peek(&mut start).await {
            Ok(n) if n >= 4 => return &start == b"POST",
            Ok(0) | Err(_) => return false,
            // Fewer bytes than the method so far: wait for the rest.
            Ok(_) => tokio::task::yield_now().await,
        }
    }
}
