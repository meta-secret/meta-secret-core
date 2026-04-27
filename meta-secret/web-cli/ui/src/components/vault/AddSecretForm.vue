<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { PlainPassInfo } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { vaultSecrets } from '@/locales/en';

type SecretType = 'password' | 'seed';

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (event: 'added'): void;
  (event: 'close'): void;
}>();

const appState = AppState();
const appManager = appState.appManager as any;

const secretType = ref<SecretType>('password');
const wordCount = ref<12 | 24>(12);
const passwordSecret = ref('');
const seedWords = ref<string[]>(Array.from({ length: 24 }, () => ''));
const isSubmitting = ref(false);
const formError = ref<string | null>(null);

const activeWords = computed(() => seedWords.value.slice(0, wordCount.value));
const isCompact24 = computed(() => secretType.value === 'seed' && wordCount.value === 24);
const modalTitle = computed(() => (
  secretType.value === 'seed' ? vaultSecrets.addSeedPhraseTitle : vaultSecrets.addSecretTitle
));
const submitLabel = computed(() => (
  secretType.value === 'seed' ? vaultSecrets.addSeedPhraseSubmit : vaultSecrets.addSecretSubmit
));

const resetState = () => {
  secretType.value = 'password';
  wordCount.value = 12;
  passwordSecret.value = '';
  seedWords.value = Array.from({ length: 24 }, () => '');
  isSubmitting.value = false;
  formError.value = null;
};

const close = () => {
  if (isSubmitting.value) return;
  resetState();
  emit('close');
};

watch(() => props.show, (isOpen) => {
  if (!isOpen) {
    resetState();
  }
});

const setWordCount = (count: 12 | 24) => {
  wordCount.value = count;
};

const setSeedWord = (index: number, value: string) => {
  seedWords.value[index] = value.trim();
};

const pasteSeedPhrase = async () => {
  try {
    const text = await navigator.clipboard.readText();
    if (!text) return;
    const words = text.trim().split(/\s+/).filter(Boolean);
    const nextWords = Array.from({ length: 24 }, (_, index) => words[index] || '');
    seedWords.value = nextWords;
    if (words.length === 12 || words.length === 24) {
      wordCount.value = words.length;
    }
  } catch (e) {
    console.error('Failed to read clipboard seed phrase', e);
  }
};

const validate = () => {
  if (secretType.value === 'password') {
    if (!passwordSecret.value.trim()) {
      formError.value = vaultSecrets.addSecretValidationPasswordRequired;
      return false;
    }
    return true;
  }

  const hasEmptyWord = activeWords.value.some((word) => !word.trim());
  if (hasEmptyWord) {
    formError.value = vaultSecrets.addSecretValidationSeedRequired;
    return false;
  }
  return true;
};

const buildPassId = () => {
  const existingNames = new Set(
    (appState.passwords || []).map((secret: any) => String(secret.name || '').toLowerCase()),
  );
  const base = secretType.value === 'seed' ? 'seed' : 'secret';
  let index = 1;
  while (existingNames.has(`${base}${index}`)) {
    index += 1;
  }
  return `${base}${index}`;
};

const submit = async () => {
  if (isSubmitting.value) return;
  formError.value = null;
  if (!validate()) return;

  const passId = buildPassId();
  const secretPayload = secretType.value === 'password'
    ? passwordSecret.value.trim()
    : activeWords.value.map((word) => word.trim()).join(' ');

  isSubmitting.value = true;
  try {
    const payload = new PlainPassInfo(passId, secretPayload);
    await appManager.cluster_distribution(payload);
    await appState.updateState();
    resetState();
    emit('added');
  } catch (e) {
    console.error('Failed to add secret', e);
    formError.value = vaultSecrets.addSecretSubmitError;
  } finally {
    isSubmitting.value = false;
  }
};
</script>

<template>
  <div v-if="show" class="overlay" @click="close">
    <div class="modal" @click.stop>
      <div class="header">
        <h2 class="title">{{ modalTitle }}</h2>
        <button class="close-btn" @click="close">×</button>
      </div>

      <div class="content" :class="{ compact24: isCompact24 }">
        <label class="label">{{ vaultSecrets.addSecretTypeLabel }}</label>
        <div class="segmented">
          <button
            class="segment-btn"
            :class="{ active: secretType === 'password' }"
            @click="secretType = 'password'"
          >
            🔑 {{ vaultSecrets.addSecretTypePassword }}
          </button>
          <button
            class="segment-btn"
            :class="{ active: secretType === 'seed' }"
            @click="secretType = 'seed'"
          >
            🌱 {{ vaultSecrets.addSecretTypeSeedPhrase }}
          </button>
        </div>

        <template v-if="secretType === 'password'">
          <label class="label">{{ vaultSecrets.addSecretValueLabel }}</label>
          <input
            v-model="passwordSecret"
            class="text-input"
            :placeholder="vaultSecrets.addSecretValuePlaceholder"
            autocomplete="off"
          />
        </template>

        <template v-else>
          <div class="count-row">
            <label class="label">{{ vaultSecrets.addSecretWordCountLabel }}</label>
            <div class="count-buttons">
              <button class="count-btn" :class="{ active: wordCount === 12 }" @click="setWordCount(12)">12</button>
              <button class="count-btn" :class="{ active: wordCount === 24 }" @click="setWordCount(24)">24</button>
            </div>
          </div>

          <button class="paste-block" @click="pasteSeedPhrase">
            <span class="paste-icon">📋</span>
            <span>
              <span class="paste-title">{{ vaultSecrets.addSecretPasteTitle }}</span>
              <span class="paste-hint">{{ vaultSecrets.addSecretPasteHint }}</span>
            </span>
          </button>

          <div class="seed-grid" :class="{ compact: wordCount === 12, compact24: isCompact24 }">
            <div v-for="(_, idx) in activeWords" :key="idx" class="seed-cell">
              <span class="seed-index">{{ idx + 1 }}.</span>
              <input
                class="seed-input"
                :value="seedWords[idx]"
                :placeholder="vaultSecrets.addSecretWordPlaceholder"
                autocomplete="off"
                @input="setSeedWord(idx, ($event.target as HTMLInputElement).value)"
              />
            </div>
          </div>
        </template>

        <div v-if="formError" class="error">{{ formError }}</div>
      </div>

      <div class="actions">
        <button class="btn-secondary" :disabled="isSubmitting" @click="close">{{ vaultSecrets.addSecretCancel }}</button>
        <button class="btn-primary" :disabled="isSubmitting" @click="submit">{{ submitLabel }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 250;
  padding: 24px;
}

.modal {
  width: 100%;
  max-width: 860px;
  max-height: 90vh;
  background: #0d1726;
  border: 1px solid #1e3050;
  border-radius: 28px;
  box-shadow: 0 32px 80px rgba(0, 0, 0, 0.6);
  padding: 28px 30px;
  display: flex;
  flex-direction: column;
}

.header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.title {
  margin: 0;
  color: #ffffff;
  font-size: 17px;
  font-weight: 800;
}

.close-btn {
  width: 40px;
  height: 40px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: transparent;
  color: #4a6080;
  font-size: 30px;
  cursor: pointer;
}

.close-btn:hover {
  border-color: #1e3050;
}

.content {
  display: flex;
  flex-direction: column;
  gap: 14px;
  overflow-y: auto;
  padding-right: 4px;
  min-height: 0;
}

.content.compact24 {
  gap: 10px;
}

.content::-webkit-scrollbar {
  width: 8px;
}

.content::-webkit-scrollbar-thumb {
  background: #1e3050;
  border-radius: 6px;
}

.label {
  color: #4a6080;
  font-size: 18px;
  font-weight: 600;
}

.text-input {
  width: 100%;
  height: 58px;
  background: #111e30;
  border: 1px solid #1e3050;
  border-radius: 16px;
  padding: 0 18px;
  color: #ffffff;
  font-size: 18px;
  outline: none;
}

.text-input::placeholder {
  color: #3a5070;
}

.segmented {
  background: #080f1c;
  border-radius: 16px;
  padding: 5px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px;
}

.segment-btn {
  height: 58px;
  border: none;
  border-radius: 13px;
  background: transparent;
  color: #4a6080;
  font-size: 20px;
  font-weight: 700;
  cursor: pointer;
}

.segment-btn.active {
  background: #2563eb;
  color: #ffffff;
}

.count-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.count-buttons {
  display: flex;
  gap: 8px;
}

.count-btn {
  width: 76px;
  height: 48px;
  border-radius: 14px;
  border: 1px solid #1e3050;
  background: #111e30;
  color: #4a6080;
  font-size: 20px;
  font-weight: 700;
  cursor: pointer;
}

.count-btn.active {
  background: #2563eb;
  border-color: #2563eb;
  color: #ffffff;
}

.paste-block {
  width: 100%;
  border: 1px solid #1a2840;
  border-radius: 14px;
  background: #080f1c;
  padding: 14px 16px;
  color: inherit;
  display: flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
  text-align: left;
}

.paste-icon {
  font-size: 20px;
}

.paste-title {
  display: block;
  color: #8aaacf;
  font-size: 18px;
  font-weight: 700;
}

.paste-hint {
  display: block;
  margin-top: 2px;
  color: #3a5070;
  font-size: 16px;
}

.seed-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

.seed-grid.compact {
  margin-bottom: 8px;
}

.seed-cell {
  height: 52px;
  border: 1px solid #1a2840;
  border-radius: 12px;
  background: #111e30;
  display: flex;
  align-items: center;
  padding: 0 10px;
  gap: 8px;
}

.seed-grid.compact24 {
  gap: 8px;
}

.seed-grid.compact24 .seed-cell {
  height: 44px;
  padding: 0 8px;
}

.seed-grid.compact24 .seed-index {
  font-size: 14px;
}

.seed-grid.compact24 .seed-input {
  font-size: 16px;
}

.content.compact24 .text-input {
  height: 52px;
}

.content.compact24 .label {
  font-size: 17px;
}

.seed-index {
  color: #3a5070;
  font-size: 16px;
  font-weight: 700;
}

.seed-input {
  width: 100%;
  border: none;
  background: transparent;
  outline: none;
  color: #ffffff;
  font-size: 18px;
}

.seed-input::placeholder {
  color: #3a5070;
}

.error {
  margin-top: 4px;
  color: #fca5a5;
  background: #3a1820;
  border: 1px solid #a3213d;
  border-radius: 10px;
  padding: 10px 12px;
  font-size: 14px;
}

.actions {
  margin-top: 18px;
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  flex-shrink: 0;
}

.btn-secondary,
.btn-primary {
  min-width: 184px;
  height: 58px;
  border-radius: 18px;
  border: none;
  font-size: 15px;
  font-weight: 700;
  cursor: pointer;
}

.btn-secondary {
  background: #111e30;
  border: 1px solid #1e3050;
  color: #8aaacf;
}

.btn-primary {
  background: #2563eb;
  color: #ffffff;
}

.btn-secondary:disabled,
.btn-primary:disabled {
  opacity: 0.7;
  cursor: wait;
}

@media (max-width: 1100px) {
  .title {
    font-size: 17px;
  }

  .label,
  .text-input,
  .paste-title,
  .seed-input,
  .segment-btn {
    font-size: 16px;
  }

  .count-btn {
    font-size: 18px;
    width: 68px;
    height: 44px;
  }

  .btn-secondary,
  .btn-primary {
    height: 52px;
    min-width: 156px;
    font-size: 14px;
  }
}
</style>