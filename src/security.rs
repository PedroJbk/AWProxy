use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::TcpStream;
use anyhow::Result;
use log::{info, debug, warn};
use tokio::time::{timeout, Duration};

pub async fn handle_security(mut socket: TcpStream, status: &str) -> Result<()> {
    info!("🔐 SECURITY handshake iniciado...");

    // 1. Resposta IMEDIATA: 101 Switching Protocols
    // O Injector espera o 101 antes de enviar o payload completo.
    let resp1 = format!("HTTP/1.1 101 Switching Protocols\r\n\
                         Upgrade: security\r\n\
                         Connection: Upgrade\r\n\
                         \r\n");
    socket.write_all(resp1.as_bytes()).await?;
    debug!("📤 Resposta 1: 101 Switching Protocols");

    // 2. Tentar ler o payload do Injector (ACL / HTTP/1.1 + headers)
    let mut buf = [0u8; 4096];
    let _ = timeout(Duration::from_millis(500), socket.read(&mut buf)).await;

    // 3. Segunda Resposta: 200 OK
    socket.write_all(b"HTTP/1.1 200 OK\r\n\
                        Connection: Upgrade\r\n\
                        Upgrade: security\r\n\
                        \r\n").await?;
    debug!("📤 Resposta 2: 200 OK (Upgrade: security)");

    // 4. Terceira Resposta: 200 com o status configurado
    let final_status = if status.is_empty() { "OK" } else { status };
    let resp3 = format!("HTTP/1.1 200 {}\r\n\r\n", final_status);
    socket.write_all(resp3.as_bytes()).await?;
    debug!("📤 Resposta 3: 200 {}", final_status);

    info!("🔐 SECURITY handshake completo! Status: {}", final_status);

    // Detecção de backend: tentar SSH primeiro, fallback para VPN
    info!("🔗 Conectando ao backend SSH (127.0.0.1:22)...");
    let mut remote = match TcpStream::connect("127.0.0.1:22").await {
        Ok(s) => s,
        Err(e) => {
            warn!("⚠️ SSH falhou ({}), tentando VPN (127.0.0.1:1194)...", e);
            TcpStream::connect("127.0.0.1:1194").await?
        }
    };

    info!("✅ Túnel SECURITY iniciado!");
    let _ = copy_bidirectional(&mut socket, &mut remote).await;
    info!("🔚 Túnel SECURITY finalizado.");

    Ok(())
}
