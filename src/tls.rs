use tokio::net::TcpStream;
use tokio::io::copy_bidirectional;
use anyhow::Result;
use log::info;

/// Handler para TLS/HTTPS que realiza passthrough para o serviço local.
/// Isso permite que o próprio aplicativo VPN gerencie a criptografia.
pub async fn handle_tls(mut socket: TcpStream) -> Result<()> {
    info!("🛡️ TLS/HTTPS passthrough detectado...");

    // Tentar SSH primeiro (Porta 22)
    match TcpStream::connect("127.0.0.1:22").await {
        Ok(mut remote) => {
            info!("✅ TLS Passthrough -> SSH:22 conectado");
            let _ = copy_bidirectional(&mut socket, &mut remote).await;
            Ok(())
        }
        Err(_) => {
            // Se SSH falhar, tentar VPN (Porta 1194)
            match TcpStream::connect("127.0.0.1:1194").await {
                Ok(mut remote) => {
                    info!("✅ TLS Passthrough -> VPN:1194 conectado");
                    let _ = copy_bidirectional(&mut socket, &mut remote).await;
                    Ok(())
                }
                Err(e) => {
                    info!("❌ Falha no passthrough TLS: {}", e);
                    Err(e.into())
                }
            }
        }
    }
}
