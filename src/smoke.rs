//! Diagnostic-only localhost provider. No user settings or credentials are changed.
use anyhow::{Context, Result};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

pub fn local_provider() -> Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    listener.set_nonblocking(true)?;
    thread::Builder::new().name("smoke-provider".into()).spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut socket, _)) => {
                    let _ = socket.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut buffer = [0u8; 8192];
                    if socket.read(&mut buffer).is_ok() {
                        let body = r#"{"choices":[{"message":{"content":"Hello, this is a translation test."}}]}"#;
                        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                        let _ = socket.write_all(response.as_bytes());
                    }
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(10)),
                Err(_) => return,
            }
        }
    }).context("Impossible de démarrer le provider de diagnostic")?;
    Ok(format!("http://{address}/v1"))
}
