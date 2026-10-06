/**
 * 桌面窗口的右键菜单。
 *
 * WebView2 默认会在右键时弹出自己的菜单（刷新 / 另存为 / 打印 / 检查 / 后退），
 * 这些对一个剪贴板应用没有意义，所以在窗口层面统一屏蔽：`contextmenu` 的默认行为
 * 就是弹这个菜单，`preventDefault()` 即可取消，不用动 Tauri 的窗口配置。
 *
 * **但不能一刀切**：设备名输入框里右键 → 粘贴、选中一段文字后右键 → 复制，都是
 * 用户会正常期待的系统能力，全砍掉等于把剪贴板操作本身也砍掉。所以放行规则只有
 * 两条，都表示「用户此刻在跟可编辑内容或选区打交道」：
 *   1. 事件目标落在 input / textarea / contenteditable 里 —— 需要原生的粘贴、全选；
 *   2. 页面上存在非空选区 —— 选区可能起于卡片文本，此时事件目标却是外层容器。
 * 其余位置（卡片、列表、状态条、空白处）一律 preventDefault。
 *
 * 键盘上的菜单键和 Shift+F10 触发的是同一个事件，所以一并覆盖。这个文件只属于
 * 桌面 app：Android 的长按菜单是另一套东西，不在这里管。
 */
const EDITABLE_SELECTOR = 'input, textarea, [contenteditable]:not([contenteditable="false"])';

/** 事件目标是否在可编辑控件内部。 */
function inEditable(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(EDITABLE_SELECTOR) !== null;
}

/** 当前是否存在用户选中的文本。 */
function hasSelection(): boolean {
  return (window.getSelection()?.toString() ?? "") !== "";
}

function onContextMenu(event: MouseEvent): void {
  // 可编辑内容或选区：把菜单交回系统，粘贴 / 复制还得靠它。
  if (inEditable(event.target) || hasSelection()) return;
  event.preventDefault();
}

/** 在 `App.vue` 里挂一次。 */
export function useContextMenuGuard(): void {
  if (typeof window === "undefined") return;
  window.addEventListener("contextmenu", onContextMenu);
}
