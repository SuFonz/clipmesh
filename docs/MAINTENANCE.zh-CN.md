# 维护须知

[English](MAINTENANCE.md) | **简体中文**

本文件覆盖**改动这份代码之前**需要知道的东西：`tauri android init` 不会复现的 `gen/android`
接线、Rust ↔ Kotlin 桥、R8 keep 规则、edge-to-edge 留白、数据存放位置，以及怎么换应用图标。

构建与运行见 [`README.md`](../README.zh-CN.md)。

---

## 1. 原生插件接线（重要）

Kotlin 插件在 `apps/android/plugins/bridge/`，是**独立的 Gradle library 模块**，
不在 `gen/android` 里 —— 这样重新执行 `tauri android init` 不会覆盖它。

除 README 里的构建步骤之外，`gen/android` 里有**四处**需要手工维护，都**已经写入仓库**，此处记录是为了说明为什么：

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
`RECEIVE_BOOT_COMPLETED`、`CHANGE_WIFI_MULTICAST_STATE`，以及截图同步用的
`READ_MEDIA_IMAGES` / `READ_MEDIA_VISUAL_USER_SELECTED` / `READ_EXTERNAL_STORAGE`）
以及 Service / Receiver / Activity / FileProvider 声明合并进应用。

**③ 同一个文件 —— release 签名**

`signingConfigs` 块，以及 `buildTypes.release` 里的 `signingConfig = …` 那一行，见
[`README.zh-CN.md`](../README.zh-CN.md) 的 Android「环境变量」一节。
Tauri 的模板里这两样都没有，所以重新生成的项目打出来的 release 是未签名的，直到把它们补回去。

**④ `apps/android/src-tauri/gen/android/app/src/main/java/app/cm/clipmesh/MainActivity.kt`**

让网页内容避开系统栏。生成出来的项目里没有任何东西做这件事：`enableEdgeToEdge()` 让 webview 铺满整块屏幕
（而 `targetSdk = 35` 之后，就算删掉这行调用，系统也照样强制 edge-to-edge），所以 `onWebViewCreate` 用
`systemBars() | displayCutout()` 给 webview 的父容器（内容帧）加内边距。见 §4 —— 代码很短，但它是底部
标签栏和导航键之间唯一的那道防线。

> 如果重新生成了 `gen/android`（删除后跑 `tauri android init`），
> **上面四处**都需要重新加上。这些是仅有的、需要手工维护的生成文件改动。
> （`apps/android/plugins/bridge/consumer-rules.pro` 故意**不在**这份清单里：它是 R8 keep 规则而不是接线，
> 位于 `gen/android` 之外的插件模块中，通过 `consumerProguardFiles` 生效 —— 见 §3。）
>
> **FileProvider 以前是这里的第五处，现在不是了。** 分享图片需要 `content://` URI，而
> `androidx.core.content.FileProvider` 原先写在 `gen/android/app/src/main/AndroidManifest.xml` 里，
> 路径表在 `gen/android/app/src/main/res/xml/file_paths.xml`。它现在搬进了插件自己的清单
> （`apps/android/plugins/bridge/src/main/AndroidManifest.xml`，authority 同样是
> `${applicationId}.fileprovider`，路径表是 `bridge/src/main/res/xml/clipmesh_file_paths.xml`），
> 于是重新生成 `gen/android` 也不会把它弄丢，那份清单里只留下一段「不要再加回来」的注释 ——
> 同一个 provider 声明两次会被 manifest merger 合并，而 `FILE_PROVIDER_PATHS` 的
> `meta-data` 有两个不同取值时合并会**直接报错**，不是无害的重复。

---

## 2. 前端怎么和 Kotlin 说话

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

**截图同步走的是同一条路**（设置页的开关，默认关）：

```
MediaStore 变化
  ↓  ScreenshotWatcher 的 ContentObserver（进程级、注册在 application context 上）
  ↓  只认截图目录里的新图片（Pictures/Screenshots、DCIM/Screenshots 等）
Kotlin 后台线程读图 → BroadcastHandoff.deposit(Request.Screenshot)
  ↓  Rust broadcast::spawn 轮询 takeBroadcast（响应里 source = "screenshot"）
Rust  engine.send_explicit(image)  → 对端，并写入历史
```

与剪贴板那条的唯一区别是**不写回剪贴板缓存**（`AndroidClipboardProvider::converted`
而不是 `stage`）：截图本来就不在剪贴板上，缓存它会让之后一次后台读取把截图当成
“你刚复制的东西”。

Rust ↔ Kotlin 的方法名一一对应，**改一边必须改另一边**（运行时才报错）：

| Rust (`plugin.rs`) | Kotlin (`ClipMeshPlugin.kt`) |
| --- | --- |
| `readClipboard` | `readClipboard` |
| `setText` | `setText` |
| `setImage` | `setImage` |
| `showReceived` | `showReceived` |
| `showReceivedImage` | `showReceivedImage` |
| `shareImage` | `shareImage` |
| `startService` / `stopService` / `serviceRunning` | 同名 |
| `requestNotificationPermission` | `requestNotificationPermission` |
| `screenshotPermission` | `screenshotPermission` |
| `setScreenshotSync` | `setScreenshotSync` |
| `leaveApp` | `leaveApp` |
| `takeBroadcast` | `takeBroadcast` |

Kotlin 类名与包名在 `apps/android/src-tauri/src/plugin.rs` 的
`ANDROID_PLUGIN_PACKAGE` / `ANDROID_PLUGIN_CLASS` 常量里。

---

## 3. release 构建：R8 与插件的 keep 规则

release 是开了压缩混淆的（`app/build.gradle.kts` 里的 `optimization { enable = true }`），而插件唯一依赖的
反射路径 R8 看不见：`Invoke.parseArgs(SetTextArgs::class.java)` 用 **Jackson** 反序列化命令参数，靠的是运行时
拿到的那个 class。在 keep 规则存在之前，release 会把它改名成 `d20` 并删掉构造函数和 setter，于是**所有带参数的
`@Command`**（`setText`、`setImage`、`showReceived`、`showReceivedImage`、`shareImage`、
`setScreenshotSync`）在运行时全部失败 —— 而不做混淆的 debug 构建一切正常：

```
Cannot construct instance of `d20` (no Creators, like default constructor, exist)
```

规则在 **`apps/android/plugins/bridge/consumer-rules.pro`**，通过插件模块 `defaultConfig` 里的
`consumerProguardFiles("consumer-rules.pro")` 挂上。这是 library 模块声明「使用方不能删掉什么」的惯用位置：
AGP 会把 library 的 consumer 规则合进每一个开启压缩的使用方，本仓库的 R8 配置就能证明这条链路是通的
（`build/outputs/mapping/*/configuration.txt` 里有 "Local project :::tauri-android" 与
"…:::tauri-plugin-opener" 两段），所以不需要往 `gen/android/app/proguard-rules.pro` 里加任何东西 ——
那个文件迟早会被 `tauri android init` 覆盖。

**新增一个带参数的 `@Command`，就要把它的参数类加到那份规则里。** `@Command` 方法本身由 `:tauri-android`
自带的 consumer 规则保住，清单里的组件（Activity / Service / Receiver）由 AGP 的 `aapt_rules.txt` 保住 ——
参数类是唯一漏掉的一环。

---

## 4. edge-to-edge、系统栏与 `env(safe-area-inset-*)`

`targetSdk = 37` 意味着系统强制 Activity 走 edge-to-edge，webview 会铺满整块屏幕，底部导航栏压在它上面。
而 Android **不会**把这个 inset 交给 CSS：WebView 只按「显示挖孔」填充 `env(safe-area-inset-*)`，而且只在它占满
整屏时才会填，所以没有刘海的手机上这些值全是 `0px`。这个留白只能做成布局内边距，位置就是
**④ `MainActivity.kt`**（`onWebViewCreate` 用 `systemBars() | displayCutout()` 给 webview 的父容器加内边距）；
它在哪里接线见 §1。

动任何一侧之前，有两点值得先知道：

- `apps/android/ui` 的 `MobileLayout.vue` **不要**再叠加 `env(safe-area-inset-*)`：在真的会报系统栏的 WebView
  上两者会相加，留白变成两倍。
- 内边距加在**父容器**而不是 webview 上：加在 webview 上，它自己的边界仍然覆盖着系统栏那一条，而状态栏那一条
  不该由应用来画 —— 那个窗口在应用窗口之上，还会吃掉触摸。留给系统栏的空白会退回主题的 DayNight
  `windowBackground`。

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

## 6. 更换应用图标

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
