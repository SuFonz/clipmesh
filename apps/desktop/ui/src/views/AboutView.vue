<script setup lang="ts">
import { RouterLink } from "vue-router";

import {
  AppCard,
  AppIcon,
  StatusPill,
  isMock,
  platformLabel,
  shortFingerprint,
  useAppVersion,
  useStatusStore,
} from "@clipmesh/ui-core";

/**
 * 桌面「关于」页。
 *
 * 开头那段与 Android 设置页的「关于」卡片逐条对应：应用名 + 版本、运行时指示、
 * 无中心服务器 / 直连 TLS、当前监听端口；后面按整页展开成本机运行状态与安全模型。
 *
 * 版本号来自 Tauri 包信息（`useAppVersion()`），页面里不写死任何数字 ——
 * 浏览器 mock 下没有 Tauri 宿主，读不到就显示占位符。
 */
const statusStore = useStatusStore();
const version = useAppVersion();

const mock = isMock();
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">关于</h1>
        <p class="cm-page-sub">应用版本、运行环境，以及本机此刻的监听情况。</p>
      </div>
      <div class="cm-page-actions">
        <StatusPill
          :label="statusStore.running ? '运行中' : '已停止'"
          :tone="statusStore.running ? 'ok' : 'idle'"
          :pulse="statusStore.running"
        />
      </div>
    </header>

    <div class="cm-grid">
      <AppCard title="关于" icon="info">
        <div class="about">
          <span class="logo"><AppIcon name="link" :size="20" /></span>
          <div class="about-text">
            <span class="about-name">ClipMesh</span>
            <span class="about-version cm-mono">版本 {{ version ?? "—" }}</span>
          </div>
          <StatusPill
            :label="mock ? '浏览器 MOCK' : 'Tauri 运行时'"
            :tone="mock ? 'warn' : 'ok'"
            size="sm"
          />
        </div>
        <p class="cm-help mt">
          无中心服务器，设备之间直接通过 TLS 通信。当前监听端口
          {{ statusStore.listenPort }}。
        </p>
        <p v-if="mock" class="cm-help mt-sm">
          浏览器 MOCK 运行时没有 Tauri 宿主，读不到打包时的版本号，所以这里显示占位符。
        </p>
        <p class="cm-help mt-sm">
          以 MIT 许可证发布，全文见仓库根目录的 <span class="cm-mono">LICENSE</span>。
        </p>
      </AppCard>

      <AppCard title="运行状态" icon="radar" subtitle="本机身份与监听端口">
        <div class="stats">
          <div class="stat">
            <span class="k">设备名</span>
            <span class="v cm-truncate">{{ statusStore.deviceName }}</span>
          </div>
          <div class="stat">
            <span class="k">平台</span>
            <span class="v">{{ platformLabel(statusStore.platform) }}</span>
          </div>
          <div class="stat">
            <span class="k">引擎</span>
            <span class="v">
              <StatusPill
                :label="statusStore.running ? '运行中' : '已停止'"
                :tone="statusStore.running ? 'ok' : 'idle'"
                size="sm"
              />
            </span>
          </div>
          <div class="stat">
            <span class="k">监听端口</span>
            <span class="v cm-mono">{{ statusStore.listenPort }}</span>
          </div>
          <div class="stat">
            <span class="k">证书指纹</span>
            <span class="v cm-mono">{{ shortFingerprint(statusStore.fingerprint, 3) || "—" }}</span>
          </div>
        </div>
        <p class="cm-help mt">
          端口优先用 47711（netstat 里好认，也方便写防火墙规则），被别的程序占用时自动换成
          系统分配的临时端口，真正算数的是 mDNS 广播出去的那个。设备名和完整证书在<RouterLink
            to="/"
            class="link"
            >首页</RouterLink
          >的「本机身份」里。
        </p>
      </AppCard>

      <AppCard title="安全" icon="shield" subtitle="配对才是信任边界">
        <ul class="facts">
          <li>每台设备持有自己的 Ed25519 密钥与自签证书，通信全程走 TLS 1.3 双向认证加密。</li>
          <li>发现不等于信任：必须在两台设备上对照证书指纹，并手动接受配对。</li>
          <li>未配对的设备只能发配对请求，其余消息一律丢弃。</li>
        </ul>
      </AppCard>
    </div>
  </div>
</template>

<style scoped>
.view {
  display: block;
}

.mt {
  margin-top: 12px;
}

.mt-sm {
  margin-top: 6px;
}

/* ---------- 关于 ---------- */

.about {
  display: flex;
  align-items: center;
  gap: 10px;
}

.logo {
  display: grid;
  place-items: center;
  width: 38px;
  height: 38px;
  flex: none;
  border-radius: var(--radius-sm);
  background: var(--accent);
  color: var(--accent-fg);
  box-shadow: var(--shadow-1);
}

.about-text {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  line-height: 1.3;
}

.about-name {
  font-size: 15px;
  font-weight: 680;
  letter-spacing: -0.01em;
}

.about-version {
  color: var(--text-muted);
  font-size: 12px;
}

/* ---------- 运行状态 ---------- */

.stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 10px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
  padding: 9px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.k {
  color: var(--text-dim);
  font-size: 11.5px;
  font-weight: 600;
}

.v {
  min-width: 0;
  font-size: 12.5px;
}

.link {
  font-size: 12.5px;
  font-weight: 600;
}

/* ---------- 安全 ---------- */

.facts {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  padding-left: 18px;
  color: var(--text-muted);
  font-size: 12.5px;
  line-height: 1.5;
}

.facts li::marker {
  color: var(--accent);
}
</style>
