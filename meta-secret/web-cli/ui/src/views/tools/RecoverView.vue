<script lang="ts">
import init, { restore_password } from 'meta-secret-web-cli';
import QrScanner from 'qr-scanner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent } from '@/components/ui/card';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Separator } from '@/components/ui/separator';

export default {
  components: { Button, Input, Label, Card, CardContent, Alert, AlertDescription, Separator },
  data() {
    return {
      recoveredPassword: '',
      imagesLoaded: 0,
      error: '',
    };
  },
  methods: {
    recoverPassword() {
      this.error = '';
      if (this.imagesLoaded === 0) {
        this.error = 'Please upload at least one QR code image.';
        return;
      }
      init().then(() => {
        const imagesElement = document.getElementById('qrImages')!;
        const qrCodes = imagesElement.getElementsByTagName('img');
        if (qrCodes.length === 0) {
          this.error = 'Could not find any QR codes.';
          return;
        }
        const asyncShares: Promise<{ data: string }>[] = [];
        Array.from(qrCodes).forEach((qr) => {
          asyncShares.push(QrScanner.scanImage(qr, { returnDetailedScanResult: true }));
        });
        Promise.all(asyncShares)
          .then((qrShares) => {
            const shares = qrShares.map((s) => JSON.parse(s.data));
            this.recoveredPassword = restore_password(shares) as string;
          })
          .catch((err) => {
            this.error = 'Unable to process QR codes: ' + err;
          });
      });
    },
    openFile(event: Event) {
      const input = event.target as HTMLInputElement;
      const imagesElement = document.getElementById('qrImages')!;
      imagesElement.innerHTML = '';
      this.imagesLoaded = 0;
      this.error = '';
      if (!input.files?.length) return;
      Array.from(input.files).forEach((file) => {
        const reader = new FileReader();
        reader.onload = () => {
          const img = document.createElement('img');
          img.className = 'qr-image';
          img.src = reader.result as string;
          img.onload = () => {
            this.imagesLoaded++;
          };
          imagesElement.appendChild(img);
        };
        reader.readAsDataURL(file);
      });
    },
  },
};
</script>

<template>
  <div class="mx-auto max-w-2xl px-4 py-10">
    <div class="mb-8 text-center">
      <h1 class="text-3xl font-bold">Password Recovery</h1>
      <p class="mt-2 text-muted-foreground">Upload your QR code shares to retrieve your password.</p>
    </div>

    <Card class="mb-8">
      <CardContent class="flex flex-col gap-5 pt-6">
        <div class="space-y-1.5">
          <Label for="file-upload">Your QR Codes</Label>
          <Input id="file-upload" type="file" accept="image/*" multiple class="cursor-pointer" @change="openFile" />
          <p v-if="imagesLoaded > 0" class="text-xs text-green-600">
            {{ imagesLoaded }} {{ imagesLoaded === 1 ? 'image' : 'images' }} loaded
          </p>
        </div>

        <Button class="w-full" @click="recoverPassword">🔑 Retrieve Password</Button>

        <Alert v-if="error" variant="destructive">
          <AlertDescription>{{ error }}</AlertDescription>
        </Alert>

        <template v-if="recoveredPassword">
          <Separator />
          <div class="space-y-1.5">
            <Label>Your Password</Label>
            <Input :model-value="recoveredPassword" readonly />
          </div>
        </template>
      </CardContent>
    </Card>

    <div id="qrImages" class="flex flex-wrap justify-center gap-4"></div>
  </div>
</template>

<style>
.qr-image {
  display: inline-block;
  margin: 8px;
}
</style>
