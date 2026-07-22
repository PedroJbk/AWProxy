use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::TcpStream;
use anyhow::Result;
use log::{info, debug, warn};
use tokio::time::{timeout, Duration};

pub async fn handle_security(mut socket: TcpStream, status: &str) -> Result<()> {
    info!("🔐 SECURITY handshake (Tripla Resposta)...");

    // ============================================================
    // ETAPA 1: Primeira Resposta - 101 Switching Protocols
    // Envia IMEDIATAMENTE sem ler nada do cliente
    // ============================================================
    let response1 = format!("HTTP/1.1 101 Switching Protocols\r\n\
                             Upgrade: security\r\n\
                             Connection: Upgrade\r\n\
                             Status: {}\r\n\
                             \r\n", status);
    socket.write_all(response1.as_bytes()).await?;
    socket.flush().await?;
    info!("📤 Resposta 1: 101 Switching Protocols (status: {})", status);

    // ============================================================
    // ETAPA 2: Leitura do payload do Injector (headers enviados)
    // ============================================================
    let mut buffer = [0u8; 4096];
    let _ = timeout(Duration::from_millis(500), socket.read(&mut buffer)).await;

    // ============================================================
    // ETAPA 3: Segunda Resposta - 200 OK com Upgrade
    // ============================================================
    let response2 = "HTTP/1.1 200 OK\r\n\
                     Connection: Upgrade\r\n\
                     Upgrade: security\r\n\
                     \r\n";
    socket.write_all(response2.as_bytes()).await?;
    socket.flush().await?;
    info!("📤 Resposta 2: 200 OK (Upgrade: security)");

    // ============================================================
    // ETAPA 4: Terceira Resposta - 200 com status
    // ============================================================
    let response3 = format!("HTTP/1.1 200 {}\r\n\r\n", status);
    socket.write_all(response3.as_bytes()).await?;
    socket.flush().await?;
    info!("📤 Resposta 3: 200 {} (final)", status);

    info!("🔐 SECURITY handshake completo! (3 respostas enviadas)");

    // ============================================================
    // ETAPA 5: Detecção de backend (SSH vs VPN)
    // ============================================================
    let mut peek_buffer = [0u8; 1024];
    let addr_proxy = match timeout(Duration::from_millis(500), socket.peek(&mut peek_buffer)).await {
        Ok(Ok(n)) if n > 0 => {
            let data = String::from_utf8_lossy(&peek_buffer[..n]);
            debug!("🔍 Peek SECURITY ({} bytes): {:?}", n, &data[..std::cmp::min(n, 200)]);
            if data.contains("SSH") || data.starts_with("SSH-") {
                "127.0.0.1:22"
            } else {
                "127.0.0.1:1194"
            }
        }
        _ => {
            info!("⚠️ Peek timeout/vazio, usando SSH fallback");
            "127.0.0.1:22"
        }
    };

    info!("🔗 SECURITY -> Conectando ao backend: {}", addr_proxy);

    // ============================================================
    // ETAPA 6: Túnel bidirecional
    // ============================================================
    let mut remote = match TcpStream::connect(addr_proxy).await {
        Ok(s) => s,
        Err(e) => {
            warn!("❌ Falha ao conectar em {}: {}, tentando SSH fallback", addr_proxy, e);
            TcpStream::connect("127.0.0.1:22").await?
        }
    };

    info!("✅ SECURITY Túnel iniciado!");
    let _ = copy_bidirectional(&mut socket, &mut remote).await;
    info!("🔚 SECURITY Túnel finalizado.");

    Ok(())
}
