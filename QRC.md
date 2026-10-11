# QRC 解密与解析

模块位于 `src/qrc/`，公开为 `qqmusic::qrc`。本模块仅实现格式处理，不请求 QQ 服务器。取词和标准化在独立 `src/lyrics/` 与 `src/provider/lyrics.rs` 中，通过 C ABI `getLyrics` 返回句/字和翻译时间轴；协议见 [PLUGIN_API.md](../../docs/PLUGIN_API.md)。播放器底栏的歌词面板是该接口的消费者，UI 不进入插件格式模块。

## 入口

| 函数 | 输入 | 返回 |
| --- | --- | --- |
| `decrypt_qrc_hex` | 十六进制密文 | 解密后的 UTF-8 文本 |
| `decrypt_qrc_api` | Base64 包装的十六进制密文 | 解密后的 UTF-8 文本 |
| `parse_qrc` | QRC XML，或其中的逐字歌词正文 | `QrcLyric` |
| `decode_qrc_hex` / `decode_qrc_api` | 对应编码的密文 | 解密并解析后的 `QrcLyric` |

不自动猜编码：`decrypt_qrc_api` 对应 JADX 所见的 Base64 外层消费链，`decrypt_qrc_hex` 对应 `QRCDesDecrypt.doDecryptionLyric` 的直接输入。旧 camelCase 函数名保留为兼容别名；Rust 结构字段使用 snake_case，序列化 JSON 仍保持原 camelCase。

```rust
use qqmusic::qrc::parse_qrc;

let lyric = parse_qrc("[1000,2000]你(1000,500)好(1500,500)")?;
assert_eq!(lyric.lines[0].words[1].start_ms, 1500);
```

## 解密依据

纯 Rust 移植 MIT 许可的 [WXRIW/QQMusicDecoder](https://github.com/WXRIW/QQMusicDecoder/tree/0e1494194523dd885405812a91ee9b9702bfb30c)，固定参考版本 `0e1494194523dd885405812a91ee9b9702bfb30c`，保留许可于 `LICENSE-QRC`。

解密为独立 8 字节块的三段修改版 DES（D/E/D），然后 zlib inflate。保留参考实现的两个非标准 S-box 项、每个 32 位字的字节序，以及密钥 D 半部的特殊索引；**不是** RustCrypto 的标准 3DES，也不是音频 QMC 解密。静态格式常量来自该公开参考实现，不读取账号或客户端密钥。

要求完整 zlib 流结束、校验和正确、流尾最多 8 字节零填充，输出必须为 UTF-8；不依赖 Python、FFmpeg、.NET 或客户端 native 库运行。

**验证边界**：已与独立 C# 参考实现生成的合成向量逐字节对照。2026-10-07 经用户明确授权，用当前插件配置读取《ストーリー》/西野加奈的实际歌词响应；最初复现 `LYRICS_FORMAT`，修复后同一响应离线回放成功，保留71句、595个逐字片段。会话文件哈希前后不变，样本仅在本地保存、不提交歌曲全文或账号数据。没有分析当前 APK 的 `libdesdecrypt.so`，不将这一首的验证泛化为全部服务端格式。

## 解析语义

- XML 提取唯一 `LyricContent` 属性，也支持直接正文；保留该属性内的实际换行，并只解码一次 XML 实体。不接受 DTD、自定义实体、重复歌词属性或多个根节点。
- 兼容已实测的 QQ 旧式 XML 包装：仅当严格 XML 解析失败，且唯一的末尾 `LyricContent` 属性含歌词原文的未转义引号时，按 `/>` 前的结束引号识别边界、转义内层引号，然后重新执行完整 XML 和 QRC 校验。不吞并后续属性、不修复无效根节点、实体或时间数据，仍受大小/深度限制。
- 正文格式为 `[行起始毫秒,行时长毫秒]文字(字起始毫秒,字时长毫秒)…`；每个字片段可包含多个字符。
- 行和字使用同一首歌的**绝对时间线**。不把字起点再加上行起点。
- 保留行的有符号起点；负字起点/时长按 JADX 所见归零。非法数字、溢出、负行时长和缺失逐字时间报告错误。
- 支持 `ti/ar/al/offset` 与其它 `[key:value]` 元数据；键为 ASCII 字母开头的字母数字/下划线/连字符，值允许逗号和冒号。行按起点稳定排序。
- 字索引为 Java UTF-16 code unit 的半开区间，同时保留各字片段文本，避免在 Rust 中按 UTF-8 字节误切 emoji。
- **不应用 offset、不复制 APK 的行提前显示修正**，只保留原始时间和 `offset_ms`；显示/同步策略留给后续播放器接入。
- 空正文或仅元数据是正常空歌词；无效结构不会被悄悄降级成空成功。

纯 QRC 模块不处理本地 `.qrc` 文件的额外封装、不解析 LRC、不获取译文/罗马音。Provider 会请求原文和译文，LRC 由独立解析模块处理；暂不返回罗马音。除了上述已验证的引号包装，不尝试通用 XML 修复或猜测未验证的明文返回分支。

Provider 对译文中精确为 `//`（允许首尾空白）的行清空文本及字片段，但保留起点/时长，避免上一句译文越过空译文边界；全为空译文时返回无翻译。原文不应用这个规则。

经用户确认，QRC 编码/解密/格式错误时，可以使用**同一次成功响应中有效的有时间 LRC**；返回的字片段为空，播放器明确显示“句级歌词（逐字歌词不可用）”。无有效 LRC 时保留 QRC 错误；服务器/应用权限拒绝、字段类型错误和大小限制不得被回退掩盖。

显式开发诊断工具 `fluent-qqmusic-plugin.exe --capture-lyrics <歌曲名> <歌手> <新样本目录>` 仅按指定歌曲与歌手搜索、获取一次歌词，允许读取插件自己配置的会话，不刷新/保存会话。仅保存允许列表内的歌词和状态字段；不输出 Cookie、token、应用密钥、签名请求或账号身份。使用 Worker 的本地数据目录时应明确设置 `QQMUSIC_DATA_DIR`。`--replay-lyrics <response.json>` 与 `cargo run -p fluent-qqmusic-plugin --example qrc_replay -- <original.qrc>` 用于离线回放，不发送网络请求。上述诊断不属于 C ABI，不在正常播放中自动开启。

## 边界与错误

- 密文字节最多 512 KiB，原始 hex 最多 1 MiB，Base64 输入最多 2 MiB。
- 解压文本/解析输入最多 2 MiB；最多 10,000 行、100,000 个字片段、128 项元数据。
- XML 最多 40,000 个读取事件、嵌套深度 64。
- 错误使用安全 `ApiError` 文案：`LYRICS_ENCODING`、`LYRICS_DECRYPT`、`LYRICS_FORMAT`、`LYRICS_LIMIT`；不包含原始歌词、密文或上游错误内容。

## 验证

```powershell
cargo test -p fluent-qqmusic-plugin --test qrc_decrypt --test qrc_parser
cargo test -p fluent-qqmusic-plugin
cargo clippy -p fluent-qqmusic-plugin --all-targets -- -D warnings
```

`tests/fixtures/qrcVectors.json` 为固定的合成测试向量，常规测试无需网络。包括 XML/Unicode/大输出、无效 UTF-8、截断 zlib、错误校验和、非零填充与解压炸弹。

重新生成向量可在 Windows PowerShell 中执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File plugins/qqmusic/tests/generateQrcVectors.ps1
```

生成脚本下载上述固定提交的 MIT C# 实现，仅将三处范围切片语法适配旧编译器，使用 `Add-Type` 编译参考算法，再生成 zlib 压缩和加密向量；不调用本项目的 Rust 解密器，不联网获取任何歌曲。该工具只用于开发验证，并非插件运行依赖。
