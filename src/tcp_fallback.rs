use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use anyhow::Result;
use log::info;

pub async fn handle_tcp(mut socket: TcpStream) -> Result<()> {
    info!("📦 TCP echo");
    
    let mut buf = [0u8; 1024];
    loop {
        match socket.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let msg = String::from_utf8_lossy(&buf[..n]);
                let response = format!("TCP: {}", msg);
                socket.write_all(response.as_bytes()).await?;
            }
            Err(e) => anyhow::bail!("TCP error: {}", e),
        }
    }
    
    Ok(())
}
