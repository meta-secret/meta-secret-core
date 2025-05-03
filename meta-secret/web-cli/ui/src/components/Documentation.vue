<script>
import { defineComponent } from 'vue';

export default defineComponent({});
</script>

<template>
  <div :class="$style.docContainer">
    <h2 :class="$style.title">Documentation</h2>
  </div>

  <div :class="$style.contentWrapper">
    <div :class="$style.contentBox">
      <p :class="$style.sectionTitle">Split Password:</p>
      <ul :class="$style.list">
        <li>Go to "Split" page - <RouterLink :class="$style.navLink" to="/split">click on "Split" button on this page</RouterLink></li>
        <li>Type your password and notes</li>
        <li>Click on "split" button, which generates qr codes (password shares)</li>
        <li>Save qr codes on you computer: right click on each qr code and then click "Save Image As..."</li>
        <li>Keep Pieces of the secret on different devices</li>
      </ul>
    </div>

    <div :class="$style.spacer"></div>

    <div :class="$style.contentBox">
      <p :class="$style.sectionTitle">Restore Password:</p>
      <ul :class="$style.list">
        <li>Gather minimum number of qr images (2 in our case) on your computer</li>
        <li>Go to "Recover" page</li>
        <li>Click on "Choose files"</li>
        <li>Click on "Recover" button, it will recover the password from qr codes and shows in a "password" box</li>
      </ul>
    </div>

    <div :class="$style.spacerLarge"></div>

    <div :class="$style.contentBox">
      <p :class="$style.sectionTitle">Security Aspect:</p>
      <ul :class="$style.list">
        <li>
          <p>This site is a simple HTML page that run only on your local computer, so it's completely secure to use
            it</p>
          <p>There is no any other 3-rd parties involved</p>
        </li>
        <li>
          <a :class="$style.link" href="https://github.com/meta-secret/meta-secret-node">Open Source</a>
        </li>
      </ul>
    </div>
  </div>
</template>

<style module>
.docContainer {
  @apply flex flex-col items-center justify-center;
  @apply container;
}

.title {
  @apply text-2xl;
}

.contentWrapper {
  @apply flex flex-col items-center justify-center py-4 px-4;
  @apply container;
}

.contentBox {
  @apply flex flex-col;
  max-width: 800px;
}

.sectionTitle {
  @apply text-lg;
}

.list {
  @apply list-disc px-8;
}

.navLink {
  @apply font-bold;
}

.link {
  @apply text-blue-500 hover:text-blue-700 underline;
}

.spacer {
  @apply py-4;
}

.spacerLarge {
  @apply py-6;
}
</style>