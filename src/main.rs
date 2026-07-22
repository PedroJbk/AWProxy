use std::env;
use std::io::Error;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

mod socks5;
mod websocket;
mod security;
mod tcp_fallback;
mod tls;

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

    if use_tls {
        return tls::handle_tls(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
    }

    if ssh_only {
        let mut buffer = [0u8; 4096];
        let bytes_peeked = match timeout(Duration::from_millis(500), client_stream.peek(&mut buffer)).await {
            Ok(Ok(n)) => n,
            _ => 0,
        };

        if bytes_peeked > 0 {
            let data = String::from_utf8_lossy(&buffer[..bytes_peeked]);
            let data_upper = data.to_uppercase();

            // Verificar SECURITY antes de tratar como HTTP normal
            if is_security_request(&data_upper) {
                log::info!("🔐 SECURITY detectado (SSH-Only)");
                return security::handle_security(client_stream, status).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
            }

            if is_http_request(&data) {
                return websocket::handle_websocket(client_stream, status).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
            }
        }

        // Se não for HTTP, faz o túnel direto
        let mut server_stream = match TcpStream::connect("127.0.0.1:22").await {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };
        let _ = copy_bidirectional(&mut client_stream, &mut server_stream).await;
        return Ok(());
    }

    // Espiada rápida no buffer (Peek)
    let mut buffer = [0u8; 4096];
    let bytes_read = match timeout(Duration::from_millis(500), client_stream.peek(&mut buffer)).await {
        Ok(Ok(n)) => n,
        _ => 0,
    };

    if bytes_read > 0 {
        let first_byte = buffer[0];
        let data = String::from_utf8_lossy(&buffer[..bytes_read]);
        let data_upper = data.to_uppercase();

        log::debug!("🔍 Peek ({} bytes): {:?}", bytes_read, &data[..std::cmp::min(bytes_read, 200)]);

        // 1. SOCKS5
        if first_byte == 0x05 {
            return socks5::handle_socks5(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
        }

        // 2. TLS/SSL Handshake (0x16)
        if first_byte == 0x16 {
            return tls::handle_tls(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
        }

        // 3. HTTP / WebSocket / Custom Methods
        if is_http_request(&data) {
            // === DETECÇÃO DE SECURITY ===
            if is_security_request(&data_upper) {
                log::info!("🔐 SECURITY detectado - ativando handshake");
                return security::handle_security(client_stream, status).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
            }
            // WebSocket padrão (Tripla Resposta)
            return websocket::handle_websocket(client_stream, status).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e));
        }
    }

    // Fallback: TCP puro
    tcp_fallback::handle_tcp(client_stream).await.map_err(|e| Error::new(std::io::ErrorKind::Other, e))
}

/// Detecção robusta de requisições SECURITY/ACL
/// Cobre: método SECURITY, método ACL, headers com SECURITY, padrão [SPLIT]
fn is_security_request(data_upper: &str) -> bool {
    // Método SECURITY
    if data_upper.starts_with("SECURITY") { return true; }
    // Método ACL (usado pelo HTTP Injector)
    if data_upper.starts_with("ACL") { return true; }
    // Método PATCH
    if data_upper.starts_with("PATCH") { return true; }
    // Método PROPFIND
    if data_upper.starts_with("PROPFIND") { return true; }
    // Headers com SECURITY
    if data_upper.contains("SECURITY") && (data_upper.contains("UPGRADE:") || data_upper.contains("X-")) { return true; }
    // Padrão [SPLIT]ACL ou [SPLIT]SECURITY
    if data_upper.contains("[SPLIT]ACL") || data_upper.contains("[SPLIT]SECURITY") { return true; }
    // Upgrade: security no header
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
