use std::{sync::Arc, thread};

use parking_lot::Mutex;
use pyo3::{
    prelude::*,
    types::{PyBytes, PyDict, PyTuple},
};

use crate::{
    error::DiscordError,
    futures,
    payload::SpeakingType,
    player::{AudioPlayer, FFmpegAudio},
    recorder::{AudioDecoder, AudioRecorder, SsrcPacketQueue},
    state::ConnectionState,
    ws::{VoiceGateway, VoiceGatewayBuilder},
};

#[pyclass]
pub(crate) struct VoiceConnection {
    gateway: Arc<Mutex<VoiceGateway>>,
    queue: Arc<Mutex<SsrcPacketQueue>>,
    player: Option<AudioPlayer>,
    recorder: Arc<Mutex<Option<AudioRecorder>>>,
}

#[pymethods]
impl VoiceConnection {
    #[text_signature = "(loop, /)"]
    fn run(&mut self, py: Python, loop_: PyObject) -> PyResult<PyObject> {
        let (ftr, res): (PyObject, PyObject) = {
            let ftr = loop_.call_method0(py, "create_future")?;
            (ftr.clone_ref(py), ftr)
        };

        let gateway = Arc::clone(&self.gateway);
        thread::spawn(move || loop {
            let result = {
                let mut lock = gateway.lock();
                lock.poll()
            };
            let gil = Python::acquire_gil();
            let py = gil.python();
            if let Err(e) = py.check_signals() {
                error!("Python Signal Error: {}", e);
                let _ = futures::set_exception(py, loop_, ftr, e);
                break;
            } else if let Err(e) = result {
                match e {
                    DiscordError::ConnectionClosed(code)
                        if code != 1000 && code != 4014 && code != 4015 =>
                    {
                        let _ = futures::set_result(py, loop_, ftr, py.None());
                        break;
                    }
                    _ => {
                        let _ = futures::set_exception(py, loop_, ftr, e.into());
                        break;
                    }
                }
            }
        });

        Ok(res)
    }

    fn disconnect(&mut self) -> PyResult<()> {
        let mut lock = self.gateway.lock();
        lock.close(1000)?;
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(player) = &self.player {
            player.stop();
        }
    }

    fn pause(&mut self) {
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    fn resume(&mut self) {
        if let Some(player) = &self.player {
            player.resume();
        }
    }

    fn is_playing(&self) -> bool {
        if let Some(player) = &self.player {
            player.is_playing()
        } else {
            false
        }
    }

    fn is_recording(&self) -> bool {
        if let Some(recoder) = &*self.recorder.lock() {
            recoder.is_recording()
        } else {
            false
        }
    }

    fn send_playing(&self) -> PyResult<()> {
        let mut lock = self.gateway.lock();
        lock.speaking(SpeakingType::MICROPHONE)?;
        Ok(())
    }

    fn play(&mut self, input: String, after: PyObject) -> PyResult<()> {
        if let Some(player) = &self.player {
            player.stop();
        }

        let source = Box::new(FFmpegAudio::new(&input)?);
        let player = AudioPlayer::new(
            move |err| {
                let gil = Python::acquire_gil();
                let py = gil.python();
                let _ = after.call1(py, PyTuple::new(py, [err].iter()));
            },
            Arc::clone(&self.gateway),
            Arc::new(Mutex::new(source)),
        );
        self.player = Some(player);
        Ok(())
    }

    fn record(&mut self, after: PyObject) {
        if let Some(recorder) = &*self.recorder.lock() {
            recorder.stop();
        }
        self.queue = Arc::new(Mutex::new(SsrcPacketQueue::new()));
        let recorder = AudioRecorder::new(
            move |err| {
                let gil = Python::acquire_gil();
                let py = gil.python();
                let _ = after.call1(py, PyTuple::new(py, [err].iter()));
            },
            Arc::clone(&self.gateway),
            Arc::clone(&self.queue),
        );
        self.recorder = Arc::new(Mutex::new(Some(recorder)));
    }

    fn stop_record(&mut self, py: Python, loop_: PyObject) -> PyResult<PyObject> {
        let (ftr, res): (PyObject, PyObject) = {
            let ftr = loop_.call_method0(py, "create_future")?;
            (ftr.clone_ref(py), ftr)
        };

        let gateway = Arc::clone(&self.gateway);
        let queue = Arc::clone(&self.queue);
        let recorder = Arc::clone(&self.recorder);

        let state = {
            let gateway = gateway.lock();
            Arc::clone(&gateway.state)
        };
        state.set_state(ConnectionState::RecordFinished);

        thread::spawn(move || {
            let gil = Python::acquire_gil();
            let py = gil.python();
            if let Err(e) = py.check_signals() {
                let _ = futures::set_exception(py, loop_, ftr, e);
                return;
            }
            let data = if let Some(recorder) = &*recorder.lock() {
                recorder.stop();
                let mut decoder = {
                    let gateway = gateway.lock();
                    match AudioDecoder::from_gateway(&*gateway) {
                        Ok(decoder) => decoder,
                        Err(e) => {
                            let _ = futures::set_exception(py, loop_, ftr, PyErr::from(e));
                            return;
                        }
                    }
                };

                let mut queue = queue.lock();
                let data = match queue.decode(&mut decoder) {
                    Ok(data) => data,
                    Err(e) => {
                        let _ = futures::set_exception(py, loop_, ftr, PyErr::from(e));
                        return;
                    }
                };
                data.unwrap_or_default()
            } else {
                vec![]
            };
            let _ = futures::set_result(py, loop_, ftr, PyBytes::new(py, &data).to_object(py));
        });
        Ok(res)
    }

    fn get_state<'py>(&self, py: Python<'py>) -> PyResult<&'py PyDict> {
        let result = PyDict::new(py);
        let gateway = self.gateway.lock();
        result.set_item("secret_key", Vec::<u8>::from(gateway.secret_key))?;
        result.set_item("encryption_mode", Into::<String>::into(gateway.encryption))?;
        result.set_item("endpoint", gateway.endpoint.clone())?;
        result.set_item("endpoint_ip", gateway.endpoint_ip.clone())?;
        result.set_item("port", gateway.port)?;
        result.set_item("token", gateway.token.clone())?;
        result.set_item("ssrc", gateway.ssrc)?;
        result.set_item(
            "last_heartbeat",
            gateway.last_heartbeat.elapsed().as_secs_f32(),
        )?;
        result.set_item("player_connected", self.player.is_some())?;
        Ok(result)
    }

    fn latency(&self) -> f64 {
        self.gateway.lock().latency()
    }

    fn average_latency(&self) -> f64 {
        self.gateway.lock().average_latency()
    }
}

#[pyclass]
pub(crate) struct VoiceConnector {
    #[pyo3(get, set)]
    session_id: String,
    #[pyo3(get, set)]
    user_id: String,
    #[pyo3(get)]
    server_id: String,
    #[pyo3(get)]
    endpoint: String,
    token: String,
}

#[pymethods]
impl VoiceConnector {
    #[new]
    fn new() -> Self {
        info!("Create new VoiceConnector;");
        Self {
            session_id: String::new(),
            user_id: String::new(),
            server_id: String::new(),
            endpoint: String::new(),
            token: String::new(),
        }
    }

    fn update_connection_config(&mut self, token: &str, server_id: &str, endpoint: &str) {
        info!("Update Connection Info;");
        self.token = token.to_string();
        self.server_id = server_id.to_string();
        self.endpoint = endpoint.to_string();
    }

    #[text_signature = "(loop, /)"]
    fn connect(&mut self, py: Python, loop_: PyObject) -> PyResult<PyObject> {
        let (ftr, res): (PyObject, PyObject) = {
            let ftr = loop_.call_method0(py, "create_future")?;
            (ftr.clone_ref(py), ftr)
        };

        let mut gateway = VoiceGatewayBuilder::default();
        gateway
            .endpoint(&self.endpoint)
            .session_id(&self.session_id)
            .user_id(&self.user_id)
            .token(&self.token)
            .server_id(&self.server_id);

        thread::spawn(move || {
            let result = match gateway.connect() {
                Ok(mut gateway) => gateway.connection_flow(false).and(Ok(gateway)),
                Err(e) => Err(e),
            };
            let gil = Python::acquire_gil();
            let py = gil.python();
            if let Err(e) = py.check_signals() {
                let _ = futures::set_exception(py, loop_, ftr, e);
                return;
            }
            match result {
                Ok(gw) => {
                    let obj = VoiceConnection {
                        gateway: Arc::new(Mutex::new(gw)),
                        queue: Arc::new(Mutex::new(SsrcPacketQueue::new())),
                        player: None,
                        recorder: Arc::new(Mutex::new(None)),
                    };
                    let _ = futures::set_result(py, loop_, ftr, obj.into_py(py));
                }
                Err(e) => {
                    let _ = futures::set_exception(py, loop_, ftr, PyErr::from(e));
                }
            };
        });
        Ok(res)
    }
}
