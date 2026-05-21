<script setup lang="ts">
import { computed } from "vue";

interface Props {
  ext?: string;
}

const props = defineProps<Props>();

const label = computed(() => (props.ext ? props.ext.toUpperCase() : "DOC"));
</script>

<template>
  <div class="docmind-file-icon flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-slate-100 text-[10px] font-semibold text-slate-600">
    {{ label }}
  </div>
</template>
