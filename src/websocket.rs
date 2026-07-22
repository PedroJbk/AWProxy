use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::TcpStream;
use anyhow::Result;
use log::info;
use tokio::time::{timeout, Duration};

pub async fn handle_websocket(mut socket: TcpStream, status: &str) -> Result<()> {
    info!("🌐 WebSocket Tripla Resposta Handshake...");

    // Espiar headers para log apenas (peek não consome)
    let mut peek_buf = vec![0u8; 4096];
    if let Ok(Ok(n)) = timeout(Duration::from_millis(200), socket.peek(&mut peek_buf)).await {
        if n > 0 {
            let data = String::from_utf8_lossy(&peek_buf[..n]);
            log::debug!("📥 WebSocket Request ({} bytes): {}", n, data.trim());
        }
    }

    // ============================================================
    // ETAPA 1: Primeira Resposta - 101 Switching Protocols
    // Envia IMEDIATAMENTE sem consumir headers
    // ============================================================
    let response1 = format!("HTTP/1.1 101 Switching Protocols\r\n\
                             Upgrade: websocket\r\n\
                             Connection: Upgrade\r\n\
                             Status: {}\r\n\
                             \r\n", status);
    socket.write_all(response1.as_bytes()).await?;
    socket.flush().await?;
    info!("📤 Resposta 1: 101 Switching Protocols (status: {})", status);

    // ============================================================
    // ETAPA 2: Leitura do payload do Injector
    // ============================================================
    let mut buffer = [0u8; 4096];
    let _ = timeout(Duration::from_millis(500), socket.read(&mut buffer)).await;

    // ============================================================
    // ETAPA 3: Segunda Resposta - 101
    // ============================================================
    let response2 = format!("HTTP/1.1 101 Switching Protocols\r\n\
                             Upgrade: websocket\r\n\
                             Connection: Upgrade\r\n\
                             \r\n");
    socket.write_all(response2.as_bytes()).await?;
    socket.flush().await?;
    info!("📤 Resposta 2: 101 Switching Protocols");

    // ============================================================
    // ETAPA 4: Terceira Resposta - 200
    // ============================================================
    let response3 = format!("HTTP/1.1 200 {}\r\n\
                             Connection: keep-alive\r\n\
                             \r\n", status);
    socket.write_all(response3.as_bytes()).await?;
    socket.flush().await?;
    info!("📤 Resposta 3: 200 {}", status);

    // ============================================================
    // ETAPA 5: Detecção de protocolo (SSH vs VPN) usando Peek
    // ============================================================
    let mut peek_buf = vec![0u8; 8192];
    let peek_result = timeout(Duration::from_secs(2), socket.peek(&mut peek_buf)).await;
    let addr_proxy = match peek_result {
        Ok(Ok(n)) if n > 0 => {
            let data = String::from_utf8_lossy(&peek_buf[..n]);
            if data.contains("SSH") || data.starts_with("SSH-") {
                "127.0.0.1:22"
            } else {
                "127.0.0.1:1194"
            }
        }
        _ => "127.0.0.1:22",
    };

    info!("🔗 Conectando ao backend: {}", addr_proxy);

    // ============================================================
    // ETAPA 6: Conectar ao backend e iniciar túnel
    // ============================================================
    let mut remote = match TcpStream::connect(addr_proxy).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("⚠️ Falha ao conectar em {}: {}, tentando SSH", addr_proxy, e);
            TcpStream::connect("127.0.0.1:22").await?
        }
    };

    info!("✅ WebSocket Túnel iniciado.");
    let _ = copy_bidirectional(&mut socket, &mut remote).await;
    info!("🔚 WebSocket Túnel finalizado.");

    Ok(())
}
