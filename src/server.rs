use serde_json::json;
use std::io;
use tokio::io::AsyncReadExt as _;
use tokio::net::TcpStream;
use tracing::debug;

use crate::config::Config;
use crate::models::VersionInfo;
use crate::protocol::{read_varint, send_pong, write_response};

pub async fn handle_client(mut stream: TcpStream, config: Config) -> io::Result<()> {
    let _packet_length = read_varint(&mut stream).await?;
    let packet_id = read_varint(&mut stream).await?;

    if packet_id == 0x00 {
        let protocol = read_varint(&mut stream).await?;
        let hostname_len = read_varint(&mut stream).await?;
        let mut hostname = vec![0; hostname_len as usize];
        stream.read_exact(&mut hostname).await?;
        let mut port_bytes = [0u8; 2];
        stream.read_exact(&mut port_bytes).await?;
        let _port = u16::from_be_bytes(port_bytes);
        let next_state = read_varint(&mut stream).await?;

        match next_state {
            1 => handle_status(&mut stream, config, protocol).await?,
            2 => handle_login(&mut stream, config).await?,
            _ => {}
        }
    }

    Ok(())
}

async fn handle_status(stream: &mut TcpStream, config: Config, protocol: i32) -> io::Result<()> {
    let mut request = vec![0; 1024];
    let n = stream.read(&mut request).await?;
    request.truncate(n);

    let mut status = config.status;

    // Set the protocol version
    // If status version is set to "same" or protocol is not set, set it to same protocol as the client
    if status.version.same == Some(true)
        || status
            .version
            .info
            .as_ref()
            .map_or(true, |info| info.protocol.is_none())
    {
        status.version.info = Some(VersionInfo {
            name: status
                .version
                .info
                .as_ref()
                .map_or("same".to_string(), |info| info.name.clone()),
            protocol: Some(protocol),
        });
    }

    if tracing::enabled!(tracing::Level::DEBUG) {
        let mut debug_status = status.clone();

        // Limit the length of the status favicon
        debug_status.favicon = debug_status.favicon.map(|f| f.chars().take(40).collect());

        let debug_json = json!(debug_status);
        debug!("Status JSON: {}", debug_json);
    }

    write_response(stream, &json!(status).to_string()).await?;

    // Handle ping
    let mut ping_data = vec![0; 1024];
    if let Ok(n) = stream.read(&mut ping_data).await {
        ping_data.truncate(n);
        if n >= 9 {
            send_pong(stream, &ping_data, n).await?;
        }
    }

    Ok(())
}

async fn handle_login(stream: &mut TcpStream, config: Config) -> io::Result<()> {
    let mut login_data = vec![0; 1024];
    let n = stream.read(&mut login_data).await?;
    login_data.truncate(n);

    let kick_msg = json!({
        "text": config.kick_msg,
    })
    .to_string();

    write_response(stream, &kick_msg).await?;

    Ok(())
}
