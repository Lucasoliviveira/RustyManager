use std::env;
use std::io::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use sha1::{Sha1, Digest};
use base64::engine::general_purpose;
use base64::Engine as _;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let port = get_port();
    let listener = TcpListener::bind(format!("[::]:{}", port)).await?;
    println!("Proxy WebSocket iniciado na porta {}", port);

    loop {
        let (mut client_stream, addr) = listener.accept().await?;
        println!("Nova conexão de {}", addr);

        tokio::spawn(async move {
            if let Err(e) = handle_client(&mut client_stream).await {
                println!("Erro na conexão {}: {}", addr, e);
            }
        });
    }
}

async fn handle_client(client_stream: &mut TcpStream) -> Result<(), Error> {
    // Executa handshake WebSocket
    websocket_handshake(client_stream).await?;

    // Endereço do servidor para encaminhar dados após handshake
    let server_addr = "127.0.0.1:22"; // Alterar para o destino desejado
    let mut server_stream = TcpStream::connect(server_addr).await?;
    println!("Conectado ao servidor {}", server_addr);

    // Proxy bidirecional dos dados pós-handshake
    let (mut client_read, mut client_write) = client_stream.split();
    let (mut server_read, mut server_write) = server_stream.split();

    tokio::try_join!(
        tokio::io::copy(&mut client_read, &mut server_write),
        tokio::io::copy(&mut server_read, &mut client_write),
    )?;

    Ok(())
}

async fn websocket_handshake(stream: &mut TcpStream) -> Result<(), Error> {
    let mut reader = BufReader::new(stream);
    let mut request = String::new();

    // Lê os headers HTTP do handshake até linha vazia
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;
        if n == 0 || line == "\r\n" {
            break;
        }
        request.push_str(&line);
    }

    // Extrai o valor do header Sec-WebSocket-Key
    let key = request.lines()
        .find_map(|line| {
            if line.to_lowercase().starts_with("sec-websocket-key:") {
                Some(line[18..].trim())
            } else {
                None
            }
        })
        .ok_or_else(|| Error::new(std::io::ErrorKind::Other, "Sec-WebSocket-Key não encontrado"))?;

    // Calcula a chave de aceitação para o handshake
    let mut sha1 = Sha1::new();
    sha1.update(format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", key));
    let result = sha1.finalize();
    let accept_key = general_purpose::STANDARD.encode(&result);

    // Monta e envia a resposta de handshake 101 Switching Protocols
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept_key
    );

    reader.get_mut().write_all(response.as_bytes()).await?;
    Ok(())
}

fn get_port() -> u16 {
    let args: Vec<String> = env::args().collect();
    let mut port = 80;
    for i in 1..args.len() {
        if args[i] == "--port" && i + 1 < args.len() {
            port = args[i + 1].parse().unwrap_or(80);
        }
    }
    port
}
