use parking_lot::Mutex;
use xsalsa20poly1305::{
    aead::{generic_array::GenericArray, Buffer, Error, NewAead},
    XSalsa20Poly1305,
};

use crate::{
    error::{DiscordError, Result},
    payload::{EncryptionMode, Encryptor, SpeakingType},
    state::{ConnectionState, State},
    ws::VoiceGateway,
};

use std::{
    fmt,
    io::{ErrorKind, Read},
    net::{SocketAddr, UdpSocket},
    process::{Child, Command, Stdio},
    slice,
    sync::Arc,
    thread, time,
};

#[allow(dead_code)]

pub const SAMPLEING_RATE: u16 = 48000;
#[allow(dead_code)]
pub const CHANNELS: u16 = 2;
#[allow(dead_code)]
pub const FRAME_LENGTH: u16 = 20;
#[allow(dead_code)]
pub const SAMPLE_SIZE: u16 = 4;
#[allow(dead_code)]
pub const SAMPLES_PER_FRAME: u32 = ((SAMPLEING_RATE / 1000) * FRAME_LENGTH) as u32;
#[allow(dead_code)]
pub const FRAME_SIZE: u32 = SAMPLES_PER_FRAME * SAMPLE_SIZE as u32;

pub(crate) trait AudioInput: Send {
    fn read_pcm_frame(&mut self, buffer: &mut [i16]) -> Option<usize>;
}

pub(crate) struct FFmpegAudio {
    process: Child,
}

impl FFmpegAudio {
    pub(crate) fn new(input: &str) -> Result<Self> {
        let process = Command::new("ffmpeg")
            .arg("-i")
            .arg(input)
            .args(&[
                "-f",
                "s16le",
                "-ar",
                "48000",
                "-ac",
                "2",
                "-loglevel",
                "warning",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .spawn()?;
        Ok(Self { process })
    }
}

impl AudioInput for FFmpegAudio {
    fn read_pcm_frame(&mut self, buffer: &mut [i16]) -> Option<usize> {
        let stdout = self.process.stdout.as_mut()?;
        let bytes =
            unsafe { slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, buffer.len() * 2) };
        stdout.read_exact(bytes).map(|_| buffer.len()).ok()
    }
}

impl Drop for FFmpegAudio {
    fn drop(&mut self) {
        if let Err(e) = self.process.kill() {
            error!("Could not kill ffmpeg process: {:?}", e);
        }
    }
}

#[derive(Debug)]
pub struct AudioBuffer<'a> {
    slice: &'a mut [u8],
    length: usize,
    capacity: usize,
}

impl AudioBuffer<'_> {
    pub(crate) fn new(slice: &mut [u8], length: usize) -> AudioBuffer<'_> {
        AudioBuffer {
            capacity: slice.len(),
            slice,
            length,
        }
    }
}

impl<'a> AsRef<[u8]> for AudioBuffer<'a> {
    fn as_ref(&self) -> &[u8] {
        &self.slice[..self.length]
    }
}

impl<'a> AsMut<[u8]> for AudioBuffer<'a> {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.slice[..self.length]
    }
}

impl Buffer for AudioBuffer<'_> {
    fn extend_from_slice(&mut self, other: &[u8]) -> std::result::Result<(), Error> {
        if self.length + other.len() > self.capacity {
            Err(Error)
        } else {
            self.slice[self.length..self.length + other.len()].copy_from_slice(&other);
            self.length += other.len();
            Ok(())
        }
    }

    fn truncate(&mut self, len: usize) {
        if len < self.length {
            for i in self.slice[len..].iter_mut() {
                *i = 0;
            }
            self.length = len;
        }
    }

    fn len(&self) -> usize {
        self.length
    }

    fn is_empty(&self) -> bool {
        self.slice.is_empty()
    }
}

const BUFSIZE: usize = 1275 + 24 + 12 + 24 + 16 + 12;

pub(crate) struct AudioEncoder {
    opus: audiopus::coder::Encoder,
    cipher: XSalsa20Poly1305,
    sequence: u16,
    timestamp: u32,
    lite_nonce: u32,
    ssrc: u32,
    pcm_buff: [i16; 1920],
    buff: [u8; BUFSIZE],
    encryption: EncryptionMode,
}

impl fmt::Debug for AudioEncoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioEncoder")
            .field("opus", &self.opus)
            .field("encryption", &self.encryption)
            .finish()
    }
}

impl AudioEncoder {
    pub(crate) fn from_gateway(gateway: &VoiceGateway) -> Result<AudioEncoder> {
        let mut encoder = audiopus::coder::Encoder::new(
            audiopus::SampleRate::Hz48000,
            audiopus::Channels::Stereo,
            audiopus::Application::Audio,
        )?;
        encoder.set_bitrate(audiopus::Bitrate::BitsPerSecond(128_000))?;
        encoder.enable_inband_fec()?;
        encoder.set_packet_loss_perc(15)?;
        encoder.set_bandwidth(audiopus::Bandwidth::Fullband)?;
        encoder.set_signal(audiopus::Signal::Auto)?;
        let key = GenericArray::clone_from_slice(&gateway.secret_key);
        let cipher = XSalsa20Poly1305::new(&key);
        let encryption = gateway.encryption;

        Ok(Self {
            opus: encoder,
            cipher,
            sequence: 0,
            timestamp: 0,
            lite_nonce: 0,
            ssrc: gateway.ssrc,
            pcm_buff: [0; 1920],
            buff: [0; BUFSIZE],
            encryption,
        })
    }

    pub(crate) fn prepare_packet(
        &mut self,
        size: usize,
    ) -> std::result::Result<usize, xsalsa20poly1305::aead::Error> {
        let mut header = [0u8; 12];
        header[0] = 0x80;
        header[1] = 0x78;
        header[2..4].copy_from_slice(&self.sequence.to_be_bytes());
        header[4..8].copy_from_slice(&self.timestamp.to_be_bytes());
        header[8..12].copy_from_slice(&self.ssrc.to_be_bytes());
        self.buff[..12].copy_from_slice(&header);
        let mut buffer = AudioBuffer::new(&mut self.buff[12..], size);
        self.encryption
            .encrypt(&self.cipher, self.lite_nonce, &header, &mut buffer)?;
        self.lite_nonce = self.lite_nonce.wrapping_add(1);
        Ok(buffer.len())
    }

    pub(crate) fn encode_pcm_buffer(
        &mut self,
    ) -> std::result::Result<usize, audiopus::error::Error> {
        self.opus.encode(&self.pcm_buff, &mut self.buff[12..])
    }

    pub(crate) fn send_opus_packet(
        &mut self,
        socket: &UdpSocket,
        addr: &SocketAddr,
        size: usize,
    ) -> Result<()> {
        self.sequence = self.sequence.wrapping_add(1);
        let size = self
            .prepare_packet(size)
            .map_err(DiscordError::EncryptionError)?;
        if let Err(e) = socket.send_to(&self.buff[0..size + 12], addr) {
            if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut {
                warn!(
                    "A packet has been dropped: (seq: {}, ssrc: {})",
                    &self.sequence, &self.ssrc
                );
                return Ok(());
            } else {
                return Err(DiscordError::from(e));
            }
        }
        self.timestamp = self.timestamp.wrapping_add(SAMPLES_PER_FRAME);
        Ok(())
    }
}

#[allow(dead_code)]
pub(crate) struct AudioPlayer {
    thread: thread::JoinHandle<()>,
    gateway: Arc<Mutex<VoiceGateway>>,
    state: Arc<State>,
    source: Arc<Mutex<Box<dyn AudioInput>>>,
}

impl fmt::Debug for AudioPlayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AudioPlayer")
            .field("thread", &self.thread)
            .field("state", &self.state)
            .finish()
    }
}

fn play_loop(
    gateway: &Arc<Mutex<VoiceGateway>>,
    state: &Arc<State>,
    source: &Arc<Mutex<Box<dyn AudioInput>>>,
) -> Result<()> {
    let mut next_iteration = time::Instant::now();

    let (mut encoder, mut socket) = {
        let mut gateway = gateway.lock();
        gateway.speaking(SpeakingType::MICROPHONE)?;
        (
            AudioEncoder::from_gateway(&*gateway)?,
            gateway.clone_socket()?,
        )
    };

    let addr = socket.peer_addr()?;
    info!("Socket connected to: {:?}", addr);

    use ConnectionState::*;
    loop {
        if state.is_state(Finished) {
            break;
        }

        if state.is_state(Paused) {
            state.wait_not_until(Paused);
            continue;
        }

        if state.is_state(Disconnected) {
            state.wait_until(Connected);
            next_iteration = time::Instant::now();
            let gw = gateway.lock();
            encoder = AudioEncoder::from_gateway(&*gw)?;
            socket = gw.clone_socket()?
        }

        next_iteration += time::Duration::from_millis(20);
        let buff_size = {
            let mut audio = source.lock();
            if audio.read_pcm_frame(&mut encoder.pcm_buff).is_some() {
                match encoder.encode_pcm_buffer() {
                    Ok(bytes) => Some(bytes),
                    Err(e) => {
                        error!("Failed to encode: {:?}", e);
                        return Err(e.into());
                    }
                }
            } else {
                None
            }
        };

        if let Some(size) = buff_size {
            if size > 0 {
                encoder.send_opus_packet(&socket, &addr, size)?;
                let now = time::Instant::now();
                next_iteration = next_iteration.max(now);
                thread::sleep(next_iteration - now);
            }
        } else {
            state.set_state(Finished)
        }
    }

    Ok(())
}

impl AudioPlayer {
    pub fn new<After>(
        after: After,
        gateway: Arc<Mutex<VoiceGateway>>,
        source: Arc<Mutex<Box<dyn AudioInput>>>,
    ) -> Self
    where
        After: FnOnce(Option<DiscordError>) + Send + 'static,
    {
        use ConnectionState::*;
        let state = {
            let gateway = gateway.lock();
            Arc::clone(&gateway.state)
        };
        state.set_state(Connected);

        Self {
            gateway: Arc::clone(&gateway),
            state: Arc::clone(&state),
            source: Arc::clone(&source),
            thread: thread::spawn(move || {
                let mut err = None;
                if let Err(e) = play_loop(&gateway, &state, &source) {
                    err = Some(e);
                }
                {
                    let mut gateway = gateway.lock();
                    let _ = gateway.speaking(SpeakingType::empty());
                }
                after(err);
            }),
        }
    }

    pub fn pause(&self) {
        self.state.set_state(ConnectionState::Paused);
    }

    pub fn resume(&self) {
        self.state.set_state(ConnectionState::Playing);
    }

    pub fn stop(&self) {
        self.state.set_state(ConnectionState::Finished);
    }

    pub fn is_paused(&self) -> bool {
        self.state.is_state(ConnectionState::Paused)
    }

    pub fn is_playing(&self) -> bool {
        self.state.is_state(ConnectionState::Playing)
    }
}
