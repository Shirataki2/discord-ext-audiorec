use std::{sync::Arc, thread};

use parking_lot::Mutex;
use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::DiscordError,
    futures,
    payload::SpeakingType,
    player::{AudioPlayer, FFmpegAudio},
    ws::{VoiceGateway, VoiceGatewayBuilder},
};

#[pyclass]
pub(crate) struct VoiceConnection {
    gateway: Arc<Mutex<VoiceGateway>>,
    player: Option<AudioPlayer>,
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
            if let Err(e) = result {
                let gil = Python::acquire_gil();
                let py = gil.python();
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

    fn send_playing(&self) -> PyResult<()> {
        let mut lock = self.gateway.lock();
        lock.speaking(SpeakingType::MICROPHONE)?;
        Ok(())
    }

    fn play(&mut self, input: String) -> PyResult<()> {
        if let Some(player) = &self.player {
            player.stop();
        }

        let source = Box::new(FFmpegAudio::new(&input)?);
        let player = AudioPlayer::new(
            |err| {
                error!("Audio Player error: {:?}", err);
            },
            Arc::clone(&self.gateway),
            Arc::new(Mutex::new(source)),
        );
        self.player = Some(player);
        Ok(())
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
            match result {
                Ok(gw) => {
                    let obj = VoiceConnection {
                        gateway: Arc::new(Mutex::new(gw)),
                        player: None,
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
