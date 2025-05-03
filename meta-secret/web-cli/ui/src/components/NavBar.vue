<script setup>
import { Disclosure, DisclosureButton, DisclosurePanel } from '@headlessui/vue';
import { MenuIcon, XIcon, ChevronDownIcon } from '@heroicons/vue/outline';
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { useRouter } from 'vue-router';
import ThemeToggle from './ThemeToggle.vue';

const router = useRouter();
const dropdownOpen = ref(false);
const dropdownRef = ref(null);

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
  <div class="min-h-full nav-style">
    <Disclosure as="nav" class="bg-gray-50 dark:bg-gray-800" v-slot="{ open }">
      <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
        <div class="flex items-center justify-between h-16">
          <div class="flex items-center">
            <div class="flex items-center flex-shrink-0">
              <img class="h-8 w-8" src="/logo.png" alt="Workflow" />
              <div class="px-6">
                <RouterLink class="dark:text-white" to="/">Meta Secret</RouterLink>
              </div>
            </div>
            <div class="hidden md:block">
              <div class="ml-10 flex items-baseline space-x-4">
                <a
                  v-for="item in navigation"
                  :key="item.name"
                  :href="item.href"
                  :class="[
                    item.current ? 'bg-gray-300 text-black dark:bg-gray-700 dark:text-white' : 'text-gray-900 dark:text-gray-300 hover:bg-gray-300 hover:text-black dark:hover:bg-gray-700 dark:hover:text-white',
                    'px-3 py-2 rounded-md text-sm font-medium',
                  ]"
                  :aria-current="item.current ? 'page' : undefined"
                  >{{ item.name }}</a
                >

                <!-- Custom Tools dropdown menu -->
                <div class="relative inline-block text-left" ref="dropdownRef">
                  <button
                    type="button"
                    @click.stop="toggleDropdown"
                    class="text-gray-900 dark:text-gray-300 hover:bg-gray-300 hover:text-black dark:hover:bg-gray-700 dark:hover:text-white px-3 py-2 rounded-md text-sm font-medium flex items-center"
                  >
                    Tools
                    <ChevronDownIcon class="ml-1 h-4 w-4" aria-hidden="true" />
                  </button>

                  <div
                    v-if="dropdownOpen"
                    class="absolute z-10 mt-2 w-36 rounded-md shadow-lg bg-white dark:bg-gray-700 ring-1 ring-black ring-opacity-5 focus:outline-none"
                  >
                    <div class="py-1">
                      <a
                        v-for="item in toolsMenu"
                        :key="item.name"
                        @click.prevent="handleItemClick(item.href, item.external)"
                        href="#"
                        class="block px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 hover:text-gray-900 dark:hover:bg-gray-600 dark:hover:text-white cursor-pointer"
                      >
                        {{ item.name }}
                      </a>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div class="flex items-center">
            <ThemeToggle />
            <div class="-mr-2 flex md:hidden ml-2">
              <!-- Mobile menu button -->
              <DisclosureButton
                class="bg-gray-800 inline-flex items-center justify-center p-2 rounded-md text-gray-400 hover:text-black dark:hover:text-white hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-gray-800 focus:ring-white"
              >
                <span class="sr-only">Open main menu</span>
                <MenuIcon v-if="!open" class="block h-6 w-6" aria-hidden="true" />
                <XIcon v-else class="block h-6 w-6" aria-hidden="true" />
              </DisclosureButton>
            </div>
          </div>
        </div>
      </div>

      <DisclosurePanel class="md:hidden">
        <div class="px-2 pt-2 pb-3 space-y-1 sm:px-3">
          <DisclosureButton
            v-for="item in navigation"
            :key="item.name"
            as="a"
            :href="item.href"
            :class="[
              item.current ? 'bg-gray-900 text-black dark:text-white' : 'text-gray-800 dark:text-gray-300 hover:bg-gray-300 hover:text-black dark:hover:bg-gray-700 dark:hover:text-white',
              'block px-3 py-2 rounded-md text-base font-medium',
            ]"
            :aria-current="item.current ? 'page' : undefined"
            >{{ item.name }}
          </DisclosureButton>

          <!-- Tools items in mobile menu -->
          <div class="mt-1 px-3 text-gray-800 dark:text-gray-300 font-medium">Tools:</div>
          <DisclosureButton
            v-for="item in toolsMenu"
            :key="item.name"
            as="button"
            @click="handleItemClick(item.href, item.external)"
            class="text-gray-700 dark:text-gray-300 hover:bg-gray-300 hover:text-black dark:hover:bg-gray-700 dark:hover:text-white block px-3 py-2 rounded-md text-base font-medium pl-6 text-left w-full"
            >{{ item.name }}
          </DisclosureButton>
        </div>
      </DisclosurePanel>
    </Disclosure>
  </div>
</template>

<style>
.nav-style {
  border-bottom: solid 1px;
  border-bottom-color: #3c3c3c3b;
}
</style>