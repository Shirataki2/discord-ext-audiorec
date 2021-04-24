use std::{str::FromStr, time};

use bitflags::bitflags;
use rand::RngCore;
use xsalsa20poly1305::{
    aead::{generic_array::GenericArray, AeadInPlace, Buffer},
    XSalsa20Poly1305,
};

use crate::error::{DiscordError, Result};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum OpCode {
    // 0
    Identify(Identify),
    // 1
    SelectProtocol(SelectProtocol),
    // 2
    Ready(Ready),
    // 3
    Heartbeat(Heartbeat),
    // 4
    SessionDescription(SessionDescription),
    // 5
    Speaking(Speaking),
    // 6
    HeartbeatAck(HeartbeatAck),
    // 7
    Resume(Resume),
    // 8
    Hello(Hello),
    // 9
    Resumed(Resumed),
    // 12
    ClientConnect(ClientConnect),
    // 13
    ClientDisconnect(ClientDisconnect),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawPayload {
    op: u8,
    d: serde_json::value::Value,
}

impl OpCode {
    pub(crate) fn from_message(msg: String) -> Result<Self> {
        let payload: RawPayload = serde_json::from_str(&msg)?;
        let data = match payload.op {
            2 => OpCode::Ready(serde_json::from_value(payload.d)?),
            4 => OpCode::SessionDescription(serde_json::from_value(payload.d)?),
            5 => OpCode::Speaking(serde_json::from_value(payload.d)?),
            6 => OpCode::HeartbeatAck(serde_json::from_value(payload.d)?),
            8 => OpCode::Hello(serde_json::from_value(payload.d)?),
            9 => OpCode::Resumed(serde_json::from_value(payload.d)?),
            12 => OpCode::ClientConnect(serde_json::from_value(payload.d)?),
            13 => OpCode::ClientDisconnect(serde_json::from_value(payload.d)?),
            code => return Err(DiscordError::InvalidOpCode(code)),
        };
        Ok(data)
    }

    pub(crate) fn identify(identify: Identify) -> Result<String> {
        let payload = serde_json::json!({
            "op": 0,
            "d": identify,
        });
        Ok(serde_json::to_string(&payload)?)
    }

    pub(crate) fn select_protocol(protocol: SelectProtocol) -> Result<String> {
        Ok(serde_json::to_string(&protocol)?)
    }

    pub(crate) fn heartbeat(heartbeat: Heartbeat) -> Result<String> {
        let payload = serde_json::json!({
            "op": 3,
            "d": heartbeat,
        });
        Ok(serde_json::to_string(&payload)?)
    }

    pub(crate) fn resume(resume: Resume) -> Result<String> {
        let payload = serde_json::json!({
            "op": 7,
            "d": resume,
        });
        Ok(serde_json::to_string(&payload)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Identify {
    pub server_id: String,
    pub user_id: String,
    pub session_id: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SelectProtocol(serde_json::Value);

impl SelectProtocol {
    pub fn new(address: &str, port: u16, mode: EncryptionMode) -> SelectProtocol {
        let mode: String = mode.into();
        Self(serde_json::json!({
            "op": 1,
            "d": {
                "protocol": "udp".to_string(),
                "data": {
                    "address": address.to_string(),
                    "port": port,
                    "mode": mode,
                }
            }
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ready {
    pub ssrc: u32,
    pub ip: String,
    pub port: u16,
    pub modes: Vec<String>,
    #[serde(skip)]
    heartbeat_interval: u32,
}

impl Ready {
    pub(crate) fn get_encryption_mode(&self) -> Vec<EncryptionMode> {
        let modes = self
            .modes
            .iter()
            .filter_map(|m| m.parse::<EncryptionMode>().ok())
            .collect::<Vec<_>>();
        modes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Heartbeat(u64);

impl Heartbeat {
    // pub fn new(instant: time::Instant) -> Heartbeat {
    //     Self(instant.elapsed().as_millis() as u64)
    // }

    pub fn now() -> Heartbeat {
        let now = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .expect("Time setting wrong");
        Self(now.as_millis() as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SessionDescription {
    pub mode: String,
    pub secret_key: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Speaking {
    pub speaking: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HeartbeatAck(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Resume {
    pub token: String,
    pub server_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Hello {
    pub heartbeat_interval: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Resumed;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClientConnect {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ClientDisconnect {}

//

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub(crate) enum EncryptionMode {
    XSalsa20Poly1305 = 0,
    XSalsa20Poly1305Suffix = 1,
    XSalsa20Poly1305Lite = 2,
}

impl Default for EncryptionMode {
    fn default() -> EncryptionMode {
        EncryptionMode::XSalsa20Poly1305
    }
}

impl FromStr for EncryptionMode {
    type Err = std::io::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "xsalsa20_poly1305" => Ok(EncryptionMode::XSalsa20Poly1305),
            "xsalsa20_poly1305_lite" => Ok(EncryptionMode::XSalsa20Poly1305Lite),
            "xsalsa20_poly1305_suffix" => Ok(EncryptionMode::XSalsa20Poly1305Suffix),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unknown encryption mode",
            )),
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<String> for EncryptionMode {
    fn into(self) -> String {
        match self {
            EncryptionMode::XSalsa20Poly1305 => "xsalsa20_poly1305",
            EncryptionMode::XSalsa20Poly1305Lite => "xsalsa20_poly1305_lite",
            EncryptionMode::XSalsa20Poly1305Suffix => "xsalsa20_poly1305_suffix",
        }
        .to_string()
    }
}

pub(crate) trait Encryptor: Sized {
    fn encrypt(
        &self,
        cipher: &XSalsa20Poly1305,
        nonce: u32,
        header: &[u8],
        buffer: &mut dyn Buffer,
    ) -> std::result::Result<(), xsalsa20poly1305::aead::Error>;

    fn decrypt(
        &self,
        cipher: &XSalsa20Poly1305,
        buffer: &mut dyn Buffer,
    ) -> std::result::Result<[u8; 12], xsalsa20poly1305::aead::Error>;
}

impl Encryptor for EncryptionMode {
    fn encrypt(
        &self,
        cipher: &XSalsa20Poly1305,
        lite: u32,
        header: &[u8],
        buffer: &mut dyn Buffer,
    ) -> std::result::Result<(), xsalsa20poly1305::aead::Error> {
        match self {
            EncryptionMode::XSalsa20Poly1305 => {
                let mut nonce = [0u8; 24];
                nonce[0..12].copy_from_slice(&header);
                let nonce = GenericArray::from_slice(&nonce);
                cipher.encrypt_in_place(nonce, b"", buffer)?;
                buffer.extend_from_slice(&nonce)?;
            }
            EncryptionMode::XSalsa20Poly1305Suffix => {
                let mut nonce = [0u8; 24];
                rand::thread_rng().fill_bytes(&mut nonce);
                let nonce = GenericArray::from_slice(&nonce);
                cipher.encrypt_in_place(nonce, b"", buffer)?;
                buffer.extend_from_slice(&nonce)?;
            }
            EncryptionMode::XSalsa20Poly1305Lite => {
                let mut nonce = [0u8; 24];
                nonce[..4].copy_from_slice(&lite.to_be_bytes());
                let nonce = GenericArray::from_slice(&nonce);
                cipher.encrypt_in_place(nonce, b"", buffer)?;
                buffer.extend_from_slice(&nonce[0..4])?;
            }
        };

        Ok(())
    }

    fn decrypt(
        &self,
        cipher: &XSalsa20Poly1305,
        buffer: &mut dyn Buffer,
    ) -> std::result::Result<[u8; 12], xsalsa20poly1305::aead::Error> {
        let header = match self {
            EncryptionMode::XSalsa20Poly1305 => {
                let mut header = [0; 12];
                let mut nonce = [0; 24];
                header.copy_from_slice(&buffer.as_ref()[..12]);
                nonce[..12].copy_from_slice(&header);
                buffer.as_mut().rotate_left(12);
                buffer.truncate(buffer.len() - 12);
                let nonce = GenericArray::from_slice(&nonce);
                cipher.decrypt_in_place(nonce, b"", buffer)?;
                header
            }
            EncryptionMode::XSalsa20Poly1305Suffix => {
                let mut header = [0; 12];
                let mut nonce = [0; 24];
                header.copy_from_slice(&buffer.as_ref()[..12]);
                nonce.copy_from_slice(&buffer.as_ref()[buffer.len() - 24..]);
                buffer.as_mut().rotate_left(12);
                buffer.truncate(buffer.len() - 36);
                let nonce = GenericArray::from_slice(&nonce);
                cipher.decrypt_in_place(nonce, b"", buffer)?;
                header
            }
            EncryptionMode::XSalsa20Poly1305Lite => {
                let mut header = [0; 12];
                let mut nonce = [0; 24];
                header.copy_from_slice(&buffer.as_ref()[..12]);
                nonce[..4].copy_from_slice(&buffer.as_ref()[buffer.len() - 4..]);
                buffer.as_mut().rotate_left(12);
                buffer.truncate(buffer.len() - 16);
                let nonce = GenericArray::from_slice(&nonce);
                cipher.decrypt_in_place(nonce, b"", buffer)?;
                header
            }
        };
        Ok(header)
    }
}

bitflags! {
    pub struct SpeakingType: u8 {
        const MICROPHONE = 0b0000_0001;
        const SOUNDSHARE = 0b0000_0010;
        const PROPRITY   = 0b0000_0100;
    }
}
