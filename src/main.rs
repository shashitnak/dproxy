extern crate alloc;
mod error;

use error::Result;

use alloc::borrow::Cow;
use std::net::{IpAddr, Ipv4Addr};
use std::result::Result as CoreResult;

use clap::{Parser, Subcommand};
use inquire::Select;
use tabled::{Table, Tabled};
use tokio::net::TcpListener;


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
    #[arg(short, long)]
    port: Option<u16>,
}

#[derive(Parser, Debug)]
struct Connect {
    #[arg(short, long)]
    region: Option<String>
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

struct ConnectRequest {
    id: u64
}


#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.mode {
        Mode::Serve(_) => {}
        Mode::Connect(Connect { region }) => {
            let mut peers = peers()?;
            if let Some(region) = region.as_ref() {
                peers = peers
                    .into_iter()
                    .filter(|peer| {
                        &peer.region == region
                    })
                    .collect();
            }
            let peer = Select::new("Select the node you would like to use", peers).prompt()?;
            println!("Connecting to {peer:?}...");
            peer.connect();
        }
        Mode::List => {
            println!("{}", Table::new(peers()?));
        }
    };

    Ok(())
}
