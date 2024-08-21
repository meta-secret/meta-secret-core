<script>

</script>

<template>

  <div class="container flex flex-col items-center justify-center">
    <h2 class="text-2xl">Documentation</h2>
  </div>

  <div class="container flex flex-col items-center justify-center py-4 px-4">
    <div class="container flex flex-col" style="max-width: 800px;">
      <p class="text-lg">Split Password:</p>
      <ul class="list-disc px-8">
        <li>Go to "Split" page - <RouterLink class="font-bold" to="/split">click on "Split" button on this page</RouterLink></li>
        <li>Type your password and notes</li>
        <li>Click on "split" button, which generates qr codes (password shares)</li>
        <li>Save qr codes on you computer: right click on each qr code and then click "Save Image As..."</li>
        <li>Keep Pieces of the secret on different devices</li>
      </ul>
    </div>

    <div class="py-4"></div>

    <div class="container flex flex-col" style="max-width: 800px;">
      <p class="text-lg">Restore Password:</p>
      <ul class="list-disc px-8">
        <li>Gather minimum number of qr images (2 in our case) on your computer</li>
        <li>Go to "Recover" page</li>
        <li>Click on "Choose files"</li>
        <li>Click on "Recover" button, it will recover the password from qr codes and shows in a "password" box</li>
      </ul>
    </div>

    <div class="py-6"></div>

    <div class="container flex flex-col" style="max-width: 800px;">
      <p class="text-lg">Security Aspect:</p>
      <ul class="list-disc px-8">
        <li>
          <p>This site is a simple HTML page that run only on your local computer, so it's completely secure to use
            it</p>
          <p>There is no any other 3-rd parties involved</p>
        </li>
        <li>
          <a href="https://github.com/meta-secret/meta-secret-node">Open Source</a>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>

</style>