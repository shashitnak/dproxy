
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, derive_more::From)]
pub enum Error {
    #[from]
    Inquire(inquire::error::InquireError),
    #[from]
    Parse(std::net::AddrParseError),
    #[from]
    IO(std::io::Error),
}


