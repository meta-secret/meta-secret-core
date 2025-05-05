<script lang="js">
import QRCodeStyling from 'qr-code-styling';

import init, { split } from 'meta-secret-web-cli';

export default {
  data() {
    return {
      password: 'top$ecret',
      note1: '',
      note2: '',
    };
  },

  methods: {
    splitPassword() {
      init().then(async () => {
        console.log('Split password: ', this.password);

        const qrImages = document.getElementById('qr-images');

        while (qrImages.firstChild) {
          qrImages.removeChild(qrImages.firstChild);
        }

        const shares = split(this.password);
        this.sharesProcessing(shares, qrImages);
      });
    },

    sharesProcessing: function (shares, qrImages) {
      shares.forEach((share) => {
        const shareId = share['share_id'];
        const shareIdText = 'part: ' + shareId + '/' + shares.length;
        const textImage = this.textToImage(this.note1, this.note2, shareIdText, shareId);
        const qrCodeStyling = this.generateQrCodeStyling(JSON.stringify(share), textImage);

        const canvasDiv = document.createElement('div');
        canvasDiv.id = 'qrCanvas' + shareId;
        canvasDiv.className = 'qr-canvas-container';

        qrCodeStyling.append(canvasDiv);
        qrImages.appendChild(canvasDiv);

        const downloadLink = document.createElement('button');
        downloadLink.onclick = function (e) {
          qrCodeStyling.download({ name: 'qr' + shareId, extension: 'png' });
        };
        downloadLink.id = 'downloadQr-' + shareId;
        downloadLink.innerHTML = 'Download QR';
        downloadLink.className = 'download-button';

        const qrDiv = document.getElementById('qrCanvas' + shareId);
        qrDiv.appendChild(downloadLink);
      });
    },

    textToImage(line1, line2, line3, id) {
      const canvas = document.createElement('canvas');
      canvas.width = 300;
      canvas.height = 300;
      canvas.id = 'canvas' + id;
      const ctx = canvas.getContext('2d');
      ctx.font = '70px Arial';
      ctx.fillText(line1, 15, 75);
      ctx.fillText(line2, 15, 150);
      ctx.fillText(line3, 15, 250);
      return canvas.toDataURL();
    },

    generateQrCodeStyling(share, textImage) {
      const options = {
        width: 300,
        height: 300,
        type: 'svg',
        data: share,
        margin: 3,
        qrOptions: {
          typeNumber: 0,
          mode: 'Byte',
          errorCorrectionLevel: 'H',
        },
        imageOptions: {
          hideBackgroundDots: true,
          imageSize: 0.2,
          margin: 1,
        },
        dotsOptions: {
          type: 'dots',
          color: '#000000',
          gradient: null,
        },
        backgroundOptions: {
          color: '#ffffff',
        },
        image: textImage,
        dotsOptionsHelper: {
          colorType: {
            single: true,
            gradient: false,
          },
          gradient: {
            linear: true,
            radial: false,
            color1: '#6a1a4c',
            color2: '#6a1a4c',
            rotation: '0',
          },
        },
        cornersSquareOptions: {
          type: 'square',
          color: '#000000',
          gradient: {
            type: 'linear',
            rotation: 0,
            colorStops: [
              {
                offset: 0,
                color: '#000000',
              },
              {
                offset: 1,
                color: '#8d8b8b',
              },
            ],
          },
        },
        cornersSquareOptionsHelper: {
          colorType: {
            single: true,
            gradient: false,
          },
          gradient: {
            linear: true,
            radial: false,
            color1: '#000000',
            color2: '#000000',
            rotation: '0',
          },
        },
        cornersDotOptions: {
          type: '',
          color: '#000000',
        },
        cornersDotOptionsHelper: {
          colorType: {
            single: true,
            gradient: false,
          },
          gradient: {
            linear: true,
            radial: false,
            color1: '#000000',
            color2: '#000000',
            rotation: '0',
          },
        },
        backgroundOptionsHelper: {
          colorType: {
            single: true,
            gradient: false,
          },
          gradient: {
            linear: true,
            radial: false,
            color1: '#ffffff',
            color2: '#ffffff',
            rotation: '0',
          },
        },
      };
      return new QRCodeStyling(options);
    },
  },
};
</script>

<template>
  <div class="split-password-container">
    <div class="header">
      <h1>Split Password</h1>
      <p class="description">
        Enter your password and optional notes to split it into secure shares
      </p>
    </div>

    <div class="form-container">
      <div class="form-group">
        <label for="note1">Note 1</label>
        <input
          class="input-field"
          type="text"
          id="note1"
          v-model="note1"
          placeholder="Enter first note (optional)"
          maxlength="10"
        />
      </div>

      <div class="form-group">
        <label for="note2">Note 2</label>
        <input
          class="input-field"
          type="text"
          id="note2"
          v-model="note2"
          placeholder="Enter second note (optional)"
          maxlength="10"
        />
      </div>

      <div class="form-group">
        <label for="password">Password</label>
        <input
          class="input-field"
          type="text"
          id="password"
          v-model="password"
          placeholder="Enter your password"
        />
      </div>

      <button class="split-button" @click="splitPassword">
        <span class="button-icon">🔒</span>
        <span>Split Password</span>
      </button>
    </div>

    <div class="qr-container" id="qr-images"></div>
  </div>
</template>

<style>
.split-password-container {
  max-width: 800px;
  margin: 0 auto;
  padding: 2rem;
  font-family: 'Inter var', system-ui, -apple-system, sans-serif;
}

.header {
  text-align: center;
  margin-bottom: 2rem;
}

.header h1 {
  font-size: 2rem;
  font-weight: 700;
  margin-bottom: 0.5rem;
  background: linear-gradient(90deg, #3b82f6, #8b5cf6);
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.description {
  color: #6b7280;
  font-size: 1rem;
}

.form-container {
  background-color: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(10px);
  border-radius: 0.75rem;
  padding: 2rem;
  box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.1);
  margin-bottom: 2rem;
}

.form-group {
  margin-bottom: 1.5rem;
}

.form-group label {
  display: block;
  margin-bottom: 0.5rem;
  font-weight: 500;
  color: #374151;
  font-size: 0.875rem;
}

.input-field {
  width: 100%;
  padding: 0.75rem 1rem;
  border: 1px solid #d1d5db;
  border-radius: 0.375rem;
  background-color: #f9fafb;
  color: #111827;
  font-size: 1rem;
  transition: all 0.2s;
  box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05);
}

.input-field:focus {
  outline: none;
  border-color: #3b82f6;
  box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.25);
}

.input-field::placeholder {
  color: #9ca3af;
}

.split-button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  padding: 0.75rem 1.5rem;
  background: linear-gradient(90deg, #3b82f6, #8b5cf6);
  color: white;
  border: none;
  border-radius: 0.375rem;
  font-weight: 600;
  font-size: 1rem;
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 4px 6px -1px rgba(59, 130, 246, 0.5);
}

.split-button:hover {
  transform: translateY(-2px);
  box-shadow: 0 6px 8px -1px rgba(59, 130, 246, 0.6);
}

.split-button:active {
  transform: translateY(0);
}

.button-icon {
  margin-right: 0.5rem;
  font-size: 1.25rem;
}

.qr-container {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 2rem;
  margin-top: 2rem;
}

.qr-canvas-container {
  position: relative;
  background-color: white;
  border-radius: 0.75rem;
  padding: 1.5rem;
  box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.1), 0 2px 4px -1px rgba(0, 0, 0, 0.06);
  transition: transform 0.2s;
  display: flex;
  flex-direction: column;
  align-items: center;
}

.qr-canvas-container:hover {
  transform: translateY(-5px);
  box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.1), 0 4px 6px -2px rgba(0, 0, 0, 0.05);
}

.download-button {
  margin-top: 1rem;
  padding: 0.5rem 1rem;
  background-color: #3b82f6;
  color: white;
  border: none;
  border-radius: 0.375rem;
  font-weight: 500;
  font-size: 0.875rem;
  cursor: pointer;
  transition: background-color 0.2s;
}

.download-button:hover {
  background-color: #2563eb;
}

@media (max-width: 768px) {
  .split-password-container {
    padding: 1rem;
  }
  
  .form-container {
    padding: 1.5rem;
  }
  
  .qr-container {
    gap: 1rem;
  }
}

/* Dark mode support */
@media (prefers-color-scheme: dark) {
  .header h1 {
    background: linear-gradient(90deg, #60a5fa, #a78bfa);
    -webkit-background-clip: text;
    background-clip: text;
  }
  
  .description {
    color: #9ca3af;
  }
  
  .form-container {
    background-color: rgba(30, 41, 59, 0.5);
    border-color: rgba(55, 65, 81, 0.5);
  }
  
  .form-group label {
    color: #e5e7eb;
  }
  
  .input-field {
    background-color: #1f2937;
    border-color: #374151;
    color: #f9fafb;
  }
  
  .input-field:focus {
    border-color: #60a5fa;
  }
  
  .input-field::placeholder {
    color: #6b7280;
  }
  
  .split-button {
    background: linear-gradient(90deg, #3b82f6, #8b5cf6);
  }
  
  .qr-canvas-container {
    background-color: #1f2937;
    border: 1px solid #374151;
  }
}
</style> 