import { getCurrentScope, onScopeDispose, ref, type Ref } from "vue";

import { formatRelative } from "../utils/format";

/**
 * 一个**全局共享**的"当前时间"心跳：所有用到相对时间的组件共用同一个
 * 15 秒定时器，而不是每个组件各起一个。
 */
const now = ref(Date.now());
let timer: ReturnType<typeof setInterval> | null = null;
let subscribers = 0;

const TICK_MS = 15_000;

function start(): void {
  if (timer !== null || typeof window === "undefined") return;
  timer = setInterval(() => {
    now.value = Date.now();
  }, TICK_MS);
}

function stop(): void {
  if (timer === null) return;
  clearInterval(timer);
  timer = null;
}

export interface RelativeTime {
  /** 心跳时间戳，模板里用它触发重算。 */
  now: Ref<number>;
  /** 把时间戳格式化成「3 分钟前」。 */
  format: (timestamp: number) => string;
  /** 立刻刷新一次（例如刚收到新条目）。 */
  tick: () => void;
}

/**
 * 「3 分钟前」这类相对时间。组件卸载时自动退订，最后一个订阅者走掉就停表。
 */
export function useRelativeTime(): RelativeTime {
  subscribers += 1;
  start();
  if (getCurrentScope()) {
    onScopeDispose(() => {
      subscribers -= 1;
      if (subscribers <= 0) stop();
    });
  }
  return {
    now,
    format: (timestamp: number): string => formatRelative(timestamp, now.value),
    tick: (): void => {
      now.value = Date.now();
    },
  };
}
