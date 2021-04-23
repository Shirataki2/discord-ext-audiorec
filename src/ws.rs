use crate::{
    error::{DiscordError, Result},
    payload::*,
    state::{ConnectionState, State},
};
use rustls::{ClientConfig, ClientSession, StreamOwned};
use std::{
    borrow::Cow,
    collections::VecDeque,
    io,
    net::{IpAddr, SocketAddr, TcpStream, UdpSocket},
    sync::Arc,
    time,
};
use tungstenite::{
    client::client as create_gateway,
    protocol::{frame::coding::CloseCode, CloseFrame},
    Message, WebSocket,
};

pub(crate) struct VoiceGateway {
    pub endpoint: String,
    user_id: String,
    server_id: String,
    pub session_id: String,
    pub token: String,
    ws: WebSocket<StreamOwned<ClientSession, TcpStream>>,
    heartbeat_interval: u64,
    pub last_heartbeat: time::Instant,
    pub ssrc: u32,
    pub port: u16,
    pub encryption: EncryptionMode,
    pub endpoint_ip: String,
    socket: Option<UdpSocket>,
    pub recent_acks: VecDeque<f64>,
    pub secret_key: [u8; 32],
    pub state: Arc<State>,
    close_code: u16,
}

impl VoiceGateway {
    pub(crate) fn poll(&mut self) -> Result<()> {
        if self.last_heartbeat.elapsed().as_millis() as u64 >= self.heartbeat_interval {
            self.handle_heartbeat()?;
        }
        let msg = match self.ws.read_message() {
            Ok(msg) => msg,
            Err(tungstenite::Error::Io(inner)) => {
                use std::io::ErrorKind;
                match inner.kind() {
                    ErrorKind::WouldBlock | ErrorKind::TimedOut => {
                        return Ok(());
                    }
                    _ => return Err(DiscordError::IoError(inner)),
                }
            }
            Err(e) => return Err(DiscordError::TungsteniteError(e)),
        };

        match msg {
            Message::Text(s) => {
                let payload = OpCode::from_message(s)?;
                match payload {
                    OpCode::Hello(hello) => {
                        info!("Payload received: {:?}", hello);
                        let interval = hello.heartbeat_interval as u64;
                        self.heartbeat_interval = interval;
                        let socket = self.ws.get_ref().get_ref();
                        socket.set_read_timeout(Some(time::Duration::from_millis(1000)))?;
                        self.last_heartbeat = time::Instant::now();
                    }
                    OpCode::Ready(ready) => {
                        info!("Payload received: {:?}", ready);
                        self.handle_ready(ready)?;
                    }
                    OpCode::Heartbeat(hb) => {
                        info!("Payload received: {:?}", hb);
                        self.handle_heartbeat()?;
                    }
                    OpCode::HeartbeatAck(ack) => {
                        self.handle_heartbeat_ack(ack);
                    }
                    OpCode::SessionDescription(sd) => {
                        info!("Payload received: {:?}", sd);
                        self.handle_session_description(sd)?;
                    }
                    _ => {}
                }
            }
            Message::Close(msg) => {
                info!("Close message received: {:?}", &msg);
                if let Some(frame) = msg {
                    self.close_code = u16::from(frame.code);
                }
                self.state.set_state(ConnectionState::Disconnected);
                return Err(DiscordError::ConnectionClosed(self.close_code));
            }
            m => {
                info!("Unknown message received: {:?}", &m);
            }
        }

        Ok(())
    }

    pub fn connection_flow(&mut self, resume: bool) -> Result<()> {
        self.poll()?; // Hello
        if resume {
            self.resume()?;
        } else {
            self.identify()?;
        }
        while self.secret_key.iter().all(|&b| b == 0) {
            self.poll()?;
        }
        Ok(())
    }

    pub fn close(&mut self, code: u16) -> Result<()> {
        self.state.set_state(ConnectionState::Disconnected);
        self.close_code = code;
        self.ws.close(Some(CloseFrame {
            code: CloseCode::from(code),
            reason: Cow::Owned(String::from("Closing Connection")),
        }))?;
        Ok(())
    }

    pub fn clone_socket(&self) -> Result<UdpSocket> {
        match &self.socket {
            Some(ref socket) => Ok(socket.try_clone()?),
            None => Err(DiscordError::from(io::Error::new(
                io::ErrorKind::Other,
                "No socket found",
            ))),
        }
    }

    fn handle_ready(&mut self, ready: Ready) -> Result<()> {
        self.ssrc = ready.ssrc;
        self.port = ready.port;
        self.encryption = ready
            .get_encryption_mode()
            .first()
            .copied()
            .unwrap_or_default();
        self.endpoint_ip = ready.ip;
        let addr = SocketAddr::new(IpAddr::V4(self.endpoint_ip.as_str().parse()?), self.port);
        info!("UDP Addr Found: {:?}", &addr);
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(&addr)?;
        self.socket = Some(socket);
        let mut retry = 0;
        let (ip, port) = loop {
            let result = self.udp_discovery();
            match (result, retry) {
                (Ok(data), _) => break data,
                (Err(e), 4) => return Err(e),
                _ => retry += 1,
            }
        };

        info!("UDP Discovery Found {}:{}", &ip, &port);

        let packet = OpCode::select_protocol(SelectProtocol::new(&ip, port, self.encryption))?;

        self.ws.write_message(Message::text(packet))?;

        Ok(())
    }

    fn handle_heartbeat(&mut self) -> Result<()> {
        let packet = OpCode::heartbeat(Heartbeat::now())?;
        info!("Heartbeating... {:?}", packet);
        self.ws.write_message(Message::text(packet))?;
        self.last_heartbeat = time::Instant::now();
        Ok(())
    }

    fn handle_heartbeat_ack(&mut self, _ack: HeartbeatAck) {
        let now = time::Instant::now();
        let delta = now.duration_since(self.last_heartbeat);
        if self.recent_acks.len() == 20 {
            self.recent_acks.pop_front();
        }
        self.recent_acks.push_back(delta.as_secs_f64());
    }

    fn handle_session_description(&mut self, description: SessionDescription) -> Result<()> {
        self.encryption = description.mode.parse()?;
        self.secret_key = description.secret_key;
        self.state.set_state(ConnectionState::Connected);
        Ok(())
    }

    pub fn identify(&mut self) -> Result<()> {
        let packet = OpCode::identify(Identify {
            server_id: self.server_id.clone(),
            user_id: self.user_id.clone(),
            session_id: self.session_id.clone(),
            token: self.token.clone(),
        })?;
        info!("Identifying: {:?}", packet);
        self.ws.write_message(Message::text(packet))?;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        let packet = OpCode::resume(Resume {
            server_id: self.server_id.clone(),
            session_id: self.session_id.clone(),
            token: self.token.clone(),
        })?;
        info!("Resuming: {:?}", packet);
        self.ws.write_message(Message::text(packet))?;
        Ok(())
    }

    pub fn speaking(&mut self, flag: SpeakingType) -> Result<()> {
        let flag = flag.bits();
        let packet = serde_json::json!({
            "op": 5,
            "d": {
                "speaking": flag,
                "delay": 0,
                "ssrc": self.ssrc
            }
        });
        self.ws
            .write_message(Message::text(serde_json::to_string(&packet)?))?;
        Ok(())
    }

    fn udp_discovery(&mut self) -> Result<(String, u16)> {
        let socket = match &self.socket {
            Some(s) => s,
            None => {
                return Err(DiscordError::IoError(io::Error::new(
                    io::ErrorKind::Other,
                    "No socket found",
                )))
            }
        };
        let mut buff = [0_u8; 70];
        buff[0..2].copy_from_slice(&1u16.to_be_bytes());
        buff[2..4].copy_from_slice(&70u16.to_be_bytes());
        buff[4..8].copy_from_slice(&self.ssrc.to_be_bytes());
        socket.send(&buff)?;
        let mut buff = [0_u8; 70];
        socket.recv(&mut buff)?;
        info!("UDP Packet Received: {:?}", &buff);
        let ip_end = &buff[4..].iter().position(|&b| b == 0).ok_or_else(|| {
            DiscordError::IoError(io::Error::new(io::ErrorKind::Other, "invalid IP found"))
        })?;
        let ip = {
            let ip_slice = &buff[4..4 + ip_end];
            let as_str = std::str::from_utf8(ip_slice).map_err(|_| {
                DiscordError::IoError(io::Error::new(io::ErrorKind::Other, "invalid IP found"))
            })?;
            String::from(as_str)
        };
        let port = u16::from_be_bytes([buff[68], buff[69]]);
        Ok((ip, port))
    }
}

#[derive(Debug, Default)]
pub(crate) struct VoiceGatewayBuilder {
    endpoint: Option<String>,
    user_id: Option<String>,
    server_id: Option<String>,
    session_id: Option<String>,
    token: Option<String>,
}

#[allow(dead_code)]
impl VoiceGatewayBuilder {
    pub(crate) fn endpoint(&mut self, endpoint: &str) -> &mut Self {
        self.endpoint = Some(endpoint.to_string());
        self
    }

    pub(crate) fn user_id(&mut self, user_id: &str) -> &mut Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    pub(crate) fn server_id(&mut self, server_id: &str) -> &mut Self {
        self.server_id = Some(server_id.to_string());
        self
    }

    pub(crate) fn session_id(&mut self, session_id: &str) -> &mut Self {
        self.session_id = Some(session_id.to_string());
        self
    }

    pub(crate) fn token(&mut self, token: &str) -> &mut Self {
        self.token = Some(token.to_string());
        self
    }

    pub(crate) fn connect(&mut self) -> Result<VoiceGateway> {
        let endpoint = self
            .endpoint
            .clone()
            .ok_or_else(|| DiscordError::BuilderMissingRequiredField("endpoint".to_string()))?;
        let user_id = self
            .user_id
            .clone()
            .ok_or_else(|| DiscordError::BuilderMissingRequiredField("user_id".to_string()))?;
        let server_id = self
            .server_id
            .clone()
            .ok_or_else(|| DiscordError::BuilderMissingRequiredField("server_id".to_string()))?;
        let session_id = self
            .session_id
            .clone()
            .ok_or_else(|| DiscordError::BuilderMissingRequiredField("session_id".to_string()))?;
        let token = self
            .token
            .clone()
            .ok_or_else(|| DiscordError::BuilderMissingRequiredField("token".to_string()))?;

        let ws = {
            // let connector = TlsConnector::new()?;
            // let stream = connector.connect(&endpoint, stream)?;
            // let (ws, resp) = create_gateway(&url, stream)?;
            // info!("Get Response: {:?}", resp);
            let mut config = ClientConfig::new();
            config
                .root_store
                .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);
            let config = Arc::new(config);
            let domain = webpki::DNSNameRef::try_from_ascii_str(&endpoint)?;
            let client = ClientSession::new(&config, domain);
            let stream = TcpStream::connect((endpoint.as_str(), 443))?;
            let stream = StreamOwned::new(client, stream);
            let url = format!("wss://{}/?v=4", endpoint);
            info!("Connecting to {}", url);
            let (ws, resp) = create_gateway(&url, stream)?;
            info!("Get Response: {:?}", resp);
            ws
        };
        info!("Esatblish Connection to {}", endpoint);

        let gateway = VoiceGateway {
            endpoint,
            user_id,
            server_id,
            session_id,
            token,
            ws,
            heartbeat_interval: std::u64::MAX,
            last_heartbeat: time::Instant::now(),
            ssrc: 0,
            port: 0,
            encryption: EncryptionMode::default(),
            endpoint_ip: String::new(),
            socket: None,
            recent_acks: VecDeque::with_capacity(20),
            secret_key: [0; 32],
            state: Arc::new(State::default()),
            close_code: 0,
        };
        Ok(gateway)
    }
}
