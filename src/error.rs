pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, derive_more::From)]
pub enum Error {
    #[from]
    Inquire(inquire::error::InquireError),
    #[from]
    ParseAddr(std::net::AddrParseError),
    #[from]
    IO(std::io::Error),
    #[from]
    Http(http::Error),
    #[from]
    Reqwest(reqwest::Error),
    #[from]
    Custom(&'static str),
    #[from]
    InvalidHeaderValue(reqwest::header::InvalidHeaderValue),
    #[from]
    InvalidHeaderName(reqwest::header::InvalidHeaderName),
}
