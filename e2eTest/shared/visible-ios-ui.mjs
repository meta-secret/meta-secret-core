import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

/** Open the visible Xcode app that presents simulator devices on this Xcode version. */
export async function openVisibleIosDeviceUi({ runAndCapture, runAndWait, simulatorUdid }) {
  if (!simulatorUdid) throw new Error('Cannot open visible iOS device UI without a simulator UDID');
  const { stdout } = await runAndCapture('xcode-select', ['-p']);
  const developerDirectory = stdout.trim();
  if (!developerDirectory) throw new Error('xcode-select -p returned an empty developer directory');

  const applicationCandidates = [
    resolve(developerDirectory, '../Applications/DeviceHub.app'),
    resolve(developerDirectory, 'Applications/Simulator.app'),
  ];
  const applicationPath = applicationCandidates.find((candidate) => existsSync(candidate));
  if (!applicationPath) {
    throw new Error(`Neither DeviceHub.app nor Simulator.app exists for Xcode at ${developerDirectory}`);
  }

  if (applicationPath.endsWith('/DeviceHub.app')) {
    await runAndWait('open', [applicationPath]);
    await runAndWait('open', [`devices://device/open?id=${encodeURIComponent(simulatorUdid)}`]);
  } else {
    await runAndWait('open', [applicationPath, '--args', '-CurrentDeviceUDID', simulatorUdid]);
  }
  console.log(`🖥️ Opened visible iOS device UI: ${applicationPath}`);
  return applicationPath;
}
