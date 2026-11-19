MyRust 用一个已构建的可执行文件与 `PackageConfig` 打包应用。原有的 `Packager::generate` 仍只生成各平台可审阅元数据；`Packager::package` 为当前宿主构建，`Packager::package_for` 可显式选择目标。公开的 `PackageConfig` 与 `Packager::generate` 签名没有改变。

```rust
use myrust_core::{
    PackageConfig, PackageOptions, PackageTarget, Packager,
};

let config = PackageConfig {
    app_id: "com.example.demo".into(),
    name: "Demo".into(),
    version: "1.2.3".into(),
    author: "Example".into(),
    identifier: "com.example.demo".into(),
    resources: vec!["assets".into()],
    icons: vec!["icon.png".into()],
};

let output = Packager::package_for(
    &config,
    "target/release/demo",
    "dist",
    PackageTarget::Linux,
    &PackageOptions::default(),
)?;
```

## Linux

Linux 打包会暂存 `usr/bin/<name>`、`usr/share/<app_id>` 下的资源、桌面入口与自包含 `install.sh`。先生成 `<package>-<version>-<arch>.tar.gz`，再用 `dpkg-deb --build --root-owner-group` 生成 `.deb`。Arch 目录会得到源 tar 包、带校验和的 `PKGBUILD` 与 install hook。`makepkg` 是可选项：默认不运行；设置 `PackageOptions { run_makepkg: true, .. }` 后，缺少 `makepkg` 会显式报错。

`install.sh` 支持两种布局。在 tar 包内，它把自带二进制与资源安装到 `${PREFIX:-$HOME/.local}`；从骨架目录运行且没有暂存 payload 时，仍可用 `install.sh <binary>` 走原来的单文件兜底。

## macOS

`Packager::package_for(..., PackageTarget::MacOs, ...)` 把可执行文件复制到 `Demo.app/Contents/MacOS`，写入 `Info.plist`，把资源与图标复制到 `Contents/Resources`，然后调用：

```text
hdiutil create -volname Demo -srcfolder Demo.app -ov -format UDZO Demo-1.2.3.dmg
```

DMG 生成只能在 macOS 上执行。其他宿主会保留 `.app` 元数据，并显式报告宿主或工具错误。签名与公证是独立步骤：

```rust
let signing = SigningConfig {
    platform: SigningPlatform::MacOs,
    identity: "Developer ID Application: Example".into(),
    artifact: "dist/macos/Demo.app".into(),
    output: None,
};
signing.sign()?;
signing.notarize()?;
```

macOS 上 `sign()` 优先使用 `codesign --force --options runtime --timestamp --sign <identity> <artifact>`；非 macOS 宿主使用 `rcodesign --pkey <identity> <artifact>`。`notarize()` 需要 macOS 与 `xcrun`，凭据来自 `NOTARYTOOL_PROFILE`，或 `APPLE_ID`、`APPLE_TEAM_ID`、`APPLE_APP_PASSWORD`。它调用 `notarytool --wait` 并 stapling 票据。凭据或工具缺失都会显式失败；测试不会访问 Apple 服务。

## Windows

Windows 打包会暂存 `windows/app/Demo.exe` 与资源，生成 `installer.nsi`，再调用 `makensis`。生成的 NSIS 脚本安装到 `%LOCALAPPDATA%\Demo`，创建开始菜单与桌面快捷方式，并包含卸载段。缺少 `makensis` 时，暂存目录与脚本会保留，错误会点名缺失工具。

Windows Authenticode 签名优先试 `signtool sign /fd SHA256 /td SHA256 /f <certificate> <artifact>`（证书文件），证书库身份则用 `/n <subject>`。`signtool` 不可用时回退到 `rcodesign sign --pkey <identity> <artifact>`。

## 失败与安全规则

- 写文件前先拒绝非 SemVer 版本与不安全的 identifier/name 字符。
- 外部工具执行前已写完骨架，因此工具失败时仍有可审阅输出与已完成的 tar 包或 app bundle。
- 工具输出会被捕获；非零退出会包含 stderr（或 stdout）与保留的输出目录。
- 所有打包命令都不经过 shell，路径作为进程参数传递。
- 本仓库 CI 未在 macOS 或 Windows 上验证 Linux/Windows 打包；真实 Apple 公证与 Authenticode 时间戳验证仍需要对应宿主与凭据。
