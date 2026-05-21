<script setup lang="ts">
import DocMindBadge from "../components/docmind/DocMindBadge.vue";
</script>

<template>
  <div class="h-full overflow-y-auto p-8">
    <div class="mb-7">
      <h1 class="text-2xl font-semibold tracking-tight text-slate-950">设置</h1>
      <p class="mt-1 text-sm text-slate-500">控制文件过滤、索引策略和本地数据。</p>
    </div>

    <div class="space-y-5">
      <div class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div class="mb-4 text-sm font-semibold text-slate-900">索引规则</div>
        <div class="space-y-4">
          <label class="flex items-center justify-between text-sm text-slate-700">
            <span>最大文件大小</span>
            <select class="rounded-xl border border-slate-200 bg-white px-3 py-2">
              <option>50 MB</option>
              <option>100 MB</option>
              <option>200 MB</option>
            </select>
          </label>
          <label class="flex items-center justify-between text-sm text-slate-700">
            <span>隐藏文件</span>
            <DocMindBadge>默认排除</DocMindBadge>
          </label>
          <label class="flex items-center justify-between text-sm text-slate-700">
            <span>支持格式</span>
            <span class="text-xs text-slate-500">TXT, MD, HTML, DOCX, PDF</span>
          </label>
        </div>
      </div>

      <div class="rounded-3xl border border-slate-200 bg-white p-6 shadow-sm">
        <div class="mb-4 text-sm font-semibold text-slate-900">排除目录</div>
        <div class="flex flex-wrap gap-2">
          <DocMindBadge v-for="x in ['node_modules', '.git', 'target', 'Library', 'Applications', 'System']" :key="x">
            {{ x }}
          </DocMindBadge>
        </div>
      </div>

      <div class="rounded-3xl border border-red-100 bg-red-50 p-6">
        <div class="text-sm font-semibold text-red-800">危险操作</div>
        <div class="mt-1 text-xs text-red-600">清空索引不会删除原始文件，只会删除 docMind 的本地索引数据。</div>
        <button class="mt-4 rounded-2xl bg-white px-4 py-2 text-sm font-medium text-red-700 shadow-sm">清空全部索引</button>
      </div>
    </div>
  </div>
</template>
