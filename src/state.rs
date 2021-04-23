use parking_lot::{Condvar, Mutex};

#[derive(Debug)]
pub struct State(Mutex<ConnectionState>, Condvar);

impl Default for State {
    fn default() -> State {
        State(Mutex::new(ConnectionState::Disconnected), Condvar::new())
    }
}

impl State {
    pub fn wait_until(&self, condition: ConnectionState) {
        let mut state = self.0.lock();
        while *state != condition {
            self.1.wait(&mut state)
        }
    }

    pub fn wait_not_until(&self, condition: ConnectionState) {
        let mut state = self.0.lock();
        while *state == condition {
            self.1.wait(&mut state)
        }
    }

    pub fn is_state(&self, condition: ConnectionState) -> bool {
        let state = self.0.lock();
        *state == condition
    }

    pub fn set_state(&self, condition: ConnectionState) {
        let mut state = self.0.lock();
        *state = condition;
        self.1.notify_all();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum ConnectionState {
    Disconnected,
    Connected,
    Playing,
    Recording,
    Paused,
    Finished,
}
