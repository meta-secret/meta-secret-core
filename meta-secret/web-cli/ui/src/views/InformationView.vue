<script setup lang="ts">
import { useRouter } from 'vue-router';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { ArrowLeft } from 'lucide-vue-next';

const router = useRouter();

const sections = [
  { id: 'overview', label: 'Overview' },
  { id: 'how-it-works', label: 'How It Works' },
  { id: 'app-structure', label: 'Application Structure' },
  { id: 'password-split', label: 'Password Split' },
  { id: 'password-recovery', label: 'Password Recovery' },
  { id: 'usage', label: 'Usage Guide' },
  { id: 'security', label: 'Security Aspects' },
  { id: 'resources', label: 'Additional Resources' },
];
</script>

<template>
  <div class="mx-auto max-w-5xl px-4 py-8">
    <div class="mb-6 flex items-center gap-3">
      <Button variant="ghost" size="icon" aria-label="Go back" @click="router.back()">
        <ArrowLeft class="h-4 w-4" />
      </Button>
      <h1 class="flex-1 text-center text-2xl font-bold">Meta Secret Documentation</h1>
    </div>

    <div class="grid grid-cols-1 gap-6 lg:grid-cols-4">
      <!-- Sidebar TOC -->
      <div class="lg:col-span-1">
        <Card class="sticky top-20">
          <CardContent class="pt-4">
            <p class="mb-2 text-sm font-semibold uppercase tracking-widest text-muted-foreground">Contents</p>
            <nav class="flex flex-col gap-1">
              <a
                v-for="s in sections"
                :key="s.id"
                :href="`#${s.id}`"
                class="text-sm text-primary hover:underline"
              >{{ s.label }}</a>
            </nav>
          </CardContent>
        </Card>
      </div>

      <!-- Main content -->
      <div class="flex flex-col gap-6 lg:col-span-3">
        <Card id="overview">
          <CardHeader><CardTitle>Overview</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground space-y-3">
            <p>Meta Secret is a decentralized password manager that uses advanced encryption and decentralized storage to securely store and manage user data.</p>
            <p>Unlike traditional password managers, Meta Secret does not rely on a master password. Instead, it uses biometric authentication and secret sharing techniques to provide secure access to your confidential information.</p>
            <p>With its decentralized and open-source infrastructure, Meta Secret eliminates single points of failure and provides increased security and privacy for users.</p>
          </CardContent>
        </Card>

        <Card id="how-it-works">
          <CardHeader><CardTitle>How It Works</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground space-y-3">
            <p>Meta Secret operates on a principle of cryptographic secret sharing. When you store a password:</p>
            <ol class="ml-5 list-decimal space-y-1">
              <li>Your password is split into multiple fragments using a cryptographic algorithm</li>
              <li>Each fragment is encrypted individually</li>
              <li>Fragments are distributed and stored across multiple devices</li>
              <li>No single device has enough information to recover the password on its own</li>
            </ol>
            <p>To recover a password, Meta Secret gathers enough fragments from your devices and reconstructs the original secret.</p>
          </CardContent>
        </Card>

        <Card id="app-structure">
          <CardHeader><CardTitle>Application Structure</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4">
            <img src="https://github.com/meta-secret/meta-secret-core/raw/main/docs/img/app/meta-secret-app.png" alt="Application Structure" class="mx-auto max-w-full rounded border" />
          </CardContent>
        </Card>

        <Card id="password-split">
          <CardHeader><CardTitle>Password Split Process</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground space-y-3">
            <p>When you enter a password, Meta Secret splits it into multiple encrypted fragments and distributes them across your devices.</p>
            <img src="https://github.com/meta-secret/meta-secret-core/raw/main/docs/img/app/secret-split.png" alt="Password Split Process" class="mx-auto max-w-full rounded border" />
          </CardContent>
        </Card>

        <Card id="password-recovery">
          <CardHeader><CardTitle>Password Recovery Process</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground space-y-3">
            <p>To recover your password, Meta Secret retrieves fragments from your devices and reconstructs the original password. Even if one device is lost, the password can still be recovered from the remaining devices.</p>
            <img src="https://github.com/meta-secret/meta-secret-core/raw/main/docs/img/app/secret-recovery.png" alt="Password Recovery Process" class="mx-auto max-w-full rounded border" />
          </CardContent>
        </Card>

        <Card id="usage">
          <CardHeader><CardTitle>Usage Guide</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground space-y-6">
            <div>
              <h3 class="mb-2 font-medium text-foreground">Splitting a Password</h3>
              <ol class="ml-5 list-decimal space-y-1">
                <li>Navigate to the <strong>Split</strong> page</li>
                <li>Enter your password in the designated field</li>
                <li>Add any notes or context if needed</li>
                <li>Click <strong>Split Password</strong> to generate QR codes</li>
                <li>Save each QR code on a different device for maximum security</li>
              </ol>
            </div>
            <div>
              <h3 class="mb-2 font-medium text-foreground">Recovering a Password</h3>
              <ol class="ml-5 list-decimal space-y-1">
                <li>Collect at least the minimum required number of QR codes (default is 2)</li>
                <li>Navigate to the <strong>Recover</strong> page</li>
                <li>Upload your saved QR codes</li>
                <li>Click <strong>Retrieve Password</strong> to reconstruct your original password</li>
              </ol>
            </div>
          </CardContent>
        </Card>

        <Card id="security">
          <CardHeader><CardTitle>Security Aspects</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4 text-sm leading-relaxed text-muted-foreground">
            <ul class="ml-5 list-disc space-y-2">
              <li><strong>Local Processing</strong>: All operations are performed locally — no data sent to external servers</li>
              <li><strong>No Single Point of Failure</strong>: Data is distributed across multiple devices</li>
              <li><strong>Cryptographic Security</strong>: Uses advanced algorithms for splitting and encrypting data</li>
              <li><strong>Partial Data Redundancy</strong>: Passwords can still be recovered even if one device is lost</li>
              <li><strong>Open Source</strong>: Code is open source for security verification and community review</li>
              <li><strong>Fragment Security</strong>: Individual fragments are useless without the others</li>
            </ul>
          </CardContent>
        </Card>

        <Card id="resources">
          <CardHeader><CardTitle>Additional Resources</CardTitle></CardHeader>
          <Separator />
          <CardContent class="pt-4">
            <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
              <a
                v-for="link in [
                  { emoji: '📱', label: 'Mobile App', href: 'https://apps.apple.com/app/metasecret/id1644286751' },
                  { emoji: '🌐', label: 'Official Website', href: 'https://meta-secret.org' },
                  { emoji: '📚', label: 'GitHub Repository', href: 'https://github.com/meta-secret/meta-secret-core' },
                ]"
                :key="link.label"
                :href="link.href"
                target="_blank"
                rel="noopener"
                class="flex flex-col items-center gap-1 rounded-lg border p-4 text-center transition-colors hover:bg-muted"
              >
                <span class="text-xl">{{ link.emoji }}</span>
                <span class="text-sm text-primary">{{ link.label }}</span>
              </a>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  </div>
</template>
