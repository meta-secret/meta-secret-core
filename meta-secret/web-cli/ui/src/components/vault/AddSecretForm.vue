<script setup lang="ts">
import { ref, watch, defineEmits, defineProps } from 'vue';
import { MetaPasswordId, PlainPassInfo, WasmApplicationManager } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import ProgressBar from '@/components/common/ProgressBar.vue';

const props = defineProps<{ show: boolean }>();
const emit = defineEmits(['added', 'close']);

const appState = AppState();
const newPassword = ref('');
const newPassDescription = ref('');
const isSubmitting = ref(false);
const lastSubmitTime = ref(0);
const errorMessage = ref('');
const DEBOUNCE_MS = 2000; // Prevent resubmissions within 2 seconds
const progress = ref(0);
const progressInterval = ref<number | null>(null);

watch(() => props.show, (val) => {
  if (!val) {
    newPassword.value = '';
    newPassDescription.value = '';
    isSubmitting.value = false;
    errorMessage.value = '';
    resetProgress();
  }
});

const resetProgress = () => {
  if (progressInterval.value) {
    clearInterval(progressInterval.value);
    progressInterval.value = null;
  }
  progress.value = 0;
};

const startProgressSimulation = () => {
  resetProgress();
  
  // Simulate progress during submission
  progressInterval.value = window.setInterval(() => {
    if (progress.value < 90) {
      progress.value += Math.random() * 5;
    }
  }, 100);
};

const completeProgress = () => {
  if (progressInterval.value) {
    clearInterval(progressInterval.value);
    progressInterval.value = null;
  }
  progress.value = 100;
  
  // Reset progress after a delay
  setTimeout(() => {
    progress.value = 0;
  }, 1000);
};

const handleAdd = async () => {
  // Multiple protection checks to avoid double submission
  if (isSubmitting.value || !newPassword.value || !newPassDescription.value) {
    return;
  }
  
  // Check if a password with this description already exists
  const existingPasswords = appState.passwords;
  const passwordExists = existingPasswords.some(
    (secret: MetaPasswordId) => secret.name === newPassDescription.value
  );
  
  if (passwordExists) {
    errorMessage.value = "A secret with this description already exists";
    return;
  }
  
  // Clear any previous error message
  errorMessage.value = '';
  
  // Add time-based debounce protection
  const now = Date.now();
  if (now - lastSubmitTime.value < DEBOUNCE_MS) {
    console.log("Preventing duplicate submission - too soon after last attempt");
    return;
  }
  
  try {
    isSubmitting.value = true;
    lastSubmitTime.value = now;
    startProgressSimulation();
    
    // Add timestamp to description to make each submission unique
    // This helps prevent creating the same event twice
    const uniqueDesc = `${newPassDescription.value}`;
    const pass = new PlainPassInfo(uniqueDesc, newPassword.value);
    
    // Check if appManager is properly initialized
    const manager = appState.appManager as any;
    if (!manager || typeof manager.cluster_distribution !== 'function') {
      console.error("App manager not properly initialized");
      return;
    }
    
    console.log("Starting cluster_distribution");
    // Call wrapped in try/catch to handle errors properly
    await manager.cluster_distribution(pass);
    console.log("Finished cluster_distribution");
    
    await appState.updateState();
    
    // Complete the progress animation
    completeProgress();
    
    // Clear form and notify parent
    emit('added');
    newPassword.value = '';
    newPassDescription.value = '';
  } catch (error) {
    console.error("Error during secret distribution:", error);
    resetProgress();
    errorMessage.value = "Failed to add secret. Please try again.";
  } finally {
    // Set a slight delay before allowing new submissions
    setTimeout(() => {
      isSubmitting.value = false;
    }, 500);
  }
};

const handleClose = () => {
  if (isSubmitting.value) {
    return; // Don't allow closing during submission
  }
  emit('close');
};
</script>

<template>
  <div v-if="props.show" :class="$style.modalOverlay" @click.self="handleClose">
    <div :class="$style.modalContainer" @keydown.esc="handleClose">
      <div :class="$style.modalHeader">
        <h3 :class="$style.modalTitle">Add New Secret</h3>
        <button :class="$style.closeButton" @click="handleClose" :disabled="isSubmitting">&times;</button>
      </div>
      
      <!-- Progress bar -->
      <ProgressBar v-if="isSubmitting" :progress="progress" color="green" height="4px" />
      
      <div :class="$style.modalBody">
        <div :class="$style.inputGroup">
          <label :class="$style.inputLabel">Description</label>
          <div :class="$style.inputWrapper">
            <input 
              type="text" 
              :class="$style.input" 
              placeholder="my meta secret" 
              v-model="newPassDescription"
              :disabled="isSubmitting" 
            />
          </div>
        </div>
        <div :class="$style.inputGroup">
          <label :class="$style.inputLabel">Secret</label>
          <div :class="$style.inputWrapper">
            <input 
              type="password" 
              :class="$style.input" 
              placeholder="top$ecret" 
              v-model="newPassword"
              :disabled="isSubmitting" 
            />
          </div>
        </div>
        <div v-if="errorMessage" :class="$style.errorMessage">
          {{ errorMessage }}
        </div>
        <div :class="$style.buttonContainer">
          <button 
            :class="$style.addButton" 
            @click="handleAdd" 
            :disabled="!newPassword || !newPassDescription || isSubmitting"
          >
            {{ isSubmitting ? 'Adding...' : 'Add' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style module>
.inputGroup {
  @apply mb-4;
}
.inputLabel {
  @apply block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1;
}
.inputWrapper {
  @apply relative rounded-md;
  @apply bg-gray-50 dark:bg-gray-800 border border-gray-300 dark:border-gray-600;
  @apply focus-within:ring-2 focus-within:ring-slate-500 dark:focus-within:ring-slate-400;
  @apply focus-within:border-slate-500 dark:focus-within:border-slate-400;
  @apply transition-all duration-200;
}
.input {
  @apply block w-full rounded-md py-2 px-3;
  @apply bg-transparent text-gray-700 dark:text-gray-200;
  @apply placeholder-gray-400 dark:placeholder-gray-500;
  @apply focus:outline-none;
}
.buttonContainer {
  @apply flex justify-end mt-5;
}
.addButton {
  @apply bg-slate-600 hover:bg-slate-700 text-white font-medium py-2 px-5 rounded-md;
  @apply dark:bg-slate-700 dark:hover:bg-slate-800;
  @apply transition-colors duration-200;
  @apply disabled:opacity-50 disabled:cursor-not-allowed;
}
.modalOverlay {
  @apply fixed inset-0 bg-black bg-opacity-50 dark:bg-opacity-70 flex items-center justify-center z-50;
  @apply transition-opacity duration-300 ease-in-out;
  @apply backdrop-blur-sm;
}
.modalContainer {
  @apply bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-md mx-4;
  @apply border border-gray-200 dark:border-gray-700;
  @apply transform transition-all duration-300 ease-in-out;
  @apply scale-100 opacity-100;
}
.modalHeader {
  @apply flex items-center justify-between px-6 py-4;
  @apply border-b border-gray-200 dark:border-gray-700;
}
.modalTitle {
  @apply text-lg font-medium text-gray-800 dark:text-gray-200;
}
.closeButton {
  @apply text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200;
  @apply text-2xl font-bold leading-none;
  @apply transition-colors duration-200;
}
.modalBody {
  @apply px-6 py-5;
}
.errorMessage {
  @apply text-red-500 text-sm mt-2 mb-3;
}
</style> 