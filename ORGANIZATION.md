# QQ 音乐插件模块组织

本插件内部遵循 Rust 命名：文件、模块、函数、变量和字段使用 `snake_case`，类型与枚举变体使用 `UpperCamelCase`，常量使用 `SCREAMING_SNAKE_CASE`。不以平台或产品名称命名实现职责。

## 职责划分

| 原文件/符号 | 当前职责模块/入口 |
| --- | --- |
| `flyme.rs` / `Client` | `recommendation::RecommendationClient` |
| 签名材料和 m1 签名 | `recommendation/signing.rs`，保留嵌入样本 SHA-256 绑定、指令校验和 TEA/HMAC 逻辑 |
| 推荐请求、严格压缩响应 | `recommendation/transport.rs`、`response.rs` |
| 基础会话、OpenID 验证、首页解析 | `recommendation/authorization.rs`、`feed.rs`、`parsing.rs` |
| `Provider.flyme` / `flyme_enabled` | `recommendation_client`；生产构建固定使用签名首页，无模式开关 |
| `enable_flyme_homepage` / `flyme_feed` | 不再需要启用调用；首页入口为 `signed_recommendation_feed` |
| `provider.rs` | `provider/mod.rs` 仅管理状态定义、构造及操作路由；授权、会话、请求、首页回退、歌单分页、歌词、歌曲解析、诊断和元数据各有职责模块 |
| `protocol.rs` | `protocol/{config,client,response,session,signing}.rs` |
| `session_store.rs` | `session_store/mod.rs` 管理应用绑定与代际；`storage.rs` 管理受控 I/O、锁和路径校验；`protection.rs` 管理 DPAPI |
| `qmc.rs` | `qmc/{keys,stream,format,conversion}.rs` |
| `discovery.rs` | `discovery/client.rs` 和 `parsing.rs`，测试服务独立于生产实现 |
| `media.rs` | `media/mod.rs` 保留授权资源选择、下载转换与生命周期；离线测试移入 `tests.rs` |
| camelCase 文件和歌词/QRC 函数 | 对应 snake_case 文件和入口，例如 `broker_media`、`plugin_config`、`parse_qrc`、`capture_lyrics` |

测试按对应职责组织，保留原有分页、公开/账号回退、授权刷新、凭证代际、媒体缓存、QMC/QRC 和响应限制断言。每个 Rust 文件不超过 350 行，`tests/structure.rs` 防止文件命名、模块大小与 blanket naming allow 回退。

## 兼容边界

- 首页默认固定使用签名推荐路径；旧 `--flyme-homepage` / `--signed-homepage` 参数仍可出现在命令行，但不再改变模式。诊断参数继续兼容旧名称。
- 插件创建时不读取模式选项；旧 `homepageMode` / `homepage_mode` / `recommendation_source` 即使保留也不影响默认路径。随插件分发的描述文件仅指向 `qqmusic.dll`。
- 新 Rust 入口为 `diagnose_recommendation_account`、`diagnose_recommendations`、`diagnose_saved_session`、`capture_lyrics`、`replay_lyrics`。旧 camelCase 入口仅作为显式兼容重导出保留；QRC 旧函数名同理。
- QRC 本地结构字段改为 snake_case，但 `serde(rename_all = "camelCase")` 保持原 JSON 格式；远端协议字段、服务端模块、URL、客户端标识、诊断 JSON 标签、错误码和原有提示内容不改。
- `plugin-api` / `plugin-abi` / Windows API 的外部字段和函数大小写保持原样。唯一命名 lint 例外是加载器规定的导出符号 `fmPluginGetApi`，且只作用于该函数。

原 `flyme.rs` 的已有本地签名样本改动和限长/尾随数据拒绝处理均从当前工作树迁移，未还原成 HEAD。签名样本仍是数据，不执行原生库、不导入 HAR、不代替播放授权。

## 离线验证

在没有现有 `.env` 的临时目录运行 cargo，并通过工作区 `--manifest-path` 指定项目。编译串行 `-j 1`，测试串行 `--test-threads=1`。测试仅使用合成会话、临时存储与回环 HTTP 服务；不运行真实账号诊断或歌词采集，不读取或改动现有登录数据，不部署 `dist`。

## 内置运行参数

`plugin_config.rs` 固定提供应用 ID `10022`、应用密钥、v2 授权、指定的 HTTPS 网关、包名 `com.tencent.qqmusicpad` 与设备名 `PLK110`。总请求/二维码期限为180秒，单次 API 请求最多15秒，二维码轮询请求起点至少相隔1000ms；显式的组合接口阶段预算仍遵守宿主期限，音频文件下载沿用其独立期限。

运行时不解析 `.env`、不读取外部公共参数文件，也不接受应用参数的环境变量覆盖。设备 ID 首次生成、恢复登录时采用加密会话内的原设备 ID；不为全部用户硬编码同一身份。DPAPI 绑定、授权刷新和登录文件位置保持原逻辑，开发进程仍可使用 `QQMUSIC_DATA_DIR` / `QQMUSIC_PERSIST_SESSION` 环境变量隔离测试存储，这两项不来自 `.env`。

`cargo build -p fluent-qqmusic-plugin --release` 直接生成 `target/release/qqmusic.dll`；Rust 库名相应为 `qqmusic`。部署脚本使用 `dist/plugins/qqmusic/qqmusic.dll`，保留旧 `.env` 与登录文件，但不再读取 `.env`。内置应用密钥可从二进制提取，不能代替用户扫码授权、版权或会员校验。
