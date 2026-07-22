use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use anyhow::Result;
use log::info;

pub async fn handle_tcp(socket: TcpStream) -> Result<()> {
    info!("📦 TCP fallback - encaminhando para SSH...");
    
    // Tentar SSH primeiro
    match TcpStream::connect("127.0.0.1:22").await {
        Ok(remote) => {
            info!("✅ TCP fallback -> SSH conectado");
            let _ = copy_bidirectional(&socket, &remote).await;
            info!("🔚 Conexão TCP fallback->SSH encerrada");
            Ok(())
        }
        Err(_) => {
            // Se SSH falhar, tentar VPN
            info!("⚠️ SSH falhou, tentando VPN...");
            match TcpStream::connect("127.0.0.1:1194").await {
                Ok(remote) => {
                    info!("✅ TCP fallback -> VPN conectado");
                    let _ = copy_bidirectional(&socket, &remote).await;
                    Ok(())
                }
                Err(e) => {
                    info!("❌ Falha TCP fallback: {}", e);
                    Err(e.into())
                }
            }
        }
    }
}
