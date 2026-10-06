# 构建与发布

[English](BUILD.md) | **简体中文**

本文件覆盖：桌面端打包、Android 构建与原生插件接线，以及常见故障排查。

---

## 1. 环境要求

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

---

## 2. 首次安装

```bash
npm install            # 仓库根目录，一次装齐所有 workspace
cargo fetch            # 可选：预热 crates 缓存
```

> **注意**：本仓库根目录有一个 `.npmrc`，内容是 `include=dev`。
> 某些环境会导出 `NODE_ENV=production`，让 npm 默认跳过全部 devDependencies，
> 结果是装完却无法构建（没有 vite / vue-tsc / tauri CLI）。
> 如果不想要这个文件，删掉后请用 `npm install --include=dev`。

---

## 3. 桌面端

### 开发

```bash
npm run dev:desktop        # = tauri dev
```

Tauri 会：
1. 在 `apps/desktop/` 下执行 `beforeDevCommand`：`npm --prefix ui run dev`（Vite，端口 1420，`strictPort`）
2. 编译 `apps/desktop/src-tauri`（首次约 3–10 分钟，之后为增量编译）
3. 打开指向 `http://localhost:1420` 的窗口

### 只看前端（不编译 Rust）

```bash
npm run dev:desktop:ui
```

浏览器打开 <http://localhost:1420>。检测不到 `__TAURI_INTERNALS__` 时，
`packages/ui-core/src/api/transport.ts` 会自动回落到 `api/mock.ts`，
提供三台示例设备、历史记录和会自己变化的假事件。适合调 UI。

### 打包

```bash
npm run build:desktop:ui     # 必须先生成 ui/dist
npm run build:desktop        # = tauri build
```

产物在仓库根的工作区 target 目录：`target/release/bundle/`。

> `tauri.conf.json` 的 `frontendDist` 是 `../ui/dist`，
> 即 `apps/desktop/ui/dist`。`tauri::generate_context!()` 在编译期读取该目录，
> **目录不存在时 `cargo build` 会失败**。所以纯 Rust 的 `cargo check` 之前，
> 至少要跑一次前端构建。`dist/` 在 `.gitignore` 里，全新克隆必须自己构建一次。

### 开发日志

```bash
CLIPMESH_LOG=debug npm run dev:desktop        # Windows PowerShell: $env:CLIPMESH_LOG="debug"
```

---

## 4. Android

### 4.1 环境变量

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

### 4.2 Rust target

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

### 4.3 原生插件接线（重要）

Kotlin 插件在 `apps/android/plugins/bridge/`，是**独立的 Gradle library 模块**，
不在 `gen/android` 里 —— 这样重新执行 `tauri android init` 不会覆盖它。

除 README 之外，接线只有三处，都**已经写入仓库**，此处记录是为了说明为什么：

**① `apps/android/src-tauri/gen/android/settings.gradle`**

```gradle
include ':bridge'
project(':bridge').projectDir = new File(rootDir, '../../../plugins/bridge')
```

`rootDir` 是 `gen/android`，要**三层** `..` 才回到 `apps/android/`：

```
gen/android  --..-->  gen  --..-->  src-tauri  --..-->  android
```

> 这里曾经写成两层（`../../`），是错的 —— Gradle 会找不到模块并报
> `Project with path ':bridge' could not be found`。

**② `apps/android/src-tauri/gen/android/app/build.gradle.kts`**

```kotlin
dependencies {
    implementation(project(":bridge"))
    ...
}
```

插件的 `AndroidManifest.xml` 会通过 manifest merger 把权限
（`FOREGROUND_SERVICE`、`FOREGROUND_SERVICE_DATA_SYNC`、`POST_NOTIFICATIONS`、
`RECEIVE_BOOT_COMPLETED`、`CHANGE_WIFI_MULTICAST_STATE`）以及
Service / Receiver 声明合并进应用。

**③ 同一个文件 —— release 签名**

`signingConfigs` 块，以及 `buildTypes.release` 里的 `signingConfig = …` 那一行，见 §4.1。
Tauri 的模板里这两样都没有，所以重新生成的项目打出来的 release 是未签名的，直到把它们补回去。

> 如果重新生成了 `gen/android`（删除后跑 `tauri android init`），
> **上面三处**都需要重新加上。这些是仅有的、需要手工维护的生成文件改动。

### 4.4 运行与打包

```bash
npm run dev:android              # = tauri android dev
npm run build:android:release    # = tauri android build  ->  已签名的 APK / AAB
```

### 4.5 前端怎么和 Kotlin 说话

```
Vue  invoke("send_clipboard")
  ↓
Rust  clipmesh_core::SyncManager → AndroidClipboardProvider
  ↓
Rust  KotlinClipboardHost → PluginHandle::run_mobile_plugin("readClipboard")
  ↓
Kotlin ClipMeshPlugin.readClipboard  → ClipboardManager
```

反向（通知栏按钮）：

```
通知「Broadcast clipboard」
  ↓  PendingIntent → BroadcastActivity（透明主题，exported=false，独立 taskAffinity）
  ↓  Android 10+ 只有前台应用能读剪贴板，所以需要一个能拿到焦点的窗口
Kotlin BroadcastActivity.onWindowFocusChanged → ClipboardAccess.read
  ↓  读到的内容（或"什么都没读到"）交给进程级 BroadcastHandoff
Rust  broadcast::spawn 轮询 takeBroadcast（前台服务在跑时 500ms，否则 5s）
  ↓
Rust  engine.send_explicit(content)  → 对端
```

通知按钮**不会**再打开可见界面，但 Activity 有三种失败模式，前两种会退回旧行为
（把真正的 `MainActivity` 带到前台、广播完再 `moveTaskToBack`）：

| 失败 | 表现 | 处理 |
| --- | --- | --- |
| 2 秒内拿不到窗口焦点 | 透明 Activity 已启动 | 回退：`ACTION_BROADCAST` 拉前台，`visible=true` |
| 拿到焦点但剪贴板读不出内容 | 同上 | 同上 |
| 通知的 `PendingIntent` 被 ROM 拦下（后台启动 Activity 限制） | 什么都没有发生 | **无法感知、无法补救**：应用侧拿不到任何回调，只能让用户用应用内那个按钮 |

回退路径由 Rust 收尾：读剪贴板（重试几次，窗口刚起来时读不到是正常的），然后
`send_explicit`，成功后再 `leaveApp()` 把用户送回原来的应用。

`send_explicit` 而不是 `AndroidClipboardProvider::push`：`push` 发出的是一条“本地剪贴板
变化”，引擎的 `handle_local_change` 会在 `autoSync` 关闭时把它丢掉 —— 而会手动按
这个按钮的，正是这类用户。

Rust ↔ Kotlin 的方法名一一对应，**改一边必须改另一边**（运行时才报错）：

| Rust (`plugin.rs`) | Kotlin (`ClipMeshPlugin.kt`) |
| --- | --- |
| `readClipboard` | `readClipboard` |
| `setText` | `setText` |
| `setImage` | `setImage` |
| `showReceived` | `showReceived` |
| `startService` / `stopService` / `serviceRunning` | 同名 |
| `requestNotificationPermission` | `requestNotificationPermission` |
| `leaveApp` | `leaveApp` |
| `takeBroadcast` | `takeBroadcast` |

Kotlin 类名与包名在 `apps/android/src-tauri/src/plugin.rs` 的
`ANDROID_PLUGIN_PACKAGE` / `ANDROID_PLUGIN_CLASS` 常量里。

---

## 5. 数据存放位置

| 平台 | 目录 |
| --- | --- |
| Windows | `%APPDATA%\ClipMesh\` |
| Linux | `~/.config/ClipMesh/` |
| macOS | `~/Library/Application Support/ClipMesh/` |
| Android | 应用私有目录 |

| 文件 | 内容 |
| --- | --- |
| `identity.json` | deviceId、设备名、创建时间 |
| `device.key` | Ed25519 私钥（PKCS#8 DER，unix 下 0600） |
| `device.crt` | 自签 X.509 证书（PEM） |
| `trusted_devices.json` | 已配对设备：证书 + 指纹 + 公钥 |
| `settings.json` | 用户设置 |
| `history.json` | 持久化的剪贴板历史（容量由设置里的 `historyCapacity` 决定） |
| `images/` | 历史图片的 PNG 像素，每个条目一个文件（`images/<id>.png`） |

**重新配对**：删掉 `trusted_devices.json`（或两台都删）即可。
**完全重置**：删掉整个目录 —— 注意这会生成新身份，所有旧配对失效。

---

## 6. 故障排查

| 现象 | 原因 / 处理 |
| --- | --- |
| ``The `frontendDist` configuration is set to `"../ui/dist"` but this path doesn't exist`` | 先跑 `npm run build:desktop:ui` |
| `Could not automatically determine the process-level CryptoProvider` | `rustls` 的 `default-features` 被打开了，同时启用了 ring 与 aws-lc-rs。见 §1 |
| 设备互相发现不了 | 检查防火墙是否放行 UDP 5353（mDNS）与 TCP 47711；某些企业 Wi-Fi 禁用组播 |
| 端口 47711 被占用 | 正常：会自动改用临时端口并通过 mDNS 广播真实端口 |
| 配对后仍不同步 | 检查设置里的 `autoSync` / `syncText` / `syncImages`；确认设备列表里对方是「在线」 |
| Android 后台不再同步 | 检查前台服务是否在运行（设置页有开关），以及通知权限是否授予 |
| Android 点「广播剪贴板」跳到前台 | 预期行为，见 §4.5 |
| `npm install` 后没有 vite | 见 §2 的 `.npmrc` 说明 |
| Gradle 找不到 `:bridge` | `gen/android` 被重新生成了，按 §4.3 补回三处接线 |
| `Error: 注释中不允许出现字符串 "--"`（`mergeUniversalDebugResources`） | 某个 `res/values/*.xml` 的**注释里出现了两个连续的连字符**。XML 规范禁止这种写法，而 aapt2 只在资源合并阶段才报，报错位置还很靠后。本仓库踩过一次：`ic_launcher_background.xml` 的注释里写了 `npm run icons -- --bg ...`。`scripts/update-icons.mjs` 现在有断言拦住这个回归 |
| `SigningConfig`/`compileSdk` 不一致 | 插件模块的 `compileSdk`/Java 版本必须与 `app/build.gradle.kts` 一致（当前 37 / Java 8） |

---

## 7. 更换应用图标

源图在仓库根目录：`icon.png`（正方形；`tauri icon` 要求 ≥1024，当前 1254×1254）。

换图后跑一条命令即可：

```bash
npm run icons
```

它做四件事（脚本在 [`scripts/update-icons.mjs`](../scripts/update-icons.mjs)）：

1. 在 `apps/desktop` 里跑 `tauri icon`，生成桌面 + 移动全部格式
2. 把平面格式同步给 `apps/android/src-tauri/icons/`
3. 把自适应图标（`mipmap-*` + `mipmap-anydpi-v26` + 背景色）复制进
   `apps/android/src-tauri/gen/android/app/src/main/res/`
4. 删掉桌面项目里多出来的 `android/` 与 `ios/` 子目录

### 背景色

Android 自适应图标的背景层是**一个纯色**。`tauri icon` 固定写 `#FFFFFF`，
对彩色图标来说，四周会露出一圈白边，所以脚本会重写这个文件：

```
apps/android/src-tauri/gen/android/app/src/main/res/values/ic_launcher_background.xml
```

默认值 `#AABFF5` 是从当前源图四条边的中点采样后取平均得到的。换了配色差别大的图之后
应当重新采样 —— 分别取上下左右四条边中点向内约 40px 处的像素求平均即可：

```bash
node scripts/update-icons.mjs --bg '#RRGGBB'
```

> 注意 `npm run icons -- --bg ...` **不可用**：npm 会把 `--bg` 当成自己的参数并报
> `EUNKNOWNCONFIG`。要看输出就带上 `--source` / `--bg` 直接跑 `node`。

### 关于透明通道

当前源图**带** alpha，圆角是真透明，因此 `.ico` / `.icns` 的圆角也是透明的。
（上一版源图没有 alpha，圆角是画上去的深色像素，Windows 任务栏里能看到方角 ——
如果以后又换回不带 alpha 的图，会复现这个问题。）

### ⚠️ 换完图标，exe 还是旧图标？

这是本项目实际踩过的坑，原因不在图标缓存，而在 **cargo 的 build script 缓存**：

`tauri-build` 在 Windows 上会生成一份 `resource.rc`，里面用绝对路径指向
`icons/icon.ico`，再编译进 exe。但它只对 `tauri.conf.json` 和 `capabilities/`
发 `cargo:rerun-if-changed`，**不为图标发**。

而 cargo 的规则是：**只要 build script 发了任意一条 `rerun-if-changed`，
“包内文件变了就重跑”的兜底逻辑就失效**，此后只认那张清单。
于是 `icons/icon.ico` 对 cargo 完全隐形 —— build.rs 永远不重跑，
那份指向旧图标的 `resource.rc` 就一直躺在缓存里，被链接进每一个新 exe，
**构建成功、程序正常、图标默默是错的**。

修复方式已经写进两个 `build.rs`（`apps/*/src-tauri/build.rs`）：
在调用 `tauri_build::build()` 之前，为 `bundle.icon` 里的每个图标发
`cargo:rerun-if-changed`。**改 `tauri.conf.json` 的 `bundle.icon` 时，
记得同步改 `build.rs` 里那份 `ICONS` 列表。**

如果怀疑遇到了这个问题，可以这样确认 exe 里究竟嵌了什么：

```powershell
Add-Type -AssemblyName System.Drawing
$ico = [System.Drawing.Icon]::ExtractAssociatedIcon("target\release\clipmesh-desktop.exe")
$ico.ToBitmap().Save("$env:TEMP\embedded.png")
```

（注意 `ExtractAssociatedIcon` 走的是 shell API，会命中图标缓存。
要绕开缓存，先把 exe 复制一份并改成新文件名，再提取。）

确认是缓存问题后强制重来：

```bash
cargo clean -p clipmesh-desktop
```

### 换了图标要重建，不要只看旧的 exe

桌面端的可执行文件生成在仓库根的 `target/release/clipmesh-desktop.exe` —— 也就是工作区的
target 目录，而不是 `apps/desktop/src-tauri/target/`。早先构建留下的 exe 里还是旧图标，
所以要重新构建，别去双击那个陈旧的文件。

---

## 8. 质量门

提交前应当全部通过：

```bash
cargo test --workspace                        # Rust 单元测试
cargo clippy --workspace --all-targets        # 建议
npm run typecheck                             # 前端类型检查
npm run build                                 # 构建两端前端
```

`cargo test --workspace` 会一并编译 `apps/android/src-tauri`，
但**不会**执行 Gradle —— Android 的 Java/Kotlin 侧需要在真机或模拟器上验证，
或用 `cd apps/android/src-tauri/gen/android && ./gradlew :bridge:assembleDebug`。
