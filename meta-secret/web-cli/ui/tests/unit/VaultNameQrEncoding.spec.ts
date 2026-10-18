import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';
import QRCodeStyling from 'qr-code-styling';
import qrcode from 'qrcode-generator';
import VaultNameQr from '@/components/vault/VaultNameQr.vue';

afterEach(() => vi.restoreAllMocks());

describe('Vault QR real encoder UTF-8 boundary', () => {
  it.each(['Семья', '家族 🔐', 'Dmitrii@Example.org'])(
    'encodes UTF-8 bytes for %s without replacing characters',
    async (name) => {
      const original = qrcode.stringToBytes;
      const expectedBytes = Array.from(Buffer.from(name, 'utf8'));
      qrcode.stringToBytes = () => expectedBytes;
      const expected = qrcode(0, 'M');
      try {
        expected.addData(name, 'Byte');
        expected.make();
      } finally {
        qrcode.stringToBytes = original;
      }

      // Pass-through spy: the installed real encoder still creates/draws the SVG.
      const ready = vi.spyOn(QRCodeStyling.prototype, 'getRawData');
      const wrapper = mount(VaultNameQr, { props: { vaultName: name } });
      try {
        await flushPromises();
        expect(wrapper.find('svg').exists()).toBe(true);
        const instance = ready.mock.contexts.at(-1) as QRCodeStyling | undefined;
        const actual = instance?._qr;
        expect(actual).toBeDefined();
        expect(actual!.getModuleCount()).toBe(expected.getModuleCount());
        const count = expected.getModuleCount();
        const matrix = (qr: { isDark(row: number, col: number): boolean }) =>
          Array.from({ length: count }, (_, row) => Array.from({ length: count }, (_, col) => qr.isDark(row, col)));
        expect(matrix(actual!)).toEqual(matrix(expected));
      } finally {
        wrapper.unmount();
      }
    },
  );
});
