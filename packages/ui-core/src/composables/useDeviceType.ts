import { computed, ref, type ComputedRef, type Ref } from "vue";

import { useStatusStore } from "../stores/status";
import type { Platform } from "../types";

export type DeviceType = "desktop" | "mobile";

/** 窄于这个宽度就当作移动端（两个 app 各自静态挂自己的 Layout，这里只作参考）。 */
export const MOBILE_BREAKPOINT = 820;

const width = ref(typeof window === "undefined" ? 1280 : window.innerWidth);
let listening = false;

function ensureListener(): void {
  if (listening || typeof window === "undefined") return;
  listening = true;
  window.addEventListener("resize", () => {
    width.value = window.innerWidth;
  });
}

function sniffPlatform(): Platform {
  if (typeof navigator === "undefined") return "unknown";
  const ua = navigator.userAgent;
  if (/Android/i.test(ua)) return "android";
  if (/iPhone|iPad|iPod/i.test(ua)) return "unknown";
  if (/Macintosh|Mac OS X/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  if (/Linux|X11/i.test(ua)) return "linux";
  return "unknown";
}

export interface DeviceTypeInfo {
  /** 后端上报的平台（拿不到时退回 UA 嗅探）。 */
  platform: ComputedRef<Platform>;
  /** 当前窗口宽度。 */
  viewportWidth: Ref<number>;
  deviceType: ComputedRef<DeviceType>;
  isMobile: ComputedRef<boolean>;
  isTouch: ComputedRef<boolean>;
  /** 是否是后端确认过的 Android 设备（决定要不要显示 Android 专属入口）。 */
  isAndroid: ComputedRef<boolean>;
}

/**
 * 依据 `platform` + 窗口宽度判断设备类型。
 *
 * 注意：两个 app **各自静态挂载自己的 Layout**（桌面挂 DesktopLayout、Android 挂
 * MobileLayout），这里不做运行时二选一，只是给组件一个判断依据。
 */
export function useDeviceType(): DeviceTypeInfo {
  ensureListener();
  const statusStore = useStatusStore();

  const platform = computed<Platform>(() => {
    const reported = statusStore.platform;
    return reported === "unknown" ? sniffPlatform() : reported;
  });

  const isAndroid = computed<boolean>(() => platform.value === "android");

  const deviceType = computed<DeviceType>(() =>
    isAndroid.value || width.value < MOBILE_BREAKPOINT ? "mobile" : "desktop",
  );

  const isTouch = computed<boolean>(
    () =>
      isAndroid.value ||
      (typeof window !== "undefined" && window.matchMedia("(pointer: coarse)").matches),
  );

  return {
    platform,
    viewportWidth: width,
    deviceType,
    isMobile: computed(() => deviceType.value === "mobile"),
    isTouch,
    isAndroid,
  };
}
