use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use anyhow::Result;
use log::info;

/// Lê e descarta os headers HTTP até encontrar o fim da requisição (\r\n\r\n ou \n\n)
async fn consume_http_headers(socket: &mut TcpStream) -> std::io::Result<()> {
    let mut buf = [0u8; 4096];
    let mut total_read = 0;

    loop {
        // Usamos read em vez de read_exact para não travar se o payload for curto
        let n = socket.read(&mut buf[total_read..]).await?;
        if n == 0 { break; }
        total_read += n;

        let current_data = &buf[..total_read];
        if current_data.windows(4).any(|w| w == b"\r\n\r\n") || 
           current_data.windows(2).any(|w| w == b"\n\n") {
            break;
        }
        if total_read >= 4096 { break; }
    }
    Ok(())
}

pub async fn handle_websocket(mut socket: TcpStream) -> Result<()> {
    info!("🌐 WebSocket/HTTP handshake universal...");
    
    // Consumir os headers da requisição (independente do método)
    let _ = consume_http_headers(&mut socket).await;
    
    // Resposta de compatibilidade: 200 OK + 101 Switching Protocols
    // Enviamos tudo em um único write para evitar fragmentação que trava alguns apps
    let response = "HTTP/1.1 200 OK\r\n\r\n\
                    HTTP/1.1 101 Switching Protocols\r\n\
                    Upgrade: websocket\r\n\
                    Connection: Upgrade\r\n\
                    Sec-WebSocket-Accept: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                    \r\n";
    
    socket.write_all(response.as_bytes()).await?;
    info!("🌐 Handshake concluído. Iniciando túnel...");
    
    // Encaminhar para SSH local (Porta 22)
    let target = "127.0.0.1:22";
    
    match TcpStream::connect(target).await {
        Ok(remote) => {
            info!("✅ Túnel estabelecido: Cliente <-> SSH:22");
            let (mut client_reader, mut client_writer) = socket.into_split();
            let (mut remote_reader, mut remote_writer) = remote.into_split();
            
            // Usamos copy bidirecional para manter a conexão viva
            let _ = tokio::try_join!(
                tokio::io::copy(&mut client_reader, &mut remote_writer),
                tokio::io::copy(&mut remote_reader, &mut client_writer)
            );
            
            info!("🔚 Conexão encerrada");
            Ok(())
        }
        Err(e) => {
            info!("❌ Falha ao conectar ao SSH: {}. Tentando VPN:1194...", e);
            match TcpStream::connect("127.0.0.1:1194").await {
                Ok(remote) => {
                    info!("✅ Túnel estabelecido: Cliente <-> VPN:1194");
                    let (mut client_reader, mut client_writer) = socket.into_split();
                    let (mut remote_reader, mut remote_writer) = remote.into_split();
                    let _ = tokio::try_join!(
                        tokio::io::copy(&mut client_reader, &mut remote_writer),
                        tokio::io::copy(&mut remote_reader, &mut client_writer)
                    );
                    Ok(())
                }
                Err(e2) => {
                    anyhow::bail!("Falha total na conexão local: SSH={}, VPN={}", e, e2)
                }
            }
        }
    }
}
