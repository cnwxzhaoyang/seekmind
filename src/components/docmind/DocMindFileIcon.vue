<script setup lang="ts">
import { computed } from "vue";

interface Props {
  ext?: string;
}

const props = defineProps<Props>();

const label = computed(() => (props.ext ? props.ext.toUpperCase() : "DOC"));
</script>

<template>
  <div class="docmind-file-icon flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-slate-100 text-[10px] font-semibold text-slate-600">
    {{ label }}
  </div>
</template>
