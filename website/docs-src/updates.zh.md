更新器保留原来的三阶段 API：`check`、`download`、`install`。现在已有可用的签名校验、delta patch、更严格的归档路径检查与回滚。`UpdateInfo` 字段保持不变；delta 元数据是独立类型，不破坏已有 manifest。

```rust
use std::{path::Path, sync::Arc};
use myrust_core::{
    Ed25519SignatureVerifier, LocalUpdateSource, Updater,
};

let source = Arc::new(LocalUpdateSource::new("update.json", "releases"));
let verifier = Arc::new(
    Ed25519SignatureVerifier::from_minisign_public_key(
        std::fs::read_to_string("release.pub")?,
    )?,
);
let updater = Updater::new(source, "1.2.3", verifier)?;
if let Some(info) = updater.check()? {
    let downloaded = updater.download(&info)?;
    updater.install(&downloaded, Path::new("installed-app"))?;
}
```

## 签名格式

`Ed25519SignatureVerifier::new` 接收原始 32 字节公钥，验证对归档字节的 base64 编码 64 字节签名。`from_minisign_public_key` 接收 minisign 公钥文件或其 base64 行，并验证标准 minisign 签名文件：

- `Ed` 直接签文件字节。
- `ED` 签文件字节的 BLAKE2b-512。
- 8 字节 key id 必须与配置公钥一致。
- 有 trusted comment 时必须验证 global signature。校验器接受标准 trusted-comment 形式与较新 minisign 变体的“签名绑定 comment”形式。

没有签名就不安装：`UnimplementedSignatureVerifier` 仍是显式 fail-closed 默认。大小和 SHA-256 检查在签名校验前执行；即使 manifest 没写 `sha256`，签名校验也不会跳过。

## Delta manifest

`DeltaUpdateManifest` 是普通 update 对象再加一个 `delta`：

```json
{
  "version": "2.0.0",
  "notes": "Release notes",
  "signature": "base64 full-archive signature",
  "size": 123456,
  "url": "app-2.0.0.zip",
  "sha256": "full archive digest",
  "delta": {
    "from_version": "1.0.0",
    "to_version": "2.0.0",
    "old_sha256": "installed archive digest",
    "new_sha256": "reconstructed archive digest",
    "url": "1.0.0-to-2.0.0.myrust-delta",
    "signature": "base64 delta signature",
    "size": 4567,
    "sha256": "delta transport digest"
  }
}
```

`Updater::check_delta` 校验 `from_version` 等于当前安装版本，`to_version` 等于 update 版本。随后 `download_delta(info, delta, installed_archive_bytes)`：

1. 校验已安装归档的 SHA-256。
2. 下载 patch 并检查大小/哈希。
3. 校验 patch 签名。
4. 应用 patch 并做内建自检。
5. 校验重建后的完整归档 SHA-256 与完整归档签名。
6. 解包到 staging 目录。

完整归档签名是强制项。只签 delta 不足以授权一棵安装树。

## Patch 格式

格式是确定的 little-endian 二进制：

```text
magic      8 bytes   "MYRDLT01"
old_hash  32 bytes   已安装归档 SHA-256
new_hash  32 bytes   重建归档 SHA-256
old_size   8 bytes   u64
new_size   8 bytes   u64
ops        4 bytes   u32 operation count

operation 0: u8 tag 0, u64 source offset, u64 length   copy old -> output
operation 1: u8 tag 1, u64 byte length, bytes           insert literal
```

`DeltaPatch::create` 以 1 KiB 旧归档块建 rolling hash 索引，再做逐字节确认。`DeltaPatch::apply` 拒绝未知 tag、越界 copy、输出溢出、截断、尾随字节与输入/输出 digest 不匹配。哈希同时写进 patch 和 manifest，任一侧被改都会失败，不会静默安装另一份字节。

## 安装与回滚

`install` 仍按目录树替换，并保留 `<target>.rollback`。它拒绝非更新版本与文件系统根目录。target 清空前先建回滚副本；暂存文件搬移失败时复制回旧树并返回错误。

## 路径安全与剩余边界

ZIP 条目使用 `enclosed_name` 加显式组件检查。Tar 拒绝绝对路径、父目录穿越、反斜杠、符号链接、硬链接、设备文件与其他特殊条目。`LocalUpdateSource` 在拼接 archive root 前拒绝父路径。

更新器尚未给解压设置字节预算或子进程沙箱，也只替换目录而非运行中的可执行文件；调用方须安排进程退出后重启。Delta 创建是简单块匹配，不是最优二进制 diff。Sparkle 式 UI、后台下载调度与平台凭据存储仍不在 core API 内。
