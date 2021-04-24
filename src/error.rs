use pyo3::create_exception;
use pyo3::PyErr;
use thiserror::Error;

#[allow(dead_code)]
pub(crate) type Result<T> = std::result::Result<T, DiscordError>;

create_exception!(ffi, MissingFieldError, pyo3::exceptions::PyException);
create_exception!(ffi, InternalError, pyo3::exceptions::PyException);
create_exception!(ffi, InternalIOError, pyo3::exceptions::PyException);
create_exception!(ffi, TlsError, pyo3::exceptions::PyException);
create_exception!(ffi, GatewayError, pyo3::exceptions::PyException);
create_exception!(ffi, TryReconnect, pyo3::exceptions::PyException);
create_exception!(ffi, EncryptionFailed, pyo3::exceptions::PyException);

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum DiscordError {
    #[error("Builder Missing Required Field: {0}")]
    BuilderMissingRequiredField(String),
    #[error("Failed to Create TLS Connector: {0}")]
    TlsConnectorCreationFailed(#[from] rustls::TLSError),
    #[error("IO Error: {0:?}")]
    IoError(#[from] std::io::Error),
    #[error("IO Error: {0:?}")]
    InvalidDnsName(#[from] webpki::InvalidDNSNameError),
    #[error("Websocket Handshake Failed: {0}")]
    WebsocketHandshakeFailed(
        #[from]
        tungstenite::HandshakeError<
            tungstenite::ClientHandshake<
                rustls::StreamOwned<rustls::ClientSession, std::net::TcpStream>,
            >,
        >,
    ),
    #[error("Gateway Received Error Message: {0}")]
    TungsteniteError(#[from] tungstenite::Error),
    #[error("(De)Serialize Error: {0:?}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Invalid OpCode: {0:?}")]
    InvalidOpCode(u8),
    #[error("IP Address Parse Error: {0}")]
    AddrParseFailed(#[from] std::net::AddrParseError),
    #[error("Connection Closed (code: {0})")]
    ConnectionClosed(u16),
    #[error("Failed to Encrypt / Decrypt: {0}")]
    EncryptionError(xsalsa20poly1305::aead::Error),
    #[error("Opus Error: {0:?}")]
    OpusError(#[from] audiopus::Error),
    #[error("Wav Error: {0:?}")]
    WavFileError(#[from] hound::Error),
}

impl From<DiscordError> for PyErr {
    fn from(err: DiscordError) -> PyErr {
        use DiscordError::*;
        match err {
            BuilderMissingRequiredField(_) => MissingFieldError::new_err(err.to_string()),
            TlsConnectorCreationFailed(_) => InternalError::new_err(err.to_string()),
            IoError(_) => InternalIOError::new_err(err.to_string()),
            InvalidDnsName(_) => InternalError::new_err(err.to_string()),
            WebsocketHandshakeFailed(_) => TlsError::new_err(err.to_string()),
            TungsteniteError(_) => GatewayError::new_err(err.to_string()),
            SerdeError(_) => InternalError::new_err(err.to_string()),
            InvalidOpCode(_) => GatewayError::new_err(err.to_string()),
            AddrParseFailed(_) => GatewayError::new_err(err.to_string()),
            ConnectionClosed(c) if ![1000, 4014, 4015].contains(&c) => {
                TryReconnect::new_err(err.to_string())
            }
            ConnectionClosed(_) => GatewayError::new_err(err.to_string()),
            EncryptionError(_) => EncryptionFailed::new_err(err.to_string()),
            OpusError(_) => InternalError::new_err(err.to_string()),
            WavFileError(_) => InternalIOError::new_err(err.to_string()),
        }
    }
}
