<script setup lang="ts">
import { Disclosure, DisclosureButton, DisclosurePanel } from '@headlessui/vue';
import { Bars3Icon, XMarkIcon, ChevronDownIcon, Cog6ToothIcon } from '@heroicons/vue/24/outline';
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { useRoute, useRouter } from 'vue-router';

const route = useRoute();
const router = useRouter();
const dropdownOpen = ref(false);
const dropdownRef = ref<HTMLElement | null>(null);

const navigation = [
  { name: 'Home', href: '/' },
  { name: 'GitHub', href: 'https://github.com/meta-secret', external: true },
  { name: 'Contact', href: '/contact' },
];

const toolsMenu = [
  { name: 'Split', href: '/tools/split', external: false },
  { name: 'Recover', href: '/tools/recover', external: false },
  { name: 'Documentation', href: '/tools/docs', external: false },
  { name: 'Download', href: 'https://github.com/meta-secret/meta-secret-node/releases', external: true },
];

const isActive = (href: string) => !href.startsWith('http') && route.path === href;

const openLink = (path: string, external?: boolean) => {
  if (external) {
    window.open(path, '_blank');
    return;
  }
  router.push(path);
};

const toggleDropdown = () => {
  dropdownOpen.value = !dropdownOpen.value;
};

const closeDropdown = () => {
  dropdownOpen.value = false;
};

const handleClickOutside = (event: MouseEvent) => {
  if (dropdownRef.value && !dropdownRef.value.contains(event.target as Node)) {
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
  <Disclosure v-slot="{ open }" as="nav" class="top-nav">
    <div class="nav-inner">
      <div class="logo-block" @click="router.push('/')">
        <img class="logo" src="/logo.png" alt="Meta Secret" />
        <span class="brand">Meta Secret</span>
      </div>

      <div class="desktop-menu">
        <button
          v-for="item in navigation"
          :key="item.name"
          class="nav-link"
          :class="{ active: isActive(item.href) }"
          @click="openLink(item.href, (item as any).external)"
        >
          {{ item.name }}
        </button>

        <div ref="dropdownRef" class="dropdown">
          <button class="nav-link" @click.stop="toggleDropdown">
            Tools
            <ChevronDownIcon class="chevron" aria-hidden="true" />
          </button>
          <div v-if="dropdownOpen" class="dropdown-menu">
            <button
              v-for="item in toolsMenu"
              :key="item.name"
              class="dropdown-item"
              @click="
                closeDropdown();
                openLink(item.href, item.external);
              "
            >
              {{ item.name }}
            </button>
          </div>
        </div>
      </div>

      <div class="right-slot">
        <span class="alpha-badge">alpha</span>
        <button class="settings-btn" aria-label="Settings" @click="router.push('/settings')">
          <Cog6ToothIcon class="settings-icon" aria-hidden="true" />
        </button>
      </div>

      <div class="mobile-menu-button">
        <DisclosureButton class="disclosure-btn">
          <Bars3Icon v-if="!open" class="menu-icon" aria-hidden="true" />
          <XMarkIcon v-else class="menu-icon" aria-hidden="true" />
        </DisclosureButton>
      </div>
    </div>

    <DisclosurePanel class="mobile-panel">
      <div class="mobile-links">
        <button
          v-for="item in navigation"
          :key="item.name"
          class="mobile-link"
          :class="{ active: isActive(item.href) }"
          @click="openLink(item.href, (item as any).external)"
        >
          {{ item.name }}
        </button>

        <div class="mobile-group">Tools</div>
        <button
          v-for="item in toolsMenu"
          :key="item.name"
          class="mobile-link"
          @click="openLink(item.href, item.external)"
        >
          {{ item.name }}
        </button>
      </div>
    </DisclosurePanel>
  </Disclosure>
</template>

<style scoped>
.top-nav {
  height: 60px;
  background: #0a1320;
  border-bottom: 1px solid #1a2840;
  position: sticky;
  top: 0;
  z-index: 120;
}

.nav-inner {
  max-width: 1150px;
  height: 100%;
  margin: 0 auto;
  padding: 0 24px;
  display: flex;
  align-items: center;
  gap: 20px;
}

.logo-block {
  display: flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
}

.logo {
  width: 28px;
  height: 28px;
  border-radius: 6px;
}

.brand {
  color: #ffffff;
  font-size: 16px;
  font-weight: 800;
}

.desktop-menu {
  display: none;
  align-items: center;
  gap: 4px;
  flex: 1;
}

.nav-link {
  border: none;
  background: transparent;
  color: #4a6080;
  cursor: pointer;
  border-radius: 8px;
  padding: 6px 14px;
  font-size: 14px;
  font-weight: 500;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.nav-link:hover,
.nav-link.active {
  color: #ffffff;
  background: #111e30;
}

.dropdown {
  position: relative;
}

.chevron {
  width: 14px;
  height: 14px;
}

.dropdown-menu {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  width: 180px;
  background: #0d1726;
  border: 1px solid #1a2840;
  border-radius: 12px;
  padding: 6px;
  box-shadow: 0 24px 40px rgba(0, 0, 0, 0.4);
}

.dropdown-item {
  width: 100%;
  text-align: left;
  background: transparent;
  border: none;
  color: #8aaacf;
  border-radius: 8px;
  padding: 10px 12px;
  font-size: 14px;
  cursor: pointer;
}

.dropdown-item:hover {
  color: #ffffff;
  background: #111e30;
}

.right-slot {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 10px;
}

.alpha-badge {
  background: #c0392b;
  color: #ffffff;
  border-radius: 6px;
  padding: 3px 8px;
  font-size: 10px;
  font-weight: 800;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.settings-btn {
  width: 36px;
  height: 36px;
  border-radius: 8px;
  border: 1px solid #1a2840;
  background: #111e30;
  color: #4a6080;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.settings-btn:hover {
  color: #ffffff;
  border-color: #2563eb55;
}

.settings-icon {
  width: 18px;
  height: 18px;
}

.mobile-menu-button {
  margin-left: auto;
  display: flex;
}

.disclosure-btn {
  border: none;
  background: transparent;
  color: #8aaacf;
  width: 36px;
  height: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
}

.disclosure-btn:hover {
  color: #ffffff;
  background: #111e30;
}

.menu-icon {
  width: 22px;
  height: 22px;
}

.mobile-panel {
  border-top: 1px solid #1a2840;
  background: #0a1320;
}

.mobile-links {
  padding: 10px 16px 14px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mobile-group {
  color: #3a5070;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  padding: 10px 12px 4px;
  font-weight: 700;
}

.mobile-link {
  border: none;
  background: transparent;
  color: #8aaacf;
  text-align: left;
  border-radius: 8px;
  padding: 10px 12px;
  font-size: 14px;
  cursor: pointer;
}

.mobile-link.active,
.mobile-link:hover {
  color: #ffffff;
  background: #111e30;
}

@media (min-width: 768px) {
  .desktop-menu {
    display: flex;
  }

  .mobile-menu-button,
  .mobile-panel {
    display: none;
  }
}
</style>
