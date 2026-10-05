import { ref } from "vue";

export type ToastTone = "info" | "success" | "warn" | "error";

export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  description: string;
}

/**
 * 轻量 toast 队列。故意**不做成 Pinia store**：它没有后端对应物，纯 UI 状态，
 * 而且要能从任何地方（包括 store 之外的工具函数）直接 push。
 *
 * 由 `<ToastHost />` 渲染。
 */
const toasts = ref<Toast[]>([]);
let seq = 0;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

const DEFAULT_TIMEOUT = 4_200;

function dismiss(id: number): void {
  const timer = timers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    timers.delete(id);
  }
  toasts.value = toasts.value.filter((t) => t.id !== id);
}

function push(tone: ToastTone, title: string, description = "", timeout = DEFAULT_TIMEOUT): number {
  seq += 1;
  const id = seq;
  // 同样的内容重复弹没有意义（例如定时事件连发）
  const duplicate = toasts.value.find((t) => t.title === title && t.description === description);
  if (duplicate) dismiss(duplicate.id);
  toasts.value = [...toasts.value, { id, tone, title, description }].slice(-4);
  if (timeout > 0) {
    timers.set(
      id,
      setTimeout(() => dismiss(id), timeout),
    );
  }
  return id;
}

export function useToast() {
  return {
    toasts,
    push,
    dismiss,
    clear: (): void => {
      for (const id of [...timers.keys()]) dismiss(id);
      toasts.value = [];
    },
    info: (title: string, description = ""): number => push("info", title, description),
    success: (title: string, description = ""): number => push("success", title, description),
    warn: (title: string, description = ""): number => push("warn", title, description),
    error: (title: string, description = ""): number => push("error", title, description, 6_000),
  };
}
