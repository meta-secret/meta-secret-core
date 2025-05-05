<script lang="js">
import init, { restore_password } from 'meta-secret-web-cli';
import QrScanner from 'qr-scanner';

export default {
  data() {
    return {
      recoveredPassword: '',
      imagesLoaded: 0,
      error: '',
    };
  },

  methods: {
    recoverPassword() {
      this.error = '';
      if (this.imagesLoaded === 0) {
        this.error = 'Please upload QR code images first';
        return;
      }

      init().then(() => {
        const imagesElement = document.getElementById('qrImages');
        const qrCodes = imagesElement.getElementsByTagName('img');

        if (qrCodes.length === 0) {
          this.error = 'No QR code found';
          return;
        }

        const asyncShares = [];

        Array.from(qrCodes).forEach((qr) => {
          asyncShares.push(QrScanner.scanImage(qr, { returnDetailedScanResult: true }));
        });

        Promise.all(asyncShares)
          .then((qrShares) => {
            //use wasm to recover from json files
            const shares = qrShares.map((share) => JSON.parse(share.data));
            console.log('restore password, js!');
            this.recoveredPassword = restore_password(shares);
          })
          .catch((err) => {
            console.error('QR Scanning Error:', err);
            this.error = 'Error recovering password: ' + err;
          });
      });
    },

    openFile(event) {
      const input = event.target;
      
      // Clear existing images first
      const imagesElement = document.getElementById('qrImages');
      imagesElement.innerHTML = '';
      this.imagesLoaded = 0;
      this.error = '';
      
      if (!input.files || input.files.length === 0) {
        return;
      }

      Array.from(input.files).forEach((qr) => {
        const reader = new FileReader();

        reader.onload = () => {
          const dataURL = reader.result;
          const outputImg = document.createElement('img');
          // Minimal styling to avoid scanner issues
          outputImg.className = 'qr-image';
          outputImg.src = dataURL;
          
          // Wait for image to load
          outputImg.onload = () => {
            this.imagesLoaded++;
          };

          imagesElement.appendChild(outputImg);
        };

        reader.readAsDataURL(qr);
      });
    },
  },
};
</script>

<template>
  <div class="recover-password-container">
    <div class="header">
      <h1>Recover Password</h1>
      <p class="description">
        Upload your QR code shares to recover your secret password
      </p>
    </div>

    <div class="form-container">
      <div class="form-group">
        <label for="file-upload">QR Code Shares</label>
        <div class="file-upload-wrapper">
          <input
            class="input-field file-input"
            id="file-upload"
            type="file"
            accept="image/*"
            @change="openFile"
            multiple
          />
          <div class="file-upload-label">Choose QR code images</div>
        </div>
        <div v-if="imagesLoaded > 0" class="images-status">
          {{ imagesLoaded }} QR images loaded
        </div>
      </div>

      <button class="recover-button" @click="recoverPassword">
        <span class="button-icon">🔑</span>
        <span>Recover Password</span>
      </button>

      <div v-if="error" class="error-message">
        {{ error }}
      </div>

      <div class="form-group result-container" v-if="recoveredPassword">
        <label for="passwordBox">Recovered Password:</label>
        <input 
          class="input-field" 
          id="passwordBox" 
          v-model="recoveredPassword" 
          readonly
        />
      </div>
    </div>

    <div class="qr-preview-container" id="qrImages"></div>
  </div>
</template>

<style>
.recover-password-container {
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

.images-status {
  margin-top: 0.5rem;
  color: #10b981;
  font-size: 0.875rem;
}

.error-message {
  color: #ef4444;
  margin: 1rem 0;
  padding: 0.5rem;
  background-color: rgba(239, 68, 68, 0.1);
  border-radius: 0.375rem;
  text-align: center;
}

.file-upload-wrapper {
  position: relative;
  overflow: hidden;
  display: inline-block;
  width: 100%;
}

.file-input {
  position: absolute;
  font-size: 100px;
  opacity: 0;
  right: 0;
  top: 0;
  cursor: pointer;
  height: 100%;
  width: 100%;
  z-index: 2;
}

.file-upload-label {
  display: block;
  padding: 0.75rem 1rem;
  border: 1px solid #d1d5db;
  border-radius: 0.375rem;
  background-color: #f9fafb;
  color: #6b7280;
  font-size: 1rem;
  text-align: center;
  cursor: pointer;
  transition: all 0.2s;
}

.file-upload-label:hover {
  background-color: #f3f4f6;
  border-color: #9ca3af;
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

.recover-button {
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

.recover-button:hover {
  transform: translateY(-2px);
  box-shadow: 0 6px 8px -1px rgba(59, 130, 246, 0.6);
}

.recover-button:active {
  transform: translateY(0);
}

.button-icon {
  margin-right: 0.5rem;
  font-size: 1.25rem;
}

.result-container {
  margin-top: 1.5rem;
  padding-top: 1.5rem;
  border-top: 1px solid #e5e7eb;
}

.qr-preview-container {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 1rem;
  margin-top: 2rem;
}

/* Minimal styling for QR images to prevent scanning issues */
.qr-image {
  display: inline-block; 
  margin: 8px;
}

@media (max-width: 768px) {
  .recover-password-container {
    padding: 1rem;
  }
  
  .form-container {
    padding: 1.5rem;
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

  .file-upload-label {
    background-color: #1f2937;
    border-color: #374151;
    color: #9ca3af;
  }
  
  .file-upload-label:hover {
    background-color: #111827;
    border-color: #4b5563;
  }
  
  .input-field:focus {
    border-color: #60a5fa;
  }
  
  .recover-button {
    background: linear-gradient(90deg, #3b82f6, #8b5cf6);
  }
  
  .result-container {
    border-top-color: #374151;
  }
}
</style> 