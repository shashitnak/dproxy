extern crate alloc;
mod error;

use error::Result;

use alloc::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::result::Result as CoreResult;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use http::request::Request;
use http_body::Body;
use inquire::Select;
use reqwest::header::{HeaderMap, HeaderName};
use rustls_pki_types::ServerName;
use tabled::{Table, Tabled};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use tokio_util::io::ReaderStream;

#[derive(Parser, Debug)]
#[command(version, about = "Decentralized VPN", long_about = None)]
struct Args {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand, Debug)]
enum Mode {
    Serve(Serve),
    Connect(Connect),
    List,
}

#[derive(Parser, Debug)]
struct Serve {
    #[arg(short, long, default_value = "4202")]
    port: u16,
}

async fn proxy_handler(stream: TcpStream, addr: SocketAddr) -> Result<()> {
    let mut stream = BufReader::new(stream);
    let mut payload = vec![];
    stream.read_until(b'\n', &mut payload).await?;
    let mut payload = vec![];
    let mut start = 0;

    let mut upstream = loop {
        let name_len = stream.read_until(b':', &mut payload).await?;
        let value_len = stream.read_until(b'\r', &mut payload).await?;

        let offset = stream.read_until(b'\n', &mut payload).await?;

        let name = &payload[start..start + name_len - 1];
        let name = std::str::from_utf8(name).unwrap();

        let value = &payload[start + name_len + 1..start + name_len + value_len - 1];
        let value = std::str::from_utf8(value).unwrap();

        start += name_len + value_len + offset;
        println!("Name({name:?})");
        println!("Value({value:?})");

        if name.to_lowercase() == "host" {
            let value = value.split(':').next().unwrap();
            println!("here {value:?}");
            let addr = dns_lookup::lookup_host(value)?
                .pop()
                .ok_or("dns lookup failed")?;

            let mut root_cert_store = RootCertStore::empty();
            root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = ClientConfig::builder()
                .with_root_certificates(root_cert_store)
                .with_no_client_auth();
            let connector = TlsConnector::from(Arc::new(config));
            let dnsname = ServerName::try_from(value.to_string()).unwrap();

            let stream = TcpStream::connect((addr, 443)).await?;
            let mut stream = connector.connect(dnsname, stream).await?;
            break stream;
        }
    };

    upstream.write_all(&payload).await?;
    let mut buffer = vec![];
    let bytes = upstream.read(&mut buffer).await?;
    println!("{}", std::str::from_utf8(&buffer).unwrap());

    if !stream.buffer().is_empty() {
        upstream.write_all(stream.buffer()).await?;
    }

    //let mut stream = stream.into_inner();
    //tokio::io::copy_bidirectional(&mut stream, &mut upstream).await?;

    Ok(())
}

impl Serve {
    async fn handle(&self) -> Result<()> {
        let listener = TcpListener::bind(("0.0.0.0", self.port)).await?;
        println!("Listening on 0.0.0.0:{}", self.port);

        loop {
            let (stream, addr) = listener.accept().await?;
            tokio::spawn(async move {
                if let Err(err) = proxy_handler(stream, dbg!(addr)).await {
                    println!("{err:?}");
                }
            });
        }

        Ok(())
    }
}

#[derive(Parser, Debug)]
struct Connect {
    #[arg(short, long)]
    region: Option<String>,
}

impl Connect {
    async fn handle(&self) -> Result<()> {
        let mut peers = peers()?;
        if let Some(region) = self.region.as_ref() {
            peers.retain(|peer| &peer.region == region);
        }
        let peer = Select::new("Select the node you would like to use", peers).prompt()?;
        println!("Connecting to {peer:?}...");
        peer.connect();
        Ok(())
    }
}

#[derive(Debug)]
struct Peer {
    ip_addr: IpAddr,
    region: String,
}

impl std::fmt::Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> CoreResult<(), std::fmt::Error> {
        write!(f, "IP: {}, Region: {}", &self.ip_addr, &self.region)
    }
}

impl Tabled for Peer {
    const LENGTH: usize = 2;

    fn fields(&self) -> Vec<Cow<'_, str>> {
        vec![
            Cow::Owned(self.ip_addr.to_string()),
            Cow::Owned(self.region.clone()),
        ]
    }

    fn headers() -> Vec<Cow<'static, str>> {
        vec![Cow::Borrowed("ip_addr"), Cow::Borrowed("region")]
    }
}

impl Peer {
    fn connect(&self) -> Option<()> {
        todo!()
    }
}

fn peers() -> Result<Vec<Peer>> {
    Ok(vec![Peer {
        ip_addr: "127.0.0.1".parse::<Ipv4Addr>()?.into(),
        region: "India".into(),
    }])
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.mode {
        Mode::Serve(serve) => serve.handle().await,
        Mode::Connect(connect) => connect.handle().await,
        Mode::List => {
            println!("{}", Table::new(peers()?));
            Ok(())
        }
    }
}
