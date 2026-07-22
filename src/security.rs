use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use anyhow::Result;
use log::info;

pub async fn handle_security(mut socket: TcpStream, status: &str) -> Result<()> {
    info!("🔐 SECURITY handshake...");

    // 1. Enviar 101 Switching Protocols (como no BSProxy)
    socket
        .write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes())
        .await?;
    info!("📤 SECURITY: 101 {}", status);

    // 2. Ler payload do Injector
    let mut buf = [0u8; 256];
    let n = socket.read(&mut buf).await?;
    let data = String::from_utf8_lossy(&buf[..n]);
    info!("📩 SECURITY payload: {}", data.trim());

    // 3. Enviar 200 OK com Upgrade (como no BSProxy)
    let response = format!("HTTP/1.1 200 {}\r\n\
                            Connection: Upgrade\r\n\
                            Upgrade: security\r\n\
                            \r\n", status);
    socket.write_all(response.as_bytes()).await?;
    info!("🔐 SECURITY complete! Status: {}", status);

    // 4. Detectar backend: se contém "SSH" ou está vazio, usa SSH; senão VPN
    let addr_proxy = if data.contains("SSH") || data.is_empty() {
        "127.0.0.1:22"
    } else {
        "127.0.0.1:1194"
    };

    info!("🔗 Conectando ao backend: {}", addr_proxy);

    let server_connect = TcpStream::connect(addr_proxy).await;
    if server_connect.is_err() {
        let alt = if addr_proxy == "127.0.0.1:22" { "127.0.0.1:1194" } else { "127.0.0.1:22" };
        info!("⚠️ Falha em {}, tentando {}", addr_proxy, alt);
        match TcpStream::connect(alt).await {
            Ok(s) => {
                info!("✅ SECURITY túnel iniciado para {}", alt);
                let (cr, cw) = socket.into_split();
                let (sr, sw) = s.into_split();
                let cr = Arc::new(Mutex::new(cr));
                let cw = Arc::new(Mutex::new(cw));
                let sr = Arc::new(Mutex::new(sr));
                let sw = Arc::new(Mutex::new(sw));
                tokio::try_join!(transfer_data(cr, sw), transfer_data(sr, cw))?;
                info!("🔚 SECURITY túnel finalizado.");
                Ok(())
            }
            Err(e) => {
                info!("❌ Ambos backends falharam: {}", e);
                Ok(())
            }
        }
    } else {
        let server_stream = server_connect?;
        info!("✅ SECURITY túnel iniciado para {}", addr_proxy);
        let (cr, cw) = socket.into_split();
        let (sr, sw) = server_stream.into_split();
        let cr = Arc::new(Mutex::new(cr));
        let cw = Arc::new(Mutex::new(cw));
        let sr = Arc::new(Mutex::new(sr));
        let sw = Arc::new(Mutex::new(sw));
        tokio::try_join!(transfer_data(cr, sw), transfer_data(sr, cw))?;
        info!("🔚 SECURITY túnel finalizado.");
        Ok(())
    }
}

async fn transfer_data(
    read_stream: Arc<Mutex<tokio::net::tcp::OwnedReadHalf>>,
    write_stream: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
) -> Result<()> {
    let mut buffer = [0; 8192];
    loop {
        let bytes_read = {
            let mut read_guard = read_stream.lock().await;
            read_guard.read(&mut buffer).await?
        };
        if bytes_read == 0 {
            break;
        }
        let mut write_guard = write_stream.lock().await;
        write_guard.write_all(&buffer[..bytes_read]).await?;
    }
    Ok(())
}
