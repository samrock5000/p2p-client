
use nakamoto_cash::client::handle;
use thiserror::Error;
use std::fmt;
use std::io;
/// An error occurring in the wallet.
#[derive(Error, Debug)]
pub enum Error {
    #[error("client handle error: {0}")]
    Handle(#[from] handle::Error),
    #[error("client error: {0}")]
    Client(#[from] nakamoto_cash::client::Error),
    #[error("channel error: {0}")]
    Channel(#[from] crossbeam_channel::RecvError), // Ensure this is present and correct
    #[error("failed to load keys: {0}")]
    Loading(std::io::ErrorKind),
    #[error("file system write error")]
    WriteError,
    #[error("cash address encoding")]
    CashAddressEncoding,
    #[error("failed to deserialize")]
    Deserialization,
    #[error("file system read error: {0}")]
    ReadWallet(String),
    #[error("wallet name not found")]
    WalletNameNotFound,
    #[error("not a pay to public key hash script")]
    Script,
    #[error("bad derivation path")]
    XprivPath,
    #[error("script not found")]
    ScriptNotFound,
    #[error("scalar bytes")]
    InvalidScalarBytes,
    #[error("dust limit")]
    Dust,
    #[error("file system io error: {0}")]
    Io(#[from] std::io::Error),
}

// Implement From<io::Error> to convert IO errors (like flush errors) to our Error type
// impl From<io::Error> for Error {
//     fn from(err: io::Error) -> Self {
//         Error::Io(err)
//     }
// }

// Required for the #[error] macro to work with custom display
// impl fmt::Display for Error {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         match self {
//             Error::Handle(err) => write!(f, "client handle error: {}", err),
//             Error::Client(err) => write!(f, "client error: {}", err),
//             Error::Channel(err) => write!(f, "channel error: {}", err),
//             Error::Loading(kind) => write!(f, "failed to load keys: {:?}", kind),
//             Error::WriteError => write!(f, "file system write error"),
//             Error::CashAddressEncoding => write!(f, "cash address encoding"),
//             Error::Deserialization => write!(f, "failed to deserialize"),
//             Error::ReadWallet(path) => write!(f, "file system read error: {}", path),
//             Error::WalletNameNotFound => write!(f, "wallet name not found"),
//             Error::Script => write!(f, "not a pay to public key hash script"),
//             Error::XprivPath => write!(f, "bad derivation path"),
//             Error::ScriptNotFound => write!(f, "script not found"),
//             Error::InvalidScalarBytes => write!(f, "scalar bytes"),
//             Error::Dust => write!(f, "dust limit"),
//             Error::Io(err) => write!(f, "file system io error: {}", err),
//         }
//     }
// }

// // Implement std::error::Error trait
// impl std::error::Error for Error {
//     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
//         match self {
//             Error::Handle(err) => Some(err),
//             Error::Client(err) => Some(err),
//             Error::Channel(err) => Some(err),
//             Error::Io(err) => Some(err),
//             _ => None,
//         }
//     }
// }
