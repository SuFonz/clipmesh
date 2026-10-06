# ClipMesh

**安全的跨设备剪贴板同步工具** — 无中心服务器，P2P 直连，端到端 TLS 加密。

Windows · Linux · macOS · Android

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

## 核心特性

| | |
| --- | --- |
| 🔒 **TLS 1.3 双向认证** | 每台设备持有自己的 Ed25519 密钥与自签证书，通信全程加密 |
| 👤 **显式配对 + 指纹校验** | 发现 ≠ 信任。必须在两台设备上对照证书指纹并手动接受 |
| 🖥 **文本与图片同步** | 图片以 PNG 二进制分片传输，**从不使用 base64** |
| 🔍 **局域网自动发现** | mDNS（`_clipmesh._tcp.local.`），无配置、无端口转发 |
| 📋 **后台剪贴板监听** | 桌面端自动监听；Android 通过前台服务与通知栏按钮 |
| 🧩 **平台无关的核心** | `crates/**` 不依赖 Tauri / Vue / Windows API / Android API |
| 🔁 **防环设计** | UUID 去重 + 回声抑制，两台设备不会互相回弹同一条内容 |

## 安全模型（第一版即具备，不是"以后再说"）

| 威胁 | 对策 |
| --- | --- |
| 局域网窃听 | TLS 1.3，全部流量加密 |
| 中间人替换设备 | 证书指纹固定 + 签名绑定 TLS 通道（见下） |
| 未授权设备读取剪贴板 | 未配对设备**只能**发 `PairRequest`，其余消息一律丢弃 |
| 设备身份伪造 | `Hello` 必须用证书内公钥对应的私钥签名 |
| 重放 / 中继 | 签名内容包含 TLS exporter secret，只在当前连接上有效 |
| 恶意超大帧耗尽内存 | 分帧层在分配前校验 16 MiB 上限 |

握手签名的内容是 `sha256("clipmesh-hello-v1" ‖ challenge ‖ TLS exporter secret)`。
exporter secret 只有这条 TLS 连接的两端能算出，因此攻击者**无法把 A 的握手消息
转发到另一条连接上**冒充 A —— 这是 ClipMesh 与"裸 TLS"的关键区别。

详见 [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)。

## 仓库结构

```
clipmesh/
├── crates/                    # 与平台完全无关的 Rust 核心
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

Rust 1.85+ · Node 20+ · （Android 另需 JDK 17+、Android SDK、NDK）

### 桌面端

```bash
npm install                       # 安装前端依赖（npm workspaces）
npm run build                     # 先构建前端（必需，见下）
cd apps/desktop
npm run dev                       # tauri dev：构建 Rust 并打开窗口
```

> `tauri.conf.json` 的 `frontendDist` 指向 `apps/desktop/ui/dist`，
> 而 `tauri::generate_context!()` 在**编译期**读取该目录 —— 目录不存在时
> `cargo build` 会直接失败。所以任何 Rust 构建之前都要先跑一次前端构建。

只想看界面？**不需要编译 Rust**：

```bash
npm --prefix apps/desktop/ui run dev     # http://localhost:1420
```

未检测到 Tauri 运行时，前端会自动切到内置的 mock 后端，
显示三台示例设备与历史记录，所有交互都可点。见
[`packages/ui-core/src/api/mock.ts`](packages/ui-core/src/api/mock.ts)。

### Android

```bash
cd apps/android
npm run dev                       # tauri android dev
```

完整的 Android 构建步骤（NDK 变量、Gradle 模块接线）见 [`docs/BUILD.md`](docs/BUILD.md)。

### 测试

```bash
cargo test --workspace            # Rust：协议 / 身份 / 安全 / 引擎 / 网络 / 剪贴板
npm run typecheck                 # 前端类型检查
npm run build                     # 构建两端前端
```

## 首次使用

1. 在两台设备上分别启动 ClipMesh。
2. 几秒内它们会互相发现（设备列表出现对方）。
3. 在任意一台点「配对」。
4. **另一台会弹出对话框，显示设备名、平台和证书指纹。**
5. 对照两台设备屏幕上的指纹，一致就点「接受」。

指纹比对才是真正的安全边界 —— 签名只能证明"对方持有那台设备的私钥"，
不能证明"那台设备就是你桌上的那台"。

## 平台差异

| | 桌面 | Android |
| --- | --- | --- |
| 剪贴板监听 | 自动轮询（Windows 用系统序列号，零成本） | Android 10+ 禁止后台读取，改由通知栏按钮触发 |
| 常驻方式 | 系统托盘，关窗口不退出 | 前台服务 + 常驻通知 |
| 接收提醒 | 应用内通知 | 系统通知，带预览 |
| 布局 | 侧边导航 + 多栏 | 底部标签 + 单列 + 48px 触摸目标 |

Android 的后台剪贴板限制是系统级约束：**没有**合法办法让后台应用读取剪贴板，
所以「广播剪贴板」按钮会把应用带到前台再读取。这是设计，不是妥协。

## 文档

| 文档 | 内容 |
| --- | --- |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | 架构、模块职责、威胁模型、数据流 |
| [`docs/IPC.md`](docs/IPC.md) | 前端 ↔ Rust 的冻结契约（命令、事件、类型） |
| [`docs/BUILD.md`](docs/BUILD.md) | 构建、打包、Android 接线、故障排查 |

## 协议版本

当前 `PROTOCOL_VERSION = 1`。版本不一致时连接会被拒绝并回报
`ERROR_CODE_PROTOCOL_VERSION_MISMATCH`，而不是尝试兼容解析。

## 许可证

MIT，全文见 [`LICENSE`](LICENSE)。
