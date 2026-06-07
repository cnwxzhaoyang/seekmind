<script setup lang="ts">
import { computed } from "vue";

interface Props {
  ext?: string;
}

const props = defineProps<Props>();

const label = computed(() => (props.ext ? props.ext.toUpperCase() : "DOC"));
</script>

<template>
  <div class="SeekMind-file-icon flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-badge text-[10px] font-semibold text-dim">
    {{ label }}
  </div>
</template>
