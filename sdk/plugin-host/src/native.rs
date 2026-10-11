#![allow(non_snake_case)]
use crate::manifest::{NativeManifest, libraryPath, validatePluginTree};
use crate::{fileBroker::FileBroker, sandboxWindows::WorkerChild, wire::*};
use fluent_plugin_api::*;
use std::{
    io::{BufReader, Write},
    path::Path,
    sync::{Arc, Mutex, Weak, mpsc},
    time::{Duration, Instant},
};

fn error(message: &str) -> ApiError {
    ApiError::new("PLUGIN_HOST", message)
}
pub struct NativeProcess {
    Child: Arc<Mutex<WorkerChild>>,
    Input: mpsc::SyncSender<InputFrame>,
    Replies: mpsc::Receiver<Result<Response, ApiError>>,
    Broker: Arc<Mutex<FileBroker>>,
    Network: crate::NetworkPolicy,
    NextId: u64,
    Broken: bool,
    pub info: PluginInfo,
}
struct InputFrame {
    Bytes: Vec<u8>,
    Complete: Option<mpsc::SyncSender<Result<(), ApiError>>>,
}
#[derive(Clone)]
pub struct NativeTerminator {
    Child: Weak<Mutex<WorkerChild>>,
}
impl NativeTerminator {
    pub fn terminate(&self) {
        if let Some(child) = self.Child.upgrade()
            && let Ok(mut child) = child.lock()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl NativeProcess {
    pub fn start(path: &Path) -> Result<Self, ApiError> {
        Self::startWithSandbox(path, true)
    }
    pub fn startWithSandbox(path: &Path, sandbox: bool) -> Result<Self, ApiError> {
        let executable = std::env::current_exe().map_err(|_| error("无法确定客户端目录"))?;
        let mut runtime = executable.parent().ok_or_else(|| error("客户端目录无效"))?;
        if runtime.file_name().is_some_and(|n| n == "deps") {
            runtime = runtime.parent().unwrap();
        }
        let worker = runtime.join("pluginloader.exe");
        Self::startWithRuntime(path, runtime, &worker, sandbox)
    }
    pub fn startWithRuntime(
        path: &Path,
        runtime: &Path,
        worker: &Path,
        sandbox: bool,
    ) -> Result<Self, ApiError> {
        let bytes = std::fs::read(path).map_err(|_| error("无法读取插件描述"))?;
        if bytes.len() > 65536 {
            return Err(error("插件描述过大"));
        }
        let manifest: NativeManifest = serde_json::from_slice(&bytes)
            .map_err(|_| error("仅支持 C ABI DLL 插件描述，不再加载 EXE 插件"))?;
        let network = crate::NetworkPolicy::new(&manifest.Manifest)?;
        let rootPath = path.parent().ok_or_else(|| error("插件目录无效"))?;
        use std::os::windows::fs::MetadataExt;
        if std::fs::symlink_metadata(rootPath)
            .map_err(|_| error("插件目录无效"))?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err(error("拒绝链接插件目录"));
        }
        let root = rootPath.canonicalize().map_err(|_| error("插件目录无效"))?;
        if root.file_name().and_then(|n| n.to_str()) != Some(manifest.Id.as_str()) {
            return Err(error("插件目录名必须与稳定插件 ID 相同"));
        }
        let library = libraryPath(&manifest, &root)?;
        validatePluginTree(&root).map_err(|_| error("插件安装目录包含链接或不支持的文件"))?;
        let worker = worker
            .canonicalize()
            .map_err(|_| error("pluginloader.exe 不存在，请重新构建或安装"))?;
        let broker = Arc::new(Mutex::new(
            FileBroker::new(runtime, &root, &manifest.Id)
                .map_err(|_| error("无法建立插件 data/temp 分区"))?,
        ));
        let options = serde_json::to_vec(&manifest.Options)
            .map_err(|_| error("插件选项无效"))?
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let args = vec![
            library.to_string_lossy().into_owned(),
            manifest.Id.clone(),
            options,
            (sandbox as u8).to_string(),
        ];
        let (child, mut input, output) =
            crate::sandboxWindows::spawn(&worker, &root, &manifest.Id, &args, sandbox).map_err(
                |e| {
                    error(&format!(
                        "{}（{}）",
                        if sandbox {
                            "安全沙箱创建失败，已拒绝加载插件；不会自动关闭沙箱"
                        } else {
                            "无法启动插件 Worker"
                        },
                        e
                    ))
                },
            )?;
        let (send, writes) = mpsc::sync_channel::<InputFrame>(4);
        std::thread::spawn(move || {
            while let Ok(frame) = writes.recv() {
                let result = input
                    .write_all(&frame.Bytes)
                    .and_then(|_| input.flush())
                    .map_err(|_| error("插件输入管道已关闭"));
                let failed = result.is_err();
                if let Some(complete) = frame.Complete {
                    let _ = complete.send(result);
                }
                if failed {
                    break;
                }
            }
        });
        let (tx, replies) = mpsc::sync_channel(2);
        let fileInput = send.clone();
        let fileBroker = broker.clone();
        let mut networkBroker = crate::networkBroker::NetworkBroker::new(network.clone());
        std::thread::spawn(move || {
            let mut output = BufReader::new(output);
            let result = (|| -> Result<(), ApiError> {
                while let Some(bytes) =
                    read_frame(&mut output).map_err(|_| error("插件响应超长或不完整"))?
                {
                    match serde_json::from_slice::<WorkerMessage>(&bytes)
                        .map_err(|_| error("Worker 协议无效"))?
                    {
                        WorkerMessage::Response(response) => {
                            if tx.send(Ok(response)).is_err() {
                                return Ok(());
                            }
                        }
                        WorkerMessage::FileRequest(request) => {
                            let reply = fileBroker
                                .lock()
                                .map_err(|_| error("文件代理已失效"))?
                                .call(request);
                            let bytes = encode(&HostMessage::FileReply(reply))
                                .map_err(|_| error("文件回复过大"))?;
                            fileInput
                                .send(InputFrame {
                                    Bytes: bytes,
                                    Complete: None,
                                })
                                .map_err(|_| error("文件代理管道已关闭"))?;
                        }
                        WorkerMessage::NetworkRequest(request) => {
                            let reply = networkBroker.call(request);
                            let bytes = encode(&HostMessage::NetworkReply(reply))
                                .map_err(|_| error("网络回复过大"))?;
                            fileInput
                                .send(InputFrame {
                                    Bytes: bytes,
                                    Complete: None,
                                })
                                .map_err(|_| error("网络代理管道已关闭"))?;
                        }
                    }
                }
                Err(error("插件 Worker 已退出"))
            })();
            if let Err(e) = result {
                let _ = tx.send(Err(e));
            }
        });
        let mut process = Self {
            Child: Arc::new(Mutex::new(child)),
            Input: send,
            Replies: replies,
            Broker: broker,
            Network: network,
            NextId: 0,
            Broken: false,
            info: PluginInfo {
                id: manifest.Id.clone(),
                name: manifest.Id.clone(),
                version: String::new(),
                api_version: API_VERSION,
                capabilities: vec![],
                extensions: vec![],
            },
        };
        match process.call(Operation::Describe)? {
            ResultData::Description(info)
                if info.id == manifest.Id && info.api_version == API_VERSION =>
            {
                process.info = info
            }
            _ => return Err(error("插件身份握手失败")),
        }
        Ok(process)
    }
    pub fn terminator(&self) -> NativeTerminator {
        NativeTerminator {
            Child: Arc::downgrade(&self.Child),
        }
    }
    pub fn call(&mut self, operation: Operation) -> Result<ResultData, ApiError> {
        self.call_with_timeout(operation, Duration::from_secs(90))
    }
    pub fn call_with_timeout(
        &mut self,
        operation: Operation,
        timeout: Duration,
    ) -> Result<ResultData, ApiError> {
        if self.Broken {
            return Err(error("插件已失效，请重新启动播放器"));
        }
        if let Some(capability) = operation.capability()
            && !self.info.supports(&capability)
        {
            return Err(ApiError::new("UNSUPPORTED", "插件没有声明此能力"));
        }
        self.NextId += 1;
        let bytes = encode(&HostMessage::Request(Request {
            api_version: API_VERSION,
            id: self.NextId,
            operation: operation.clone(),
        }))
        .map_err(|_| error("插件请求过大"))?;
        let deadline = Instant::now() + timeout;
        let (done, wait) = mpsc::sync_channel(1);
        let result = (|| {
            self.Input
                .try_send(InputFrame {
                    Bytes: bytes,
                    Complete: Some(done),
                })
                .map_err(|_| error("插件输入不可用"))?;
            wait.recv_timeout(timeout)
                .map_err(|_| error("插件写入超时"))??;
            let response = self
                .Replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| error("插件请求超时"))??;
            if response.id != self.NextId
                || response.api_version != API_VERSION
                || response.result.is_some() == response.error.is_some()
            {
                return Err(error("插件响应不匹配"));
            }
            match (response.result, response.error) {
                (Some(r), None) if operation.accepts(&r) => {
                    self.Network.checkResult(&r)?;
                    if let ResultData::Lyrics(lyrics) = &r {
                        lyrics.validate().map_err(|_| error("插件歌词响应无效"))?;
                    }
                    if let ResultData::TrackQualities(list) = &r {
                        list.validate().map_err(|_| error("插件音质响应无效"))?;
                    }
                    Ok(r)
                }
                (None, Some(e)) => Err(e),
                _ => Err(error("插件响应类型不匹配")),
            }
        })();
        if result.as_ref().is_err_and(|e| e.code == "PLUGIN_HOST") {
            self.Broken = true;
            self.terminator().terminate();
        }
        result
    }
    pub fn materialize(&self, source: MediaSource) -> Result<crate::MediaFile, ApiError> {
        self.prepareMedia(source)?.finish(|| true, false)
    }
    pub fn prepareMedia(&self, source: MediaSource) -> Result<crate::MediaTask, ApiError> {
        if let MediaSource::ManagedFile { id } = source {
            let media = self
                .Broker
                .lock()
                .map_err(|_| error("文件代理已失效"))?
                .acquireMedia(id)
                .map_err(|_| error("插件媒体句柄无效"))?;
            Ok(crate::MediaTask::Ready(crate::MediaFile {
                path: media.Path.clone(),
                _temporary: None,
                _mediaLease: Some(media),
            }))
        } else if let MediaSource::Http { url, headers } = source {
            self.Network.check(&url)?;
            let media = self
                .Broker
                .lock()
                .map_err(|_| error("文件代理已失效"))?
                .hostOutput()
                .map_err(|_| error("无法创建插件音频缓存"))?;
            Ok(crate::MediaTask::Download {
                Output: media,
                Url: url,
                Headers: headers,
            })
        } else {
            Err(ApiError::new("MEDIA", "C ABI 插件不得返回任意本地文件路径"))
        }
    }
}
impl NativeProcess {
    pub fn discardMedia(&self, source: MediaSource) {
        if let MediaSource::ManagedFile { id } = source
            && let Ok(mut broker) = self.Broker.lock()
        {
            broker.call(fluent_plugin_abi::files::FileRequest {
                Operation: 17,
                Handle: id,
                ..Default::default()
            });
        }
    }
}
impl Drop for NativeProcess {
    fn drop(&mut self) {
        self.terminator().terminate();
    }
}
