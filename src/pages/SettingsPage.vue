<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { RefreshCw, Save, Trash2 } from "lucide-vue-next";
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
import { docmindApi, formatDocmindError } from "../services/docmindApi";
import type { IndexSettingsView } from "../types/docmind";

const factoryDefaultSettings: IndexSettingsView = {
  exclude_dirs: ["node_modules", ".git", "target", "Library", "Caches", "Application Support"],
  exclude_exts: [],
  max_file_size_mb: 50,
};

const savedSettings = ref<IndexSettingsView | null>(null);
const excludeDirsText = ref("");
const excludeExtsText = ref("");
const maxFileSizeMb = ref(50);
const loading = ref(false);
const saving = ref(false);
const clearing = ref(false);
const errorMessage = ref("");
const infoMessage = ref("");

const hasChanges = computed(() => {
  if (!savedSettings.value) {
    return false;
  }

  const normalize = (input: string) =>
    input
      .split(/[\n,;]+/)
      .map((item) => item.trim())
      .filter(Boolean)
      .join(",");

  return (
    normalize(excludeDirsText.value) !== savedSettings.value.exclude_dirs.join(",") ||
    normalize(excludeExtsText.value) !== savedSettings.value.exclude_exts.join(",") ||
    Number(maxFileSizeMb.value) !== savedSettings.value.max_file_size_mb
  );
});

const parseList = (value: string) =>
  Array.from(
    new Set(
      value
        .split(/[\n,;]+/)
        .map((item) => item.trim())
        .filter(Boolean)
        .map((item) => item.replace(/^\./, "")),
    ),
  );

const applySettings = (settings: IndexSettingsView) => {
  excludeDirsText.value = settings.exclude_dirs.join(", ");
  excludeExtsText.value = settings.exclude_exts.join(", ");
  maxFileSizeMb.value = settings.max_file_size_mb;
};

const loadSettings = async () => {
  loading.value = true;
  errorMessage.value = "";

  try {
    const settings = await docmindApi.getIndexSettings();
    savedSettings.value = settings;
    applySettings(settings);
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "加载设置失败");
    console.error("[DocMind] getIndexSettings failed", error);
  } finally {
    loading.value = false;
  }
};

const saveSettings = async () => {
  saving.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  const payload: IndexSettingsView = {
    exclude_dirs: parseList(excludeDirsText.value),
    exclude_exts: parseList(excludeExtsText.value),
    max_file_size_mb: Math.max(1, Math.floor(Number(maxFileSizeMb.value) || 50)),
  };

  try {
    await docmindApi.saveIndexSettings(payload);
    infoMessage.value = "设置已保存";
    savedSettings.value = payload;
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "保存设置失败");
    console.error("[DocMind] saveIndexSettings failed", error);
  } finally {
    saving.value = false;
  }
};

const resetToDefaults = () => {
  applySettings(factoryDefaultSettings);
  infoMessage.value = "已恢复默认配置，记得点击保存";
};

const clearAllIndexes = async () => {
  if (!window.confirm("确认清空全部索引？这不会删除原始文件，只会清理本地索引数据。")) {
    return;
  }

  clearing.value = true;
  errorMessage.value = "";
  infoMessage.value = "";

  try {
    await docmindApi.clearAllIndexes();
    infoMessage.value = "已清空全部索引";
  } catch (error) {
    errorMessage.value = formatDocmindError(error, "清空全部索引失败");
    console.error("[DocMind] clearAllIndexes failed", error);
  } finally {
    clearing.value = false;
  }
};

onMounted(loadSettings);
</script>

<template>
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7 flex items-center justify-between gap-4">
      <div>
        <h1 class="text-2xl font-semibold tracking-tight text-slate-950">设置</h1>
        <p class="mt-1 text-sm text-slate-500">配置索引排除规则、最大文件大小，并管理本地索引数据。</p>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <button
          class="inline-flex items-center gap-2 rounded-2xl border border-slate-200 bg-white px-4 py-2.5 text-sm font-medium text-slate-700 shadow-sm hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="resetToDefaults"
        >
          <RefreshCw :size="16" />
          恢复默认
        </button>
        <button
          class="inline-flex items-center gap-2 rounded-2xl bg-slate-900 px-4 py-2.5 text-sm font-medium text-white disabled:cursor-not-allowed disabled:opacity-70"
          :disabled="loading || saving"
          @click="saveSettings"
        >
          <Save :size="16" />
          {{ saving ? "保存中..." : "保存设置" }}
        </button>
      </div>
    </div>

    <div v-if="errorMessage" class="mb-4 rounded-2xl border border-red-100 bg-red-50 px-4 py-3 text-sm text-red-700">
      {{ errorMessage }}
    </div>

    <div v-if="infoMessage" class="mb-4 rounded-2xl border border-emerald-100 bg-emerald-50 px-4 py-3 text-sm text-emerald-700">
      {{ infoMessage }}
    </div>

    <div v-if="loading" class="rounded-3xl border border-dashed border-slate-300 bg-white p-6 text-sm text-slate-500">
      正在读取设置...
    </div>

    <div v-else class="grid gap-5 xl:grid-cols-[minmax(0,1.3fr)_minmax(0,0.7fr)]">
      <section class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div class="mb-5 flex items-center justify-between">
          <div>
            <div class="text-sm font-semibold text-slate-900">索引规则</div>
            <div class="mt-1 text-xs text-slate-500">目录名和扩展名会按逗号、分号或换行分隔，扩展名无需输入点号。</div>
          </div>
          <DocMindBadge tone="default">本地生效</DocMindBadge>
        </div>

        <div class="space-y-4">
          <label class="block">
            <div class="mb-2 text-sm font-medium text-slate-700">排除目录</div>
            <textarea
              v-model="excludeDirsText"
              rows="5"
              class="w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
              placeholder="node_modules, .git, target"
            />
          </label>

          <label class="block">
            <div class="mb-2 text-sm font-medium text-slate-700">排除扩展名</div>
            <textarea
              v-model="excludeExtsText"
              rows="4"
              class="w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
              placeholder="log, tmp, png"
            />
          </label>

          <div class="grid gap-4 md:grid-cols-2">
            <label class="block">
              <div class="mb-2 text-sm font-medium text-slate-700">最大文件大小（MB）</div>
              <input
                v-model.number="maxFileSizeMb"
                type="number"
                min="1"
                step="1"
                class="w-full rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-sm text-slate-900 outline-none transition focus:border-slate-400 focus:bg-white"
              />
            </label>

            <div class="rounded-2xl bg-slate-50 px-4 py-3">
              <div class="text-xs text-slate-500">当前状态</div>
              <div class="mt-2 text-sm font-medium text-slate-900">
                {{ hasChanges ? "有未保存修改" : "配置已同步" }}
              </div>
            </div>
          </div>
        </div>
      </section>

      <aside class="space-y-5">
        <section class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
          <div class="mb-4 flex items-center justify-between">
            <div>
              <div class="text-sm font-semibold text-slate-900">操作说明</div>
              <div class="mt-1 text-xs text-slate-500">这些设置会在下次扫描时生效。</div>
            </div>
            <DocMindBadge tone="success">已保存到本地</DocMindBadge>
          </div>

          <div class="space-y-3 text-sm text-slate-600">
            <p>• 排除目录按目录名匹配，例如 <span class="font-medium text-slate-900">node_modules</span>、<span class="font-medium text-slate-900">.git</span>。</p>
            <p>• 排除扩展名按文件后缀匹配，例如 <span class="font-medium text-slate-900">log</span>、<span class="font-medium text-slate-900">png</span>。</p>
            <p>• 超过最大文件大小的文件会在扫描阶段被跳过，并记录原因。</p>
          </div>
        </section>

        <section class="rounded-3xl border border-amber-200 bg-amber-50 p-6 shadow-sm">
          <div class="mb-3 flex items-center justify-between">
            <div>
              <div class="text-sm font-semibold text-amber-950">危险操作</div>
              <div class="mt-1 text-xs text-amber-800">清空本地索引，不会删除原始文档。</div>
            </div>
            <Trash2 class="text-amber-700" :size="18" />
          </div>
          <button
            class="inline-flex w-full items-center justify-center gap-2 rounded-2xl bg-amber-600 px-4 py-3 text-sm font-medium text-white shadow-sm hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-70"
            :disabled="clearing"
            @click="clearAllIndexes"
          >
            <RefreshCw :size="16" :class="{ 'animate-spin': clearing }" />
            {{ clearing ? "清空中..." : "清空全部索引" }}
          </button>
          <div class="mt-3 text-xs leading-5 text-amber-900/80">
            这会删除当前本地 SQLite 数据和 Tantivy 索引，适合排查初始化和索引异常。
          </div>
        </section>
      </aside>
    </div>
  </div>
</template>
