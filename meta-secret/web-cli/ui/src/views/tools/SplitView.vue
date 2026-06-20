<script lang="ts">
import QRCodeStyling from 'qr-code-styling';
import init, { split } from 'meta-secret-web-cli';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent } from '@/components/ui/card';

export default {
  components: { Button, Input, Label, Card, CardContent },
  data() {
    return {
      password: 'top$ecret',
      note1: '',
      note2: '',
    };
  },
  methods: {
    splitPassword() {
      init().then(async () => {
        const qrImages = document.getElementById('qr-images')!;
        while (qrImages.firstChild) qrImages.removeChild(qrImages.firstChild);
        const shares = split(this.password);
        this.sharesProcessing(shares, qrImages);
      });
    },
    sharesProcessing(shares: any[], qrImages: HTMLElement) {
      shares.forEach((share) => {
        const shareId = share['share_id'];
        const shareIdText = `Share ${shareId} of ${shares.length}`;
        const textImage = this.textToImage(this.note1, this.note2, shareIdText, shareId);
        const qrCodeStyling = this.generateQrCodeStyling(JSON.stringify(share), textImage);

        const canvasDiv = document.createElement('div');
        canvasDiv.id = `qrCanvas${shareId}`;
        canvasDiv.className = 'qr-canvas-container';
        qrCodeStyling.append(canvasDiv);
        qrImages.appendChild(canvasDiv);

        const downloadBtn = document.createElement('button');
        downloadBtn.onclick = () => qrCodeStyling.download({ name: `qr${shareId}`, extension: 'png' });
        downloadBtn.innerHTML = 'Save QR Code';
        downloadBtn.className = 'download-button';
        canvasDiv.appendChild(downloadBtn);
      });
    },
    textToImage(line1: string, line2: string, line3: string, id: number) {
      const canvas = document.createElement('canvas');
      canvas.width = 300; canvas.height = 300;
      const ctx = canvas.getContext('2d')!;
      ctx.font = '70px Arial';
      ctx.fillText(line1, 15, 75);
      ctx.fillText(line2, 15, 150);
      ctx.fillText(line3, 15, 250);
      return canvas.toDataURL();
    },
    generateQrCodeStyling(share: string, textImage: string) {
      return new QRCodeStyling({
        width: 300, height: 300, type: 'svg' as any, data: share, margin: 3,
        qrOptions: { typeNumber: 0, mode: 'Byte', errorCorrectionLevel: 'H' },
        imageOptions: { hideBackgroundDots: true, imageSize: 0.2, margin: 1 },
        dotsOptions: { type: 'dots', color: '#000000' },
        backgroundOptions: { color: '#ffffff' },
        image: textImage,
        cornersSquareOptions: { type: 'square', color: '#000000' },
      });
    },
  },
};
</script>

<template>
  <div class="mx-auto max-w-2xl px-4 py-10">
    <div class="mb-8 text-center">
      <h1 class="text-3xl font-bold">Create Password Shares</h1>
      <p class="mt-2 text-muted-foreground">Enter your password and optional notes to create secure QR code shares.</p>
    </div>

    <Card class="mb-8">
      <CardContent class="flex flex-col gap-5 pt-6">
        <div class="space-y-1.5">
          <Label for="note1">Label 1</Label>
          <Input id="note1" v-model="note1" placeholder="Short label for your QR code (optional)" maxlength="10" />
        </div>
        <div class="space-y-1.5">
          <Label for="note2">Label 2</Label>
          <Input id="note2" v-model="note2" placeholder="Additional label (optional)" maxlength="10" />
        </div>
        <div class="space-y-1.5">
          <Label for="password">Your Password</Label>
          <Input id="password" v-model="password" placeholder="Enter your password" />
        </div>
        <Button class="w-full" @click="splitPassword">🔒 Split Password</Button>
      </CardContent>
    </Card>

    <div id="qr-images" class="flex flex-wrap justify-center gap-6"></div>
  </div>
</template>

<style>
.qr-canvas-container {
  background: white;
  border-radius: 0.75rem;
  padding: 1.5rem;
  box-shadow: 0 4px 6px -1px rgba(0,0,0,0.1);
  display: flex;
  flex-direction: column;
  align-items: center;
}
.download-button {
  margin-top: 1rem;
  padding: 0.5rem 1rem;
  background: hsl(var(--primary));
  color: hsl(var(--primary-foreground));
  border: none;
  border-radius: 0.375rem;
  font-weight: 500;
  font-size: 0.875rem;
  cursor: pointer;
}
.download-button:hover { opacity: 0.9; }
</style>
