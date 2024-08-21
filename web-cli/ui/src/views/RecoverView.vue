<script lang="js">

import init, {restore_password} from "meta-secret-web-cli";
import QrScanner from 'qr-scanner';

export default {
  data() {
    return {
      recoveredPassword: ''
    }
  },

  methods: {
    recoverPassword() {

      init().then(() => {
        let imagesElement = document.getElementById("qrImages");
        let qrCodes = imagesElement.getElementsByTagName('img');

        let asyncShares = [];

        Array.from(qrCodes).forEach(qr => {
          asyncShares.push(QrScanner.scanImage(qr, {returnDetailedScanResult: true}));
        });

        Promise.all(asyncShares)
            .then(qrShares => {
              //use wasm to recover from json files
              let shares = qrShares.map(share => JSON.parse(share.data));
              console.log("restore password, js!");
              this.recoveredPassword = restore_password(shares);
            })
            .catch(err => {
              alert("Error recovering password: " + err)
            });
      });
    },

    openFile(event) {
      let input = event.target;

      Array.from(input.files).forEach(qr => {
        let reader = new FileReader();

        reader.onload = function () {
          let dataURL = reader.result;
          let outputImg = document.createElement('img');
          outputImg.style.margin = "0 0 0 0";
          outputImg.src = dataURL;

          let imagesElement = document.getElementById("qrImages");
          imagesElement.appendChild(outputImg);
        };

        reader.readAsDataURL(qr);
      });
    }
  }
}
</script>

<template>
  <div class="flex justify-center">
    <p class="text-2xl">Recover Password</p>
  </div>

  <div class="py-4"/>

  <div class="container flex justify-center px-4 py-4">
    <div class="flex flex-col">
      <label for="file-upload" class="custom-file-upload">
        Choose QR codes
      </label>
      <input
          class="block text-sm text-gray-900 bg-gray-50 rounded-lg border border-gray-300 cursor-pointer
          dark:text-gray-400 focus:outline-none dark:bg-gray-700 dark:border-gray-600 dark:placeholder-gray-400"
          id="file-upload" type='file' accept='image/*' @change="openFile" multiple
      >

      <div class="py-2"/>

      <input
          class="text-gray-900 bg-white border border-gray-300 focus:outline-none hover:bg-gray-100 focus:ring-4
          focus:ring-gray-200 font-medium text-sm px-5 py-2.5 dark:bg-gray-800
          dark:text-white dark:border-gray-600 dark:hover:bg-gray-700 dark:hover:border-gray-600 dark:focus:ring-gray-700"
          type="button" id="recoverButton" value="Recover" @click="recoverPassword"
      >
      <div class="py-2"/>

      <div id="securityBox" class="container flex flex-col items-start px-2 py-2 mt-2 security-box-border">
        <h3>Recovered Password:</h3>
        <input class="dark:bg-gray-800" id="passwordBox" v-model="recoveredPassword">
      </div>
    </div>
  </div>

  <div class="container flex flex-col justify-center items-center" id="qrImages"></div>
</template>

<style>
.security-box-border {
  border: 1px solid rgb(209, 213, 219);
}
</style>