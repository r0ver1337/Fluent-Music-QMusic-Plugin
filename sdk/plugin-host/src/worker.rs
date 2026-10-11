#![allow(non_snake_case)]
use crate::wire::*;
use fluent_plugin_abi::{arena::Arena, files::*, *};
use fluent_plugin_api::*;
use std::{
    io::{self, BufReader, Write},
    path::PathBuf,
    sync::Mutex,
};

pub(crate) struct WorkerIo {
    Input: BufReader<io::Stdin>,
    Output: io::Stdout,
    pub(crate) Pending: std::collections::VecDeque<Request>,
}
impl WorkerIo {
    fn read(&mut self) -> io::Result<Option<HostMessage>> {
        if let Some(request) = self.Pending.pop_front() {
            return Ok(Some(HostMessage::Request(request)));
        }
        self.readRaw()
    }
    pub(crate) fn readRaw(&mut self) -> io::Result<Option<HostMessage>> {
        read_frame(&mut self.Input)?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(io::Error::from))
            .transpose()
    }
    pub(crate) fn write(&mut self, message: WorkerMessage) -> io::Result<()> {
        self.Output.write_all(&encode(&message)?)?;
        self.Output.flush()
    }
    fn retainMedia(&mut self, id: u64) -> io::Result<u64> {
        self.write(WorkerMessage::FileRequest(FileRequest {
            Operation: 16,
            Handle: id,
            ..Default::default()
        }))?;
        match self.readRaw()? {
            Some(HostMessage::FileReply(reply)) if reply.Status == 0 && reply.Handle != 0 => {
                Ok(reply.Handle)
            }
            _ => Err(io::Error::other("Media retention failed")),
        }
    }
}
unsafe extern "C" fn fileCall(
    context: *mut std::ffi::c_void,
    request: *const fmFileRequest,
    out: *mut fmHandle,
) -> fmStatus {
    if context.is_null() || out.is_null() {
        return -1;
    }
    unsafe {
        *out = std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let request = unsafe { decodeFileRequest(request) }.map_err(|_| ())?;
        let mut io = unsafe { &*context.cast::<Mutex<WorkerIo>>() }
            .lock()
            .map_err(|_| ())?;
        io.write(WorkerMessage::FileRequest(request))
            .map_err(|_| ())?;
        let reply = loop {
            match io.readRaw().map_err(|_| ())? {
                Some(HostMessage::FileReply(reply)) => break reply,
                Some(HostMessage::Request(request)) if io.Pending.len() < 4 => {
                    io.Pending.push_back(request)
                }
                _ => return Err(()),
            }
        };
        Ok::<_, ()>(OwnedFileReply::new(reply))
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
unsafe extern "C" fn fileReply(handle: fmHandle) -> *const fmFileReply {
    if handle.is_null() {
        return std::ptr::null();
    }
    unsafe { &(*handle.cast::<OwnedFileReply>()).View }
}
unsafe extern "C" fn releaseFileReply(handle: fmHandle) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle.cast::<OwnedFileReply>()));
        }
    }
}
unsafe extern "C" fn notCancelled(_: *mut std::ffi::c_void) -> u32 {
    0
}

pub fn runWorker() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err(io::Error::other("Invalid worker arguments"));
    }
    match args[3].as_str() {
        "1" => crate::sandboxWindows::verifySandbox()?,
        "0" => {}
        _ => return Err(io::Error::other("Invalid sandbox mode")),
    }
    let libraryPath = PathBuf::from(&args[0]);
    let optionsBytes = decodeHex(&args[2])?;
    let options: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&optionsBytes)?;
    let io = Box::new(Mutex::new(WorkerIo {
        Input: BufReader::new(io::stdin()),
        Output: io::stdout(),
        Pending: std::collections::VecDeque::new(),
    }));
    let host = fmHostApi {
        structSize: size_of::<fmHostApi>() as u32,
        abiVersion,
        fileApi: fmFileApi {
            structSize: size_of::<fmFileApi>() as u32,
            context: (&*io as *const Mutex<WorkerIo>).cast_mut().cast(),
            call: fileCall,
            getReply: fileReply,
            releaseReply: releaseFileReply,
        },
        networkApi: network::fmNetworkApi {
            structSize: size_of::<network::fmNetworkApi>() as u32,
            context: (&*io as *const Mutex<WorkerIo>).cast_mut().cast(),
            call: crate::workerNetwork::call,
            getReply: crate::workerNetwork::get,
            releaseReply: crate::workerNetwork::release,
        },
    };
    let library = unsafe {
        libloading::os::windows::Library::load_with_flags(
            &libraryPath,
            windows_sys::Win32::System::LibraryLoader::LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR
                | windows_sys::Win32::System::LibraryLoader::LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .map_err(|_| io::Error::other("Plugin DLL unavailable"))?;
    let getApi = unsafe { library.get::<fmGetApi>(b"fmPluginGetApi\0") }
        .map_err(|_| io::Error::other("C ABI entry unavailable"))?;
    let mut pointer = std::ptr::null();
    if unsafe { getApi(abiVersion, &mut pointer) } != 0 || pointer.is_null() {
        return Err(io::Error::other("C ABI handshake failed"));
    }
    let api = unsafe { readPluginApi(pointer) }
        .map_err(|_| io::Error::other("C ABI function table invalid"))?;
    let mut arena = Arena::default();
    let entries = options
        .iter()
        .map(|(k, v)| fmHeader {
            name: arena.text(k),
            value: arena.text(v),
            ..fmHeader::new()
        })
        .collect();
    let create = fmCreateInfo {
        pluginId: arena.text(&args[1]),
        options: arena.array(entries),
        ..fmCreateInfo::new()
    };
    let mut provider = std::ptr::null_mut();
    let createStatus = unsafe { (api.createProvider)(&host, &create, &mut provider) };
    if createStatus != 0 || provider.is_null() {
        return Err(io::Error::other(format!(
            "Plugin initialization failed ({createStatus})"
        )));
    }
    struct Provider {
        Handle: fmHandle,
        Api: fmPluginApi,
    }
    impl Drop for Provider {
        fn drop(&mut self) {
            unsafe {
                (self.Api.destroyProvider)(self.Handle);
            }
        }
    }
    let provider = Provider {
        Handle: provider,
        Api: api,
    };
    loop {
        let message = io
            .lock()
            .map_err(|_| io::Error::other("IPC unavailable"))?
            .read()?;
        let Some(HostMessage::Request(request)) = message else {
            break;
        };
        let result = if request.api_version != API_VERSION {
            Err(ApiError::new("VERSION", "内部请求版本不匹配"))
        } else {
            let input = OwnedRequest::new(&request.operation);
            let context = fmCallContext {
                structSize: size_of::<fmCallContext>() as u32,
                requestId: request.id,
                timeoutMs: 90000,
                userData: std::ptr::null_mut(),
                isCancelled: Some(notCancelled),
            };
            if let Some(call) = api.method(input.View.method) {
                let mut output = std::ptr::null_mut();
                let status = unsafe { call(provider.Handle, &context, &input.View, &mut output) };
                let mut result = if status != 0 || output.is_null() {
                    Err(ApiError::new("ABI", "插件调用失败"))
                } else {
                    unsafe { decodeResult((api.getResultView)(output)) }
                };
                if let Ok(ResultData::Description(info)) = &result
                    && info.supports(&Capability::TrackQualities)
                    && (api.getTrackQualities.is_none() || api.resolveTrackWithQuality.is_none())
                {
                    result = Err(ApiError::new("ABI", "插件音质接口声明不完整"));
                }
                if let Ok(ResultData::Media(MediaSource::ManagedFile { id })) = &mut result {
                    let retained = io.lock().ok().and_then(|mut io| io.retainMedia(*id).ok());
                    if let Some(retained) = retained {
                        *id = retained;
                    } else {
                        result = Err(ApiError::new("MEDIA", "无法保留插件音频资源"));
                    }
                }
                if !output.is_null() {
                    unsafe {
                        (api.releaseResult)(output);
                    }
                }
                result
            } else {
                Err(ApiError::new("UNSUPPORTED", "插件未实现此接口"))
            }
        };
        io.lock()
            .map_err(|_| io::Error::other("IPC unavailable"))?
            .write(WorkerMessage::Response(Response::new(request.id, result)))?;
    }
    drop(provider);
    Ok(())
}
fn decodeHex(text: &str) -> io::Result<Vec<u8>> {
    if text.len() > 131072 || !text.len().is_multiple_of(2) {
        return Err(io::Error::other("Invalid options"));
    }
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| {
            let text = std::str::from_utf8(b).map_err(|_| io::Error::other("Invalid options"))?;
            u8::from_str_radix(text, 16).map_err(|_| io::Error::other("Invalid options"))
        })
        .collect()
}
