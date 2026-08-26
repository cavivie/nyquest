use std::collections::{HashMap, VecDeque};
#[cfg(feature = "async")]
use std::future::poll_fn;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
#[cfg(feature = "async")]
use std::task::Poll;
use std::task::Waker;

use jni::objects::GlobalRef;
use jni::sys::jlong;
use jni::JavaVM;
use nyquest_interface::Result;

use crate::error::{clear_java_exception, java_failure, Failure};

#[derive(Clone)]
pub(crate) struct ResponseMetadata {
    pub(crate) status: u16,
    pub(crate) content_length: Option<u64>,
    pub(crate) headers: Vec<(String, String)>,
}

impl ResponseMetadata {
    pub(crate) fn header(&self, name: &str) -> Vec<String> {
        self.headers
            .iter()
            .filter(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.clone())
            .collect()
    }
}

pub(crate) struct RequestState {
    data: Mutex<RequestStateData>,
    pub(crate) changed: Condvar,
    vm: Arc<JavaVM>,
}

pub(crate) struct RequestStateData {
    pub(crate) metadata: Option<ResponseMetadata>,
    pub(crate) callback: Option<GlobalRef>,
    pub(crate) chunks: VecDeque<Vec<u8>>,
    pub(crate) terminal: Option<std::result::Result<(), Failure>>,
    pub(crate) response_waker: Option<Waker>,
    pub(crate) read_waker: Option<Waker>,
    pub(crate) read_in_flight: bool,
    pub(crate) follow_redirects: bool,
}

impl RequestState {
    pub(crate) fn new(vm: Arc<JavaVM>, follow_redirects: bool) -> Self {
        Self {
            data: Mutex::new(RequestStateData {
                metadata: None,
                callback: None,
                chunks: VecDeque::new(),
                terminal: None,
                response_waker: None,
                read_waker: None,
                read_in_flight: false,
                follow_redirects,
            }),
            changed: Condvar::new(),
            vm,
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, RequestStateData> {
        self.data
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn wake(data: &mut RequestStateData) {
        if let Some(waker) = data.response_waker.take() {
            waker.wake();
        }
        if let Some(waker) = data.read_waker.take() {
            waker.wake();
        }
    }

    pub(crate) fn fail(&self, failure: Failure) {
        let mut data = self.lock();
        if data.terminal.is_none() {
            data.terminal = Some(Err(failure));
            data.read_in_flight = false;
            Self::wake(&mut data);
            self.changed.notify_all();
        }
    }

    pub(crate) fn cancel_java(&self) {
        let callback = self.lock().callback.clone();
        let Some(callback) = callback else { return };
        if let Ok(mut env) = self.vm.attach_current_thread() {
            let _ = env.call_method(callback.as_obj(), "cancel", "()V", &[]);
            clear_java_exception(&mut env);
        }
    }

    pub(crate) fn request_read(&self) -> std::result::Result<(), Failure> {
        let callback = self
            .lock()
            .callback
            .clone()
            .ok_or_else(|| Failure::Io("Android request callback is unavailable".into()))?;
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|error| Failure::Io(format!("failed to attach to JVM: {error}")))?;
        env.call_method(callback.as_obj(), "read", "()V", &[])
            .map_err(|error| java_failure(&mut env, "failed to read Android response", error))?;
        Ok(())
    }
}

pub(crate) struct RequestGuard {
    pub(crate) _id: i64,
    pub(crate) state: Arc<RequestState>,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        if self.state.lock().terminal.is_none() {
            self.state.cancel_java();
        }
    }
}

#[cfg(feature = "async")]
pub(crate) async fn wait_for_response_async(state: &Arc<RequestState>) -> Result<ResponseMetadata> {
    poll_fn(|cx| {
        let mut data = state.lock();
        if let Some(metadata) = data.metadata.clone() {
            return Poll::Ready(Ok(metadata));
        }
        if let Some(Err(failure)) = data.terminal.clone() {
            return Poll::Ready(Err(failure.into_error()));
        }
        data.response_waker = Some(cx.waker().clone());
        Poll::Pending
    })
    .await
}

#[cfg(feature = "blocking")]
pub(crate) fn wait_for_response_blocking(state: &Arc<RequestState>) -> Result<ResponseMetadata> {
    loop {
        let data = state.lock();
        if let Some(metadata) = data.metadata.clone() {
            return Ok(metadata);
        }
        if let Some(Err(failure)) = data.terminal.clone() {
            return Err(failure.into_error());
        }
        drop(
            state
                .changed
                .wait(data)
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
    }
}

pub(crate) static NEXT_REQUEST_ID: AtomicI64 = AtomicI64::new(1);
static REQUESTS: OnceLock<Mutex<HashMap<i64, Arc<RequestState>>>> = OnceLock::new();

pub(crate) fn registry_lock() -> MutexGuard<'static, HashMap<i64, Arc<RequestState>>> {
    REQUESTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn request_state(handle: jlong) -> Option<Arc<RequestState>> {
    registry_lock().get(&handle).cloned()
}

pub(crate) fn finish_request(handle: jlong) {
    registry_lock().remove(&handle);
}
