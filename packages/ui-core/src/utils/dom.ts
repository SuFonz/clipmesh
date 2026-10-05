/** 少量浏览器 API 包装，避免在每个组件里重复写 try/catch 与兼容分支。 */

/**
 * 写系统剪贴板。优先用 Clipboard API（需要 https / localhost 或 Tauri 环境），
 * 失败时回退到隐藏 textarea + `document.execCommand("copy")`。
 */
export async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* 落到底下的 fallback */
  }
  try {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.top = "-1000px";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(area);
    return ok;
  } catch {
    return false;
  }
}

/** 触发一次本地下载（用于导出证书）。 */
export function downloadTextFile(filename: string, text: string, mime = "text/plain"): void {
  const blob = new Blob([text], { type: `${mime};charset=utf-8` });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  // 给浏览器一点时间把下载排上队再释放
  setTimeout(() => URL.revokeObjectURL(url), 1_000);
}
