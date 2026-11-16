<script setup lang="ts">
import { ref, computed } from 'vue';
import { useAuthStore } from '@/stores/auth';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';

const authStore = useAuthStore();
const isOpen = computed(() => !authStore.isAuthenticated);
const isAuthenticating = ref(false);
const isCreatingPasskey = ref(false);
const authError = ref('');
const registrationError = ref('');
const authSuccess = ref(false);

const isPasskeySupported = computed(() => authStore.isWebAuthnSupported);
const hasRegisteredPasskey = computed(() => authStore.hasRegisteredPasskey);

async function createPasskey() {
  if (!isPasskeySupported.value) {
    registrationError.value = 'Your browser does not support passkeys';
    return;
  }
  try {
    isCreatingPasskey.value = true;
    registrationError.value = '';
    const success = await authStore.createPasskeyCredential();
    if (success) {
      await authenticateWithPasskey();
    } else {
      registrationError.value = 'Failed to create passkey';
    }
  } catch (error) {
    registrationError.value = error instanceof Error ? error.message : 'Failed to create passkey';
  } finally {
    isCreatingPasskey.value = false;
  }
}

async function authenticateWithPasskey() {
  try {
    isAuthenticating.value = true;
    authError.value = '';
    const success = await authStore.authenticateWithPasskey();
    if (success) {
      authSuccess.value = true;
      setTimeout(() => {
        authSuccess.value = false;
      }, 1400);
    } else {
      authError.value = 'Authentication failed';
    }
  } catch (error) {
    authError.value = error instanceof Error ? error.message : 'Authentication failed';
  } finally {
    isAuthenticating.value = false;
  }
}
</script>

<template>
  <Dialog :open="isOpen" @update:open="() => {}">
    <DialogContent class="max-w-md text-center" :show-close="false">
      <div class="flex flex-col items-center gap-4 py-2">
        <div class="flex h-20 w-20 items-center justify-center overflow-hidden rounded-2xl bg-black">
          <img
            src="/metasecret-gray.svg"
            alt="MetaSecret"
            width="80"
            height="80"
            class="h-full w-full object-contain"
          />
        </div>

        <DialogHeader class="space-y-2">
          <DialogTitle class="text-xl font-bold">
            {{ hasRegisteredPasskey ? 'Unlock MetaSecret' : 'Set up MetaSecret' }}
          </DialogTitle>
          <DialogDescription class="text-sm">
            <template v-if="!hasRegisteredPasskey"> Create a passkey to get started on this browser. </template>
            <template v-else>Use your passkey to continue on this browser.</template>
          </DialogDescription>
        </DialogHeader>

        <p v-if="authSuccess" class="text-sm font-semibold text-green-500">Authentication successful</p>
        <p v-if="authError" class="text-sm text-destructive">{{ authError }}</p>
        <p v-if="registrationError" class="text-sm text-destructive">{{ registrationError }}</p>

        <Button
          v-if="hasRegisteredPasskey"
          class="w-full"
          :disabled="isAuthenticating"
          @click="authenticateWithPasskey"
        >
          {{ isAuthenticating ? 'Authenticating...' : 'Authenticate with Passkey' }}
        </Button>
        <Button v-else class="w-full" :disabled="isCreatingPasskey || !isPasskeySupported" @click="createPasskey">
          {{ isCreatingPasskey ? 'Creating Passkey...' : 'Create Passkey' }}
        </Button>

        <p v-if="!isPasskeySupported" class="text-xs text-muted-foreground">
          Your browser does not support passkeys. Please use a modern browser with WebAuthn support.
        </p>
      </div>
    </DialogContent>
  </Dialog>
</template>
