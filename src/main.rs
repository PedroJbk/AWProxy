use std::env;
use std::io::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Duration};

// Módulos de protocolos
mod socks5;
mod websocket;
mod security;
mod tcp_fallback;
mod tls;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let config = parse_args(&args);

    let port = config.port;
    let status = config.status.clone();
    let use_tls = config.tls;
    let ssh_only = config.ssh_only;

    let listener = TcpListener::bind(format!("[::]:{}", port)).await?;
    println!("Servidor iniciado na porta: {}", port);

    if use_tls {
        println!("TLS habilitado");
    }

    if ssh_only {
        println!("Modo SSH apenas habilitado");
    }

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
        // Se a porta for 443 e TLS estiver habilitado, tentamos o handler TLS
        if let Err(e) = tls::handle_tls(client_stream).await {
            eprintln!("Erro TLS na porta 443: {}", e);
        }
        return Ok(());
    }

    if ssh_only {
        // Modo SSH apenas - Resposta tripla para garantir compatibilidade
        let _ = client_stream.write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes()).await;
        
        let mut buffer = [0; 1024];
        let _ = client_stream.read(&mut buffer).await;

        let _ = client_stream.write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes()).await;
        let _ = client_stream.write_all(format!("HTTP/1.1 200 {}\r\n\r\n", status).as_bytes()).await;

        let mut server_stream = match TcpStream::connect("127.0.0.1:22").await {
            Ok(stream) => stream,
            Err(_) => return Ok(()),
        };

        let _ = copy_bidirectional(&mut client_stream, &mut server_stream).await;
        return Ok(());
    }

    // Modo automático - detectar protocolo
    let mut buffer = [0u8; 8192];
    let bytes_read = match timeout(Duration::from_secs(5), client_stream.peek(&mut buffer)).await {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            return tcp_fallback::handle_tcp(client_stream).await.map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "TCP fallback error")
            });
        }
    };

    let data = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();

    if bytes_read > 0 {
        let first_byte = buffer[0];

        match first_byte {
            // SOCKS5
            0x05 => {
                return socks5::handle_socks5(client_stream).await.map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::Other, "SOCKS5 error")
                });
            }

            // TLS Client Hello (0x16)
            0x16 => {
                if let Err(e) = tls::handle_tls(client_stream).await {
                    eprintln!("Erro TLS handshake: {}", e);
                }
                return Ok(());
            }

            // Detecção Universal de HTTP (Aceita QUALQUER método: GET, POST, ACL, PATCH, MOVE, etc.)
            // Verificamos se contém "HTTP/" ou se começa com letras maiúsculas seguidas de espaço (padrão de métodos HTTP)
            _ if data.contains("HTTP/") || is_http_method(&data) => {
                // Se contiver SECURITY ou AUTH, vai para o handler de segurança
                if data.contains("AUTH") || data.contains("SECURITY") || data.contains("Upgrade: security") {
                    return security::handle_security(client_stream).await.map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::Other, "Security error")
                    });
                }
                
                // Caso contrário, trata como WebSocket/HTTP Injector
                return websocket::handle_websocket(client_stream).await.map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::Other, "WebSocket error")
                });
            }

            // SSH Direto
            _ if data.contains("SSH") || data.contains("\x00SSH") => {
                let _ = client_stream.write_all(format!("HTTP/1.1 101 {}\r\n\r\n", status).as_bytes()).await;
                let mut server_stream = match TcpStream::connect("127.0.0.1:22").await {
                    Ok(stream) => stream,
                    Err(_) => return Ok(()),
                };
                let _ = copy_bidirectional(&mut client_stream, &mut server_stream).await;
                return Ok(());
            }

            _ => {}
        }
    }

    // Fallback padrão para TCP (SSH ou VPN)
    tcp_fallback::handle_tcp(client_stream).await.map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::Other, "TCP fallback error")
    })
}

// Função auxiliar para detectar se a string começa com um método HTTP válido
fn is_http_method(data: &str) -> bool {
    let methods = ["GET", "POST", "PUT", "DELETE", "CONNECT", "OPTIONS", "HEAD", "PATCH", "TRACE", "ACL", "MOVE", "COPY", "LOCK", "UNLOCK", "PROPFIND"];
    for method in methods.iter() {
        if data.starts_with(method) {
            return true;
        }
    }
    // Caso seja um método desconhecido mas siga o padrão "METODO /caminho"
    if let Some(first_space) = data.find(' ') {
        let potential_method = &data[..first_space];
        return potential_method.chars().all(|c| c.is_ascii_uppercase());
    }
    false
}

// Configuração do proxy
struct ProxyConfig {
    port: u16,
    status: String,
    tls: bool,
    ssh_only: bool,
}

fn parse_args(args: &[String]) -> ProxyConfig {
    let mut port = 80u16;
    let mut status = "@AWProxy1".to_string();
    let mut tls = false;
    let mut ssh_only = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-p" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(80);
                    i += 1;
                }
            }
            "-s" => {
                if i + 1 < args.len() {
                    status = args[i + 1].clone();
                    i += 1;
                }
            }
            "-t" => {
                tls = true;
            }
            "-ssh" => {
                ssh_only = true;
            }
            _ => {}
        }
        i += 1;
    }

    ProxyConfig { port, status, tls, ssh_only }
}
