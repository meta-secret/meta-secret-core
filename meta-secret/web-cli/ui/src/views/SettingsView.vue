<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { AppState } from '@/stores/app-state';

const router = useRouter();
const jsAppState = AppState();
const isCleaning = ref(false);
const showConfirmation = ref(false);

async function cleanDatabase() {
  if (isCleaning.value) return;
  
  isCleaning.value = true;
  try {
    await (jsAppState.appManager as any).clean_up_database();
    await jsAppState.appStateInit();
    // Navigate back to home after cleaning
    router.push('/');
  } finally {
    isCleaning.value = false;
    showConfirmation.value = false;
  }
}

function goBack() {
  router.back();
}

function toggleConfirmation() {
  showConfirmation.value = !showConfirmation.value;
}
</script>

<template>
  <div class="container mx-auto max-w-md px-4">
    <header class="settings-header">
      <button @click="goBack" class="back-button">
        <span class="back-icon">←</span>
        <span>Back</span>
      </button>
      <h1 class="settings-title">Settings</h1>
    </header>

    <div class="settings-content">
      <section class="settings-section">
        <h2 class="section-title">Data Management</h2>
        
        <div class="section-card">
          <div class="card-header">
            <div class="card-icon danger">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="icon">
                <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path>
                <line x1="12" y1="9" x2="12" y2="13"></line>
                <line x1="12" y1="17" x2="12.01" y2="17"></line>
              </svg>
            </div>
            <h3 class="card-title">Clean Database</h3>
          </div>
          
          <div class="card-content">
            <p class="card-description">
              Delete all vault data and start fresh. This action removes all secrets,
              vault configurations, and resets the application to its initial state.
            </p>
            
            <div v-if="!showConfirmation" class="card-actions">
              <button 
                class="action-button danger"
                @click="toggleConfirmation"
              >
                Clean Database
              </button>
            </div>
            
            <div v-else class="confirmation-box">
              <p class="confirmation-text">Are you sure? This action cannot be undone.</p>
              <div class="confirmation-actions">
                <button 
                  class="action-button secondary"
                  @click="toggleConfirmation"
                  :disabled="isCleaning"
                >
                  Cancel
                </button>
                <button 
                  class="action-button danger"
                  :disabled="isCleaning"
                  @click="cleanDatabase"
                >
                  <span v-if="isCleaning">Cleaning...</span>
                  <span v-else>Yes, Clean Database</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.settings-header {
  display: flex;
  align-items: center;
  margin: 2rem 0;
  position: relative;
}

.back-button {
  display: flex;
  align-items: center;
  background: none;
  border: none;
  color: #4b5563;
  font-size: 0.875rem;
  cursor: pointer;
  padding: 0.5rem 0;
  position: absolute;
  left: 0;
}

.back-button:hover {
  color: #1f2937;
}

.back-icon {
  margin-right: 0.25rem;
  font-size: 1.25rem;
}

.settings-title {
  text-align: center;
  font-size: 1.5rem;
  font-weight: 700;
  color: #111827;
  flex-grow: 1;
}

.settings-content {
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.settings-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.section-title {
  font-size: 1.125rem;
  font-weight: 600;
  color: #4b5563;
  border-bottom: 1px solid #e5e7eb;
  padding-bottom: 0.5rem;
  margin-bottom: 0.5rem;
}

.section-card {
  background-color: white;
  border-radius: 0.75rem;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.05);
  border: 1px solid #e5e7eb;
  overflow: hidden;
  transition: all 0.2s ease;
}

.card-header {
  display: flex;
  align-items: center;
  padding: 1.25rem 1.5rem;
  border-bottom: 1px solid #f3f4f6;
  gap: 1rem;
}

.card-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 0.5rem;
}

.card-icon.danger {
  background-color: rgba(239, 68, 68, 0.1);
  color: #ef4444;
}

.icon {
  width: 1.5rem;
  height: 1.5rem;
}

.card-title {
  font-size: 1.125rem;
  font-weight: 600;
  color: #111827;
  margin: 0;
}

.card-content {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.card-description {
  font-size: 0.875rem;
  color: #6b7280;
  line-height: 1.5;
  margin: 0;
}

.card-actions {
  display: flex;
  justify-content: flex-end;
}

.action-button {
  padding: 0.625rem 1.25rem;
  font-weight: 500;
  font-size: 0.875rem;
  border: none;
  border-radius: 0.375rem;
  cursor: pointer;
  transition: all 0.2s;
}

.action-button.danger {
  background-color: #ef4444;
  color: white;
}

.action-button.danger:hover:not(:disabled) {
  background-color: #dc2626;
}

.action-button.secondary {
  background-color: #f3f4f6;
  color: #4b5563;
}

.action-button.secondary:hover:not(:disabled) {
  background-color: #e5e7eb;
}

.action-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.confirmation-box {
  background-color: #fff1f2;
  border: 1px solid #fecdd3;
  border-radius: 0.5rem;
  padding: 1rem;
}

.confirmation-text {
  color: #be123c;
  font-size: 0.875rem;
  font-weight: 500;
  margin: 0 0 1rem 0;
}

.confirmation-actions {
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
}

/* Dark mode styles */
@media (prefers-color-scheme: dark) {
  .settings-title {
    color: #f9fafb;
  }
  
  .section-title {
    color: #9ca3af;
    border-bottom-color: #374151;
  }
  
  .section-card {
    background-color: #1f2937;
    border-color: #374151;
  }
  
  .card-header {
    border-bottom-color: #374151;
  }
  
  .card-title {
    color: #f3f4f6;
  }
  
  .card-description {
    color: #9ca3af;
  }
  
  .action-button.secondary {
    background-color: #374151;
    color: #d1d5db;
  }
  
  .action-button.secondary:hover:not(:disabled) {
    background-color: #4b5563;
  }
  
  .confirmation-box {
    background-color: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.3);
  }
  
  .confirmation-text {
    color: #fca5a5;
  }
  
  .back-button {
    color: #9ca3af;
  }
  
  .back-button:hover {
    color: #f3f4f6;
  }
}
</style> 