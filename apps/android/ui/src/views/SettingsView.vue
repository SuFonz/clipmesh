<script setup lang="ts">
import { onMounted, ref } from "vue";

import {
  AppCard,
  AppToggle,
  MAX_IMAGE_BYTES_OPTIONS,
  StatusPill,
  androidApi,
  formatMaxImageBytes,
  isMock,
  toMessage,
  useAppVersion,
  useSettingsStore,
  useStatusStore,
  useToast,
  type SettingsView,
} from "@clipmesh/ui-core";

/**
 * Android 设置页：只保留移动端真正需要的项。
 * 桌面专属（托盘、开机自启）不在这里出现。
 *
 * 「本机名称」和「本机身份」在首页的「本机」区块，清空历史在历史页 —— 这里不再重复。
 */
const settingsStore = useSettingsStore();
const statusStore = useStatusStore();
const toast = useToast();

const serviceAvailable = ref(true);
const serviceBusy = ref(false);

/**
 * 开关的显示值：后台常驻**此刻是否真的生效** = 通知权限已授予 且 前台服务在跑。
 *
 * 只绑通知权限的话，用户手动停掉服务后开关会自己弹回「开」；只绑服务的话，
 * 权限被拒时它还可能显示「开」—— 两种都会让开关说谎。二者本来就该同步
 * （granted ⇒ 服务在跑，denied ⇒ 服务停掉），所以「与」起来读到的就是真相。
 */
const serviceOn = ref(false);

const mock = isMock();

/** 版本号来自 Tauri 包信息（与桌面「关于」页同一个来源），这里不再写死。 */
const version = useAppVersion();

/**
 * 读一次真实状态（通知权限 + 前台服务）。
 *
 * 显示状态只能走 `isNotificationPermissionGranted`（**不弹框**）；
 * `requestNotificationPermission` 会弹系统对话框，只留给用户明确点击时用。
 *
 * 「权限没了但服务还在跑」是正常状态，不是要扳回来的不一致：Android 允许前台
 * 服务在没有 POST_NOTIFICATIONS 时运行，只是常驻通知不显示。用户拒绝通知往往
 * 只是嫌通知烦，不该顺带把后台同步一起关掉。开关显示「关」（它反映的是权限），
 * 同步照常。
 */
async function syncServiceState(): Promise<void> {
  const [granted, running] = await Promise.all([
    androidApi.isNotificationPermissionGranted(),
    androidApi.isServiceRunning(),
  ]);
  serviceOn.value = granted && running;
  // 拒绝权限时不碰服务，见上面的说明。
}

onMounted(async () => {
  try {
    await syncServiceState();
  } catch (cause) {
    serviceAvailable.value = false;
    console.info("[clipmesh] Android 专属命令不可用：", toMessage(cause));
  }
});

async function patch(changes: Partial<SettingsView>): Promise<void> {
  try {
    await settingsStore.update(changes);
  } catch (cause) {
    toast.error("设置未保存", toMessage(cause));
  }
}

function onMaxImageBytes(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  if (Number.isFinite(value)) void patch({ maxImageBytes: value });
}

/**
 * 打开：先申请通知权限。没有它就没有常驻通知，也收不到远程剪贴板提醒，
 * 所以被拒时**不启动服务**，开关自己回到「关」并说明原因。
 * （权限其实已经授予时，这一步不会弹框 —— 只是把上次停掉的服务再拉起来。）
 *
 * 关闭：停掉服务。
 */
async function toggleService(value: boolean): Promise<void> {
  serviceBusy.value = true;
  try {
    if (value) {
      const granted = await androidApi.requestNotificationPermission();
      if (!granted) {
        toast.warn(
          "没有通知权限",
          "后台同步照常，但没有常驻通知，也收不到远程剪贴板的提醒。可以在系统设置里重新允许通知。",
        );
        // 开关必须自己回到「关」—— 它反映的是权限，不是服务在不在跑。
        await syncServiceState();
        return;
      }

      await androidApi.startService();
      await patch({ androidForegroundService: true });
      serviceOn.value = true;
      toast.success("前台服务已启动", "通知栏会显示常驻通知与「广播剪贴板」按钮。");
      return;
    }

    await androidApi.stopService();
    await patch({ androidForegroundService: false });
    serviceOn.value = false;
    toast.success("前台服务已停止");
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
    // 出错后以真实状态为准：开关绝不能停在「开」而服务其实没起来。
    try {
      await syncServiceState();
    } catch {
      serviceOn.value = false;
    }
  } finally {
    serviceBusy.value = false;
  }
}
</script>

<template>
  <div class="view">
    <AppCard title="同步" icon="refresh">
      <AppToggle
        :model-value="settingsStore.settings?.autoSync ?? false"
        label="后台自动同步"
        description="剪贴板变化时自动推送。"
        @update:model-value="(v: boolean) => patch({ autoSync: v })"
      />
      <AppToggle
        :model-value="settingsStore.settings?.syncText ?? false"
        label="同步文本"
        @update:model-value="(v: boolean) => patch({ syncText: v })"
      />
      <AppToggle
        :model-value="settingsStore.settings?.syncImages ?? false"
        label="同步图片"
        description="移动网络下建议关掉，图片可能很大。"
        @update:model-value="(v: boolean) => patch({ syncImages: v })"
      />

      <label class="cm-field mt">
        <span class="cm-label">图片大小上限</span>
        <select
          class="cm-select"
          :value="settingsStore.settings?.maxImageBytes ?? 0"
          @change="onMaxImageBytes"
        >
          <option v-for="option in MAX_IMAGE_BYTES_OPTIONS" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>
        <span class="cm-help">
          当前上限 {{ formatMaxImageBytes(settingsStore.settings?.maxImageBytes ?? 0) }}，超过的图片会被跳过。
        </span>
      </label>
    </AppCard>

    <AppCard v-if="serviceAvailable" title="后台常驻" icon="bell" subtitle="前台服务 + 通知">
      <AppToggle
        :model-value="serviceOn"
        :disabled="serviceBusy"
        label="常驻前台服务"
        description="保持后台运行，通知栏会显示常驻通知与「广播剪贴板」按钮。"
        @update:model-value="toggleService"
      />
      <p class="cm-help mt-sm">
        这个开关跟着通知权限走：权限被拒绝时它是关的，前台服务也会停掉 —— 没有通知权限就没有常驻通知，
        也收不到远程剪贴板提醒。打开时会申请一次权限（系统里已经允许过就不再弹框）；
        被拒绝的话再点一次可以重新申请。
      </p>
    </AppCard>

    <AppCard title="关于" icon="info">
      <div class="about">
        <span>ClipMesh {{ version ?? "—" }}</span>
        <StatusPill
          :label="mock ? '浏览器 MOCK' : 'Tauri 运行时'"
          :tone="mock ? 'warn' : 'ok'"
          size="sm"
        />
      </div>
      <p class="cm-help mt-sm">
        无中心服务器，设备之间直接通过 TLS 通信。当前监听端口
        {{ statusStore.listenPort }}。
      </p>
      <p class="cm-help mt-sm">
        以 MIT 许可证发布，全文见仓库根目录的 <span class="cm-mono">LICENSE</span>。
      </p>
    </AppCard>
  </div>
</template>

<style scoped>
.view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.mt {
  margin-top: 12px;
}

.mt-sm {
  margin-top: 8px;
}

.about {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 13.5px;
  font-weight: 600;
}
</style>
