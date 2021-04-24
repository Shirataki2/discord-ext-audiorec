use std::{
    collections::{BTreeMap, VecDeque},
    io::Cursor,
    ops::{Deref, DerefMut},
    sync::Arc,
    thread,
};

use hound::{SampleFormat, WavSpec, WavWriter};
use parking_lot::Mutex;
use rtp_rs::Seq;
use std::time;
use xsalsa20poly1305::{
    aead::{generic_array::GenericArray, Buffer, NewAead},
    XSalsa20Poly1305,
};

use crate::{
    error::{DiscordError, Result},
    payload::{EncryptionMode, Encryptor},
    player::*,
    state::{ConnectionState, State},
    ws::VoiceGateway,
};

pub(crate) struct AudioDecoder {
    opus: audiopus::coder::Decoder,
    cipher: XSalsa20Poly1305,
    encryption: EncryptionMode,
}

impl AudioDecoder {
    pub(crate) fn from_gateway(gateway: &VoiceGateway) -> Result<Self> {
        let decoder = audiopus::coder::Decoder::new(
            audiopus::SampleRate::Hz48000,
            audiopus::Channels::Stereo,
        )?;
        info!("Audio Decoder created from gateway");
        let key = GenericArray::clone_from_slice(&gateway.secret_key);
        let cipher = XSalsa20Poly1305::new(&key);
        let encryption = gateway.encryption;
        info!("Use encryption mode: {:?}", encryption);
        Ok(Self {
            opus: decoder,
            cipher,
            encryption,
        })
    }

    pub(crate) fn decrypt_from_buffer(
        &mut self,
        buffer: &mut dyn Buffer,
    ) -> Result<Option<[u8; 12]>> {
        let byte = buffer.as_ref()[1];
        debug!("Start decrypting data[:5]] {:?}", &buffer.as_ref()[..5]);
        if let _n @ 200..=204 = byte {
            // RTCP Header
            debug!("Receive RTCP Packet");
            Ok(None)
        } else {
            debug!("Receive RTP Packet");
            let header = self
                .encryption
                .decrypt(&self.cipher, buffer)
                .map_err(DiscordError::EncryptionError)?;
            Ok(Some(header))
        }
    }

    pub(crate) fn decode_packets(&mut self, queue: &mut PacketQueue) -> (f64, Vec<f32>) {
        let mut pcmdata = Vec::new();
        let mut start_time = std::f64::MAX;
        let mut last_timestamp = None;
        loop {
            debug!("Packet Decode Loop Start");
            use PacketResult::*;
            match queue.get_packet() {
                Find(packet) => {
                    debug!(
                        "Recieve Valid Packet: {} {} {:?} {}",
                        packet.1, packet.2, packet.3, packet.4
                    );
                    start_time = start_time.min(packet.4);
                    if packet.1 < 10 {
                        last_timestamp = Some(packet.2);
                        continue;
                    }
                    if let Some(timestamp) = last_timestamp {
                        let mut elapsed =
                            (packet.2 as f64 - timestamp as f64) / SAMPLING_RATE as f64;
                        if elapsed > 0.02 {
                            elapsed = elapsed.min(1.0);
                            let mut margin = vec![
                                0.0;
                                (SAMPLE_SIZE as f64 * (elapsed - 0.02) * SAMPLING_RATE as f64)
                                    as usize
                            ];
                            pcmdata.append(&mut margin);
                        }
                    }
                    let mut pcm = self.decode_raw(&packet.0, packet.1);
                    pcmdata.append(&mut pcm);
                    last_timestamp = Some(packet.2)
                }
                Dropped => {
                    debug!("Recieve Dropped Packet");
                    let mut pcm = self.decode_dropped_frame();
                    pcmdata.append(&mut pcm);
                    last_timestamp = None;
                    continue;
                }
                End => {
                    info!("Recieve Task Finished");
                    break;
                }
            }
        }
        (start_time, pcmdata)
    }

    fn decode_raw(&mut self, data: &[u8], size: usize) -> std::vec::Vec<f32> {
        debug!("Decoding Packet: SoundData: {:?}", &data[0..size.min(5)]);
        let mut output = [0f32; 1920];
        let size = self
            .opus
            .decode_float(Some(&data[..size]), &mut output[..], false)
            .unwrap_or(0);
        let mut output = output.to_vec();
        output.truncate(size * 2);
        output
    }

    fn decode_dropped_frame(&mut self) -> Vec<f32> {
        debug!("Decoding Packet: DroppedData");
        let n = self
            .opus
            .last_packet_duration()
            .unwrap_or(SAMPLES_PER_FRAME) as usize;
        if n == 0 {
            return vec![];
        }
        let mut output = [0f32; 1920];
        let size = self
            .opus
            .decode_float::<&[u8], _>(None, &mut output[..n], false)
            .unwrap_or(0);
        debug!("{}", size);
        let mut output = output.to_vec();
        output.truncate(size * 2);
        output
    }
}

/// .0: Data
/// .1: Length
/// .2: Timestamp
/// .3: Seq
/// .4: Recieved Time
type Packet = ([u8; BUFSIZE], usize, u32, Seq, f64);

pub(crate) struct PacketQueue(VecDeque<Packet>, Option<Seq>);

pub(crate) enum PacketResult<T> {
    Find(T),
    Dropped,
    End,
}

impl PacketQueue {
    pub(crate) fn new() -> PacketQueue {
        Self(VecDeque::new(), None)
    }

    #[allow(clippy::needless_range_loop)]
    pub(crate) fn get_packet(&mut self) -> PacketResult<Packet> {
        use PacketResult::*;
        match self.1 {
            None => {
                if let Some(packet) = self.0.pop_front() {
                    debug!("First Packet");
                    self.1 = Some(packet.3);
                    Find(packet)
                } else {
                    End
                }
            }
            Some(seq) => {
                if let Some(packet) = self.0.pop_front() {
                    // Seqが連続して届いた場合
                    if seq.next() == packet.3 {
                        debug!("Sequential Packet");
                        self.1 = Some(packet.3);
                        Find(packet)
                    } else {
                        debug!("No-sequential Packet");
                        for i in 1..1000.min(self.0.len()) {
                            if self.0.get(i).unwrap().3.next() == packet.3 {
                                let front = self.0.drain(..i).collect::<Vec<_>>();
                                for i in 0..front.len() - 1 {
                                    self.0.push_front(front[i]);
                                }
                                let packet = front.last().copied().unwrap();
                                self.1 = Some(packet.3);
                                return Find(packet);
                            }
                        }
                        Dropped
                    }
                } else {
                    End
                }
            }
        }
    }
}

impl Deref for PacketQueue {
    type Target = VecDeque<Packet>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PacketQueue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub(crate) struct SsrcPacketQueue {
    queue: BTreeMap<u32, PacketQueue>,
}

impl SsrcPacketQueue {
    pub(crate) fn new() -> Self {
        Self {
            queue: BTreeMap::new(),
        }
    }

    // pub(crate) fn reset(&mut self) {
    //     self.queue = BTreeMap::new();
    // }
    pub(crate) fn decode(&mut self, decoder: &mut AudioDecoder) -> Result<Option<Vec<u8>>> {
        let wavspec = WavSpec {
            channels: CHANNELS,
            sample_rate: SAMPLING_RATE as u32,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut buffer = vec![];
        {
            let cursor = Cursor::new(&mut buffer);
            let mut wavwriter = WavWriter::new(cursor, wavspec)?;

            let mut pcm_list = self
                .queue
                .iter_mut()
                .map(|(&_ssrc, mut queue)| decoder.decode_packets(&mut queue))
                .collect::<Vec<(f64, Vec<f32>)>>();
            pcm_list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            debug!("PCM List: len:{}", pcm_list.len());
            if pcm_list.is_empty() {
                return Ok(None);
            } else {
                let first_time = pcm_list.get(0).unwrap().0;

                let mut r_channel = vec![];
                let mut l_channel = vec![];
                let mut pcms = vec![];
                for i in 0..pcm_list.len() {
                    let (time, packet) = pcm_list.get_mut(i).unwrap();
                    let mut margin =
                        vec![0f32; (SAMPLING_RATE as f64 * 2.0 * (*time - first_time)) as usize];
                    let mut packet = packet.clone();
                    margin.append(&mut packet);
                    pcms.push(margin);
                }
                let range = pcms.iter().map(|v| v.len()).max().unwrap();
                for frame in 0..range {
                    let mut result = 0.0;
                    for user in 0..pcms.len() {
                        let byte = pcms[user][frame];
                        result = match (result, byte) {
                            (r, b) if r < 0.0 && b < 0.0 => r + b - (r * b * -1.0),
                            (r, b) if r > 0.0 && b > 0.0 => r + b - (r * b),
                            (r, b) => r + b,
                        };
                    }
                    if frame % 2 == 0 {
                        r_channel.push(result.min(1.0).max(-1.0));
                    } else {
                        l_channel.push(result.min(1.0).max(-1.0));
                    }
                }
                for (&l, &r) in l_channel.iter().zip(r_channel.iter()) {
                    let r: i16 = (r * 32767.0) as i16;
                    let l: i16 = (l * 32767.0) as i16;
                    wavwriter.write_sample(r)?;
                    wavwriter.write_sample(l)?;
                }
            }
            wavwriter.finalize()?;
        }
        Ok(Some(buffer))
    }
}

impl Deref for SsrcPacketQueue {
    type Target = BTreeMap<u32, PacketQueue>;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

impl DerefMut for SsrcPacketQueue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.queue
    }
}

#[allow(dead_code)]
pub(crate) struct AudioRecorder {
    thread: thread::JoinHandle<()>,
    gateway: Arc<Mutex<VoiceGateway>>,
    state: Arc<State>,
    queue: Arc<Mutex<SsrcPacketQueue>>,
}

fn recv_loop(
    gateway: &Arc<Mutex<VoiceGateway>>,
    state: &Arc<State>,
    queue: &Arc<Mutex<SsrcPacketQueue>>,
) -> Result<()> {
    let (mut decoder, socket) = {
        let gateway = gateway.lock();
        (
            AudioDecoder::from_gateway(&*gateway)?,
            gateway.clone_socket()?,
        )
    };

    let addr = socket.peer_addr()?;
    info!("Socket connected to: {:?}", addr);

    use ConnectionState::*;
    loop {
        debug!("State: {:?}", state);
        if !state.is_state(Recording) {
            let mut data = [0; 10_000];
            let _ = socket.recv(&mut data)?;
            break;
        }
        let mut data = [0; BUFSIZE];

        let mut size = socket.recv(&mut data)?;
        debug!("Datagram Received: Length: {}", size);

        let mut buffer = AudioBuffer::new(&mut data, size);
        if let Some(raw_header) = decoder.decrypt_from_buffer(&mut buffer)? {
            let ssrc = {
                (raw_header[8] as u32) << 24
                    | (raw_header[9] as u32) << 16
                    | (raw_header[10] as u32) << 8
                    | raw_header[11] as u32
            };
            let timestamp = {
                (raw_header[4] as u32) << 24
                    | (raw_header[5] as u32) << 16
                    | (raw_header[6] as u32) << 8
                    | raw_header[7] as u32
            };
            let seq = Seq::from((raw_header[2] as u16) << 8 | raw_header[3] as u16);

            size -= 12;
            let offset = calc_offset(&data);
            data.rotate_left(offset);
            size -= offset;

            let mut queue = queue.lock();
            queue
                .entry(ssrc)
                .or_insert_with(PacketQueue::new)
                .push_back((
                    data,
                    size,
                    timestamp,
                    seq,
                    time::SystemTime::now()
                        .duration_since(time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64(),
                ));
        }
    }
    Ok(())
}

fn calc_offset(data: &[u8]) -> usize {
    if !(data[0] == 0xBE && data[1] == 0xDE && data.len() > 4) {
        return 0;
    }
    debug!("Calculating offset");
    let ext_length = u16::from_be_bytes([data[2], data[3]]);
    let mut offset = 4_usize;
    for _ in 0..ext_length {
        let byte = data[offset];
        offset += 1;
        if byte == 0 {
            continue;
        }
        offset += 1 + (0xF & (byte as usize >> 4));
    }
    debug!("First Bit {}", data[offset + 1]);
    if data[offset + 1] == 0 || data[offset + 1] == 2 {
        offset += 1;
    }
    debug!("offset is {}", offset + 1);
    offset + 1
}

impl AudioRecorder {
    pub fn new<After>(
        after: After,
        gateway: Arc<Mutex<VoiceGateway>>,
        queue: Arc<Mutex<SsrcPacketQueue>>,
    ) -> Self
    where
        After: FnOnce(Option<DiscordError>) + Send + 'static,
    {
        use ConnectionState::*;
        let state = {
            let gateway = gateway.lock();
            Arc::clone(&gateway.state)
        };
        state.set_state(Recording);

        Self {
            gateway: Arc::clone(&gateway),
            state: Arc::clone(&state),
            queue: Arc::clone(&queue),
            thread: thread::spawn(move || {
                let mut err = None;
                if let Err(e) = recv_loop(&gateway, &state, &queue) {
                    err = Some(e);
                }
                after(err);
            }),
        }
    }

    pub fn stop(&self) {
        self.state.set_state(ConnectionState::RecordFinished);
    }

    pub fn is_recording(&self) -> bool {
        self.state.is_state(ConnectionState::Recording)
    }
}
