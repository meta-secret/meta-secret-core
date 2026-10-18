type FormFactorHints = {
  mobile?: boolean;
  getHighEntropyValues?: (hints: string[]) => Promise<unknown>;
};

export type WebQrNavigator = Pick<Navigator, 'userAgent' | 'platform' | 'maxTouchPoints'> & {
  userAgentData?: FormFactorHints;
  mediaDevices?: {
    getUserMedia?: unknown;
    enumerateDevices?: () => Promise<readonly Pick<MediaDeviceInfo, 'kind'>[]>;
  };
};

async function isPhoneOrTablet(nav: WebQrNavigator): Promise<boolean> {
  const hints = nav.userAgentData;
  if (hints?.getHighEntropyValues) {
    try {
      const values = await hints.getHighEntropyValues(['formFactors']);
      if (values && typeof values === 'object' && 'formFactors' in values) {
        const factors = values.formFactors;
        if (Array.isArray(factors) && factors.length) {
          return factors.some((factor) => factor === 'Mobile' || factor === 'Tablet');
        }
      }
    } catch {
      // Hints are optional; Safari/iPadOS also need the fallback below.
    }
  }
  return (
    hints?.mobile === true ||
    /Android|iPhone|iPad|iPod|\bTablet\b/i.test(nav.userAgent) ||
    (nav.platform === 'MacIntel' && nav.maxTouchPoints > 1)
  );
}

/** Presentation capability only. Never requests camera permission or opens a stream. */
export async function readWebQrAvailability(
  nav: WebQrNavigator = navigator,
  secureContext: boolean = window.isSecureContext,
): Promise<boolean> {
  if (!secureContext || !(await isPhoneOrTablet(nav))) return false;
  const media = nav.mediaDevices;
  if (typeof media?.getUserMedia !== 'function' || typeof media.enumerateDevices !== 'function') return false;
  try {
    const devices = await media.enumerateDevices();
    return devices.some((device) => device.kind === 'videoinput');
  } catch {
    return false;
  }
}
