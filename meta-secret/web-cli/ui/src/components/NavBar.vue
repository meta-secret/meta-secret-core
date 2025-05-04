<script setup>
import { Disclosure, DisclosureButton, DisclosurePanel } from '@headlessui/vue';
import { MenuIcon, XIcon, ChevronDownIcon } from '@heroicons/vue/outline';
import { ref, onMounted, onBeforeUnmount, watch, computed } from 'vue';
import { useRouter } from 'vue-router';
import ThemeToggle from './ThemeToggle.vue';
import { useThemeStore } from '../stores/theme';

const router = useRouter();
const dropdownOpen = ref(false);
const dropdownRef = ref(null);
const themeStore = useThemeStore();

// Compute whether dark mode is active
const isDarkMode = computed(() => {
  if (typeof document !== 'undefined') {
    return document.documentElement.classList.contains('dark');
  }
  return false;
});

// Watch for theme changes to ensure navbar updates
watch(() => themeStore.theme, () => {
  console.log('Theme changed in navbar component');
});

watch(isDarkMode, (newVal) => {
  console.log('Dark mode changed:', newVal);
});

const navigation = [
  { name: 'Home', href: '/', current: false },
  { name: 'GitHub', href: 'https://github.com/meta-secret', current: false },
  { name: 'Contact', href: '/contact', current: false },
];

const toolsMenu = [
  { name: 'Split', href: '/tools/split', external: false },
  { name: 'Recover', href: '/tools/recover', external: false },
  { name: 'Documentation', href: '/tools/docs', external: false },
  { name: 'Download', href: 'https://github.com/meta-secret/meta-secret-node/releases', external: true },
];

const toggleDropdown = () => {
  dropdownOpen.value = !dropdownOpen.value;
};

const closeDropdown = () => {
  dropdownOpen.value = false;
};

const handleItemClick = (path, isExternal) => {
  closeDropdown();
  if (isExternal) {
    window.open(path, '_blank');
  } else {
    router.push(path);
  }
};

const handleClickOutside = (event) => {
  if (dropdownRef.value && !dropdownRef.value.contains(event.target)) {
    closeDropdown();
  }
};

onMounted(() => {
  document.addEventListener('click', handleClickOutside);
});

onBeforeUnmount(() => {
  document.removeEventListener('click', handleClickOutside);
});
</script>

<template>
  <div :class="$style.navContainer">
    <Disclosure as="nav" 
      :class="[$style.navbar, isDarkMode ? 'dark-navbar' : 'light-navbar']" 
      :style="isDarkMode ? { backgroundColor: '#111827', color: 'white' } : {}"
      v-slot="{ open }">
      <div :class="$style.navInner">
        <div :class="$style.navFlex">
          <div :class="$style.logoSection">
            <div :class="$style.logoContainer">
              <img :class="$style.logo" src="/logo.png" alt="Workflow" />
              <div :class="$style.logoText">
                <RouterLink :class="$style.brandLink" to="/">Meta Secret</RouterLink>
              </div>
            </div>
            <div :class="$style.desktopMenu">
              <div :class="$style.menuItems">
                <a
                  v-for="item in navigation"
                  :key="item.name"
                  :href="item.href"
                  :class="[
                    item.current ? $style.activeNavItem : $style.navItem,
                  ]"
                  :aria-current="item.current ? 'page' : undefined"
                  >{{ item.name }}</a
                >

                <!-- Custom Tools dropdown menu -->
                <div :class="$style.dropdown" ref="dropdownRef">
                  <button
                    type="button"
                    @click.stop="toggleDropdown"
                    :class="$style.dropdownButton"
                  >
                    Tools
                    <ChevronDownIcon :class="$style.chevronIcon" aria-hidden="true" />
                  </button>

                  <div
                    v-if="dropdownOpen"
                    :class="$style.dropdownMenu"
                  >
                    <div :class="$style.dropdownMenuInner">
                      <a
                        v-for="item in toolsMenu"
                        :key="item.name"
                        @click.prevent="handleItemClick(item.href, item.external)"
                        href="#"
                        :class="$style.dropdownItem"
                      >
                        {{ item.name }}
                      </a>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div :class="$style.navRight">
            <ThemeToggle />
            <div :class="$style.mobileMenuButton">
              <!-- Mobile menu button -->
              <DisclosureButton
                :class="$style.disclosureBtn"
              >
                <span :class="$style.srOnly">Open main menu</span>
                <MenuIcon v-if="!open" :class="$style.menuIcon" aria-hidden="true" />
                <XIcon v-else :class="$style.menuIcon" aria-hidden="true" />
              </DisclosureButton>
            </div>
          </div>
        </div>
      </div>

      <DisclosurePanel :class="$style.mobilePanel">
        <div :class="$style.mobileMenuItems">
          <DisclosureButton
            v-for="item in navigation"
            :key="item.name"
            as="a"
            :href="item.href"
            :class="[
              item.current ? $style.activeMobileItem : $style.mobileNavItem,
            ]"
            :aria-current="item.current ? 'page' : undefined"
            >{{ item.name }}
          </DisclosureButton>

          <!-- Tools items in mobile menu -->
          <div :class="$style.mobileGroupLabel">Tools:</div>
          <DisclosureButton
            v-for="item in toolsMenu"
            :key="item.name"
            as="button"
            @click="handleItemClick(item.href, item.external)"
            :class="$style.mobileToolItem"
            >{{ item.name }}
          </DisclosureButton>
        </div>
      </DisclosurePanel>
    </Disclosure>
  </div>
</template>

<style module>
.navContainer {
  @apply min-h-full;
}

.navbar {
  @apply bg-white dark:!bg-gray-900 transition-colors duration-300;
}

.navInner {
  @apply max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 transition-colors duration-300;
}

.navFlex {
  @apply flex items-center justify-between h-16 transition-colors duration-300;
}

.logoSection {
  @apply flex items-center;
}

.logoContainer {
  @apply flex items-center flex-shrink-0;
}

.logo {
  @apply h-8 w-8;
}

.logoText {
  @apply px-6;
}

.brandLink {
  @apply text-gray-900 dark:text-white;
}

.desktopMenu {
  @apply hidden md:block;
}

.menuItems {
  @apply ml-10 flex items-baseline space-x-4;
}

.activeNavItem {
  @apply bg-gray-200 text-gray-900 dark:bg-gray-700 dark:text-white;
  @apply px-3 py-2 rounded-md text-sm font-medium;
}

.navItem {
  @apply text-orange-300 dark:text-gray-300;
  @apply hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-700 dark:hover:text-white;
  @apply px-3 py-2 rounded-md text-sm font-medium;
}

.dropdown {
  @apply relative inline-block text-left;
}

.dropdownButton {
  @apply text-orange-300 dark:text-gray-300;
  @apply hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-700 dark:hover:text-white;
  @apply px-3 py-2 rounded-md text-sm font-medium flex items-center;
}

.chevronIcon {
  @apply ml-1 h-4 w-4;
}

.dropdownMenu {
  @apply absolute z-10 mt-2 w-36 rounded-md shadow-lg;
  @apply bg-white dark:bg-gray-800;
  @apply ring-1 ring-black ring-opacity-5 focus:outline-none;
}

.dropdownMenuInner {
  @apply py-1;
}

.dropdownItem {
  @apply block px-4 py-2 text-sm;
  @apply text-gray-700 dark:text-gray-200;
  @apply hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-700 dark:hover:text-white;
  @apply cursor-pointer;
}

.navRight {
  @apply flex items-center;
}

.mobileMenuButton {
  @apply -mr-2 flex md:hidden;
}

.disclosureBtn {
  @apply inline-flex items-center justify-center p-2 rounded-md;
  @apply text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-gray-700;
  @apply focus:outline-none focus:ring-2 focus:ring-inset focus:ring-white;
}

.srOnly {
  @apply sr-only;
}

.menuIcon {
  @apply block h-6 w-6;
}

.mobilePanel {
  @apply md:hidden;
}

.mobileMenuItems {
  @apply px-2 pt-2 pb-3 space-y-1;
}

.activeMobileItem {
  @apply bg-gray-200 text-gray-900 dark:bg-gray-700 dark:text-white block px-3 py-2 rounded-md text-base font-medium;
}

.mobileNavItem {
  @apply text-gray-700 dark:text-gray-300 hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-700 dark:hover:text-white;
  @apply block px-3 py-2 rounded-md text-base font-medium;
}

.mobileGroupLabel {
  @apply text-gray-500 dark:text-gray-400 px-3 py-2 text-sm font-medium;
}

.mobileToolItem {
  @apply text-gray-700 dark:text-gray-300 hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-700 dark:hover:text-white w-full text-left;
  @apply block px-3 py-2 rounded-md text-base font-medium;
}
</style>

<style>
/* Global styles to ensure dark mode properly works */
:root.dark .navbar {
  background-color: #1a1a1a !important;
}

html.dark .navbar {
  background-color: #1a1a1a !important;
}

.dark .navbar {
  background-color: #1a1a1a !important;
}

.dark-navbar {
  background-color: #111827 !important; /* gray-900 */
  color: white !important;
}

.light-navbar {
  background-color: white !important;
  color: #111827 !important;
}
</style>