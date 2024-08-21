<script lang="js">
import QRCodeStyling from "qr-code-styling";

import init, {split} from "meta-secret-web-cli";

export default {
  data() {
    return {
      password: "top$ecret",
      note1: '',
      note2: ''
    }
  },

  methods: {
    splitPassword() {
      init().then(async () => {
        console.log("Split password: ", this.password);

        let qrImages = document.getElementById('qr-images');

        while (qrImages.firstChild) {
          qrImages.removeChild(qrImages.firstChild);
        }

        let shares = split(this.password);
        this.sharesProcessing(shares, qrImages);
      });
    },

    sharesProcessing: function (shares, qrImages) {
      shares.forEach(share => {
        let shareId = share['share_id'];
        let shareIdText = 'part: ' + shareId + '/' + shares.length;
        let textImage = this.textToImage(this.note1, this.note2, shareIdText, shareId);
        let qrCodeStyling = this.generateQrCodeStyling(JSON.stringify(share), textImage);

        let canvasDiv = document.createElement("div");
        canvasDiv.id = 'qrCanvas' + shareId;

        qrCodeStyling.append(canvasDiv);
        qrImages.appendChild(canvasDiv);

        let downloadLink = document.createElement("button");
        downloadLink.onclick = function (e) {
          qrCodeStyling.download({ name: "qr" + shareId, extension: "png" });
        }
        downloadLink.id = "downloadQr-" + shareId;
        downloadLink.innerHTML = "download";
        downloadLink.style.marginLeft = "30px";
        downloadLink.className = "submit-button m-2";

        let qrDiv = document.getElementById('qrCanvas' + shareId)
        qrDiv.appendChild(downloadLink);

        //generateQrCode(qr, share);
      });
    },

    textToImage(line1, line2, line3, id) {
      let canvas = document.createElement("canvas");
      canvas.width = 300;
      canvas.height = 300;
      canvas.id = 'canvas' + id;
      let ctx = canvas.getContext('2d');
      ctx.font = "70px Arial";
      ctx.fillText(line1, 15, 75);
      ctx.fillText(line2, 15, 150);
      ctx.fillText(line3, 15, 250);
      return canvas.toDataURL();
    },

    generateQrCodeStyling(share, textImage) {
      return new QRCodeStyling(
          {
            width: 300,
            height: 300,
            type: "svg",
            data: share,
            margin: 3,
            qrOptions: {
              typeNumber: 0,
              mode: "Byte",
              errorCorrectionLevel: "H"
            },
            imageOptions: {
              hideBackgroundDots: true,
              imageSize: 0.2,
              margin: 1
            },
            dotsOptions: {
              type: "dots",
              color: "#000000",
              gradient: null
            },
            backgroundOptions: {
              color: "#ffffff"
            },
            image: textImage,
            dotsOptionsHelper: {
              colorType: {
                single: true,
                gradient: false
              },
              gradient: {
                linear: true,
                radial: false,
                color1: "#6a1a4c",
                color2: "#6a1a4c",
                rotation: "0"
              }
            },
            cornersSquareOptions: {
              type: "square",
              color: "#000000",
              gradient: {
                type: "linear",
                rotation: 0,
                colorStops: [
                  {
                    offset: 0,
                    color: "#000000"
                  },
                  {
                    offset: 1,
                    color: "#8d8b8b"
                  }
                ]
              }
            },
            cornersSquareOptionsHelper: {
              colorType: {
                single: true,
                gradient: false
              },
              gradient: {
                linear: true,
                radial: false,
                color1: "#000000",
                color2: "#000000",
                rotation: "0"
              }
            },
            cornersDotOptions: {
              type: "",
              color: "#000000"
            },
            cornersDotOptionsHelper: {
              colorType: {
                single: true,
                gradient: false
              },
              gradient: {
                linear: true,
                radial: false,
                color1: "#000000",
                color2: "#000000",
                rotation: "0"
              }
            },
            backgroundOptionsHelper: {
              colorType: {
                single: true,
                gradient: false
              },
              gradient: {
                linear: true,
                radial: false,
                color1: "#ffffff",
                color2: "#ffffff",
                rotation: "0"
              }
            }
          }
      );
    }
  }
}
</script>

<template>
  <div class="flex justify-center">
    <p class="text-2xl">Split Password</p>
  </div>

  <div class="container flex justify-center px-4">
    <div class="flex flex-col items-start">
      <div>
        <label for="note1">Note1:</label>
        <input class="input-element" type="text" v-model="note1" max="10" size="10">
      </div>

      <div>
        <label for="note2">Note2:</label>
        <input class="input-element" type="text" id="note2" v-model="note2" max="10" size="10">
      </div>

      <label for="password">password:</label>
      <div class="flex flex-col items-stretch">
        <input class="input-element" type="text" id="password" v-model="password" size="50">
        <input class="submit-button dark:text-white" type="button" id="splitButton" value="Split" @click="splitPassword">
      </div>
    </div>
  </div>

  <div class="container flex flex-col justify-center items-center py-4" id="qr-images"></div>

</template>

<style>
.input-element {
  width: 100%;
  padding: 12px;
  margin: 6px 0 4px;
  border: 1px solid #ccc;
  background: #fafafa;
  color: #000;
  font-family: sans-serif;
  font-size: 12px;
  line-height: normal;
  box-sizing: border-box;
  border-radius: 2px;
}

.submit-button {
  background-color: #FFFFFF;
  border: 1px solid rgb(209, 213, 219);
  border-radius: .1rem;
  box-sizing: border-box;
  color: #111827;
  font-family: "Inter var", ui-sans-serif, system-ui, -apple-system, system-ui, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji";
  font-size: .875rem;
  font-weight: 600;
  line-height: 1.25rem;
  padding: .75rem 1rem;
  text-align: center;
  text-decoration: none #D1D5DB solid;
  text-decoration-thickness: auto;
  box-shadow: 0 1px 2px 0 rgba(0, 0, 0, 0.05);
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
  touch-action: manipulation;
}

.submit-button:focus {
  outline: 2px solid transparent;
  outline-offset: 2px;
}

.submit-button:focus-visible {
  box-shadow: none;
}
</style>