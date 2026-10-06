/**
 * ============================================================================
 *  DEV-ONLY MOCK —— 仅在**没有 Tauri 运行时**时启用（浏览器里 `npm run dev`）。
 * ============================================================================
 *
 * 职责：给 UI 一个可交互的假后端，使 `packages/ui-core` + 两个 app 可以在纯浏览器
 * 里完整演示，不需要编译 Rust。
 *
 * 启用条件（见 `transport.ts`）：`!("__TAURI_INTERNALS__" in window)`。
 * 也就是说，只要跑在真正的 Tauri 窗口里，这个文件里的任何代码都不会被执行；
 * 它只会在浏览器里被动态使用，不会污染真实后端行为。
 *
 * 它实现了 `docs/IPC.md` 里的**全部**命令与事件，并且会：
 *   - 返回合理的样例数据（3 台设备 / 2 台已信任 / 1 个待处理配对请求 / 6 条历史含图片）
 *   - 对动作做真实的状态变更（配对、信任、发剪贴板、清历史……）
 *   - 用定时器伪造 `clipmesh://` 事件，让界面肉眼可见地动起来
 *
 * 生产包里这段代码不会被打进主 chunk（transport 用动态 import 隔离不了它，
 * 但 vite 会 tree-shake 掉未被调用的分支；真正在乎体积时可以把整个文件删掉，
 * 只保留 `mockInvoke` 的签名）。
 */

import type {
  Args,
  ClipboardItemView,
  CommandName,
  EventName,
  EventPayload,
  IdentityView,
  PairingPrompt,
  PeerView,
  Platform,
  Result,
  SendResult,
  SettingsView,
  StatusView,
  TrustedDeviceView,
  UnlistenFn,
} from "../types";

/* ------------------------------------------------------------------ *
 * 配置
 * ------------------------------------------------------------------ */

export interface MockConfig {
  /** 伪造的本机平台。Android UI 传 "android" 来打开 Android 专属命令。 */
  platform: Platform;
  /** 每次 `invoke` 的假延迟（毫秒），让 loading 态可见。 */
  latencyMs: number;
  /** 是否启动伪造事件定时器。 */
  events: boolean;
}

const config: MockConfig = {
  platform: "windows",
  latencyMs: 80,
  events: true,
};

function defaultDeviceName(platform: Platform): string {
  return platform === "android" ? "Pixel 8 Pro" : "CLIPMESH-WORKSTATION";
}

/**
 * 覆盖 mock 行为。在 Tauri 里调用它是无害的空操作（mock 永远不会被触发）。
 *
 * 注意：模块加载时状态已经按默认平台建好了，所以切平台要连带把
 * status / identity / settings 里跟平台相关的字段一起刷新
 * （Android UI 会在 `main.ts` 里 `configureMock({ platform: "android" })`）。
 */
export function configureMock(patch: Partial<MockConfig>): void {
  const platformChanged = patch.platform !== undefined && patch.platform !== config.platform;
  Object.assign(config, patch);
  if (!platformChanged) return;
  const isAndroid = config.platform === "android";
  const name = defaultDeviceName(config.platform);
  state.status = { ...state.status, platform: config.platform, deviceName: name };
  state.identity = { ...state.identity, platform: config.platform, deviceName: name };
  state.settings = {
    ...state.settings,
    deviceName: name,
    androidForegroundService: isAndroid,
  };
  state.androidServiceRunning = isAndroid;
  state.notificationPermission = isAndroid;
}

/* ------------------------------------------------------------------ *
 * 样例数据
 * ------------------------------------------------------------------ */

const SELF_ID = "0b7c4d51-6a2f-4f7e-9c1d-2f5b8a3e6d10";
const WIN_ID = "8f2a41c9-3d7b-4e05-a1f6-77c9d0b2e341";
const PIXEL_ID = "d4e6b180-9f3c-42a7-8b51-6e0d7c2a9f88";
const MAC_ID = "3c9d5e27-1b64-4a8f-90de-5a7b1c3f2e64";
const THINKPAD_ID = "7a1f8b30-2c95-4d6e-b7a4-0e3f9c8d5b21";

const FINGERPRINTS: Record<string, string> = {
  [SELF_ID]: "4C8A 91D2 6F30 B7E5 1A9C 3D72 8E45 06FB",
  [WIN_ID]: "A1B2 C3D4 E5F6 0718 293A 4B5C 6D7E 8F90",
  [PIXEL_ID]: "9F3E 7C21 58BD A064 3E19 D8F2 7B45 C1A3",
  [MAC_ID]: "2D7B 40E9 C153 8A6F B2D4 91E7 053C 7A8B",
  [THINKPAD_ID]: "6E0A 3F92 B745 C18D 20A6 5D3E 9C71 4B08",
};

const SAMPLE_PUBLIC_KEY =
  "302a300506032b6570032100" +
  "9d4e7c1b8a3f2506d1e9b47c0a8f3d62" +
  "5c1a7e94b0d38f26a4c7e15b9d028f3a";

const SAMPLE_CERT_PEM = [
  "-----BEGIN CERTIFICATE-----",
  "MIIB4TCCAYegAwIBAgIUQ2xpcE1lc2hEZXZpY2VJZGVudGl0eTEwCgYIKoZIzj0E",
  "AwIwHjEcMBoGA1UEAwwTQ2xpcE1lc2ggRGV2aWNlIENBMB4XDTI1MDEwMTAwMDAw",
  "NloXDTM1MDEwMTAwMDAwNlowHjEcMBoGA1UEAwwTQ2xpcE1lc2ggRGV2aWNlIElE",
  "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEq7Vv0m5nJmT8yGkP2xWc1r4bF9dQ",
  "3sL6hK0pYvN8aXe2ZmC5tRj7UoB1wSgH4iE9fA2nD6kM3pQ8vZxY0jR2TaNTBQ",
  "-----END CERTIFICATE-----",
].join("\n");

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;

/** 所有时间戳都相对这个基准，保证样例数据永远「刚刚发生过」。 */
const BASE = Date.now();
const at = (offsetMs: number): number => BASE - offsetMs;

interface MockState {
  status: StatusView;
  identity: IdentityView;
  settings: SettingsView;
  peers: PeerView[];
  trusted: TrustedDeviceView[];
  prompts: PairingPrompt[];
  history: ClipboardItemView[];
  /** 伪系统剪贴板，只用于 mock 内部的一致性。 */
  clipboard: string;
  androidServiceRunning: boolean;
  notificationPermission: boolean;
  /** 递增的假 UUID 计数器。 */
  seq: number;
}

function samplePeers(): PeerView[] {
  return [
    {
      deviceId: WIN_ID,
      name: "DESKTOP-7F3A",
      platform: "windows",
      address: "192.168.1.24:47711",
      fingerprint: FINGERPRINTS[WIN_ID],
      trusted: true,
      connected: true,
      pairing: false,
      lastSeen: at(2_000),
    },
    {
      deviceId: PIXEL_ID,
      name: "Pixel 8",
      platform: "android",
      address: "192.168.1.31:47711",
      fingerprint: FINGERPRINTS[PIXEL_ID],
      trusted: true,
      connected: true,
      pairing: false,
      lastSeen: at(5_000),
    },
    {
      deviceId: MAC_ID,
      name: "MacBook Pro 14",
      platform: "macos",
      address: "192.168.1.42:47711",
      fingerprint: FINGERPRINTS[MAC_ID],
      trusted: false,
      connected: false,
      pairing: true,
      lastSeen: at(1_200),
    },
  ];
}

function sampleTrusted(): TrustedDeviceView[] {
  return [
    {
      deviceId: WIN_ID,
      name: "DESKTOP-7F3A",
      platform: "windows",
      fingerprint: FINGERPRINTS[WIN_ID],
      trustedAt: at(42 * 24 * HOUR),
      online: true,
    },
    {
      deviceId: PIXEL_ID,
      name: "Pixel 8",
      platform: "android",
      fingerprint: FINGERPRINTS[PIXEL_ID],
      trustedAt: at(9 * 24 * HOUR),
      online: true,
    },
  ];
}

function samplePrompts(): PairingPrompt[] {
  return [
    {
      deviceId: MAC_ID,
      name: "MacBook Pro 14",
      platform: "macos",
      fingerprint: FINGERPRINTS[MAC_ID],
      address: "192.168.1.42:47711",
      requestedAt: at(35_000),
      direction: "incoming",
    },
  ];
}

function sampleHistory(): ClipboardItemView[] {
  return [
    {
      kind: "text",
      id: "h-6f21",
      sourceDevice: PIXEL_ID,
      timestamp: at(3 * MINUTE),
      content:
        "会议室改成 B-207 了，评审 14:30 开始。线上同步：https://meet.example.com/clipmesh-sync",
    },
    {
      kind: "image",
      id: "h-5a90",
      sourceDevice: PIXEL_ID,
      timestamp: at(9 * MINUTE),
      mime: "image/png",
      size: 284_913,
      width: 1280,
      height: 720,
    },
    {
      kind: "text",
      id: "h-4c37",
      sourceDevice: WIN_ID,
      timestamp: at(24 * MINUTE),
      content: "cargo tauri android dev --device emulator-5554",
    },
    {
      kind: "text",
      id: "h-3b12",
      sourceDevice: SELF_ID,
      timestamp: at(58 * MINUTE),
      content: 'ssh-keygen -t ed25519 -C "clipmesh@workstation" -f ~/.ssh/clipmesh_ed25519',
    },
    {
      kind: "image",
      id: "h-2e48",
      sourceDevice: WIN_ID,
      timestamp: at(3 * HOUR),
      mime: "image/png",
      size: 1_048_576,
      width: 1920,
      height: 1080,
    },
    {
      kind: "text",
      id: "h-1904",
      sourceDevice: MAC_ID,
      timestamp: at(5 * HOUR),
      content:
        "https://github.com/clipmesh/clipmesh/pull/42#discussion_r1823410 —— TLS 会话复用那段建议再看一下。",
    },
  ];
}

function initialState(): MockState {
  const platform = config.platform;
  return {
    status: {
      running: true,
      autoSync: true,
      deviceId: SELF_ID,
      deviceName: defaultDeviceName(platform),
      platform,
      fingerprint: FINGERPRINTS[SELF_ID],
      listenPort: 47711,
      connectedPeers: 2,
      trustedPeers: 2,
      lastError: null,
    },
    identity: {
      deviceId: SELF_ID,
      deviceName: defaultDeviceName(platform),
      platform,
      fingerprint: FINGERPRINTS[SELF_ID],
      publicKey: SAMPLE_PUBLIC_KEY,
      certificatePem: SAMPLE_CERT_PEM,
    },
    settings: {
      deviceName: defaultDeviceName(platform),
      autoSync: true,
      syncText: true,
      syncImages: true,
      maxImageBytes: 8 * 1024 * 1024,
      startMinimized: false,
      launchAtLogin: true,
      androidForegroundService: platform === "android",
    },
    peers: samplePeers(),
    trusted: sampleTrusted(),
    prompts: samplePrompts(),
    history: sampleHistory(),
    clipboard: "",
    androidServiceRunning: platform === "android",
    notificationPermission: platform === "android",
    seq: 0,
  };
}

let state: MockState = initialState();

/* ------------------------------------------------------------------ *
 * 事件总线
 * ------------------------------------------------------------------ */

type Handler = (payload: unknown) => void;

const handlers = new Map<EventName, Set<Handler>>();

/** 只给 mock 内部用的广播；负载类型由 `EventMap` 约束。 */
function emit<K extends EventName>(name: K, payload: EventPayload<K>): void {
  const set = handlers.get(name);
  if (!set) return;
  for (const handler of set) {
    try {
      handler(payload);
    } catch {
      /* 一个监听者出错不应该影响其他监听者 */
    }
  }
}

/** 供 `transport.ts` 订阅伪造事件。 */
export function mockListen<K extends EventName>(
  name: K,
  handler: (payload: EventPayload<K>) => void,
): Promise<UnlistenFn> {
  let set = handlers.get(name);
  if (!set) {
    set = new Set<Handler>();
    handlers.set(name, set);
  }
  const wrapped: Handler = (payload) => handler(payload as EventPayload<K>);
  set.add(wrapped);
  startJobs();
  return Promise.resolve(() => {
    set.delete(wrapped);
  });
}

/* ------------------------------------------------------------------ *
 * 定时伪造事件：让 UI 肉眼可见地变化
 * ------------------------------------------------------------------ */

interface Job {
  everyMs: number;
  nextAt: number;
  run: () => void;
}

const jobs: Job[] = [];
const timeouts = new Set<ReturnType<typeof setTimeout>>();
let ticker: ReturnType<typeof setInterval> | null = null;

function later(fn: () => void, ms: number): void {
  const id = setTimeout(() => {
    timeouts.delete(id);
    fn();
  }, ms);
  timeouts.add(id);
}

function job(everyMs: number, run: () => void, delayMs = everyMs): void {
  jobs.push({ everyMs, nextAt: Date.now() + delayMs, run });
}

function startJobs(): void {
  if (ticker !== null || !config.events || jobs.length === 0) return;
  if (typeof window === "undefined") return;
  ticker = setInterval(() => {
    const now = Date.now();
    for (const j of jobs) {
      if (now >= j.nextAt) {
        j.nextAt = now + j.everyMs;
        try {
          j.run();
        } catch {
          /* mock 出错不应该打断 UI */
        }
      }
    }
  }, 1_000);
}

/** 停止定时器（HMR / 卸载时调用）。 */
export function stopMock(): void {
  if (ticker !== null) {
    clearInterval(ticker);
    ticker = null;
  }
  for (const id of timeouts) clearTimeout(id);
  timeouts.clear();
}

function refreshCounts(): void {
  state.status.connectedPeers = state.peers.filter((p) => p.connected).length;
  state.status.trustedPeers = state.trusted.length;
  state.status.autoSync = state.settings.autoSync;
}

function broadcastStatus(): void {
  refreshCounts();
  emit("clipmesh://status", { ...state.status });
}

function broadcastPeers(): void {
  emit("clipmesh://peers", state.peers.map((p) => ({ ...p })));
}

function broadcastTrusted(): void {
  emit("clipmesh://trusted", state.trusted.map((d) => ({ ...d })));
}

function broadcastPrompts(): void {
  emit("clipmesh://pairing-requests", state.prompts.map((p) => ({ ...p })));
}

function broadcastHistory(): void {
  emit("clipmesh://history", state.history.map((i) => ({ ...i })));
}

function nextId(prefix: string): string {
  state.seq += 1;
  return `${prefix}-${state.seq.toString(36)}${Math.floor(Math.random() * 4096).toString(16)}`;
}

function pushHistory(item: ClipboardItemView): void {
  state.history = [item, ...state.history].slice(0, 50);
  broadcastHistory();
}

function deliveredCount(): number {
  return state.trusted.filter((d) => d.online).length;
}

const CLIPBOARD_SAMPLES = [
  "https://clipmesh.dev/docs/pairing#fingerprint",
  "cargo test --workspace --all-features",
  "记得把指纹念给对方核对：4C8A 91D2 6F30 …",
  'git commit -m "fix(net): 重新握手时复用会话缓存"',
  "Phase 5 完成：mDNS 发现已经在局域网里联调通过。",
];

const REMOTE_SAMPLES: Array<{ from: string; content: string }> = [
  { from: PIXEL_ID, content: "刚拍的白板照片已发送，记得查收 📸" },
  { from: WIN_ID, content: "npm --prefix apps/desktop/ui run dev —— 浏览器里就能看 UI" },
  { from: PIXEL_ID, content: "地铁上想到的：历史条目应该支持置顶。" },
  { from: WIN_ID, content: "SELECT * FROM clipboard_items ORDER BY timestamp DESC LIMIT 50;" },
];

function registerJobs(): void {
  // 在线状态/心跳抖动
  job(6_000, () => {
    const now = Date.now();
    for (const peer of state.peers) {
      if (peer.connected) peer.lastSeen = now - Math.floor(Math.random() * 4_000);
    }
    const flip = state.peers.find((p) => p.trusted);
    if (flip && Math.random() < 0.25) {
      flip.connected = !flip.connected;
      if (!flip.connected) flip.lastSeen = now;
      const trusted = state.trusted.find((d) => d.deviceId === flip.deviceId);
      if (trusted) trusted.online = flip.connected;
      broadcastTrusted();
    }
    broadcastPeers();
    broadcastStatus();
  });

  // 收到远端剪贴板
  job(21_000, () => {
    const online = state.trusted.filter((d) => d.online);
    if (online.length === 0) return;
    const sample = REMOTE_SAMPLES[Math.floor(Math.random() * REMOTE_SAMPLES.length)];
    if (!sample || !online.some((d) => d.deviceId === sample.from)) return;
    const item: ClipboardItemView = {
      kind: "text",
      id: nextId("h"),
      sourceDevice: sample.from,
      timestamp: Date.now(),
      content: sample.content,
    };
    pushHistory(item);
    emit("clipmesh://clipboard-received", { ...item });
  }, 34_000);

  // 非致命错误提示
  job(64_000, () => {
    const messages = [
      "与 Pixel 8 的连接中断，正在重连…",
      "图片超过 8 MiB 上限，已跳过 1 个条目。",
      "mDNS 广播失败一次，已自动重试。",
    ];
    const message = messages[Math.floor(Math.random() * messages.length)];
    emit("clipmesh://error", message);
  }, 47_000);

  // 新设备出现并发起配对
  job(120_000, () => {
    if (state.peers.some((p) => p.deviceId === THINKPAD_ID)) {
      state.peers = state.peers.filter((p) => p.deviceId !== THINKPAD_ID);
      state.prompts = state.prompts.filter((p) => p.deviceId !== THINKPAD_ID);
      broadcastPeers();
      broadcastPrompts();
      broadcastStatus();
      return;
    }
    state.peers = [
      ...state.peers,
      {
        deviceId: THINKPAD_ID,
        name: "ThinkPad X1",
        platform: "linux",
        address: "192.168.1.57:47711",
        fingerprint: FINGERPRINTS[THINKPAD_ID],
        trusted: false,
        connected: false,
        pairing: false,
        lastSeen: Date.now(),
      },
    ];
    broadcastPeers();
    broadcastStatus();
    later(() => {
      if (!state.peers.some((p) => p.deviceId === THINKPAD_ID)) return;
      const peer = state.peers.find((p) => p.deviceId === THINKPAD_ID);
      if (peer) peer.pairing = true;
      state.prompts = [
        ...state.prompts,
        {
          deviceId: THINKPAD_ID,
          name: "ThinkPad X1",
          platform: "linux",
          fingerprint: FINGERPRINTS[THINKPAD_ID],
          address: "192.168.1.57:47711",
          requestedAt: Date.now(),
          direction: "incoming",
        },
      ];
      broadcastPeers();
      broadcastPrompts();
    }, 6_000);
  }, 38_000);
}

registerJobs();

/* ------------------------------------------------------------------ *
 * 缩略图：用 canvas 现场画一张，避免仓库里塞二进制资源
 * ------------------------------------------------------------------ */

const FALLBACK_THUMB =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180">' +
      '<rect width="320" height="180" fill="#1a212c"/>' +
      '<text x="160" y="96" fill="#6b7a90" font-family="monospace" font-size="14" text-anchor="middle">no preview</text>' +
      "</svg>",
  );

function hash(seed: string): number {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i += 1) {
    h ^= seed.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return Math.abs(h);
}

const thumbCache = new Map<string, string>();

function drawThumbnail(seed: string, width: number, height: number): string {
  const cached = thumbCache.get(seed);
  if (cached) return cached;
  try {
    const w = Math.max(96, Math.min(width, 480));
    const h = Math.max(64, Math.min(Math.round((height / width) * w) || 160, 320));
    const canvas = document.createElement("canvas");
    canvas.width = w;
    canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return FALLBACK_THUMB;
    const hue = hash(seed) % 360;
    const gradient = ctx.createLinearGradient(0, 0, w, h);
    gradient.addColorStop(0, `hsl(${hue} 62% 34%)`);
    gradient.addColorStop(1, `hsl(${(hue + 48) % 360} 58% 16%)`);
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, w, h);
    // 几条"窗口"似的装饰线，让缩略图看起来像截图
    ctx.fillStyle = "rgba(255,255,255,.14)";
    ctx.fillRect(0, 0, w, Math.max(12, h * 0.11));
    ctx.fillStyle = "rgba(255,255,255,.22)";
    for (let i = 0; i < 3; i += 1) {
      ctx.beginPath();
      ctx.arc(12 + i * 14, Math.max(6, h * 0.055), 3.5, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.fillStyle = "rgba(255,255,255,.10)";
    const rows = 4;
    for (let i = 0; i < rows; i += 1) {
      const y = h * 0.28 + i * (h * 0.16);
      const lineW = w * (0.32 + ((hash(seed + i) % 50) / 100));
      ctx.fillRect(w * 0.08, y, lineW, Math.max(4, h * 0.05));
    }
    const url = canvas.toDataURL("image/png");
    thumbCache.set(seed, url);
    return url;
  } catch {
    return FALLBACK_THUMB;
  }
}

/* ------------------------------------------------------------------ *
 * 参数解析：mock 运行在浏览器里，invoke 的参数是 unknown，需要自己收窄
 * ------------------------------------------------------------------ */

function record(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
}

function str(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function patchOf(value: unknown): Partial<SettingsView> {
  const raw = record(value);
  const patch: Partial<SettingsView> = {};
  if (typeof raw.deviceName === "string") patch.deviceName = raw.deviceName;
  if (typeof raw.autoSync === "boolean") patch.autoSync = raw.autoSync;
  if (typeof raw.syncText === "boolean") patch.syncText = raw.syncText;
  if (typeof raw.syncImages === "boolean") patch.syncImages = raw.syncImages;
  if (typeof raw.maxImageBytes === "number") patch.maxImageBytes = raw.maxImageBytes;
  if (typeof raw.startMinimized === "boolean") patch.startMinimized = raw.startMinimized;
  if (typeof raw.launchAtLogin === "boolean") patch.launchAtLogin = raw.launchAtLogin;
  if (typeof raw.androidForegroundService === "boolean") {
    patch.androidForegroundService = raw.androidForegroundService;
  }
  return patch;
}

const ANDROID_ONLY = "android commands are only available on Android";

function requireAndroid(): void {
  if (config.platform !== "android") throw new Error(ANDROID_ONLY);
}

/* ------------------------------------------------------------------ *
 * 命令分发
 * ------------------------------------------------------------------ */

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function send(item: ClipboardItemView): SendResult {
  pushHistory(item);
  const delivered = deliveredCount();
  emit("clipmesh://clipboard-sent", { item: { ...item }, delivered });
  broadcastStatus();
  return { id: item.id, delivered };
}

async function dispatch(name: CommandName, args: unknown): Promise<unknown> {
  const a = record(args);
  switch (name) {
    /* --- 状态与列表 --- */
    case "get_status":
      refreshCounts();
      return { ...state.status };
    case "list_peers":
      return state.peers.map((p) => ({ ...p }));
    case "list_trusted_devices":
      return state.trusted.map((d) => ({ ...d }));
    case "list_pairing_requests":
      return state.prompts.map((p) => ({ ...p }));
    case "get_history":
      return state.history.map((i) => ({ ...i }));
    case "get_settings":
      return { ...state.settings };
    case "get_identity":
      return { ...state.identity };

    /* --- 生命周期 --- */
    case "start_engine":
      state.status.running = true;
      state.status.lastError = null;
      broadcastStatus();
      broadcastPeers();
      return { ...state.status };
    case "stop_engine":
      state.status.running = false;
      for (const peer of state.peers) peer.connected = false;
      for (const device of state.trusted) device.online = false;
      broadcastStatus();
      broadcastPeers();
      broadcastTrusted();
      return { ...state.status };
    case "clear_error":
      state.status.lastError = null;
      broadcastStatus();
      return { ...state.status };
    case "update_settings": {
      const patch = patchOf(a.patch);
      state.settings = { ...state.settings, ...patch };
      if (patch.deviceName !== undefined && patch.deviceName.trim() !== "") {
        state.identity = { ...state.identity, deviceName: patch.deviceName };
        state.status.deviceName = patch.deviceName;
      }
      state.status.autoSync = state.settings.autoSync;
      broadcastStatus();
      return { ...state.settings };
    }
    case "set_device_name": {
      const next = str(a.name).trim() || state.settings.deviceName;
      state.settings = { ...state.settings, deviceName: next };
      state.identity = { ...state.identity, deviceName: next };
      state.status = { ...state.status, deviceName: next };
      broadcastStatus();
      return { ...state.identity };
    }

    /* --- 配对与信任 --- */
    case "request_pairing": {
      const deviceId = str(a.deviceId);
      const peer = state.peers.find((p) => p.deviceId === deviceId);
      if (!peer) throw new Error(`unknown device: ${deviceId}`);
      if (peer.trusted) throw new Error(`${peer.name} 已经在信任列表里了`);
      peer.pairing = true;
      broadcastPeers();
      // 假的后端会在一小会儿之后"被对方接受"
      later(() => {
        const target = state.peers.find((p) => p.deviceId === deviceId);
        if (!target || !target.pairing) return;
        target.pairing = false;
        target.trusted = true;
        target.connected = true;
        target.lastSeen = Date.now();
        state.trusted = [
          ...state.trusted,
          {
            deviceId: target.deviceId,
            name: target.name,
            platform: target.platform,
            fingerprint: target.fingerprint,
            trustedAt: Date.now(),
            online: true,
          },
        ];
        state.prompts = state.prompts.filter((p) => p.deviceId !== deviceId);
        broadcastPeers();
        broadcastTrusted();
        broadcastPrompts();
        broadcastStatus();
      }, 2_600);
      return undefined;
    }
    case "respond_pairing": {
      const deviceId = str(a.deviceId);
      const accept = bool(a.accept, false);
      const prompt = state.prompts.find((p) => p.deviceId === deviceId);
      state.prompts = state.prompts.filter((p) => p.deviceId !== deviceId);
      const peer = state.peers.find((p) => p.deviceId === deviceId);
      if (peer) peer.pairing = false;
      if (accept && prompt) {
        if (peer) {
          peer.trusted = true;
          peer.connected = true;
          peer.lastSeen = Date.now();
        }
        if (!state.trusted.some((d) => d.deviceId === deviceId)) {
          state.trusted = [
            ...state.trusted,
            {
              deviceId,
              name: prompt.name,
              platform: prompt.platform,
              fingerprint: prompt.fingerprint,
              trustedAt: Date.now(),
              online: true,
            },
          ];
        }
      }
      broadcastPrompts();
      broadcastPeers();
      broadcastTrusted();
      broadcastStatus();
      return undefined;
    }
    case "unpair_device": {
      const deviceId = str(a.deviceId);
      state.trusted = state.trusted.filter((d) => d.deviceId !== deviceId);
      const peer = state.peers.find((p) => p.deviceId === deviceId);
      if (peer) {
        peer.trusted = false;
        peer.connected = false;
        peer.pairing = false;
      }
      broadcastTrusted();
      broadcastPeers();
      broadcastStatus();
      return undefined;
    }

    /* --- 剪贴板 --- */
    case "send_clipboard": {
      const content = CLIPBOARD_SAMPLES[state.seq % CLIPBOARD_SAMPLES.length] ?? CLIPBOARD_SAMPLES[0];
      state.clipboard = content;
      return send({
        kind: "text",
        id: nextId("h"),
        sourceDevice: state.status.deviceId,
        timestamp: Date.now(),
        content,
      });
    }
    case "send_text": {
      const content = str(a.content);
      if (content.trim() === "") throw new Error("内容不能为空");
      state.clipboard = content;
      return send({
        kind: "text",
        id: nextId("h"),
        sourceDevice: state.status.deviceId,
        timestamp: Date.now(),
        content,
      });
    }
    case "resend_history_item": {
      const item = state.history.find((i) => i.id === str(a.id));
      if (!item) throw new Error(`unknown history item: ${str(a.id)}`);
      const delivered = deliveredCount();
      emit("clipmesh://clipboard-sent", { item: { ...item }, delivered });
      return { id: item.id, delivered };
    }
    case "copy_history_item": {
      const item = state.history.find((i) => i.id === str(a.id));
      if (!item) throw new Error(`unknown history item: ${str(a.id)}`);
      state.clipboard = item.kind === "text" ? item.content : `[image ${item.width}x${item.height}]`;
      return undefined;
    }
    case "clear_history":
      state.history = [];
      broadcastHistory();
      return undefined;
    case "get_image_thumbnail": {
      const item = state.history.find((i) => i.id === str(a.id));
      if (!item || item.kind !== "image") return FALLBACK_THUMB;
      return drawThumbnail(item.id, item.width, item.height);
    }

    /* --- Android 专属 --- */
    case "android_start_service":
      requireAndroid();
      state.androidServiceRunning = true;
      state.settings = { ...state.settings, androidForegroundService: true };
      return undefined;
    case "android_stop_service":
      requireAndroid();
      state.androidServiceRunning = false;
      state.settings = { ...state.settings, androidForegroundService: false };
      return undefined;
    case "android_service_running":
      requireAndroid();
      return state.androidServiceRunning;
    case "android_notification_permission":
      requireAndroid();
      return state.notificationPermission;
    case "android_request_notification_permission":
      requireAndroid();
      await sleep(280);
      state.notificationPermission = true;
      return true;
    case "android_leave_app":
      // 真机上会把应用退回后台 —— 浏览器里没有对应动作，静默成功即可。
      requireAndroid();
      return undefined;
  }
}

/**
 * mock 版的 `invoke`。签名与真实 transport 完全一致，因此调用方无感。
 */
export async function mockInvoke<K extends CommandName>(
  name: K,
  args?: Args<K>,
): Promise<Result<K>> {
  if (config.latencyMs > 0) await sleep(config.latencyMs);
  const result = await dispatch(name, args);
  return result as Result<K>;
}

/** 把 mock 恢复到初始样例数据（HMR / 手动重载用）。 */
export function resetMock(): void {
  stopMock();
  thumbCache.clear();
  state = initialState();
}

/** 当前是否处于 mock 模式由 transport 决定，这里只暴露一个调试入口。 */
export const mockDebug = {
  reset: resetMock,
  stop: stopMock,
  state: (): MockState => state,
};
