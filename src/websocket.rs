use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use anyhow::Result;
use log::info;

/// Lê e descarta os headers HTTP até encontrar \r\n\r\n
async fn consume_http_headers(socket: &mut TcpStream) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 1];
    loop {
        socket.read_exact(&mut tmp).await?;
        buf.push(tmp[0]);
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
        if buf.len() > 8192 {
            break;
        }
    }
    Ok(())
}

pub async fn handle_websocket(mut socket: TcpStream, status: &str) -> Result<()> {
    info!("🌐 WebSocket/HTTP handshake...");

    // Consumir headers HTTP (qualquer método)
    consume_http_headers(&mut socket).await?;

    // Resposta de upgrade WebSocket (101 Switching Protocols)
    let response = format!("HTTP/1.1 101 {}\r\n\
                            Upgrade: websocket\r\n\
                            Connection: Upgrade\r\n\
                            \r\n", status);
    socket.write_all(response.as_bytes()).await?;
    info!("🌐 WebSocket handshake complete! Encaminhando para backend...");

    // Encaminhar para SSH (porta 22) - padrão do BSProxy
    let target = "127.0.0.1:22";

    match TcpStream::connect(target).await {
        Ok(remote) => {
            info!("✅ Conectado ao backend na porta {}", target);
            let (cr, cw) = socket.into_split();
            let (sr, sw) = remote.into_split();
            let cr = Arc::new(Mutex::new(cr));
            let cw = Arc::new(Mutex::new(cw));
            let sr = Arc::new(Mutex::new(sr));
            let sw = Arc::new(Mutex::new(sw));
            tokio::try_join!(transfer_data(cr, sw), transfer_data(sr, cw))?;
            info!("🔚 Conexão WebSocket finalizada");
            Ok(())
        }
        Err(e) => {
            info!("❌ Falha ao conectar ao backend: {}", e);
            // Tentar VPN como fallback
            match TcpStream::connect("127.0.0.1:1194").await {
                Ok(remote) => {
                    info!("✅ Fallback para VPN:1194");
                    let (cr, cw) = socket.into_split();
                    let (sr, sw) = remote.into_split();
                    let cr = Arc::new(Mutex::new(cr));
                    let cw = Arc::new(Mutex::new(cw));
                    let sr = Arc::new(Mutex::new(sr));
                    let sw = Arc::new(Mutex::new(sw));
                    tokio::try_join!(transfer_data(cr, sw), transfer_data(sr, cw))?;
                    Ok(())
                }
                Err(e2) => {
                    anyhow::bail!("Backend connection failed: {}", e2)
                }
            }
        }
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
