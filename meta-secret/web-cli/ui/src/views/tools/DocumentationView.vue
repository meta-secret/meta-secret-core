<script setup lang="ts">
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
</script>

<template>
  <div class="mx-auto max-w-2xl px-4 py-10">
    <div class="mb-8 text-center">
      <h1 class="text-3xl font-bold">Documentation</h1>
      <p class="mt-2 text-muted-foreground">How to use the Split and Recover tools.</p>
    </div>

    <div class="flex flex-col gap-6">
      <Card>
        <CardHeader>
          <CardTitle>Split Password</CardTitle>
        </CardHeader>
        <Separator />
        <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground">
          <ul class="ml-5 list-disc space-y-2">
            <li>
              Go to the
              <RouterLink class="font-semibold text-foreground underline-offset-4 hover:underline" to="/tools/split"
                >Split</RouterLink
              >
              page and enter your password and notes.
            </li>
            <li>
              Click <span class="font-medium text-foreground">Split</span> to generate QR codes (password shares).
            </li>
            <li>Save each QR code: right-click the image and choose “Save Image As…”.</li>
            <li>Store shares on different devices for safety.</li>
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Restore Password</CardTitle>
        </CardHeader>
        <Separator />
        <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground">
          <ul class="ml-5 list-disc space-y-2">
            <li>Gather at least two QR code images on your computer.</li>
            <li>
              Open the
              <RouterLink class="font-semibold text-foreground underline-offset-4 hover:underline" to="/tools/recover"
                >Recover</RouterLink
              >
              page.
            </li>
            <li>Click “Choose files” and select your QR images.</li>
            <li>Click <span class="font-medium text-foreground">Retrieve Password</span> to recover the secret.</li>
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Security</CardTitle>
        </CardHeader>
        <Separator />
        <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground space-y-3">
          <p>
            These tools run entirely in your browser. Your password and shares are not sent to a third-party server
            during split or recover.
          </p>
          <p>
            Meta Secret is
            <a
              class="font-semibold text-foreground underline-offset-4 hover:underline"
              href="https://github.com/meta-secret/meta-secret-node"
              target="_blank"
              rel="noopener noreferrer"
              >open source</a
            >.
          </p>
        </CardContent>
      </Card>
    </div>
  </div>
</template>
