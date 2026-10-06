/**
 * 手写的内联 SVG 图标集。不引任何图标包 —— 全部是 24×24 网格上的描边路径，
 * 颜色继承 `currentColor`，所以能直接被任何文字颜色驱动。
 *
 * `filled: true` 的图标用 `fill` 而不是 `stroke` 渲染（例如 Windows 的四格标）。
 */

export type IconName =
  | "dashboard"
  | "devices"
  | "history"
  | "settings"
  | "shield"
  | "link"
  | "send"
  | "copy"
  | "share"
  | "trash"
  | "refresh"
  | "check"
  | "close"
  | "alert"
  | "info"
  | "chevron-right"
  | "chevron-down"
  | "search"
  | "image"
  | "text"
  | "clipboard"
  | "power"
  | "plus"
  | "download"
  | "radar"
  | "bell"
  | "lock"
  | "key"
  | "inbox"
  | "zap"
  | "phone"
  | "monitor"
  | "windows"
  | "android"
  | "macos"
  | "linux"
  | "unknown";

export interface IconDefinition {
  /** `<svg>` 的子节点，直接 v-html 进去（内容是常量，可信）。 */
  body: string;
  /** true = 用 fill 渲染（实心图标），false/省略 = stroke 渲染。 */
  filled?: boolean;
}

export const ICONS: Record<IconName, IconDefinition> = {
  dashboard: {
    body:
      '<rect x="3" y="3" width="7.6" height="7.6" rx="1.8"/>' +
      '<rect x="13.4" y="3" width="7.6" height="7.6" rx="1.8"/>' +
      '<rect x="3" y="13.4" width="7.6" height="7.6" rx="1.8"/>' +
      '<rect x="13.4" y="13.4" width="7.6" height="7.6" rx="1.8"/>',
  },
  devices: {
    body:
      '<rect x="2.4" y="4" width="13" height="10" rx="2.2"/>' +
      '<path d="M6 18.2h5.4"/><path d="M8.9 14v4.2"/>' +
      '<rect x="16.6" y="9" width="5.4" height="11" rx="1.7"/>',
  },
  history: {
    body:
      '<path d="M3.6 11.9a8.4 8.4 0 1 0 2.5-6"/>' +
      '<path d="M3.6 4.6V9h4.4"/>' +
      '<path d="M12 8.2v4.4l3 1.8"/>',
  },
  settings: {
    body:
      '<path d="M3.6 6.4h9.2"/><path d="M18.6 6.4h1.8"/><circle cx="15.6" cy="6.4" r="2.1"/>' +
      '<path d="M3.6 12h3.6"/><path d="M12.6 12h7.8"/><circle cx="9.6" cy="12" r="2.1"/>' +
      '<path d="M3.6 17.6h7.4"/><path d="M16.4 17.6h4"/><circle cx="13.4" cy="17.6" r="2.1"/>',
  },
  shield: {
    body:
      '<path d="M12 2.9l7.4 2.9v5.5c0 4.4-3 8.3-7.4 9.8-4.4-1.5-7.4-5.4-7.4-9.8V5.8z"/>' +
      '<path d="M8.9 12.1l2.2 2.2 4.2-4.6"/>',
  },
  link: {
    body:
      '<path d="M10.4 13.6a3.6 3.6 0 0 0 5.1 0l3-3a3.6 3.6 0 0 0-5.1-5.1l-1.2 1.2"/>' +
      '<path d="M13.6 10.4a3.6 3.6 0 0 0-5.1 0l-3 3a3.6 3.6 0 0 0 5.1 5.1l1.2-1.2"/>',
  },
  send: {
    body:
      '<path d="M20.6 3.4L3.9 9.8a.6.6 0 0 0 .05 1.14l6.5 2.1 2.1 6.5a.6.6 0 0 0 1.14.05z"/>' +
      '<path d="M20.6 3.4L10.45 13.04"/>',
  },
  copy: {
    body:
      '<rect x="8.6" y="8.6" width="11.8" height="11.8" rx="2.4"/>' +
      '<path d="M15.4 5.4a2.4 2.4 0 0 0-2.4-2.4H5.6a2.4 2.4 0 0 0-2.4 2.4v7.4a2.4 2.4 0 0 0 2.4 2.4"/>',
  },
  trash: {
    body:
      '<path d="M4 6.4h16"/>' +
      '<path d="M9.6 6.4V5.1A1.6 1.6 0 0 1 11.2 3.5h1.6a1.6 1.6 0 0 1 1.6 1.6v1.3"/>' +
      '<path d="M6.6 6.4l.78 12.1a2 2 0 0 0 2 1.9h5.24a2 2 0 0 0 2-1.9l.78-12.1"/>' +
      '<path d="M10.4 10.4v6.2M13.6 10.4v6.2"/>',
  },
  /** Android 系统分享面板的经典「三个节点」形状。 */
  share: {
    body:
      '<circle cx="17.8" cy="5.4" r="2.6"/>' +
      '<circle cx="6.2" cy="12" r="2.6"/>' +
      '<circle cx="17.8" cy="18.6" r="2.6"/>' +
      '<path d="M8.5 10.7l7-4.1M8.5 13.3l7 4.1"/>',
  },
  refresh: {
    body:
      '<path d="M4 12a8 8 0 0 1 13.66-5.66L20 8.6"/>' +
      '<path d="M20 3.9v4.7h-4.7"/>' +
      '<path d="M20 12a8 8 0 0 1-13.66 5.66L4 15.4"/>' +
      '<path d="M4 20.1v-4.7h4.7"/>',
  },
  check: { body: '<path d="M4.6 12.6l4.9 4.9L19.4 6.9"/>' },
  close: { body: '<path d="M6.2 6.2l11.6 11.6M17.8 6.2L6.2 17.8"/>' },
  alert: {
    body:
      '<path d="M12 3.6l9.2 16.1H2.8z"/>' +
      '<path d="M12 9.4v4.6"/><path d="M12 17.3h.01"/>',
  },
  info: {
    body:
      '<circle cx="12" cy="12" r="9"/>' +
      '<path d="M12 11.2v5.4"/><path d="M12 7.6h.01"/>',
  },
  "chevron-right": { body: '<path d="M9.4 5.4l6.6 6.6-6.6 6.6"/>' },
  "chevron-down": { body: '<path d="M5.4 9.4l6.6 6.6 6.6-6.6"/>' },
  search: { body: '<circle cx="11" cy="11" r="6.6"/><path d="M15.9 15.9l4.4 4.4"/>' },
  image: {
    body:
      '<rect x="3" y="4.6" width="18" height="14.8" rx="2.6"/>' +
      '<circle cx="8.7" cy="10" r="1.8"/>' +
      '<path d="M3.6 17.1l4.7-4.3a2 2 0 0 1 2.7 0l3.9 3.6"/>' +
      '<path d="M14.6 15.3l1.9-1.8a2 2 0 0 1 2.7 0l1.2 1.1"/>',
  },
  text: {
    body:
      '<path d="M5 6.6V5h14v1.6"/>' +
      '<path d="M12 5v14"/><path d="M9.2 19h5.6"/>',
  },
  clipboard: {
    body:
      '<rect x="5" y="4.6" width="14" height="16" rx="2.6"/>' +
      '<path d="M9 4.6v-.9a1.3 1.3 0 0 1 1.3-1.3h3.4A1.3 1.3 0 0 1 15 3.7v.9z"/>' +
      '<path d="M9 11h6M9 15h4"/>',
  },
  power: { body: '<path d="M12 3.4v8.2"/><path d="M6.9 6.9a7.4 7.4 0 1 0 10.2 0"/>' },
  plus: { body: '<path d="M12 5.4v13.2M5.4 12h13.2"/>' },
  download: {
    body:
      '<path d="M12 3.8v11.4"/><path d="M7.6 10.8L12 15.2l4.4-4.4"/>' +
      '<path d="M4.8 19.6h14.4"/>',
  },
  radar: {
    body:
      '<circle cx="12" cy="12" r="2"/>' +
      '<path d="M8.6 15.4a4.8 4.8 0 0 1 0-6.8"/><path d="M15.4 8.6a4.8 4.8 0 0 1 0 6.8"/>' +
      '<path d="M5.7 18.3a8.9 8.9 0 0 1 0-12.6"/><path d="M18.3 5.7a8.9 8.9 0 0 1 0 12.6"/>',
  },
  bell: {
    body:
      '<path d="M18.2 16.6V11a6.2 6.2 0 1 0-12.4 0v5.6l-1.6 2.2h15.6z"/>' +
      '<path d="M10 21.3a2.2 2.2 0 0 0 4 0"/>',
  },
  lock: {
    body:
      '<rect x="4.6" y="10.4" width="14.8" height="10.2" rx="2.6"/>' +
      '<path d="M8.1 10.4V7.9a3.9 3.9 0 0 1 7.8 0v2.5"/>',
  },
  key: {
    body:
      '<circle cx="8" cy="15.4" r="3.6"/>' +
      '<path d="M10.6 12.8L20.2 3.2"/>' +
      '<path d="M16.6 6.8l2.6 2.6"/><path d="M14 9.4l2.6 2.6"/>',
  },
  inbox: {
    body:
      '<path d="M3.4 13.4h4.2l1.4 2.5h6l1.4-2.5h4.2"/>' +
      '<path d="M5.7 5.2h12.6l2.3 8.2v3.5a2.1 2.1 0 0 1-2.1 2.1H5.5a2.1 2.1 0 0 1-2.1-2.1v-3.5z"/>',
  },
  zap: { body: '<path d="M13.4 2.6L5.2 13.6h6.1l-.7 7.8 8.2-11h-6.1z"/>' },
  phone: {
    body:
      '<rect x="6.6" y="2.4" width="10.8" height="19.2" rx="2.8"/>' +
      '<path d="M10.6 5.4h2.8"/><path d="M11 18.8h2"/>',
  },
  monitor: {
    body:
      '<rect x="2.6" y="3.8" width="18.8" height="12.6" rx="2.4"/>' +
      '<path d="M9 20.2h6"/><path d="M12 16.4v3.8"/>',
  },
  windows: {
    filled: true,
    body:
      '<path d="M3.4 6.1l7.3-1v6.2H3.4z"/>' +
      '<path d="M12.1 4.9l8.5-1.2v7.6h-8.5z"/>' +
      '<path d="M3.4 12.7h7.3v6.2l-7.3-1z"/>' +
      '<path d="M12.1 12.7h8.5v7.6l-8.5-1.2z"/>',
  },
  android: {
    body:
      '<path d="M6.4 9.6h11.2v6.8a1.6 1.6 0 0 1-1.6 1.6H8a1.6 1.6 0 0 1-1.6-1.6z"/>' +
      '<path d="M6.4 9.6a5.6 5.6 0 0 1 11.2 0"/>' +
      '<path d="M7.7 4.4l1.5 2.2M16.3 4.4l-1.5 2.2"/>' +
      '<path d="M9.7 7.1h.01M14.3 7.1h.01"/>' +
      '<path d="M9.4 18v2.4M14.6 18v2.4"/>',
  },
  macos: {
    body:
      '<path d="M9.3 9.3h5.4v5.4H9.3z"/>' +
      '<path d="M9.3 9.3H7.7a2.4 2.4 0 1 1 2.4-2.4z"/>' +
      '<path d="M14.7 9.3h1.6a2.4 2.4 0 1 0-2.4-2.4z"/>' +
      '<path d="M9.3 14.7H7.7a2.4 2.4 0 1 0 2.4 2.4z"/>' +
      '<path d="M14.7 14.7h1.6a2.4 2.4 0 1 1-2.4 2.4z"/>',
  },
  linux: {
    body:
      '<rect x="2.8" y="4.4" width="18.4" height="15.2" rx="2.6"/>' +
      '<path d="M7.2 9.4l3 2.8-3 2.8"/><path d="M12.6 15.4h4.2"/>',
  },
  unknown: {
    body:
      '<circle cx="12" cy="12" r="9"/>' +
      '<path d="M9.3 9.5a2.8 2.8 0 1 1 3.8 2.6c-.8.35-1.1.9-1.1 1.7v.3"/>' +
      '<path d="M12 17.3h.01"/>',
  },
};

/** `Platform` -> 图标名。 */
export const PLATFORM_ICONS: Record<string, IconName> = {
  windows: "windows",
  linux: "linux",
  macos: "macos",
  android: "android",
  unknown: "unknown",
};
