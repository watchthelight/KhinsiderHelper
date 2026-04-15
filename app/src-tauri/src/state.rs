use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::Mutex;

pub struct KhinsiderState {
    pub cookies: Mutex<Option<Arc<reqwest::cookie::Jar>>>,
    pub client: Mutex<Option<reqwest::Client>>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for KhinsiderState {
    fn default() -> Self {
        Self {
            cookies: Mutex::new(None),
            client: Mutex::new(None),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}
