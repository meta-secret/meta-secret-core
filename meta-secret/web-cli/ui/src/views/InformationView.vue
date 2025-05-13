<script setup lang="ts">
import { useRouter } from 'vue-router';

const router = useRouter();

function goBack() {
  router.back();
}
</script>

<template>
  <div class="container mx-auto px-4 py-8 max-w-4xl">
    <div class="flex items-center mb-6">
      <button 
        @click="goBack"
        class="p-2 mr-4 bg-gray-100 hover:bg-gray-200 dark:bg-gray-800 dark:hover:bg-gray-700 rounded-full transition-colors"
        aria-label="Go back"
      >
        ←
      </button>
      <h1 class="text-2xl font-bold text-center flex-1">Meta Secret Documentation</h1>
    </div>
    
    <div class="grid grid-cols-1 lg:grid-cols-4 gap-6">
      <!-- Sidebar / Table of Contents -->
      <div class="lg:col-span-1 bg-gray-50 dark:bg-gray-800 p-4 rounded-lg">
        <nav class="sticky top-4">
          <p class="font-semibold mb-2 text-lg">Contents</p>
          <ul class="space-y-1">
            <li><a href="#overview" class="text-blue-600 hover:underline dark:text-blue-400">Overview</a></li>
            <li><a href="#how-it-works" class="text-blue-600 hover:underline dark:text-blue-400">How It Works</a></li>
            <li><a href="#app-structure" class="text-blue-600 hover:underline dark:text-blue-400">Application Structure</a></li>
            <li><a href="#password-split" class="text-blue-600 hover:underline dark:text-blue-400">Password Split</a></li>
            <li><a href="#password-recovery" class="text-blue-600 hover:underline dark:text-blue-400">Password Recovery</a></li>
            <li><a href="#usage" class="text-blue-600 hover:underline dark:text-blue-400">Usage Guide</a></li>
            <li><a href="#security" class="text-blue-600 hover:underline dark:text-blue-400">Security Aspects</a></li>
            <li><a href="#resources" class="text-blue-600 hover:underline dark:text-blue-400">Additional Resources</a></li>
          </ul>
        </nav>
      </div>
      
      <!-- Main Content -->
      <div class="lg:col-span-3">
        <section id="overview" class="mb-8 p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">Overview</h2>
          <p class="mb-3">
            Meta Secret is a decentralized password manager that uses advanced encryption and decentralized storage to 
            securely store and manage user data.
          </p>
          <p class="mb-3">
            Unlike traditional password managers, Meta Secret does not rely on a master password to grant access to your passwords.
            Instead, it uses a combination of biometric authentication and secret sharing techniques to provide secure access 
            to your confidential information.
          </p>
          <p>
            With its decentralized and open-source infrastructure, Meta Secret eliminates single points of failure and provides 
            increased security and privacy for users.
          </p>
        </section>
        
        <section id="how-it-works" class="mb-8 p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">How It Works</h2>
          <p class="mb-4">
            Meta Secret operates on a principle of cryptographic secret sharing. When you store a password:
          </p>
          <ol class="list-decimal ml-6 mb-4 space-y-2">
            <li>Your password is split into multiple fragments using a cryptographic algorithm</li>
            <li>Each fragment is encrypted individually</li>
            <li>Fragments are distributed and stored across multiple devices</li>
            <li>No single device has enough information to recover the password on its own</li>
          </ol>
          <p>
            To recover a password, Meta Secret gathers enough fragments from your devices and reconstructs the original secret.
          </p>
        </section>
        
        <section id="app-structure" class="mb-8 p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">Application Structure</h2>
          <div class="flex justify-center">
            <img src="https://github.com/meta-secret/meta-secret-core/raw/main/docs/img/app/meta-secret-app.png" 
                 alt="Meta Secret Application Structure" 
                 class="border rounded shadow-sm max-w-full" />
          </div>
        </section>
        
        <section id="password-split" class="mb-8 p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">Password Split Process</h2>
          <p class="mb-4">
            When you enter a password, Meta Secret splits it into multiple encrypted fragments and distributes
            them across your devices. Each device only stores a fragment of the password and cannot reconstruct
            the original password without the other fragments.
          </p>
          <div class="flex justify-center">
            <img src="https://github.com/meta-secret/meta-secret-core/raw/main/docs/img/app/secret-split.png" 
                 alt="Password Split Process" 
                 class="border rounded shadow-sm max-w-full" />
          </div>
        </section>
        
        <section id="password-recovery" class="mb-8 p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">Password Recovery Process</h2>
          <p class="mb-4">
            To recover your password, Meta Secret retrieves the fragments from your devices and reconstructs
            the original password. Even if one device is lost, the password can still be recovered from the
            remaining devices due to partial data redundancy.
          </p>
          <div class="flex justify-center">
            <img src="https://github.com/meta-secret/meta-secret-core/raw/main/docs/img/app/secret-recovery.png" 
                 alt="Password Recovery Process" 
                 class="border rounded shadow-sm max-w-full" />
          </div>
        </section>
        
        <section id="usage" class="mb-8 p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">Usage Guide</h2>
          
          <div class="mb-6">
            <h3 class="text-lg font-medium mb-3">Splitting a Password</h3>
            <ol class="list-decimal ml-6 space-y-2">
              <li>Navigate to the <strong>Split</strong> page</li>
              <li>Enter your password in the designated field</li>
              <li>Add any notes or context if needed</li>
              <li>Click the <strong>Split</strong> button to generate QR codes (password shares)</li>
              <li>Save each QR code: right-click on the QR code and select <strong>Save Image As...</strong></li>
              <li>Store each QR code on a different device for maximum security</li>
            </ol>
          </div>
          
          <div class="mb-6">
            <h3 class="text-lg font-medium mb-3">Recovering a Password</h3>
            <ol class="list-decimal ml-6 space-y-2">
              <li>Collect at least the minimum required number of QR codes (default is 2)</li>
              <li>Navigate to the <strong>Recover</strong> page</li>
              <li>Click the <strong>Choose files</strong> button and select your saved QR codes</li>
              <li>Click the <strong>Recover</strong> button to reconstruct your original password</li>
              <li>Your password will be displayed in the password field</li>
            </ol>
          </div>
        </section>
        
        <section id="security" class="mb-8 p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">Security Aspects</h2>
          <ul class="list-disc ml-6 space-y-3">
            <li>
              <strong>Local Processing</strong>: All operations are performed locally on your device - no data is sent to external servers
            </li>
            <li>
              <strong>No Single Point of Failure</strong>: Data is distributed across multiple devices, eliminating central points of vulnerability
            </li>
            <li>
              <strong>Cryptographic Security</strong>: Uses advanced cryptographic algorithms for splitting and encrypting data
            </li>
            <li>
              <strong>Partial Data Redundancy</strong>: Even if one device is lost, your passwords can still be recovered
            </li>
            <li>
              <strong>Open Source</strong>: The code is open source, allowing for security verification and community review
            </li>
            <li>
              <strong>Fragment Security</strong>: Individual fragments are useless on their own - an attacker needs multiple fragments to recover any password
            </li>
          </ul>
        </section>
        
        <section id="resources" class="p-6 bg-white dark:bg-gray-900 rounded-lg shadow-sm">
          <h2 class="text-xl font-semibold mb-4 pb-2 border-b dark:border-gray-700">Additional Resources</h2>
          <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
            <a href="https://apps.apple.com/app/metasecret/id1644286751" 
               class="p-4 border rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 text-center transition-colors"
               target="_blank" rel="noopener">
               <span class="block mb-2 text-lg">📱</span>
               <span class="text-blue-600 dark:text-blue-400">Mobile App</span>
            </a>
            <a href="https://meta-secret.org" 
               class="p-4 border rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 text-center transition-colors"
               target="_blank" rel="noopener">
               <span class="block mb-2 text-lg">🌐</span>
               <span class="text-blue-600 dark:text-blue-400">Official Website</span>
            </a>
            <a href="https://github.com/meta-secret/meta-secret-core" 
               class="p-4 border rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 text-center transition-colors"
               target="_blank" rel="noopener">
               <span class="block mb-2 text-lg">📚</span>
               <span class="text-blue-600 dark:text-blue-400">GitHub Repository</span>
            </a>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
img {
  max-width: 100%;
  height: auto;
}
</style> 