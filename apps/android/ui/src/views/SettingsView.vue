<script setup lang="ts">
import { onMounted, ref } from "vue";

import {
  AppCard,
  AppToggle,
  LANGUAGE_OPTIONS,
  MAX_IMAGE_BYTES_OPTIONS,
  StatusPill,
  androidApi,
  formatMaxImageBytes,
  isMock,
  languageLabel,
  normalizeLanguageSetting,
  t,
  toMessage,
  useAppVersion,
  useSettingsStore,
  useStatusStore,
  useToast,
  type SettingsView,
} from "@clipmesh/ui-core";

/**
 * Android 设置页：只保留移动端真正需要的项。
 * 桌面专属（托盘、启动即最小化）不在这里出现。
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

/**
 * 截图同步的开关值：同样反映**此刻是否真的在监听**。
 *
 * 截图同步要读媒体库，所以「设置里打开」和「权限拿到手」缺一不可：权限被系统撤掉
 * 之后，设置文件里仍是 `true`，但观察者根本没注册 —— 那种情况下开关必须显示成关，
 * 否则用户会以为截图在同步而其实什么都没有。
 */
const screenshotOn = ref(false);
const screenshotAvailable = ref(true);
const screenshotBusy = ref(false);
/** Android 14 的「仅选中的照片」：能拿到权限，但看不到新截图。 */
const screenshotPartial = ref(false);

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
    await Promise.all([syncServiceState(), syncScreenshotState()]);
  } catch (cause) {
    serviceAvailable.value = false;
    screenshotAvailable.value = false;
    console.info("[clipmesh] Android 专属命令不可用：", toMessage(cause));
  }
});

async function patch(changes: Partial<SettingsView>): Promise<void> {
  try {
    await settingsStore.update(changes);
  } catch (cause) {
    toast.error(t("settings.saveFailed"), toMessage(cause));
  }
}

function onMaxImageBytes(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  if (Number.isFinite(value)) void patch({ maxImageBytes: value });
}

/** 语言只认三种取值，收窄交给 i18n 层（与 Rust 侧同一条规则）。 */
function onLanguage(event: Event): void {
  const value = normalizeLanguageSetting((event.target as HTMLSelectElement).value);
  void patch({ language: value });
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
          t("settings.foreground.noPermission.title"),
          t("settings.foreground.noPermission.description"),
        );
        // 开关必须自己回到「关」—— 它反映的是权限，不是服务在不在跑。
        await syncServiceState();
        return;
      }

      await androidApi.startService();
      await patch({ androidForegroundService: true });
      serviceOn.value = true;
      toast.success(
        t("settings.foreground.started.title"),
        t("settings.foreground.started.description"),
      );
      return;
    }

    await androidApi.stopService();
    await patch({ androidForegroundService: false });
    serviceOn.value = false;
    toast.success(t("settings.foreground.stopped"));
  } catch (cause) {
    toast.error(t("common.actionFailed"), toMessage(cause));
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

// ---------------------------------------------------------------------------
// 截图同步
// ---------------------------------------------------------------------------

/**
 * 读一次真实状态（媒体权限 + 观察者是否在跑）。
 *
 * **只查询、不弹框、也不启动任何东西**：显示一个开关不该问用户任何问题，申请权限只
 * 发生在用户拨开关的那一刻。开关显示的是 `watching` —— 权限被系统撤掉之后，配置文件
 * 里仍是「开」，但观察者并没有注册，那时开关必须显示成关。
 */
async function syncScreenshotState(): Promise<void> {
  const state = await androidApi.screenshotPermission();
  screenshotPartial.value = state.partial;
  screenshotOn.value = state.watching;
}

/**
 * 拨动开关。
 *
 * 打开：先申请读取照片的权限（已经允许过就不会再弹框），拿到完整权限才注册观察者，
 * 也才把设置写进配置文件 —— 权限被拒时开关留在「关」，并说明原因。
 *
 * 关闭：注销观察者并写回设置。权限不动：那是用户在系统里给的，不归应用收回。
 */
async function toggleScreenshot(value: boolean): Promise<void> {
  screenshotBusy.value = true;
  try {
    if (value) {
      const state = await androidApi.setScreenshotSync(true);
      screenshotPartial.value = state.partial;

      if (!state.watching) {
        toast.warn(
          t(
            state.partial
              ? "settings.screenshot.partial.title"
              : "settings.screenshot.noPermission.title",
          ),
          t(
            state.partial
              ? "settings.screenshot.partial.description"
              : "settings.screenshot.noPermission.description",
          ),
        );
        screenshotOn.value = false;
        return;
      }

      await patch({ androidScreenshotSync: true });
      screenshotOn.value = true;
      toast.success(
        t("settings.screenshot.started.title"),
        t("settings.screenshot.started.description"),
      );
      return;
    }

    await androidApi.setScreenshotSync(false);
    await patch({ androidScreenshotSync: false });
    screenshotOn.value = false;
    toast.success(t("settings.screenshot.stopped"));
  } catch (cause) {
    toast.error(t("common.actionFailed"), toMessage(cause));
    // 出错后以真实状态为准，和上面的常驻开关同一条规矩。
    try {
      const permission = await androidApi.screenshotPermission();
      screenshotPartial.value = permission.partial;
    } catch {
      /* 命令本身都不通了，下面这行已经是最保守的显示 */
    }
    screenshotOn.value = false;
  } finally {
    screenshotBusy.value = false;
  }
}
</script>

<template>
  <div class="view">
    <AppCard
      :title="t('settings.language.label')"
      icon="settings"
      :subtitle="t('settings.language.help')"
    >
      <select
        class="cm-select"
        :value="settingsStore.language"
        :aria-label="t('settings.language.label')"
        @change="onLanguage"
      >
        <option v-for="value in LANGUAGE_OPTIONS" :key="value" :value="value">
          {{ languageLabel(value) }}
        </option>
      </select>
    </AppCard>

    <AppCard :title="t('settings.sync.title')" icon="refresh">
      <AppToggle
        :model-value="settingsStore.settings?.autoSync ?? false"
        :label="t('settings.autoSync.label')"
        :description="t('settings.autoSync.descriptionMobile')"
        @update:model-value="(v: boolean) => patch({ autoSync: v })"
      />
      <AppToggle
        :model-value="settingsStore.settings?.syncText ?? false"
        :label="t('settings.syncText.label')"
        @update:model-value="(v: boolean) => patch({ syncText: v })"
      />
      <AppToggle
        :model-value="settingsStore.settings?.syncImages ?? false"
        :label="t('settings.syncImages.label')"
        :description="t('settings.syncImages.descriptionMobile')"
        @update:model-value="(v: boolean) => patch({ syncImages: v })"
      />

      <label class="cm-field mt">
        <span class="cm-label">{{ t("settings.maxImageBytes.label") }}</span>
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
          {{
            t("settings.maxImageBytes.helpCurrent", {
              size: formatMaxImageBytes(settingsStore.settings?.maxImageBytes ?? 0),
            })
          }}
        </span>
      </label>
    </AppCard>

    <AppCard
      v-if="serviceAvailable"
      :title="t('settings.foreground.title')"
      icon="bell"
      :subtitle="t('settings.foreground.subtitle')"
    >
      <AppToggle
        :model-value="serviceOn"
        :disabled="serviceBusy"
        :label="t('settings.foreground.label')"
        :description="t('settings.foreground.description')"
        @update:model-value="toggleService"
      />
      <p class="cm-help mt-sm">
        {{ t("settings.foreground.help") }}
      </p>
    </AppCard>

    <AppCard
      v-if="screenshotAvailable"
      :title="t('settings.screenshot.title')"
      icon="image"
      :subtitle="t('settings.screenshot.subtitle')"
    >
      <AppToggle
        :model-value="screenshotOn"
        :disabled="screenshotBusy"
        :label="t('settings.screenshot.label')"
        :description="t('settings.screenshot.description')"
        @update:model-value="toggleScreenshot"
      />
      <!--
        部分授权（Android 14 的「仅选中的照片」）是一条**必须单独说**的提示：
        权限确实给了，但观察者看不到新截图，界面不能让开关假装能用。
      -->
      <p v-if="screenshotPartial" class="cm-help mt-sm warn">
        {{ t("settings.screenshot.partial.description") }}
      </p>
      <p v-else class="cm-help mt-sm">
        {{ t("settings.screenshot.help") }}
      </p>
    </AppCard>

    <AppCard :title="t('settings.about.title')" icon="info">
      <div class="about">
        <span>ClipMesh {{ version ?? "—" }}</span>
        <StatusPill
          :label="mock ? t('settings.about.mock') : t('settings.about.tauri')"
          :tone="mock ? 'warn' : 'ok'"
          size="sm"
        />
      </div>
      <p class="cm-help mt-sm">
        {{ t("settings.about.body", { port: statusStore.listenPort }) }}
      </p>
      <p class="cm-help mt-sm">
        {{ t("settings.about.licenseBefore") }}
        <span class="cm-mono">LICENSE</span>{{ t("settings.about.licenseAfter") }}
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

/* 需要用户去系统设置里改点什么时，这段话不能看起来像普通说明。 */
.warn {
  color: var(--warn);
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
