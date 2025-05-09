<script setup>
import { ref, watch, onBeforeUnmount } from 'vue';

const props = defineProps({
  isActive: Boolean,
  completed: Boolean,
  title: {
    type: String,
    default: 'Creating Vault...'
  },
  message: {
    type: String,
    default: "Please don't close this page. Vault creation is in progress..."
  }
});

const progress = ref(0);
const progressInterval = ref(null);

// Define all functions before using them in watchers
const stopProgressSimulation = () => {
  if (progressInterval.value) {
    clearInterval(progressInterval.value);
    progressInterval.value = null;
  }
};

const startProgressSimulation = () => {
  // Reset progress
  progress.value = 0;
  
  // Clear any existing interval
  stopProgressSimulation();
  
  // Simulate progress with intervals (since we don't have actual progress feedback)
  progressInterval.value = setInterval(() => {
    // Never reach 100% until actual completion
    if (progress.value < 90) {
      progress.value += Math.random() * 10;
    }
  }, 200);
};

const completeProgress = () => {
  stopProgressSimulation();
  progress.value = 100;
};

// Add the watcher after functions are defined
watch(() => props.isActive, (isActive) => {
  if (isActive) {
    startProgressSimulation();
  } else {
    stopProgressSimulation();
    progress.value = 0;
  }
}, { immediate: true });

// Watch for completed prop
watch(() => props.completed, (completed) => {
  if (completed && props.isActive) {
    completeProgress();
  }
}, { immediate: true });

onBeforeUnmount(() => {
  stopProgressSimulation();
});

// Expose methods to parent component
defineExpose({
  completeProgress
});
</script>

<template>
  <div v-if="isActive" :class="$style.progressContainer">
    <div :class="$style.warningBox">
      <div :class="$style.warningHeader">
        <span :class="$style.warningIcon">⚠️</span>
        <span :class="$style.warningTitle">{{ title }}</span>
      </div>
      <p :class="$style.warningMessage">{{ message }}</p>
      
      <div :class="$style.progressBarContainer">
        <div :class="$style.progressBar" :style="{ width: `${Math.min(progress, 100)}%` }"></div>
      </div>
      
      <p :class="$style.progressText">{{ Math.floor(progress) }}%</p>
    </div>
  </div>
</template>

<style module>
.progressContainer {
  @apply w-full max-w-md mt-8;
}

.warningBox {
  @apply bg-gray-800 border border-yellow-600 rounded-lg p-4;
  @apply shadow-lg transition-all duration-200;
}

.warningHeader {
  @apply flex items-center mb-2;
}

.warningIcon {
  @apply text-xl mr-2;
}

.warningTitle {
  @apply text-yellow-500 font-bold text-lg;
}

.warningMessage {
  @apply text-gray-300 mb-4 text-sm;
}

.progressBarContainer {
  @apply w-full h-2 bg-gray-700 rounded-full mb-2;
  @apply overflow-hidden;
}

.progressBar {
  @apply h-full bg-yellow-500 rounded-full;
  @apply transition-all duration-200;
}

.progressText {
  @apply text-center text-gray-400 text-sm;
}
</style>
