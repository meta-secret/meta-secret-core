import { DeviceUiCategory } from 'meta-secret-web-cli';

export type DeviceCategoryMessages = {
  deviceCategoryAndroid: string;
  deviceCategoryIphone: string;
  deviceCategoryTablet: string;
  deviceCategoryDesktop: string;
  deviceCategoryCli: string;
  deviceCategoryWeb: string;
  deviceCategoryOther: string;
  deviceCategoryUnavailable: string;
};

export function deviceCategoryLabel(
  category: DeviceUiCategory,
  unavailable: boolean,
  messages: DeviceCategoryMessages,
): string {
  if (unavailable) {
    return messages.deviceCategoryUnavailable;
  }
  switch (category) {
    case DeviceUiCategory.Android:
      return messages.deviceCategoryAndroid;
    case DeviceUiCategory.Iphone:
      return messages.deviceCategoryIphone;
    case DeviceUiCategory.Tablet:
      return messages.deviceCategoryTablet;
    case DeviceUiCategory.Desktop:
      return messages.deviceCategoryDesktop;
    case DeviceUiCategory.Cli:
      return messages.deviceCategoryCli;
    case DeviceUiCategory.Web:
      return messages.deviceCategoryWeb;
    case DeviceUiCategory.Other:
    default:
      return messages.deviceCategoryOther;
  }
}
