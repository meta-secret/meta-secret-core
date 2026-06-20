<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { PlainPassInfo } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { vaultSecrets } from '@/locales/en';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Clipboard } from 'lucide-vue-next';

type SecretType = 'password' | 'seed';

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ (event: 'added'): void; (event: 'close'): void }>();

const appState = AppState();
const appManager = appState.appManager as any;

const secretType = ref<SecretType>('password');
const wordCount = ref<12 | 24>(12);
const description = ref('');
const passwordSecret = ref('');
const seedWords = ref<string[]>(Array.from({ length: 24 }, () => ''));
const isSubmitting = ref(false);
const formError = ref<string | null>(null);

const activeWords = computed(() => seedWords.value.slice(0, wordCount.value));
const modalTitle = computed(() =>
  secretType.value === 'seed' ? vaultSecrets.addSeedPhraseTitle : vaultSecrets.addSecretTitle,
);
const submitLabel = computed(() =>
  secretType.value === 'seed' ? vaultSecrets.addSeedPhraseSubmit : vaultSecrets.addSecretSubmit,
);

const resetState = () => {
  secretType.value = 'password';
  wordCount.value = 12;
  description.value = '';
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

watch(() => props.show, (isOpen) => { if (!isOpen) resetState(); });

const setSeedWord = (index: number, value: string) => { seedWords.value[index] = value.trim(); };

const pasteSeedPhrase = async () => {
  try {
    const text = await navigator.clipboard.readText();
    if (!text) return;
    const words = text.trim().split(/\s+/).filter(Boolean);
    seedWords.value = Array.from({ length: 24 }, (_, i) => words[i] || '');
    if (words.length === 12 || words.length === 24) wordCount.value = words.length;
  } catch { /* clipboard denied */ }
};

const canSubmit = computed(() => {
  if (!description.value.trim()) return false;
  if (secretType.value === 'password') return !!passwordSecret.value.trim();
  return activeWords.value.every((w) => !!w.trim());
});

const submit = async () => {
  if (isSubmitting.value) return;
  formError.value = null;
  if (!description.value.trim()) { formError.value = vaultSecrets.addSecretValidationNameRequired; return; }
  if (secretType.value === 'password' && !passwordSecret.value.trim()) {
    formError.value = vaultSecrets.addSecretValidationPasswordRequired; return;
  }
  if (secretType.value === 'seed' && activeWords.value.some((w) => !w.trim())) {
    formError.value = vaultSecrets.addSecretValidationSeedRequired; return;
  }

  const passId = description.value.trim();
  const secretPayload =
    secretType.value === 'password'
      ? passwordSecret.value.trim()
      : activeWords.value.map((w) => w.trim()).join(' ');

  isSubmitting.value = true;
  try {
    const payload = new PlainPassInfo(passId, secretPayload);
    await appManager.cluster_distribution(payload);
    await appState.updateState();
    resetState();
    emit('added');
  } catch {
    formError.value = vaultSecrets.addSecretSubmitError;
  } finally {
    isSubmitting.value = false;
  }
};
</script>

<template>
  <Dialog :open="show" @update:open="(v) => { if (!v) close(); }">
    <DialogContent class="max-w-2xl">
      <DialogHeader>
        <DialogTitle>{{ modalTitle }}</DialogTitle>
      </DialogHeader>

      <div class="flex max-h-[60vh] flex-col gap-4 overflow-y-auto pr-1">
        <!-- Name -->
        <div class="space-y-1.5">
          <Label>{{ vaultSecrets.addSecretDescriptionLabel }}</Label>
          <Input v-model="description" :placeholder="vaultSecrets.addSecretDescriptionPlaceholder" autocomplete="off" />
        </div>

        <!-- Type toggle -->
        <div class="space-y-1.5">
          <Label>{{ vaultSecrets.addSecretTypeLabel }}</Label>
          <div class="grid grid-cols-2 gap-1 rounded-xl border bg-muted p-1">
            <button
              class="rounded-lg px-4 py-2.5 text-sm font-semibold transition-colors"
              :class="secretType === 'password' ? 'bg-background shadow-sm' : 'text-muted-foreground'"
              @click="secretType = 'password'"
            >
              🔑 {{ vaultSecrets.addSecretTypePassword }}
            </button>
            <button
              class="rounded-lg px-4 py-2.5 text-sm font-semibold transition-colors"
              :class="secretType === 'seed' ? 'bg-background shadow-sm' : 'text-muted-foreground'"
              @click="secretType = 'seed'"
            >
              🌱 {{ vaultSecrets.addSecretTypeSeedPhrase }}
            </button>
          </div>
        </div>

        <!-- Password input -->
        <template v-if="secretType === 'password'">
          <div class="space-y-1.5">
            <Label>{{ vaultSecrets.addSecretValueLabel }}</Label>
            <Input v-model="passwordSecret" :placeholder="vaultSecrets.addSecretValuePlaceholder" autocomplete="off" />
          </div>
        </template>

        <!-- Seed phrase -->
        <template v-else>
          <div class="flex items-center justify-between">
            <Label>{{ vaultSecrets.addSecretWordCountLabel }}</Label>
            <div class="flex gap-1 rounded-lg border p-0.5">
              <button
                class="rounded-md px-4 py-1 text-sm font-bold transition-colors"
                :class="wordCount === 12 ? 'bg-primary text-primary-foreground' : 'text-muted-foreground'"
                @click="wordCount = 12"
              >12</button>
              <button
                class="rounded-md px-4 py-1 text-sm font-bold transition-colors"
                :class="wordCount === 24 ? 'bg-primary text-primary-foreground' : 'text-muted-foreground'"
                @click="wordCount = 24"
              >24</button>
            </div>
          </div>

          <Button variant="outline" class="justify-start gap-2" @click="pasteSeedPhrase">
            <Clipboard class="h-4 w-4" />
            <span>
              <span class="block">{{ vaultSecrets.addSecretPasteTitle }}</span>
              <span class="block text-xs font-normal text-muted-foreground">{{ vaultSecrets.addSecretPasteHint }}</span>
            </span>
          </Button>

          <div class="grid grid-cols-3 gap-2">
            <div
              v-for="(_, idx) in activeWords"
              :key="idx"
              class="flex items-center gap-1.5 rounded-lg border bg-muted/30 px-2 py-1.5"
            >
              <span class="min-w-[1.25rem] text-xs font-bold text-muted-foreground">{{ idx + 1 }}.</span>
              <input
                class="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground/50"
                :value="seedWords[idx]"
                :placeholder="vaultSecrets.addSecretWordPlaceholder"
                autocomplete="off"
                @input="setSeedWord(idx, ($event.target as HTMLInputElement).value)"
              />
            </div>
          </div>
        </template>

        <Alert v-if="formError" variant="destructive">
          <AlertDescription>{{ formError }}</AlertDescription>
        </Alert>
      </div>

      <DialogFooter>
        <Button variant="outline" :disabled="isSubmitting" @click="close">{{ vaultSecrets.addSecretCancel }}</Button>
        <Button :disabled="isSubmitting || !canSubmit" @click="submit">{{ submitLabel }}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
