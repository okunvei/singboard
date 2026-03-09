<script setup lang="ts">
import { computed } from 'vue'
import { useConfigStore } from '@/stores/config'
import ConfigEditor from '@/components/settings/ConfigEditor.vue'

const { config } = useConfigStore()

const configPath = computed(() => config.value.configPath)
const singboxPath = computed(() => config.value.singboxPath)
const workingDir = computed(() => config.value.workingDir)
</script>

<template>
  <div class="flex flex-col h-full min-h-0 gap-3">
    <h1 class="text-xl font-bold shrink-0">配置编辑</h1>
    <ConfigEditor
      class="flex-1 min-h-0"
      :config-path="configPath"
      :singbox-path="singboxPath"
      :working-dir="workingDir"
    />
  </div>
</template>
