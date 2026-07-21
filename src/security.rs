use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use anyhow::Result;
use log::info;

pub async fn handle_security(mut socket: TcpStream) -> Result<()> {
    info!("🔐 SECURITY handshake...");
    
    let mut buf = [0u8; 256];
    let n = socket.read(&mut buf).await?;
    let data = String::from_utf8_lossy(&buf[..n]);
    
    info!("📩 SECURITY: {}", data);
    
    let response = "HTTP/1.1 200 OK\r\n\
                    Connection: Upgrade\r\n\
                    Upgrade: security\r\n\
                    \r\n";
    
    socket.write_all(response.as_bytes()).await?;
    info!("🔐 SECURITY complete!");
    
    Ok(())
}
