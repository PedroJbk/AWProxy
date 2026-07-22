use std::env;
use std::io::Error;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

mod socks5;
mod websocket;
mod security;
mod tcp_fallback;
mod tls;
mod protocol;
mod ssh;

#[tokio::main]
async fn main() -> Result<(), Error> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let config = parse_args(&args);

    let port = config.port;
    let status = config.status.clone();
    let use_tls = config.tls;
    let ssh_only = config.ssh_only;

    log::info!("🚀 AWProxy iniciando na porta {} | Status: '{}'", port, status);

    let listener = TcpListener::bind(format!("[::]:{}", port)).await?;
    println!("Servidor iniciado na porta: {}", port);

    start_proxy(listener, status, ssh_only, use_tls).await;
    Ok(())
}

async fn start_proxy(listener: TcpListener, status: String, ssh_only: bool, use_tls: bool) {
    loop {
        let status_clone = status.clone();
        match listener.accept().await {
            Ok((client_stream, addr)) => {
                tokio::spawn(async move {
                    if let Err(e) = handle_client(client_stream, &status_clone, ssh_only, use_tls).await {
                        eprintln!("Erro ao processar cliente {}: {}", addr, e);
                    }
                });
            }
            Err(e) => eprintln!("Erro ao aceitar conexão: {}", e),
        }
    }
}

async fn handle_client(mut client_stream: TcpStream, status: &str, ssh_only: bool, use_tls: bool) -> Result<(), Error> {

    // Modo TLS/HTTPS: apenas passthrough
    if use_tls {
        return tls::handle_tls(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
    }

    // ============================================================
    // FLUXO PRINCIPAL: Baseado no BSProxy que funciona perfeitamente
    // ============================================================

    // Passo 1: Detectar protocolo via Peek
    let mut peek_buf = vec![0u8; 4096];
    let bytes_peeked = match timeout(Duration::from_millis(200), client_stream.peek(&mut peek_buf)).await {
        Ok(Ok(n)) => n,
        _ => 0,
    };

    if bytes_peeked > 0 {
        let data = String::from_utf8_lossy(&peek_buf[..bytes_peeked]);
        let data_upper = data.to_uppercase();
        let first_byte = peek_buf[0];

        log::debug!("🔍 Peek ({} bytes): {:?}", bytes_peeked, &data[..std::cmp::min(bytes_peeked, 200)]);

        // 1. SOCKS5 (primeiro byte = 0x05)
        if first_byte == 0x05 {
            return socks5::handle_socks5(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
        }

        // 2. TLS/SSL Handshake (0x16)
        if first_byte == 0x16 {
            return tls::handle_tls(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
        }

        // 3. SECURITY request (antes de qualquer handshake HTTP)
        if is_security_request(&data_upper) {
            log::info!("🔐 SECURITY detectado - ativando handshake");
            return security::handle_security(client_stream, status).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
        }

        // 4. HTTP / WebSocket / Custom Methods
        if is_http_request(&data) {
            // SSH-Only mode: encaminhar direto para SSH após handshake simples
            if ssh_only {
                // Handshake básico: 101 -> read -> 200 (como BSProxy)
                client_stream
                    .write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes())
                    .await?;
                let mut buffer = vec![0; 1024];
                let _ = client_stream.read(&mut buffer).await;
                client_stream
                    .write_all(format!("HTTP/1.1 200 {}\r\n\r\n", status).as_bytes())
                    .await?;
                // Encaminhar para backend
                return tunnel_to_backend(client_stream, &data).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
            }
            return websocket::handle_websocket(client_stream, status).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
        }

        // 5. SSH puro (não é HTTP, não é SOCKS5, não é TLS)
        if ssh_only || data.starts_with("SSH-") || first_byte < 0x20 {
            // SSH direto: sem handshake HTTP
            let mut server_stream = match TcpStream::connect("127.0.0.1:22").await {
                Ok(s) => s,
                Err(_) => {
                    // Fallback VPN
                    match TcpStream::connect("127.0.0.1:1194").await {
                        Ok(s) => s,
                        Err(_) => return Ok(()),
                    }
                },
            };
            let _ = tokio::io::copy_bidirectional(&mut client_stream, &mut server_stream).await;
            return Ok(());
        }
    }

    // Fallback: TCP puro -> tentar SSH, depois VPN
    tcp_fallback::handle_tcp(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e))
}

/// Túnel bidirecional usando Arc<Mutex> como no BSProxy que funciona
async fn tunnel_to_backend(client_stream: TcpStream, data: &str) -> Result<(), Error> {
    // Detectar backend: se contém "SSH" ou está vazio, usa SSH; senão VPN
    let addr_proxy = if data.contains("SSH") || data.is_empty() {
        "127.0.0.1:22"
    } else {
        "127.0.0.1:1194"
    };

    let server_connect = TcpStream::connect(addr_proxy).await;
    if server_connect.is_err() {
        // Tentar o outro backend como fallback
        let alt = if addr_proxy == "127.0.0.1:22" { "127.0.0.1:1194" } else { "127.0.0.1:22" };
        log::warn!("⚠️ Falha em {}, tentando {}", addr_proxy, alt);
        match TcpStream::connect(alt).await {
            Ok(s) => {
                let (cr, cw) = client_stream.into_split();
                let (sr, sw) = s.into_split();
                let cr = Arc::new(Mutex::new(cr));
                let cw = Arc::new(Mutex::new(cw));
                let sr = Arc::new(Mutex::new(sr));
                let sw = Arc::new(Mutex::new(sw));
                tokio::try_join!(transfer_data(cr, sw), transfer_data(sr, cw))?;
                Ok(())
            }
            Err(_) => {
                log::warn!("⚠️ Ambos backends falharam");
                Ok(())
            }
        }
    } else {
        let server_stream = server_connect?;
        let (cr, cw) = client_stream.into_split();
        let (sr, sw) = server_stream.into_split();
        let cr = Arc::new(Mutex::new(cr));
        let cw = Arc::new(Mutex::new(cw));
        let sr = Arc::new(Mutex::new(sr));
        let sw = Arc::new(Mutex::new(sw));
        tokio::try_join!(transfer_data(cr, sw), transfer_data(sr, cw))?;
        Ok(())
    }
}

async fn transfer_data(
    read_stream: Arc<Mutex<tokio::net::tcp::OwnedReadHalf>>,
    write_stream: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
) -> Result<(), Error> {
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

/// Detecção robusta de requisições SECURITY/ACL
fn is_security_request(data_upper: &str) -> bool {
    if data_upper.starts_with("SECURITY") { return true; }
    if data_upper.starts_with("ACL") { return true; }
    if data_upper.starts_with("PATCH") { return true; }
    if data_upper.starts_with("PROPFIND") { return true; }
    if data_upper.contains("SECURITY") && (data_upper.contains("UPGRADE:") || data_upper.contains("X-")) { return true; }
    if data_upper.contains("[SPLIT]ACL") || data_upper.contains("[SPLIT]SECURITY") { return true; }
    if data_upper.contains("UPGRADE: SECURITY") || data_upper.contains("UPGRADE:SECURITY") { return true; }
    false
}

fn is_http_request(data: &str) -> bool {
    let methods = ["GET", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "HEAD", "PATCH", "ACL", "MOVE", "PROPFIND", "SECURITY"];
    let data_upper = data.to_uppercase();
    for m in methods {
        if data_upper.starts_with(m) || data_upper.contains(&format!("[SPLIT]{}", m)) { return true; }
    }
    data_upper.contains("HTTP/1.") || data_upper.contains("HTTP/2.")
}

struct ProxyConfig {
    port: u16,
    status: String,
    tls: bool,
    ssh_only: bool,
}

fn parse_args(args: &[String]) -> ProxyConfig {
    let mut port = 80u16;
    let mut status = "200 OK".to_string();
    let mut tls = false;
    let mut ssh_only = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-p" => { if i+1 < args.len() { port = args[i+1].parse().unwrap_or(80); i+=1; } }
            "-s" => { if i+1 < args.len() { status = args[i+1].clone(); i+=1; } }
            "-t" => { tls = true; }
            "-ssh" => { ssh_only = true; }
            _ => {}
        }
        i += 1;
    }
    ProxyConfig { port, status, tls, ssh_only }
}
