use std::{
    fmt::Debug,
    rc::Rc,
    sync::{RwLock, mpsc::Sender},
};

use js_sys::Date;
use ratzilla::event::KeyCode;
use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use web_sys::console;
use web_time::Duration;

pub struct TimeoutHandle {
    id: Option<i32>,
    closure: Closure<dyn FnMut()>,
}

impl TimeoutHandle {
    pub fn new(f: Box<dyn FnMut()>) -> Self {
        let closure = Closure::new(f);
        Self { id: None, closure }
    }

    pub fn reset(&mut self, timeout: Duration) {
        let window = web_sys::window().unwrap();
        self.clear();
        let interval_id = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                self.closure.as_ref().unchecked_ref(),
                timeout.as_secs() as i32,
            )
            .inspect_err(|err| {
                log_err(
                    "set_timeout_with_callback_and_timeout_and_arguments_0",
                    Some(err),
                )
            })
            .unwrap();
        self.id = Some(interval_id);
    }

    pub fn clear(&mut self) {
        if let Some(id) = self.id.take() {
            web_sys::window().unwrap().clear_timeout_with_handle(id);
        }
    }

    pub fn callback(&mut self, f: Box<dyn FnMut()>) {
        self.closure = Closure::new(f);
    }
}

impl Drop for TimeoutHandle {
    fn drop(&mut self) {
        self.clear();
    }
}

pub fn log_err<T: Debug>(msg: &str, ctx: Option<T>) {
    console::error_2(
        &JsValue::from_str(msg),
        &JsValue::from_str(ctx.map(|f| format!("{f:?}")).unwrap_or_default().as_str()),
    );
}

pub fn log_info<T: Debug>(msg: &str, ctx: Option<T>) {
    let now = Date::new_0();

    console::log_2(
        &JsValue::from_str(&format!("[{}] {msg}", now.to_string())),
        &JsValue::from_str(ctx.map(|f| format!("{f:?}")).unwrap_or_default().as_str()),
    );
}

const EVENT_BUFFER_INITIAL_CAPACITY: usize = 100;

struct BufferedKeyEventsInner {
    buffered: Vec<KeyCode>,
    last_timeout: TimeoutHandle,
}

pub struct BufferedKeyEvents {
    sender: Sender<Vec<KeyCode>>,
    inner: RwLock<BufferedKeyEventsInner>,
}

impl BufferedKeyEvents {
    pub fn new(sender: Sender<Vec<KeyCode>>) -> Rc<Self> {
        let obj = Rc::new(Self {
            sender,
            inner: RwLock::new(BufferedKeyEventsInner {
                buffered: Vec::with_capacity(EVENT_BUFFER_INITIAL_CAPACITY),
                last_timeout: TimeoutHandle::new(Box::new(|| {})),
            }),
        });

        {
            let mut inner = obj.inner.write().expect("unable to get write lock");
            let clone = obj.clone();
            inner.last_timeout.callback(Box::new(move || clone.flush()));
        }

        obj
    }

    pub fn push(&self, key_code: KeyCode) {
        let mut inner = self.inner.write().expect("unable to get write lock");
        inner.buffered.push(key_code);
        inner.last_timeout.reset(Duration::from_millis(5));
    }

    fn flush(&self) {
        let mut inner = self.inner.write().expect("unable to get write lock");
        let collected = std::mem::replace(
            &mut inner.buffered,
            Vec::with_capacity(EVENT_BUFFER_INITIAL_CAPACITY),
        );
        self.sender
            .send(collected)
            .inspect_err(|e| {
                log_err("channel send error", Some(e));
            })
            .ok();
    }
}
