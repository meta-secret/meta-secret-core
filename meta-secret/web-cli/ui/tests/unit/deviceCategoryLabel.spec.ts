import { deviceCategoryLabel } from '@/utils/deviceCategoryLabel';
import { DeviceUiCategory } from 'meta-secret-web-cli';
import { describe, expect, it } from 'vitest';

const testMessages = {
  deviceCategoryAndroid: 'MSG_ANDROID',
  deviceCategoryIphone: 'MSG_IPHONE',
  deviceCategoryTablet: 'MSG_TABLET',
  deviceCategoryDesktop: 'MSG_DESKTOP',
  deviceCategoryCli: 'MSG_CLI',
  deviceCategoryWeb: 'MSG_WEB',
  deviceCategoryOther: 'MSG_OTHER',
  deviceCategoryUnavailable: 'MSG_UNAVAILABLE',
};

describe('deviceCategoryLabel', () => {
  it('returns unavailable message when flag is set', () => {
    expect(deviceCategoryLabel(DeviceUiCategory.Android, true, testMessages)).toBe('MSG_UNAVAILABLE');
  });

  it('maps each DeviceUiCategory to the matching message key', () => {
    expect(deviceCategoryLabel(DeviceUiCategory.Android, false, testMessages)).toBe('MSG_ANDROID');
    expect(deviceCategoryLabel(DeviceUiCategory.Iphone, false, testMessages)).toBe('MSG_IPHONE');
    expect(deviceCategoryLabel(DeviceUiCategory.Tablet, false, testMessages)).toBe('MSG_TABLET');
    expect(deviceCategoryLabel(DeviceUiCategory.Desktop, false, testMessages)).toBe('MSG_DESKTOP');
    expect(deviceCategoryLabel(DeviceUiCategory.Cli, false, testMessages)).toBe('MSG_CLI');
    expect(deviceCategoryLabel(DeviceUiCategory.Web, false, testMessages)).toBe('MSG_WEB');
    expect(deviceCategoryLabel(DeviceUiCategory.Other, false, testMessages)).toBe('MSG_OTHER');
  });
});
