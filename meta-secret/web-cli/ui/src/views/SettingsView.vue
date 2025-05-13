<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { AppState } from '@/stores/app-state';
import AlphaBadge from '@/components/common/AlphaBadge.vue';

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
    await router.push('/');
  } finally {
    isCleaning.value = false;
    showConfirmation.value = false;
  }
}

function goBack() {
  router.push('/');
}

function toggleConfirmation() {
  showConfirmation.value = !showConfirmation.value;
}
</script>

<template>
  <div class="container mx-auto max-w-md px-4">
    <div class="relative">
      <AlphaBadge />
    </div>
    
    <header :class="$style.settingsHeader">
      <button @click="goBack" :class="$style.backButton">
        <span :class="$style.backIcon">←</span>
        <span>Back</span>
      </button>
      <h1 :class="$style.settingsTitle">Settings</h1>
    </header>

    <div :class="$style.settingsContent">
      <section :class="$style.settingsSection">
        <h2 :class="$style.sectionTitle">Data Management</h2>
        
        <div :class="$style.sectionCard">
          <div :class="$style.cardHeader">
            <div :class="[$style.cardIcon, $style.danger]">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :class="$style.icon">
                <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path>
                <line x1="12" y1="9" x2="12" y2="13"></line>
                <line x1="12" y1="17" x2="12.01" y2="17"></line>
              </svg>
            </div>
            <h3 :class="$style.cardTitle">Clean Database</h3>
          </div>
          
          <div :class="$style.cardContent">
            <p :class="$style.cardDescription">
              Delete all vault data and start fresh. This action removes all secrets,
              vault configurations, and resets the application to its initial state.
            </p>
            
            <div v-if="!showConfirmation" :class="$style.cardActions">
              <button 
                :class="[$style.actionButton, $style.danger]"
                @click="toggleConfirmation"
              >
                Clean Database
              </button>
            </div>
            
            <div v-else :class="$style.confirmationBox">
              <p :class="$style.confirmationText">Are you sure? This action cannot be undone.</p>
              <div :class="$style.confirmationActions">
                <button 
                  :class="[$style.actionButton, $style.secondary]"
                  @click="toggleConfirmation"
                  :disabled="isCleaning"
                >
                  Cancel
                </button>
                <button 
                  :class="[$style.actionButton, $style.danger]"
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

<style module>
.settingsHeader {
  display: flex;
  align-items: center;
  margin: 2rem 0;
  position: relative;
}

.backButton {
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

.backButton:hover {
  color: #1f2937;
}

:global(.dark) .backButton {
  color: #9ca3af;
}

:global(.dark) .backButton:hover {
  color: #f3f4f6;
}

.backIcon {
  margin-right: 0.25rem;
  font-size: 1.25rem;
}

.settingsTitle {
  text-align: center;
  font-size: 1.5rem;
  font-weight: 700;
  color: #111827;
  flex-grow: 1;
}

:global(.dark) .settingsTitle {
  color: #f9fafb;
}

.settingsContent {
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.settingsSection {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.sectionTitle {
  font-size: 1.125rem;
  font-weight: 600;
  color: #4b5563;
  border-bottom: 1px solid #e5e7eb;
  padding-bottom: 0.5rem;
  margin-bottom: 0.5rem;
}

:global(.dark) .sectionTitle {
  color: #9ca3af;
  border-bottom-color: #374151;
}

.sectionCard {
  background-color: white;
  border-radius: 0.75rem;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.05);
  border: 1px solid #e5e7eb;
  overflow: hidden;
  transition: all 0.2s ease;
}

:global(.dark) .sectionCard {
  background-color: #1f2937;
  border-color: #374151;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.2);
}

.cardHeader {
  display: flex;
  align-items: center;
  padding: 1.25rem 1.5rem;
  border-bottom: 1px solid #f3f4f6;
  gap: 1rem;
}

:global(.dark) .cardHeader {
  border-bottom-color: #374151;
}

.cardIcon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 2.5rem;
  height: 2.5rem;
  border-radius: 0.5rem;
}

.cardIcon.danger {
  background-color: rgba(239, 68, 68, 0.1);
  color: #ef4444;
}

.icon {
  width: 1.5rem;
  height: 1.5rem;
}

.cardTitle {
  font-size: 1.125rem;
  font-weight: 600;
  color: #111827;
  margin: 0;
}

:global(.dark) .cardTitle {
  color: #f3f4f6;
}

.cardContent {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.cardDescription {
  font-size: 0.875rem;
  color: #6b7280;
  line-height: 1.5;
  margin: 0;
}

:global(.dark) .cardDescription {
  color: #9ca3af;
}

.cardActions {
  display: flex;
  justify-content: flex-end;
}

.actionButton {
  padding: 0.625rem 1.25rem;
  font-weight: 500;
  font-size: 0.875rem;
  border: none;
  border-radius: 0.375rem;
  cursor: pointer;
  transition: all 0.2s;
}

.actionButton.danger {
  background-color: #ef4444;
  color: white;
}

.actionButton.danger:hover:not(:disabled) {
  background-color: #dc2626;
}

.actionButton.secondary {
  background-color: #f3f4f6;
  color: #4b5563;
}

.actionButton.secondary:hover:not(:disabled) {
  background-color: #e5e7eb;
}

:global(.dark) .actionButton.secondary {
  background-color: #374151;
  color: #d1d5db;
}

:global(.dark) .actionButton.secondary:hover:not(:disabled) {
  background-color: #4b5563;
}

.actionButton:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.confirmationBox {
  background-color: #fff1f2;
  border: 1px solid #fecdd3;
  border-radius: 0.5rem;
  padding: 1rem;
}

:global(.dark) .confirmationBox {
  background-color: rgba(239, 68, 68, 0.1);
  border-color: rgba(239, 68, 68, 0.3);
}

.confirmationText {
  color: #be123c;
  font-size: 0.875rem;
  font-weight: 500;
  margin: 0 0 1rem 0;
}

:global(.dark) .confirmationText {
  color: #fca5a5;
}

.confirmationActions {
  display: flex;
  justify-content: flex-end;
  gap: 0.75rem;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(-5px); }
  to { opacity: 1; transform: translateY(0); }
}

.animate-fadeIn {
  animation: fadeIn 0.2s ease-out forwards;
}
</style> 