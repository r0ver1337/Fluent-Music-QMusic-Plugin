# 构建与验证

Windows，Rust 1.92 或更新版本，MSVC 与 Windows SDK。

```powershell
cargo build --release --locked --workspace -j 1
cargo test --release --locked -p fluent-qqmusic-plugin -j 1
```

插件文件为 `target/release/qqmusic.dll`；将其与根目录 `plugin.json`、`LICENSE`、`LICENSE-QMC`、`LICENSE-QRC` 放入播放器的 `plugins/qqmusic`。播放器使用匹配 API / ABI 2 的 `pluginloader.exe`。

本仓库附带固定版本的 MIT SDK 源码，不依赖另一个私有仓库、原工作区、分析目录或 `libmer.so`。默认测试使用合成数据，不需要真实账号。

独立迁移验证：库与兼容进程入口各128项测试通过，另有30项集成 / 格式测试通过；每个入口各忽略1项仅用于强制退出测试的子进程助手。实际 DLL / AppContainer / FileApi 握手通过；签名材料 SHA-256 与移除二进制依赖前的实现相同。未因此声称验证了所有真实账号或在线音轨。
