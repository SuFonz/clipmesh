# ClipMesh

[English](README.md) | **简体中文**

<p align="center">
  <img src="icon.png" alt="ClipMesh" width="160" />
</p>

**安全的跨设备剪贴板同步工具** — 无中心服务器，P2P 直连，端到端 TLS 加密。

---

## 这是什么

ClipMesh 让你在**自己的**设备之间同步剪贴板：在笔记本上复制一段文字或一张截图，
手机上立刻收到通知，一键粘贴。

没有账号，没有云，没有中转服务器。设备在局域网里通过 mDNS 互相发现，
直接建立 **TLS 1.3 双向认证**连接，数据只在两台设备之间流动。

```
     ┌──────────┐        mDNS 发现        ┌──────────┐
     │  Laptop  │◄───────────────────────►│  Phone   │
     │          │                         │          │
     │          │   TCP + TLS 1.3 (mTLS)  │          │
     │          │◄───────────────────────►│          │
     └──────────┘   剪贴板 / 图片 / 配对    └──────────┘
              没有服务器。没有第三方。
```

## 截图

> 截图待补充。

<!--
  补图时把图片放进 docs/ 并取消下面的注释：

  <p align="center">
    <img src="docs/desktop-home.png" alt="桌面端 - 首页" width="720" />
  </p>
-->


## 核心特性

| | |
| --- | --- |
| 🔒 **TLS 1.3 双向认证** | 每台设备持有自己的 Ed25519 密钥与自签证书，通信全程加密 |
| 👤 **显式配对 + 指纹校验** | 发现 ≠ 信任。必须在两台设备上对照证书指纹并手动接受 |
| 🖥 **文本与图片同步** | 图片以 PNG 二进制分片传输，**从不使用 base64** |
| 🔍 **局域网自动发现** | mDNS（`_clipmesh._tcp.local.`），无配置、无端口转发 |
| 📋 **后台剪贴板监听** | 桌面端自动监听；Android 通过前台服务与通知栏按钮 |
| 🧩 **平台无关的核心** | `crates/**`（除按平台区分的 `crates/clipboard` 之外）不依赖 Tauri / Vue / Windows API / Android API |
| 🔁 **防环设计** | UUID 去重 + 回声抑制，两台设备不会互相回弹同一条内容 |

## 安全模型（第一版即具备，不是“以后再说”）

| 威胁 | 对策 |
| --- | --- |
| 局域网窃听 | TLS 1.3，全部流量加密 |
| 中间人替换设备 | 证书指纹固定 + 签名绑定 TLS 通道（见下） |
| 未授权设备读取剪贴板 | 未配对设备**只能**发配对消息 —— `PairRequest`，或应答本设备所发请求的 `PairAccept`；其余消息一律丢弃 |
| 设备身份伪造 | `Hello` 必须用证书内公钥对应的私钥签名 |
| 重放 / 中继 | 签名内容包含 TLS exporter secret，只在当前连接上有效 |
| 恶意超大帧耗尽内存 | 分帧层在分配前校验 16 MiB 上限 |

握手签名的内容是 `sha256("clipmesh-hello-v1" ‖ challenge ‖ TLS exporter secret)`。
exporter secret 只有这条 TLS 连接的两端能算出，因此攻击者**无法把 A 的握手消息
转发到另一条连接上**冒充 A —— 这是 ClipMesh 与“裸 TLS”的关键区别。

详见 [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.zh-CN.md)。

## 仓库结构

```
clipmesh/
├── crates/                    # Rust 核心；除 clipboard/ 外与平台无关
│   ├── protocol/              # protobuf 协议、分帧、payload 模型
│   ├── identity/              # Ed25519、自签证书、指纹、信任库
│   ├── security/              # rustls 配置、证书策略、通道绑定握手
│   ├── core/                  # 同步引擎：Provider trait、去重、设备表、事件
│   ├── network/               # mDNS + TCP + TLS 会话
│   └── clipboard/             # 各平台剪贴板（Android 走 Kotlin 桥）
├── packages/ui-core/          # 两端共享的前端：类型 / store / API / 通用组件
├── apps/
│   ├── desktop/{src-tauri,ui} # 桌面 Tauri 宿主 + 桌面布局
│   └── android/
│       ├── src-tauri/         # 移动 Tauri 宿主（复用桌面的命令层）
│       ├── ui/                # 移动布局
│       └── plugins/bridge/   # Kotlin 插件：剪贴板 / 通知 / 前台服务
└── docs/
```

## 快速开始

### 环境要求

| 组件 | 版本 | 说明 |
| --- | --- | --- |
| Rust | **1.85+**（edition 2024） | `rustup default stable` |
| Node.js | **20+** | 前端与 npm workspaces |
| JDK | **17+** | 仅 Android |
| Android SDK | compileSdk **37** | 仅 Android |
| Android NDK | **26+** | 仅 Android |

**不需要 `protoc`。** 协议编译走 `protox`（纯 Rust 实现），
因此全新克隆的机器只要有 Rust 工具链就能构建，CI 与交叉编译宿主都一样。

加密后端固定为 **ring**：`rustls` 与 `tokio-rustls` 都显式关闭了默认特性，
避免 `aws-lc-rs` 引入 C 工具链 / cmake / NASM。请勿在 `Cargo.toml` 里
把它们的 `default-features` 打开 —— 那会同时启用两个 provider，
`ServerConfig::builder()` 会在运行时 panic。

> **实际验证过的组合。** 只有 **Windows** 构建环境跑通过：在 Windows 上构建能同时产出
> 可用的 Windows 程序和 Android APK。**其他组合都未经验证** —— 三个桌面平台都已接线
> （见按平台的打包配置 `apps/desktop/src-tauri/tauri.*.conf.json`），平台相关的代码也只集中在
> `crates/clipboard`，但 **Linux 和 macOS 的构建从未跑起来过**。
> **请当作未验证，而不是已知可用。**

### 首次安装

```bash
npm install            # 仓库根目录，一次装齐所有 workspace
cargo fetch            # 可选：预热 crates 缓存
```

> **注意**：本仓库根目录有一个 `.npmrc`，内容是 `include=dev`。
> 某些环境会导出 `NODE_ENV=production`，让 npm 默认跳过全部 devDependencies，
> 结果是装完却无法构建（没有 vite / vue-tsc / tauri CLI）。
> 如果不想要这个文件，删掉后请用 `npm install --include=dev`。

### 桌面端

#### 开发

```bash
npm run dev:desktop        # = tauri dev
```

Tauri 会：
1. 在 `apps/desktop/` 下执行 `beforeDevCommand`：`npm --prefix ui run dev`（Vite，端口 1420，`strictPort`）
2. 编译 `apps/desktop/src-tauri`（首次约 3–10 分钟，之后为增量编译）
3. 打开指向 `http://localhost:1420` 的窗口

#### 只看前端（不编译 Rust）

```bash
npm run dev:desktop:ui
```

浏览器打开 <http://localhost:1420>。检测不到 Tauri 运行时（没有 `__TAURI_INTERNALS__`）时，
`packages/ui-core/src/api/transport.ts` 会自动切到内置的 mock 后端：三台示例设备与历史记录，
所有交互都可点，假事件会自己变化。适合调 UI。见
[`packages/ui-core/src/api/mock.ts`](packages/ui-core/src/api/mock.ts)。

#### 打包

```bash
npm run build:desktop:ui     # 必须先生成 ui/dist
npm run build:desktop        # = tauri build
```

产物在仓库根的工作区 target 目录：`target/release/bundle/`。

> `tauri.conf.json` 的 `frontendDist` 是 `../ui/dist`，
> 即 `apps/desktop/ui/dist`。`tauri::generate_context!()` 在编译期读取该目录，
> **目录不存在时 `cargo build` 会失败**。所以纯 Rust 的 `cargo check` 之前，
> 至少要跑一次前端构建。`dist/` 在 `.gitignore` 里，全新克隆必须自己构建一次。

#### 开发日志

```bash
CLIPMESH_LOG=debug npm run dev:desktop        # Windows PowerShell: $env:CLIPMESH_LOG="debug"
```

### Android

Gradle 模块接线 —— `gen/android` 里四处 `tauri android init` 不会复现的改动 —— 见
[`docs/MAINTENANCE.md`](docs/MAINTENANCE.zh-CN.md) §1。

#### 环境变量

```powershell
$env:JAVA_HOME      = "D:\Program Files\Java\jdk-17"      # 或 Android Studio 自带 jbr
$env:ANDROID_HOME   = "D:\Program Files\Android\Sdk"
$env:NDK_HOME       = "$env:ANDROID_HOME\ndk\29.0.13846066"
```

Tauri 还会读 `TAURI_ANDROID_PROJECT_PATH`（默认 `src-tauri/gen/android`）。

**release 签名。** `app/build.gradle.kts` 从**仓库之外**的四个值取签名材料 —— 先查 Gradle 属性，再查环境变量。放进**全局**的 `~/.gradle/gradle.properties` 就不会进这个项目，也不会出现在 `git status` 里：

```properties
KEYSTORE_FILE=C:\\path\\to\\store.keystore
KEYSTORE_PASSWORD=…
KEY_ALIAS=…
KEY_PASSWORD=…
```

| 变量 | 含义 |
| --- | --- |
| `KEYSTORE_FILE` | `.jks` / `.keystore` 的路径 —— 绝对路径，或相对 `app/` 的路径 |
| `KEYSTORE_PASSWORD` | 密钥库口令 |
| `KEY_ALIAS` | 库中密钥的别名 |
| `KEY_PASSWORD` | 该密钥的口令 |

**只有 release 需要它们。** debug 构建 —— `npm run dev:android`、`assembleDebug` —— 用 debug 密钥签名，四个值一个都不看。release 构建缺任何一个都能配置成功、也能跑完，但产物是**未签名的 APK/AAB，装不上**；Gradle 会打印警告，列出缺的是哪几个。

> 签名密钥与已安装应用不一致时，Android 会拒绝安装这次更新。绕过它就得先卸载 —— 而卸载会删掉应用的私有目录，这台设备的身份就在那里。**所有已配对的关系都会失效，必须重新配对。** 请保管好 release 密钥库并做好备份。

#### Rust target

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

#### 运行与打包

```bash
npm run dev:android              # = tauri android dev
npm run build:android:release    # = tauri android build  ->  已签名的 APK / AAB
```

### 质量门

提交前应当全部通过：

```bash
cargo test --workspace                        # Rust：协议 / 身份 / 安全 / 引擎 / 网络 / 剪贴板
cargo clippy --workspace --all-targets        # 建议
npm run typecheck                             # 前端类型检查
npm run build:ui                              # 构建两端前端
```

`cargo test --workspace` 会一并编译 `apps/android/src-tauri`，
但**不会**执行 Gradle —— Android 的 Java/Kotlin 侧需要在真机或模拟器上验证，
或用 `cd apps/android/src-tauri/gen/android && ./gradlew :bridge:assembleDebug`。

### 故障排查

| 现象 | 原因 / 处理 |
| --- | --- |
| ``The `frontendDist` configuration is set to `"../ui/dist"` but this path doesn't exist`` | 先跑 `npm run build:desktop:ui` |
| `Could not automatically determine the process-level CryptoProvider` | `rustls` 的 `default-features` 被打开了，同时启用了 ring 与 aws-lc-rs。见上面的**环境要求** |
| 设备互相发现不了 | 检查防火墙是否放行 UDP 5353（mDNS）与 TCP 47711；某些企业 Wi-Fi 禁用组播 |
| 端口 47711 被占用 | 正常：会自动改用临时端口并通过 mDNS 广播真实端口 |
| 配对后仍不同步 | 检查设置里的 `autoSync` / `syncText` / `syncImages`；确认设备列表里对方是「在线」 |
| Android 后台不再同步 | 检查前台服务是否在运行（设置页有开关），以及通知权限是否授予 |
| Android 点「广播剪贴板」跳到前台 | 预期行为，见 [`docs/MAINTENANCE.md`](docs/MAINTENANCE.zh-CN.md) §2 |
| `npm install` 后没有 vite | 见上面**首次安装**里的 `.npmrc` 说明 |
| Gradle 找不到 `:bridge` | `gen/android` 被重新生成了，按 [`docs/MAINTENANCE.md`](docs/MAINTENANCE.zh-CN.md) §1 补回四处 |
| **release** 构建在写剪贴板（或图片、通知）时报 ``Cannot construct instance of `d20` (no Creators, like default constructor, exist)`` | R8 删掉了 `Invoke.parseArgs` 用反射反序列化的参数类。`apps/android/plugins/bridge/consumer-rules.pro` 负责保住它们；如果新加了带参数的 `@Command`，要把它一起加进去。见 [`docs/MAINTENANCE.md`](docs/MAINTENANCE.zh-CN.md) §3 |
| 内容被状态栏或导航栏盖住 | webview 没让开。在 Android 上 `env(safe-area-inset-*)` 解决不了（见 [`docs/MAINTENANCE.md`](docs/MAINTENANCE.zh-CN.md) §4）—— 先确认 [`docs/MAINTENANCE.md`](docs/MAINTENANCE.zh-CN.md) §1 的第 ④ 处还在 `MainActivity.kt` 里，再确认移动端布局没有把 `env()` 加回来 |
| `Error: 注释中不允许出现字符串 "--"`（`mergeUniversalDebugResources`） | 某个 `res/values/*.xml` 的**注释里出现了两个连续的连字符**。XML 规范禁止这种写法，而 aapt2 只在资源合并阶段才报，报错位置还很靠后。本仓库踩过一次：`ic_launcher_background.xml` 的注释里写了 `npm run icons -- --bg ...`。`scripts/update-icons.mjs` 现在有断言拦住这个回归 |
| `SigningConfig`/`compileSdk` 不一致 | 插件模块的 `compileSdk`/Java 版本必须与 `app/build.gradle.kts` 一致（当前 37 / Java 8） |

## 首次使用

1. 在两台设备上分别启动 ClipMesh。
2. 几秒内它们会互相发现（设备列表出现对方）。
3. 在任意一台点「配对」。
4. **另一台会在「设备」页显示一张「配对请求」卡片，上面有设备名、平台和证书指纹。**
5. 对照两台设备屏幕上的指纹，一致就点「接受」。

指纹比对才是真正的安全边界 —— 签名只能证明“对方持有那台设备的私钥”，
不能证明“那台设备就是你桌上的那台”。

## 平台差异

| | 桌面 | Android |
| --- | --- | --- |
| 剪贴板监听 | 自动轮询（Windows 用系统序列号，零成本） | Android 10+ 禁止后台读取，改由通知栏按钮触发 |
| 常驻方式 | 系统托盘，关窗口不退出 | 前台服务 + 常驻通知 |
| 接收提醒 | 应用内通知 | 系统通知，带预览 |
| 布局 | 侧边导航 + 多栏 | 底部标签栏 + 单列 + 48px 触摸目标 |

Android 的后台剪贴板限制是系统级约束：**没有**合法办法让后台应用读取剪贴板，
所以「广播剪贴板」按钮会把应用带到前台再读取。这是设计，不是妥协。

## 文档

| 文档 | 内容 |
| --- | --- |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.zh-CN.md) | 架构、模块职责、威胁模型、数据流 |
| [`docs/IPC.md`](docs/IPC.zh-CN.md) | 前端 ↔ Rust 的冻结契约（命令、事件、类型） |
| [`docs/MAINTENANCE.md`](docs/MAINTENANCE.zh-CN.md) | Android 插件接线、R8 keep 规则、edge-to-edge 留白、数据位置、更换应用图标 |

## 协议版本

当前 `PROTOCOL_VERSION = 1`。版本不一致时连接会被拒绝并返回
`ERROR_CODE_PROTOCOL_VERSION_MISMATCH`，而不是尝试兼容解析。

## 许可证

MIT，全文见 [`LICENSE`](LICENSE)。
