<script setup lang="ts">
import { ref, watch, defineEmits, defineProps, computed, onMounted } from 'vue';
import { MetaPasswordId, PlainPassInfo, WasmApplicationManager } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import ProgressBar from '@/components/common/ProgressBar.vue';
import * as bip39 from 'bip39';

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
const secretType = ref('password'); // 'password' or 'note'
const seedPhraseLength = ref(12); // 12 or 24 words
const seedWords = ref<string[]>(Array(24).fill(''));
const currentFocusedIndex = ref(-1);
const suggestions = ref<string[]>([]);
const showSuggestions = ref(false);
const selectedSuggestionIndex = ref(0); // Track the currently selected suggestion

// Initialize BIP39 on component creation
onMounted(() => {
  // Focus the first seed word input if seed phrase is selected
  if (secretType.value === 'password') {
    setTimeout(() => {
      const firstInput = document.getElementById('seed-word-0');
      if (firstInput) {
        firstInput.focus();
      }
    }, 100);
  }
});

// Watch for changes in the combined seed phrase and update newPassword
watch(seedWords, () => {
  const combined = seedWords.value.slice(0, seedPhraseLength.value).join(' ').trim();
  newPassword.value = combined;
}, { deep: true });

watch(seedPhraseLength, () => {
  const combined = seedWords.value.slice(0, seedPhraseLength.value).join(' ').trim();
  newPassword.value = combined;
});

watch(() => props.show, (val) => {
  if (!val) {
    newPassword.value = '';
    newPassDescription.value = '';
    isSubmitting.value = false;
    errorMessage.value = '';
    secretType.value = 'password';
    seedPhraseLength.value = 12;
    seedWords.value = Array(24).fill('');
    currentFocusedIndex.value = -1;
    suggestions.value = [];
    showSuggestions.value = false;
    resetProgress();
  }
});

// Manually focus the first input field when seed phrase is selected
watch(secretType, (newVal) => {
  if (newVal === 'password') {
    setTimeout(() => {
      const firstInput = document.getElementById('seed-word-0');
      if (firstInput) {
        firstInput.focus();
      }
    }, 100);
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

// Validate the mnemonic using bip39
const validateSeedPhrase = (): boolean => {
  const phrase = seedWords.value.slice(0, seedPhraseLength.value).join(' ').trim();
  return bip39.validateMnemonic(phrase);
};

const handleAdd = async () => {
  // Multiple protection checks to avoid double submission
  if (isSubmitting.value || !newPassword.value || !newPassDescription.value) {
    return;
  }
  
  // For seed phrases, validate it's a proper BIP39 phrase
  if (secretType.value === 'password') {
    const isValid = validateSeedPhrase();
    if (!isValid) {
      errorMessage.value = "Invalid seed phrase. Please check your words.";
      return;
    }
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
    seedWords.value = Array(24).fill('');
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

// Seed word input field utilities
const setFocusedWordIndex = (index: number) => {
  currentFocusedIndex.value = index;
  const input = seedWords.value[index] || '';
  updateSuggestions(input);
};

// Clear focused index when clicking away
const clearFocusedIndex = () => {
  // Small delay to allow other click handlers to run first
  setTimeout(() => {
    currentFocusedIndex.value = -1;
    showSuggestions.value = false;
  }, 100);
};

// Get word suggestions using BIP39 wordlist directly 
const updateSuggestions = (input: string) => {
  if (!input || !input.trim()) {
    suggestions.value = [];
    showSuggestions.value = false;
    return;
  }
  
  const inputLower = input.toLowerCase().trim();
  
  // Get suggestions directly from the BIP39 library
  if (bip39.wordlists && bip39.wordlists.english) {
    suggestions.value = bip39.wordlists.english
      .filter(word => word.startsWith(inputLower))
      .slice(0, 5);
  } else {
    suggestions.value = [];
  }
  
  showSuggestions.value = suggestions.value.length > 0;
  selectedSuggestionIndex.value = 0; // Reset selection to first item
};

// Handle input changes
const handleInput = (index: number) => {
  const input = seedWords.value[index] || '';
  updateSuggestions(input);
  
  // Ensure this field is focused
  currentFocusedIndex.value = index;
};

// Apply suggestion
const applySuggestion = (suggestion: string) => {
  if (currentFocusedIndex.value >= 0) {
    seedWords.value[currentFocusedIndex.value] = suggestion;
    showSuggestions.value = false;
    
    // Move to next field
    if (currentFocusedIndex.value < seedPhraseLength.value - 1) {
      const nextInput = document.getElementById(`seed-word-${currentFocusedIndex.value + 1}`);
      if (nextInput) {
        nextInput.focus();
      }
    }
  }
};

// Apply the currently selected suggestion
const applySelectedSuggestion = () => {
  if (suggestions.value.length > 0) {
    const suggestion = suggestions.value[selectedSuggestionIndex.value];
    applySuggestion(suggestion);
  }
};

// Move to next input field based on key press, not on every input
const handleKeyDown = (event: KeyboardEvent, index: number) => {
  // If Tab key is pressed, default browser behavior will handle focus
  
  // Handle suggestion selection with arrow keys
  if (showSuggestions.value && suggestions.value.length > 0) {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      // Move selection down in the list, wrapping around if needed
      selectedSuggestionIndex.value = (selectedSuggestionIndex.value + 1) % suggestions.value.length;
      return;
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      // Move selection up in the list, wrapping around if needed
      selectedSuggestionIndex.value = (selectedSuggestionIndex.value - 1 + suggestions.value.length) % suggestions.value.length;
      return;
    } else if (event.key === 'Enter' && suggestions.value.length > 0) {
      event.preventDefault();
      applySelectedSuggestion();
      return;
    } else if (event.key === 'Escape') {
      event.preventDefault();
      showSuggestions.value = false;
      return;
    }
  }
  
  // If Enter or Space is pressed, move to next field
  if ((event.key === 'Enter' || event.key === ' ') && index < seedPhraseLength.value - 1) {
    event.preventDefault(); // Prevent space from being entered
    
    // If space, add the current value without the space
    if (event.key === ' ') {
      seedWords.value[index] = seedWords.value[index].trim();
    }
    
    // Check if there's a single suggestion we can apply
    if (suggestions.value.length === 1) {
      seedWords.value[index] = suggestions.value[0];
    }
    
    // Move to next field
    const nextInput = document.getElementById(`seed-word-${index + 1}`);
    if (nextInput) {
      nextInput.focus();
    }
    
    showSuggestions.value = false;
  }
};

// Handle pasting multiple words
const handlePaste = (event: ClipboardEvent, index: number) => {
  event.preventDefault();
  const pastedText = event.clipboardData?.getData('text') || '';
  const words = pastedText.trim().split(/\s+/);
  
  if (words.length > 1) {
    // This is a multi-word paste, likely a full seed phrase
    const totalWords = Math.min(words.length, seedPhraseLength.value);
    
    // Update seed phrase length if needed
    if (words.length === 24 && seedPhraseLength.value !== 24) {
      seedPhraseLength.value = 24;
    }
    
    // Fill in all the words
    for (let i = 0; i < totalWords; i++) {
      seedWords.value[i] = words[i].toLowerCase();
    }
    
    // Validate the complete phrase
    errorMessage.value = validateSeedPhrase() ? '' : 'Warning: This may not be a valid seed phrase';
  } else {
    // Single word paste
    seedWords.value[index] = pastedText.toLowerCase();
    const nextIndex = Math.min(index + 1, seedPhraseLength.value - 1);
    const nextInput = document.getElementById(`seed-word-${nextIndex}`);
    if (nextInput) {
      nextInput.focus();
    }
  }
};

// Check if all required seed words are filled
const allSeedWordsFilled = computed(() => {
  return seedWords.value.slice(0, seedPhraseLength.value).every(word => word.trim() !== '');
});

// Check if word is in BIP39 wordlist
const isValidBip39Word = (word: string): boolean => {
  if (!word) return true; // Skip validation for empty words
  if (!bip39.wordlists || !bip39.wordlists.english) return true; // Skip if wordlist not available
  
  return bip39.wordlists.english.includes(word.toLowerCase());
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
      
      <div :class="$style.modalBody" @click="clearFocusedIndex">
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
        
        <!-- Secret Type Selector -->
        <div :class="$style.inputGroup">
          <label :class="$style.inputLabel">Secret Type</label>
          <div :class="$style.radioGroup">
            <label :class="$style.radioLabel">
              <input 
                type="radio" 
                value="password" 
                v-model="secretType"
                :disabled="isSubmitting" 
              />
              <span :class="$style.radioText">Seed Phrase</span>
            </label>
            <label :class="$style.radioLabel">
              <input 
                type="radio" 
                value="note" 
                v-model="secretType"
                :disabled="isSubmitting" 
              />
              <span :class="$style.radioText">Secret Note</span>
            </label>
          </div>
        </div>
        
        <!-- Dynamic Secret Input based on type -->
        <div v-if="secretType === 'password'" :class="$style.seedPhraseSection">
          <!-- Seed Phrase Length Selection -->
          <div :class="$style.seedLengthSelector">
            <label :class="$style.inputLabel">Seed Phrase Length</label>
            <div :class="$style.radioGroup">
              <label :class="$style.radioLabel">
                <input 
                  type="radio" 
                  :value="12" 
                  v-model="seedPhraseLength"
                  :disabled="isSubmitting" 
                />
                <span :class="$style.radioText">12 words</span>
              </label>
              <label :class="$style.radioLabel">
                <input 
                  type="radio" 
                  :value="24" 
                  v-model="seedPhraseLength"
                  :disabled="isSubmitting" 
                />
                <span :class="$style.radioText">24 words</span>
              </label>
            </div>
          </div>
          
          <!-- Seed Word Inputs -->
          <div :class="$style.seedWordsContainer">
            <div 
              v-for="index in seedPhraseLength" 
              :key="index-1"
              :class="$style.seedWordInputGroup"
            >
              <label 
                :class="[$style.seedWordLabel, 
                  seedWords[index-1] && !isValidBip39Word(seedWords[index-1]) ? $style.invalidWord : '']"
              >
                {{ index }}
              </label>
              <div :class="$style.seedWordInputWrapper">
                <input 
                  :id="`seed-word-${index-1}`"
                  type="text" 
                  :class="[$style.seedWordInput, 
                    seedWords[index-1] && !isValidBip39Word(seedWords[index-1]) ? $style.invalidWordInput : '']" 
                  :placeholder="`Word ${index}`"
                  v-model="seedWords[index-1]"
                  :disabled="isSubmitting"
                  @paste="handlePaste($event, index-1)"
                  @input="handleInput(index-1)"
                  @keydown="handleKeyDown($event, index-1)"
                  @focus="setFocusedWordIndex(index-1)"
                  autocomplete="off"
                />
                <!-- Word suggestions dropdown -->
                <div 
                  v-if="showSuggestions && currentFocusedIndex === index-1 && suggestions.length > 0"
                  :class="$style.suggestionsDropdown"
                  @click.stop
                >
                  <div 
                    v-for="(suggestion, i) in suggestions" 
                    :key="suggestion"
                    :class="[
                      $style.suggestionItem, 
                      i === selectedSuggestionIndex ? $style.selectedSuggestion : ''
                    ]"
                    @click="applySuggestion(suggestion)"
                  >
                    {{ suggestion }}
                  </div>
                </div>
              </div>
            </div>
          </div>
          
          <!-- Seed phrase instructions -->
          <div :class="$style.seedPhraseInstructions">
            <p>Enter your seed phrase or paste it all at once in any field</p>
            <p class="text-xs mt-1">Word suggestions will appear as you type</p>
          </div>
        </div>
        
        <!-- Secret Note Textarea -->
        <div v-if="secretType === 'note'" :class="$style.inputGroup">
          <label :class="$style.inputLabel">Secret Note</label>
          <div :class="$style.inputWrapper">
            <textarea 
              :class="[$style.input, $style.textarea]" 
              placeholder="Enter your secret note" 
              v-model="newPassword"
              :disabled="isSubmitting" 
              rows="4"
            ></textarea>
          </div>
        </div>
        
        <div v-if="errorMessage" :class="$style.errorMessage">
          {{ errorMessage }}
        </div>
        <div :class="$style.buttonContainer">
          <button 
            :class="$style.addButton" 
            @click="handleAdd" 
            :disabled="(secretType === 'password' && !allSeedWordsFilled) || 
                      (secretType === 'note' && !newPassword) || 
                      !newPassDescription || 
                      isSubmitting"
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
.textarea {
  @apply resize-y min-h-[100px];
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
.radioGroup {
  @apply flex space-x-6 mt-1;
}
.radioLabel {
  @apply flex items-center cursor-pointer;
}
.radioText {
  @apply ml-2 text-gray-700 dark:text-gray-300;
}

/* Seed Phrase Specific Styles */
.seedPhraseSection {
  @apply mb-4;
}
.seedLengthSelector {
  @apply mb-4;
}
.seedWordsContainer {
  @apply grid grid-cols-3 gap-2;
  @apply max-h-[300px] overflow-y-auto pr-2;
}
.seedWordInputGroup {
  @apply flex items-center mb-2;
  @apply relative;
}
.seedWordLabel {
  @apply flex items-center justify-center w-6 h-6 rounded-full bg-slate-200 dark:bg-slate-700;
  @apply text-xs font-medium text-slate-700 dark:text-slate-300 mr-2;
}
.invalidWord {
  @apply bg-red-200 dark:bg-red-800 text-red-800 dark:text-red-200;
}
.seedWordInputWrapper {
  @apply flex-1 relative rounded-md;
  @apply bg-gray-50 dark:bg-gray-800 border border-gray-300 dark:border-gray-600;
  @apply focus-within:ring-2 focus-within:ring-slate-500 dark:focus-within:ring-slate-400;
  @apply focus-within:border-slate-500 dark:focus-within:border-slate-400;
  @apply transition-all duration-200;
}
.seedWordInput {
  @apply block w-full rounded-md py-1 px-2 text-sm;
  @apply bg-transparent text-gray-700 dark:text-gray-200;
  @apply placeholder-gray-400 dark:placeholder-gray-500;
  @apply focus:outline-none;
}
.invalidWordInput {
  @apply border-red-300 dark:border-red-700 text-red-600 dark:text-red-400;
}
.seedPhraseInstructions {
  @apply text-sm text-gray-500 dark:text-gray-400 mt-3 text-center;
}
.suggestionsDropdown {
  @apply absolute z-10 mt-1 w-full bg-white dark:bg-gray-700 shadow-lg;
  @apply rounded-md border border-gray-200 dark:border-gray-600 py-1;
  @apply max-h-32 overflow-y-auto;
}
.suggestionItem {
  @apply px-3 py-1 text-sm text-gray-700 dark:text-gray-200 cursor-pointer;
  @apply hover:bg-slate-100 dark:hover:bg-slate-600;
}
.selectedSuggestion {
  @apply bg-slate-100 dark:bg-slate-600;
}
</style> 