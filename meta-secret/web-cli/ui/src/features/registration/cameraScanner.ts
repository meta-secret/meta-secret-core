import QrScanner from 'qr-scanner';
import { parseVaultName } from './registrationState';

export type CameraScanner = { start(): Promise<void>; stop(): void };
export type CameraScannerFactory = (
  video: HTMLVideoElement,
  onScan: (name: string) => void,
  onError: (message: string) => void,
) => CameraScanner;

export function createCameraScanner(
  video: HTMLVideoElement,
  onScan: (name: string) => void,
  onError: (message: string) => void,
) {
  let scanner: QrScanner | undefined;
  let closed = false;
  let completed = false;
  const stop = () => {
    closed = true;
    scanner?.destroy();
    scanner = undefined;
  };
  return {
    async start() {
      if (closed || scanner) return;
      scanner = new QrScanner(
        video,
        (result) => {
          if (closed || completed) return;
          try {
            const name = parseVaultName(result.data);
            completed = true;
            stop();
            onScan(name);
          } catch {
            onError('This QR code is not a Vault name.');
          }
        },
        {
          returnDetailedScanResult: true,
          preferredCamera: 'environment',
          highlightScanRegion: false,
          highlightCodeOutline: false,
        },
      );
      try {
        await scanner.start();
        if (closed) scanner?.destroy();
      } catch {
        const alreadyClosed = closed;
        stop();
        if (!alreadyClosed) onError('Camera access is off.');
      }
    },
    stop,
  };
}
