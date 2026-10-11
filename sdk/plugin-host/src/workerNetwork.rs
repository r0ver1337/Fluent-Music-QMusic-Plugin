use crate::{wire::*, worker::WorkerIo};
use fluent_plugin_abi::{network::*, *};
use std::sync::Mutex;

pub unsafe extern "C" fn call(
    context: *mut std::ffi::c_void,
    request: *const fmStringView,
    out: *mut fmHandle,
) -> fmStatus {
    if context.is_null() || out.is_null() {
        return -1;
    }
    unsafe {
        *out = std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let bytes = unsafe { readView(request) }?;
        let request: NetworkRequest = serde_json::from_slice(&bytes)?;
        let mut io = unsafe { &*context.cast::<Mutex<WorkerIo>>() }
            .lock()
            .map_err(|_| std::io::Error::other("IPC unavailable"))?;
        io.write(WorkerMessage::NetworkRequest(request))?;
        let reply = loop {
            match io.readRaw()? {
                Some(HostMessage::NetworkReply(reply)) => break reply,
                Some(HostMessage::Request(request)) if io.Pending.len() < 4 => {
                    io.Pending.push_back(request)
                }
                _ => return Err(std::io::Error::other("Invalid network IPC reply")),
            }
        };
        OwnedNetworkReply::new(reply)
    }));
    match result {
        Ok(Ok(reply)) => {
            unsafe {
                *out = Box::into_raw(Box::new(reply)).cast();
            }
            0
        }
        _ => -1,
    }
}
pub unsafe extern "C" fn get(handle: fmHandle) -> *const fmStringView {
    if handle.is_null() {
        return std::ptr::null();
    }
    unsafe { &(*handle.cast::<OwnedNetworkReply>()).View }
}
pub unsafe extern "C" fn release(handle: fmHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle.cast::<OwnedNetworkReply>()));
        }
    }
}
