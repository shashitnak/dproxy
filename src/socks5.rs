use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const SOCKS5_VERSION: u8 = 0x05;
const SOCKS5_AUTH_NONE: u8 = 0x00;
const SOCKS5_CMD_CONNECT: u8 = 0x01;
const SOCKS5_ATYP_IPV4: u8 = 0x01;
const SOCKS5_ATYP_DOMAIN: u8 = 0x03;
const SOCKS5_ATYP_IPV6: u8 = 0x04;

#[derive(Debug)]
pub enum Socks5Error {
    InvalidVersion,
    InvalidCommand,
    InvalidAddressType,
    IoError(std::io::Error),
    ConnectionFailed,
}

impl From<std::io::Error> for Socks5Error {
    fn from(err: std::io::Error) -> Self {
        Socks5Error::IoError(err)
    }
}

pub struct Socks5Server {
    listener: TcpListener,
    proxy_addr: Option<SocketAddr>,
}

impl Socks5Server {
    pub async fn new(listen_addr: &str) -> Result<Self, Socks5Error> {
        let listener = TcpListener::bind(listen_addr).await?;
        println!("SOCKS5 proxy listening on {}", listen_addr);
        Ok(Self {
            listener,
            proxy_addr: None,
        })
    }

    pub fn set_proxy(&mut self, addr: SocketAddr) {
        self.proxy_addr = Some(addr);
    }

    pub async fn run(&mut self) -> Result<(), Socks5Error> {
        loop {
            let (stream, addr) = self.listener.accept().await?;
            println!("New SOCKS5 connection from {}", addr);

            let proxy_addr = self.proxy_addr;
            tokio::spawn(async move {
                if let Err(e) = handle_client(stream, proxy_addr).await {
                    eprintln!("Error handling client: {:?}", e);
                }
            });
        }
    }
}

async fn handle_client(mut stream: TcpStream, proxy_addr: Option<SocketAddr>) -> Result<(), Socks5Error> {
    // 1. Handshake
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;

    let version = buf[0];
    let nmethods = buf[1];

    if version != SOCKS5_VERSION {
        return Err(Socks5Error::InvalidVersion);
    }

    // Read authentication methods
    let mut methods = vec![0u8; nmethods as usize];
    stream.read_exact(&mut methods).await?;

    // Send auth method (no authentication)
    stream.write_all(&[SOCKS5_VERSION, SOCKS5_AUTH_NONE]).await?;

    // 2. Request
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await?;

    let version = buf[0];
    let cmd = buf[1];
    let _rsv = buf[2];
    let atyp = buf[3];

    if version != SOCKS5_VERSION {
        return Err(Socks5Error::InvalidVersion);
    }

    if cmd != SOCKS5_CMD_CONNECT {
        return Err(Socks5Error::InvalidCommand);
    }

    // Read destination address
    let dest_addr = match atyp {
        SOCKS5_ATYP_IPV4 => {
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await?;
            IpAddr::V4(Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]))
        }
        SOCKS5_ATYP_IPV6 => {
            let mut buf = [0u8; 16];
            stream.read_exact(&mut buf).await?;
            IpAddr::V6(Ipv6Addr::from(buf))
        }
        SOCKS5_ATYP_DOMAIN => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).await?;
            let mut domain = vec![0u8; len[0] as usize];
            stream.read_exact(&mut domain).await?;

            let domain_str = String::from_utf8_lossy(&domain);
            // Resolve domain
            match tokio::net::lookup_host(format!("{}:0", domain_str)).await?.next() {
                Some(addr) => addr.ip(),
                None => return Err(Socks5Error::ConnectionFailed),
            }
        }
        _ => return Err(Socks5Error::InvalidAddressType),
    };

    // Read destination port
    let mut port_buf = [0u8; 2];
    stream.read_exact(&mut port_buf).await?;
    let dest_port = u16::from_be_bytes(port_buf);

    let dest = SocketAddr::new(dest_addr, dest_port);
    println!("SOCKS5 request to connect to {}", dest);

    // 3. Connect to destination (directly or through proxy)
    let mut remote = if let Some(proxy) = proxy_addr {
        // Route through proxy peer
        println!("Routing through proxy at {}", proxy);
        match TcpStream::connect(proxy).await {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("Failed to connect to proxy: {}", e);
                // Send failure response
                stream.write_all(&[
                    SOCKS5_VERSION,
                    0x01, // General failure
                    0x00,
                    SOCKS5_ATYP_IPV4,
                    0, 0, 0, 0, // Bind addr
                    0, 0, // Bind port
                ]).await?;
                return Err(Socks5Error::ConnectionFailed);
            }
        }
    } else {
        // Connect directly
        match TcpStream::connect(dest).await {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("Failed to connect to destination: {}", e);
                // Send failure response
                stream.write_all(&[
                    SOCKS5_VERSION,
                    0x01, // General failure
                    0x00,
                    SOCKS5_ATYP_IPV4,
                    0, 0, 0, 0, // Bind addr
                    0, 0, // Bind port
                ]).await?;
                return Err(Socks5Error::ConnectionFailed);
            }
        }
    };

    // Send success response
    stream.write_all(&[
        SOCKS5_VERSION,
        0x00, // Success
        0x00,
        SOCKS5_ATYP_IPV4,
        0, 0, 0, 0, // Bind addr
        0, 0, // Bind port
    ]).await?;

    // 4. Relay data bidirectionally
    let (mut client_read, mut client_write) = stream.split();
    let (mut remote_read, mut remote_write) = remote.split();

    let client_to_remote = async {
        tokio::io::copy(&mut client_read, &mut remote_write).await
    };

    let remote_to_client = async {
        tokio::io::copy(&mut remote_read, &mut client_write).await
    };

    tokio::select! {
        result = client_to_remote => {
            if let Err(e) = result {
                eprintln!("Error copying from client to remote: {}", e);
            }
        }
        result = remote_to_client => {
            if let Err(e) = result {
                eprintln!("Error copying from remote to client: {}", e);
            }
        }
    }

    Ok(())
}
