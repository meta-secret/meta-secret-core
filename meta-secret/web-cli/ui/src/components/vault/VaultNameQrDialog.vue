<script setup lang="ts">
import VaultNameQr from '@/components/vault/VaultNameQr.vue';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { vaultDevices } from '@/locales/en';

defineProps<{ open: boolean; vaultName: string }>();
const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <Dialog
    :open="open && !!vaultName"
    @update:open="
      (isOpen) => {
        if (!isOpen) emit('close');
      }
    "
  >
    <DialogContent v-if="open && vaultName" class="max-h-[calc(100dvh-2rem)] overflow-y-auto">
      <DialogHeader>
        <DialogTitle>{{ vaultDevices.addDevice }}</DialogTitle>
        <DialogDescription>{{ vaultDevices.qrDescription }}</DialogDescription>
      </DialogHeader>
      <VaultNameQr :vault-name="vaultName" />
      <p class="min-w-0 break-all text-center font-medium" data-testid="vault-qr-name">{{ vaultName }}</p>
      <DialogFooter>
        <Button data-testid="close-vault-qr" variant="outline" @click="emit('close')">{{
          vaultDevices.qrClose
        }}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
